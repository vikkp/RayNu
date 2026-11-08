//! Virtio MSI-X for ADR-021 B8 (outside Proven Core).
//!
//! Pillar: [Z]
//! Proven Core: **outside** (ADR-002). The local APIC that receives the
//! vector is `lapic_virt`, also outside the core.
//! VERIFICATION: L1 host tests.
//!
//! A virtio function still uses shared PIC IRQ 11 until the guest enables
//! MSI-X and programs a queue vector. The table lives in BAR0 at `0x800`
//! (four entries). Message address `0xFEE00000` is the virtual local APIC;
//! the low byte of message data is the vector latched into IRR. Host/CI
//! never print an iron `*-OK` marker. The firmware line
//! `boot: B8 virtio msi-x vec=` is the iron evidence, and it is not that close.

use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};

use crate::devices::lapic_virt;

/// Virtio `VIRTIO_MSI_NO_VECTOR`.
pub const NO_VECTOR: u16 = 0xFFFF;
pub const ENTRIES: usize = 4;
pub const TABLE_OFF: u16 = 0x800;
pub const PBA_OFF: u16 = 0x880;
pub const CAP_OFF: u8 = 0x84;
const ENTRY_BYTES: u16 = 16;
const TABLE_BYTES: u16 = (ENTRIES as u16) * ENTRY_BYTES;
/// xAPIC MSI window. Destination bits are ignored until a second vCPU exists.
const MSI_ADDR_PAGE: u64 = 0xFEE0_0000;
const CTRL_ENABLE: u16 = 1 << 15;
const CTRL_FMASK: u16 = 1 << 14;
const CTRL_SIZE: u16 = (ENTRIES as u16) - 1;

static NOTE_ONCE: AtomicBool = AtomicBool::new(false);
static LAST_VEC: AtomicU8 = AtomicU8::new(0);

#[derive(Clone, Copy)]
struct Entry {
    addr: u64,
    data: u32,
    masked: bool,
}

impl Entry {
    const fn empty() -> Self {
        Self {
            addr: 0,
            data: 0,
            masked: true,
        }
    }
}

/// One function's MSI-X capability, table, and pending bits.
#[derive(Clone, Copy)]
pub struct Msix {
    enable: bool,
    fmask: bool,
    config_vec: u16,
    queue_vec: [u16; 2],
    entry: [Entry; ENTRIES],
    pending: u32,
}

impl Msix {
    pub const fn empty() -> Self {
        Self {
            enable: false,
            fmask: false,
            config_vec: NO_VECTOR,
            queue_vec: [NO_VECTOR; 2],
            entry: [Entry::empty(); ENTRIES],
            pending: 0,
        }
    }

    pub fn enabled(&self) -> bool {
        self.enable
    }

    /// Message control. Table size is fixed. Enable and function mask are the guest's.
    pub fn control(&self) -> u16 {
        let mut c = CTRL_SIZE;
        if self.fmask {
            c |= CTRL_FMASK;
        }
        if self.enable {
            c |= CTRL_ENABLE;
        }
        c
    }

    pub fn set_control(&mut self, val: u16) {
        let was_mask = self.fmask;
        self.fmask = val & CTRL_FMASK != 0;
        self.enable = val & CTRL_ENABLE != 0;
        if !self.enable {
            self.pending = 0;
            return;
        }
        if was_mask && !self.fmask {
            self.flush_pending();
        }
    }

    pub fn cfg_is(off: u8) -> bool {
        (CAP_OFF..CAP_OFF + 12).contains(&off) && off & 3 == 0
    }

    /// PCI config dword for the MSI-X capability. `0` when `off` is not ours.
    pub fn cfg_dword(&self, off: u8) -> u32 {
        match off {
            CAP_OFF => u32::from(self.control()) << 16 | 0x0011,
            x if x == CAP_OFF + 4 => u32::from(TABLE_OFF),
            x if x == CAP_OFF + 8 => u32::from(PBA_OFF),
            _ => 0,
        }
    }

    /// Write the message-control field. Other capability bytes are read-only.
    /// Returns whether `off` belongs to this capability.
    pub fn write_cfg(&mut self, off: u8, size: u8, val: u32) -> bool {
        if off < CAP_OFF || off >= CAP_OFF + 12 {
            return false;
        }
        let end = off.saturating_add(size);
        if off < CAP_OFF + 4 && end > CAP_OFF + 2 {
            let mut ctrl = self.control();
            let mut i = 0u8;
            while i < size {
                let at = off + i;
                if at == CAP_OFF + 2 {
                    ctrl = (ctrl & 0xff00) | ((val >> (8 * i)) as u16 & 0xff);
                } else if at == CAP_OFF + 3 {
                    ctrl = (ctrl & 0x00ff) | (((val >> (8 * i)) as u16 & 0xff) << 8);
                }
                i += 1;
            }
            self.set_control(ctrl);
        }
        true
    }

