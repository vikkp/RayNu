//! Devices that belong only to the seated guest.
//!
//! Virtio-blk, the local APIC, and the I/O APIC registers here are not the
//! ones the running guest uses. A queue notify can name one read of the
//! seated disk. The bytes themselves are fetched by the caller.
//!
//! Pillar: [Z]
//! Proven Core: **outside** (ADR-002 / ADR-021).
//! VERIFICATION: L1 (host tests).

use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

const OFF_NOTIFY: u16 = 0x300;
const OFF_ISR: u16 = 0x100;
const OFF_DEVICE: u16 = 0x200;
const QUEUE_MAX: u16 = 128;

struct Disk {
    regs: [u8; 0x400],
    last_avail: u16,
    used_idx: u16,
    capacity: u64,
    notifies: u32,
    pending: bool,
}

struct Apic {
    regs: [u8; 0x400],
}

struct Ioapic {
    index: u8,
    regs: [u32; 64],
}

static mut DISK: Disk = Disk {
    regs: [0; 0x400],
    last_avail: 0,
    used_idx: 0,
    capacity: 0,
    notifies: 0,
    pending: false,
};
static mut APIC: Apic = Apic { regs: [0; 0x400] };
static mut IOAPIC: Ioapic = Ioapic {
    index: 0,
    regs: [0; 64],
};
static HIT: AtomicBool = AtomicBool::new(false);
static NOTIFY: AtomicU32 = AtomicU32::new(0);

pub fn reset_for_test() {
    unsafe {
        DISK = Disk {
            regs: [0; 0x400],
            last_avail: 0,
            used_idx: 0,
            capacity: 0,
            notifies: 0,
            pending: false,
        };
        APIC = Apic { regs: [0; 0x400] };
        IOAPIC = Ioapic {
            index: 0,
            regs: [0; 64],
        };
    }
    HIT.store(false, Ordering::Release);
    NOTIFY.store(0, Ordering::Release);
}

pub fn set_capacity_bytes(bytes: u64) {
    unsafe { DISK.capacity = bytes / 512 }
}

pub fn note_hit() -> bool {
    HIT.swap(true, Ordering::AcqRel) == false
}

fn disk() -> &'static mut Disk {
    unsafe { &mut *core::ptr::addr_of_mut!(DISK) }
}

fn read_reg(regs: &[u8], off: u16, size: u8) -> u64 {
    let o = off as usize;
    if o >= regs.len() {
        return 0;
    }
    let n = match size {
        1 => 1,
        2 => 2,
        4 => 4,
        8 => 8,
        _ => 4,
    };
    if o + n > regs.len() {
        return 0;
    }
    let mut buf = [0u8; 8];
    buf[..n].copy_from_slice(&regs[o..o + n]);
    u64::from_le_bytes(buf)
}

fn write_reg(regs: &mut [u8], off: u16, size: u8, val: u64) {
    let o = off as usize;
    let n = match size {
        1 => 1,
        2 => 2,
        4 => 4,
        8 => 8,
        _ => 4,
    };
    if o + n > regs.len() {
        return;
    }
    let bytes = val.to_le_bytes();
    regs[o..o + n].copy_from_slice(&bytes[..n]);
}

/// Virtio-pci common cfg, ISR, device config, and notify. `true` when the
/// guest rang the queue doorbell.
pub fn disk_access(off: u16, is_write: bool, size: u8, val: u64) -> (u64, bool) {
    let d = disk();
    if !is_write {
        let v = if off == 0x04 {
            let sel = u32::from_le_bytes(d.regs[0..4].try_into().unwrap_or([0; 4]));
            let feat = crate::devices::guest_virtio_blk::VIRTIO_BLK_DEVICE_FEATURES;
            if sel == 0 { feat & 0xffff_ffff } else { feat >> 32 }
        } else if off == OFF_ISR {
            let isr = u64::from(d.regs[OFF_ISR as usize]);
            d.regs[OFF_ISR as usize] = 0;
            isr
        } else if off == OFF_DEVICE || off == OFF_DEVICE + 4 {
            let shift = if off == OFF_DEVICE { 0 } else { 32 };
            (d.capacity >> shift) & if size >= 8 { u64::MAX } else { 0xffff_ffff }
        } else if off == 0x12 {
            1
        } else if off == 0x18 {
            let n = u16::from_le_bytes([d.regs[0x18], d.regs[0x19]]);
            u64::from(if n == 0 || n > QUEUE_MAX { QUEUE_MAX } else { n })
        } else {
            read_reg(&d.regs, off, size)
        };
        return (v, false);
    }
    if off == OFF_NOTIFY {
        d.notifies = d.notifies.saturating_add(1);
        NOTIFY.fetch_add(1, Ordering::AcqRel);
        return (0, true);
    }
    write_reg(&mut d.regs, off, size, val);
    (0, false)
}

