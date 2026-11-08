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
const OFF_DEVICE: u16 = 0x200;
const QUEUE_MAX: u16 = 128;

struct Disk {
    regs: [u8; 0x400],
    last_avail: u16,
    capacity: u64,
    notifies: u32,
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
    capacity: 0,
    notifies: 0,
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
            capacity: 0,
            notifies: 0,
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
        let v = if off == OFF_DEVICE || off == OFF_DEVICE + 4 {
            let shift = if off == OFF_DEVICE { 0 } else { 32 };
            (d.capacity >> shift) & if size >= 8 { u64::MAX } else { 0xffff_ffff }
        } else if off == 0x12 {
            1
        } else if off == 0x18 {
            u64::from(QUEUE_MAX)
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

pub fn disk_queue() -> (u64, u64, u64, u16) {
    let d = disk();
    (
        read_reg(&d.regs, 0x20, 8),
        read_reg(&d.regs, 0x28, 8),
        read_reg(&d.regs, 0x30, 8),
        d.last_avail,
    )
}

pub fn disk_set_last_avail(v: u16) {
    disk().last_avail = v;
}

pub fn notifies() -> u32 {
    NOTIFY.load(Ordering::Acquire)
}

/// One IN request: sector, data GPA, data length, status GPA, descriptor head.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InReq {
    pub sector: u64,
    pub data_gpa: u64,
    pub data_len: u32,
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

/// Pull one virtio-blk IN (type 0) chain. `last` is the last consumed avail index.
pub fn pull_in_request(
    read: &mut impl FnMut(u64, &mut [u8]) -> bool,
    desc: u64,
    driver: u64,
    last: u16,
) -> Option<InReq> {
    let idx = read_u16(read, driver.wrapping_add(2))?;
    if idx == last {
        return None;
    }
    let ring = driver.wrapping_add(4).wrapping_add(u64::from(last % QUEUE_MAX) * 2);
    let head = read_u16(read, ring)?;
    let (hdr_gpa, hdr_len, flags, next) = read_desc(read, desc.wrapping_add(u64::from(head) * 16))?;
    if hdr_len < 16 || flags & 1 == 0 {
        return None;
    }
    let mut hdr = [0u8; 16];
    if !read(hdr_gpa, &mut hdr) || u32::from_le_bytes(hdr[0..4].try_into().ok()?) != 0 {
        return None;
    }
    let sector = u64::from_le_bytes(hdr[8..16].try_into().ok()?);
    let (data_gpa, data_len, data_flags, data_next) =
        read_desc(read, desc.wrapping_add(u64::from(next) * 16))?;
    if data_flags & 2 == 0 || data_len == 0 {
        return None;
    }
    let (status_gpa, status_len, status_flags, _) =
        read_desc(read, desc.wrapping_add(u64::from(data_next) * 16))?;
    if status_flags & 2 == 0 || status_len < 1 {
        return None;
    }
    let _ = flags;
    Some(InReq {
        sector,
        data_gpa,
        data_len,
        status_gpa,
        head,
    })
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
        let req = pull_in_request(&mut read, 0, 0x400, 0).expect("req");
        assert_eq!(req.sector, 7);
        assert_eq!(req.data_gpa, 0x200);
        assert_eq!(req.data_len, 512);
        assert_eq!(req.status_gpa, 0x300);
        assert_eq!(req.head, 0);
        reset_for_test();
    }
}