    pub fn config_byte(&self, i: u16) -> u8 {
        (self.config_vec >> (8 * i)) as u8
    }

    pub fn queue_byte(&self, q: u16, i: u16) -> u8 {
        (self.queue_vec(q) >> (8 * i)) as u8
    }

    pub fn config_vec(&self) -> u16 {
        self.config_vec
    }

    pub fn set_config_vec(&mut self, vec: u16) {
        self.config_vec = vec;
    }

    pub fn queue_vec(&self, q: u16) -> u16 {
        self.queue_vec.get(q as usize).copied().unwrap_or(NO_VECTOR)
    }

    pub fn set_queue_vec(&mut self, q: u16, vec: u16) {
        if let Some(slot) = self.queue_vec.get_mut(q as usize) {
            *slot = vec;
        }
    }

    /// Virtio device reset. The PCI table and the enable bit stay.
    pub fn virtio_reset(&mut self) {
        self.config_vec = NO_VECTOR;
        self.queue_vec = [NO_VECTOR; 2];
        self.pending = 0;
    }

    pub fn mmio_hit(off: u16) -> bool {
        (TABLE_OFF..TABLE_OFF + TABLE_BYTES).contains(&off) || (PBA_OFF..PBA_OFF + 8).contains(&off)
    }

    pub fn mmio_read(&self, off: u16, size: u8) -> u64 {
        let n = match size {
            1 => 1u16,
            2 => 2,
            4 => 4,
            8 => 8,
            _ => 4,
        };
        let mut val = 0u64;
        let mut i = 0u16;
        while i < n {
            val |= u64::from(self.mmio_byte(off.wrapping_add(i))) << (8 * i);
            i += 1;
        }
        val
    }

    pub fn mmio_write(&mut self, off: u16, size: u8, val: u64) {
        let n = match size {
            1 => 1u16,
            2 => 2,
            4 => 4,
            8 => 8,
            _ => 4,
        };
        let mut i = 0u16;
        while i < n {
            self.mmio_write_byte(off.wrapping_add(i), (val >> (8 * i)) as u8);
            i += 1;
        }
    }

    /// `true` when MSI-X owns the interrupt, including a masked or unassigned
    /// vector. The caller keeps PIC IRQ 11 only when this returns `false`.
    pub fn deliver_queue(&mut self, q: u16) -> bool {
        if !self.enable {
            return false;
        }
        if let Some(idx) = self.index_of(self.queue_vec(q)) {
            self.try_fire(idx);
        }
        true
    }

    /// Config-change vector (`msix_config`). Same ownership rule as a queue.
    pub fn deliver_config(&mut self) -> bool {
        if !self.enable {
            return false;
        }
        if let Some(idx) = self.index_of(self.config_vec) {
            self.try_fire(idx);
        }
        true
    }

    fn index_of(&self, vec: u16) -> Option<usize> {
        if vec == NO_VECTOR || vec as usize >= ENTRIES {
            None
        } else {
            Some(vec as usize)
        }
    }

    fn try_fire(&mut self, idx: usize) {
        if self.fmask || self.entry[idx].masked {
            self.pending |= 1 << idx;
            return;
        }
        self.pending &= !(1 << idx);
        self.fire(idx);
    }

    fn fire(&self, idx: usize) {
        let e = &self.entry[idx];
        let vec = (e.data & 0xFF) as u8;
        if e.addr & 0xFFFF_F000 != MSI_ADDR_PAGE || vec < 32 {
            return;
        }
        lapic_virt::latch_irr(vec);
        LAST_VEC.store(vec, Ordering::Release);
        if NOTE_ONCE.swap(true, Ordering::AcqRel) {
            return;
        }
        crate::boot::serial::write_str_nowait("boot: B8 virtio msi-x vec=");
        write_hex(vec);
        crate::boot::serial::write_byte_nowait(b'\n');
    }

    fn flush_pending(&mut self) {
        let bits = self.pending;
        self.pending = 0;
        for i in 0..ENTRIES {
            if bits & (1 << i) == 0 {
                continue;
            }
            if self.entry[i].masked {
                self.pending |= 1 << i;
            } else {
                self.fire(i);
            }
        }
    }

    fn mmio_byte(&self, off: u16) -> u8 {
        if (PBA_OFF..PBA_OFF + 4).contains(&off) {
            let sh = (off - PBA_OFF) * 8;
            return ((self.pending >> sh) & 0xFF) as u8;
        }
        let Some((idx, byte)) = entry_pos(off) else {
            return 0;
        };
        let e = &self.entry[idx];
        match byte {
            0..=7 => (e.addr >> (8 * byte)) as u8,
            8..=11 => (e.data >> (8 * (byte - 8))) as u8,
            12 => u8::from(e.masked),
            _ => 0,
        }
    }