pub fn disk_queue() -> (u64, u64, u64, u16, u16, u16) {
    let d = disk();
    let n = u16::from_le_bytes([d.regs[0x18], d.regs[0x19]]);
    (
        read_reg(&d.regs, 0x20, 8),
        read_reg(&d.regs, 0x28, 8),
        read_reg(&d.regs, 0x30, 8),
        d.last_avail,
        d.used_idx,
        if n == 0 || n > QUEUE_MAX { QUEUE_MAX } else { n },
    )
}

pub fn disk_advance(last_avail: u16, used_idx: u16) {
    let d = disk();
    d.last_avail = last_avail;
    d.used_idx = used_idx;
}

/// The queue interrupt bit. Reading the ISR register clears it.
pub fn arm_disk_irq() {
    let d = disk();
    d.regs[OFF_ISR as usize] |= 1;
    d.pending = true;
}

pub fn disk_irq_pending() -> bool {
    disk().pending
}

pub fn clear_disk_irq() {
    disk().pending = false;
}

pub fn notifies() -> u32 {
    NOTIFY.load(Ordering::Acquire)
}

pub const DATA_SEGS: usize = 16;
/// Install-disk INTx. Slot 2 is GSI 17 in the product DSDT.
pub const SEAT_DISK_GSI: u8 = 17;

/// One data descriptor. `to_guest` is the virtio WRITE flag.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Seg {
    pub gpa: u64,
    pub len: u32,
    pub to_guest: bool,
}

/// One virtio-blk request pulled from the avail ring.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlkReq {
    pub ty: u32,
    pub sector: u64,
    pub segs: [Seg; DATA_SEGS],
    pub nseg: usize,
    pub status_gpa: u64,
    pub head: u16,
}

fn read_u16(read: &mut impl FnMut(u64, &mut [u8]) -> bool, gpa: u64) -> Option<u16> {
    let mut b = [0u8; 2];
    if !read(gpa, &mut b) {
        return None;
    }
    Some(u16::from_le_bytes(b))
}

fn read_desc(
    read: &mut impl FnMut(u64, &mut [u8]) -> bool,
    gpa: u64,
) -> Option<(u64, u32, u16, u16)> {
    let mut b = [0u8; 16];
    if !read(gpa, &mut b) {
        return None;
    }
    let addr = u64::from_le_bytes(b[0..8].try_into().ok()?);
    let len = u32::from_le_bytes(b[8..12].try_into().ok()?);
    let flags = u16::from_le_bytes(b[12..14].try_into().ok()?);
    let next = u16::from_le_bytes(b[14..16].try_into().ok()?);
    Some((addr, len, flags, next))
}