    fn mmio_write_byte(&mut self, off: u16, val: u8) {
        if (PBA_OFF..PBA_OFF + 8).contains(&off) {
            return;
        }
        let Some((idx, byte)) = entry_pos(off) else {
            return;
        };
        let e = &mut self.entry[idx];
        match byte {
            0..=7 => {
                let sh = 8 * byte;
                e.addr = (e.addr & !(0xffu64 << sh)) | (u64::from(val) << sh);
            }
            8..=11 => {
                let sh = 8 * (byte - 8);
                e.data = (e.data & !(0xffu32 << sh)) | (u32::from(val) << sh);
            }
            12 => {
                let was = e.masked;
                e.masked = val & 1 != 0;
                if was && !e.masked && self.pending & (1 << idx) != 0 && self.enable {
                    self.try_fire(idx);
                }
            }
            _ => {}
        }
    }
}

fn entry_pos(off: u16) -> Option<(usize, u16)> {
    if off < TABLE_OFF || off >= TABLE_OFF + TABLE_BYTES {
        return None;
    }
    let rel = off - TABLE_OFF;
    Some(((rel / ENTRY_BYTES) as usize, rel % ENTRY_BYTES))
}

fn write_hex(b: u8) {
    const H: &[u8; 16] = b"0123456789abcdef";
    crate::boot::serial::write_byte_nowait(H[(b >> 4) as usize]);
    crate::boot::serial::write_byte_nowait(H[(b & 0xf) as usize]);
}

/// Last vector latched by [`Msix::fire`]. `0` before the first delivery.
pub fn last_vector() -> u8 {
    LAST_VEC.load(Ordering::Acquire)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn armed_entry(vec: u8) -> Msix {
        let mut m = Msix::empty();
        m.mmio_write(TABLE_OFF, 4, MSI_ADDR_PAGE);
        m.mmio_write(TABLE_OFF + 8, 4, u64::from(vec));
        m.mmio_write(TABLE_OFF + 12, 4, 0);
        m.set_queue_vec(0, 0);
        m.set_control(CTRL_ENABLE | CTRL_SIZE);
        m
    }

    /// One function so the shared virtual LAPIC is not reset by a sibling test.
    #[test]
    fn b8_msix_queue_reaches_the_lapic() {
        lapic_virt::reset();
        let mut bare = Msix::empty();
        assert!(!bare.enabled());
        assert_eq!(bare.queue_vec(0), NO_VECTOR);
        assert!(!bare.deliver_queue(0), "MSI-X off stays on INTx");

        bare.set_control(CTRL_ENABLE);
        assert!(bare.deliver_queue(0), "enabled with no vector owns the IRQ");
        assert!(!lapic_virt::has_irr_vec(0x51));

        let mut m = armed_entry(0x51);
        assert!(m.deliver_queue(0));
        assert!(lapic_virt::has_irr_vec(0x51));
        assert_eq!(last_vector(), 0x51);
        assert_eq!(m.cfg_dword(CAP_OFF) & 0xFF, 0x11);
        assert_eq!(m.cfg_dword(CAP_OFF + 4), u32::from(TABLE_OFF));

        lapic_virt::reset();
        let mut held = armed_entry(0x41);
        held.mmio_write(TABLE_OFF + 12, 4, 1);
        assert!(held.deliver_queue(0));
        assert!(
            !lapic_virt::has_irr_vec(0x41),
            "masked vector waits in the PBA"
        );
        held.mmio_write(TABLE_OFF + 12, 4, 0);
        assert!(lapic_virt::has_irr_vec(0x41));

        held.virtio_reset();
        assert_eq!(held.queue_vec(0), NO_VECTOR);
        assert!(held.enabled());
        assert_eq!(held.mmio_read(TABLE_OFF + 8, 1), 0x41);

        lapic_virt::reset();
        let mut refuse = Msix::empty();
        refuse.mmio_write(TABLE_OFF, 4, MSI_ADDR_PAGE);
        refuse.mmio_write(TABLE_OFF + 8, 4, 0x33);
        refuse.mmio_write(TABLE_OFF + 12, 4, 0);
        refuse.set_queue_vec(0, 7);
        refuse.set_control(CTRL_ENABLE | CTRL_SIZE);
        assert!(
            refuse.deliver_queue(0),
            "an index past the table owns the IRQ"
        );
        assert!(!lapic_virt::has_irr_vec(0x33));
        refuse.set_queue_vec(0, 0);
        assert!(refuse.deliver_queue(0));
        assert!(lapic_virt::has_irr_vec(0x33));
    }
}