/// Pull one virtio-blk chain. `last` is the last consumed avail index.
pub fn pull_request(
    read: &mut impl FnMut(u64, &mut [u8]) -> bool,
    desc: u64,
    driver: u64,
    last: u16,
    qsize: u16,
) -> Option<BlkReq> {
    if qsize == 0 {
        return None;
    }
    let idx = read_u16(read, driver.wrapping_add(2))?;
    if idx == last {
        return None;
    }
    let ring = driver
        .wrapping_add(4)
        .wrapping_add(u64::from(last % qsize) * 2);
    let head = read_u16(read, ring)?;
    let mut cur = head;
    let mut segs = [Seg {
        gpa: 0,
        len: 0,
        to_guest: false,
    }; DATA_SEGS];
    let mut nseg = 0usize;
    let mut status_gpa = 0u64;
    let mut ty = 0u32;
    let mut sector = 0u64;
    let mut got_hdr = false;
    let mut chain = 0u16;
    loop {
        if chain > qsize {
            return None;
        }
        let (addr, len, flags, next) = read_desc(read, desc.wrapping_add(u64::from(cur) * 16))?;
        let more = flags & 1 != 0;
        let to_guest = flags & 2 != 0;
        if !got_hdr && len >= 16 && !to_guest {
            let mut hdr = [0u8; 16];
            if !read(addr, &mut hdr) {
                return None;
            }
            ty = u32::from_le_bytes(hdr[0..4].try_into().ok()?);
            sector = u64::from_le_bytes(hdr[8..16].try_into().ok()?);
            got_hdr = true;
        } else if to_guest && len == 1 && status_gpa == 0 {
            status_gpa = addr;
        } else if len > 0 && nseg < DATA_SEGS {
            segs[nseg] = Seg {
                gpa: addr,
                len,
                to_guest,
            };
            nseg += 1;
        }
        if !more {
            break;
        }
        cur = next;
        chain += 1;
    }
    if !got_hdr {
        return None;
    }
    Some(BlkReq {
        ty,
        sector,
        segs,
        nseg,
        status_gpa,
        head,
    })
}

/// Used-ring element GPA and the used-index GPA for this completion.
pub fn used_slot(device: u64, qsize: u16, used_idx: u16) -> Option<(u64, u64)> {
    if qsize == 0 {
        return None;
    }
    let slot = u64::from(used_idx % qsize);
    Some((device.wrapping_add(4 + slot * 8), device.wrapping_add(2)))
}

pub fn used_bytes(head: u16, in_len: u32) -> [u8; 8] {
    let mut elem = [0u8; 8];
    elem[0..4].copy_from_slice(&u32::from(head).to_le_bytes());
    elem[4..8].copy_from_slice(&in_len.to_le_bytes());
    elem
}

/// Vector for `gsi` when that redirection entry is unmasked.
pub fn gsi_vector(gsi: u8) -> Option<u8> {
    let io = unsafe { &*core::ptr::addr_of!(IOAPIC) };
    let idx = 0x10usize + (gsi as usize) * 2;
    if idx >= io.regs.len() {
        return None;
    }
    let low = io.regs[idx];
    if low & (1 << 16) != 0 {
        return None;
    }
    let vec = (low & 0xff) as u8;
    if vec < 32 {
        None
    } else {
        Some(vec)
    }
}

pub fn apic_access(off: u16, is_write: bool, size: u8, val: u64) -> u64 {
    let a = unsafe { &mut *core::ptr::addr_of_mut!(APIC) };
    if !is_write {
        if off == 0x30 {
            return 0x0005_0014;
        }
        return read_reg(&a.regs, off, size);
    }
    if off == 0x30 {
        return 0;
    }
    write_reg(&mut a.regs, off, size, val);
    0
}

pub fn ioapic_access(off: u16, is_write: bool, size: u8, val: u64) -> u64 {
    let io = unsafe { &mut *core::ptr::addr_of_mut!(IOAPIC) };
    let reg = off & !0xf;
    if reg == 0 {
        if is_write {
            io.index = val as u8;
            return 0;
        }
        return u64::from(io.index);
    }
    if reg == 0x10 {
        let idx = (io.index as usize) & 63;
        if is_write {
            io.regs[idx] = val as u32;
            return 0;
        }
        return u64::from(io.regs[idx]);
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disk_notify_and_apic_stay_private() {
        reset_for_test();
        set_capacity_bytes(1024 * 512);
        let (cap, ding) = disk_access(OFF_DEVICE, false, 4, 0);
        assert_eq!(cap, 1024);
        assert!(!ding);
        let (_, ding) = disk_access(OFF_NOTIFY, true, 2, 0);
        assert!(ding);
        assert_eq!(notifies(), 1);
        assert_eq!(apic_access(0x30, false, 4, 0), 0x0005_0014);
        assert_eq!(apic_access(0x80, true, 4, 0x20), 0);
        assert_eq!(apic_access(0x80, false, 4, 0), 0x20);
        assert_eq!(ioapic_access(0x00, true, 4, 0x10), 0);
        assert_eq!(ioapic_access(0x10, true, 4, 0xAB), 0);
        assert_eq!(ioapic_access(0x10, false, 4, 0), 0xAB);
        reset_for_test();
    }

    #[test]
    fn one_in_request_names_a_sector() {
        reset_for_test();
        let mut mem = vec![0u8; 4096];
        // header at 0x100, data at 0x200, status at 0x300
        mem[0x108..0x110].copy_from_slice(&7u64.to_le_bytes());
        // desc 0
        mem[0x00..0x08].copy_from_slice(&0x100u64.to_le_bytes());
        mem[0x08..0x0C].copy_from_slice(&16u32.to_le_bytes());
        mem[0x0C..0x0E].copy_from_slice(&1u16.to_le_bytes());
        mem[0x0E..0x10].copy_from_slice(&1u16.to_le_bytes());
        // desc 1 data write
        mem[0x10..0x18].copy_from_slice(&0x200u64.to_le_bytes());
        mem[0x18..0x1C].copy_from_slice(&512u32.to_le_bytes());
        mem[0x1C..0x1E].copy_from_slice(&3u16.to_le_bytes());
        mem[0x1E..0x20].copy_from_slice(&2u16.to_le_bytes());
        // desc 2 status write
        mem[0x20..0x28].copy_from_slice(&0x300u64.to_le_bytes());
        mem[0x28..0x2C].copy_from_slice(&1u32.to_le_bytes());
        mem[0x2C..0x2E].copy_from_slice(&2u16.to_le_bytes());
        // avail idx 1, ring[0] = 0
        mem[0x402..0x404].copy_from_slice(&1u16.to_le_bytes());
        let mut read = |gpa: u64, buf: &mut [u8]| {
            let o = gpa as usize;
            if o + buf.len() > mem.len() {
                return false;
            }
            buf.copy_from_slice(&mem[o..o + buf.len()]);
            true
        };
        let req = pull_request(&mut read, 0, 0x400, 0, 128).expect("req");
        assert_eq!(req.ty, 0);
        assert_eq!(req.sector, 7);
        assert_eq!(req.nseg, 1);
        assert_eq!(req.segs[0].gpa, 0x200);
        assert_eq!(req.segs[0].len, 512);
        assert!(req.segs[0].to_guest);
        assert_eq!(req.status_gpa, 0x300);
        assert_eq!(req.head, 0);
        let (elem_gpa, idx_gpa) = used_slot(0x800, 128, 0).expect("slot");
        assert_eq!(elem_gpa, 0x804);
        assert_eq!(idx_gpa, 0x802);
        assert_eq!(used_bytes(0, 513), {
            let mut e = [0u8; 8];
            e[4] = 1;
            e[5] = 2;
            e
        });
        reset_for_test();
    }

    #[test]
    fn features_isr_and_gsi_stay_on_this_device() {
        reset_for_test();
        let (lo, _) = disk_access(0x04, false, 4, 0);
        assert_eq!(lo, 1 << 9);
        let _ = disk_access(0x00, true, 4, 1);
        let (hi, _) = disk_access(0x04, false, 4, 0);
        assert_eq!(hi, 1);
        arm_disk_irq();
        assert!(disk_irq_pending());
        let (isr, _) = disk_access(0x100, false, 1, 0);
        assert_eq!(isr, 1);
        let (isr, _) = disk_access(0x100, false, 1, 0);
        assert_eq!(isr, 0);
        assert!(gsi_vector(SEAT_DISK_GSI).is_none());
        let _ = ioapic_access(0x00, true, 4, u64::from(0x10 + SEAT_DISK_GSI * 2));
        let _ = ioapic_access(0x10, true, 4, 0x31);
        assert_eq!(gsi_vector(SEAT_DISK_GSI), Some(0x31));
        let _ = ioapic_access(0x10, true, 4, 0x31 | (1 << 16));
        assert!(gsi_vector(SEAT_DISK_GSI).is_none());
        reset_for_test();
    }
}
