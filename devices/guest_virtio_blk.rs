//! Guest-visible PCI virtio-blk for the private guest-UEFI VMCS.
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** (ADR-002 / ADR-014)
//! VERIFICATION: L1 (runtime + host tests; QEMU is the enum gate)
//!
//! Empty virtio 1.0 block function at `00:00.0` (Red Hat `1AF4:1042`).
//! Nested VT-x: this OVMF PEI only `inw` Device ID of `00:00.0`. The probe
//! slot is virtio so that read can enum it. Header Type is multifunction so a
//! walk finds IDE `00:00.1`. PIIX `00:01.1` is the same CD. Boot order is CD
//! then disk (fw_cfg `bootorder`).
//! This is not the M4.3 virtio-mmio probe, not a completed firmware CD boot,
//! not an installer.

use crate::devices::guest_platform::{
    boot_order_cd_then_disk, pci_bdf, pci_cfg_offset, PCI_HEADER_MULTIFUNCTION,
};
use core::sync::atomic::{AtomicBool, Ordering};

/// QEMU / serial marker when guest-UEFI sees virtio-blk + CD→disk order.
pub const M7_E5_OVMF_VIRTIO_OK_MARKER: &str = "RAYNU-V-M7-E5-OVMF-VIRTIO-OK";

pub const GUEST_VIRTIO_PCI_BUS: u8 = 0;
pub const GUEST_VIRTIO_PCI_DEV: u8 = 0;
pub const GUEST_VIRTIO_PCI_FN: u8 = 0;
/// Virtio 1.0 PCI vendor (Red Hat).
pub const GUEST_VIRTIO_PCI_VENDOR: u16 = 0x1AF4;
/// Virtio 1.0 block device id.
pub const GUEST_VIRTIO_PCI_DEVICE: u16 = 0x1042;
/// Virtio blk subsystem device (legacy id 2).
pub const GUEST_VIRTIO_PCI_SUBSYS: u16 = 0x0002;

struct VirtioPci {
    visible: bool,
    pci_enum: bool,
    pci_addr: u32,
    pci_cmd: u16,
    bar0: u32,
}

impl VirtioPci {
    const fn empty() -> Self {
        Self {
            visible: false,
            pci_enum: false,
            pci_addr: 0,
            pci_cmd: 0,
            bar0: 0,
        }
    }
}

// JUSTIFICATION: one guest-UEFI virtio-blk; firmware is single-threaded after EBS.
struct GuestVirtio(core::cell::UnsafeCell<VirtioPci>);

// SAFETY: exclusive access is enforced by `VIRTIO_LOCK`.
// KANI-TARGET: guest-UEFI virtio-blk mutex (outside Proven Core).
unsafe impl Sync for GuestVirtio {}

static VIRTIO: GuestVirtio = GuestVirtio(core::cell::UnsafeCell::new(VirtioPci::empty()));
static VIRTIO_LOCK: AtomicBool = AtomicBool::new(false);
static VISIBLE: AtomicBool = AtomicBool::new(false);
static PCI_ENUM: AtomicBool = AtomicBool::new(false);
static MARKER: AtomicBool = AtomicBool::new(false);

fn with_virtio<R>(f: impl FnOnce(&mut VirtioPci) -> R) -> R {
    while VIRTIO_LOCK.swap(true, Ordering::Acquire) {
        core::hint::spin_loop();
    }
    // SAFETY: lock held; exclusive mutable access.
    // KANI-TARGET: guest-UEFI virtio-blk mutex (outside Proven Core).
    let out = unsafe { f(&mut *VIRTIO.0.get()) };
    VIRTIO_LOCK.store(false, Ordering::Release);
    out
}

pub fn pci_addr_selects_virtio(addr: u32) -> bool {
    if (addr & 0x8000_0000) == 0 {
        return false;
    }
    let (bus, dev, fun, _) = pci_bdf(addr);
    bus == GUEST_VIRTIO_PCI_BUS && dev == GUEST_VIRTIO_PCI_DEV && fun == GUEST_VIRTIO_PCI_FN
}

/// PCI config address for the guest virtio-blk function (`00:00.0`).
pub fn pci_config_addr() -> u32 {
    0x8000_0000
        | (u32::from(GUEST_VIRTIO_PCI_BUS) << 16)
        | (u32::from(GUEST_VIRTIO_PCI_DEV) << 11)
        | (u32::from(GUEST_VIRTIO_PCI_FN) << 8)
}

/// Honest evidence: empty virtio-blk is live, firmware enumerated it, and
/// boot order is CD then disk. Not a completed install. Not ISO-INSTALL-OK.
pub fn virtio_disk_evidence(visible: bool, pci_enum: bool, boot_cd_disk: bool) -> bool {
    visible && pci_enum && boot_cd_disk
}

pub fn is_visible() -> bool {
    VISIBLE.load(Ordering::Acquire)
}

pub fn pci_enumerated() -> bool {
    PCI_ENUM.load(Ordering::Acquire)
}

pub fn marker_printed() -> bool {
    MARKER.load(Ordering::Acquire)
}

pub fn reset() {
    with_virtio(|v| *v = VirtioPci::empty());
    VISIBLE.store(false, Ordering::Release);
    PCI_ENUM.store(false, Ordering::Release);
    MARKER.store(false, Ordering::Release);
}

/// Mark the empty virtio-blk function live on the private guest-UEFI VMCS.
pub fn present() -> bool {
    with_virtio(|v| {
        *v = VirtioPci::empty();
        v.visible = true;
        v.bar0 = 0xFE00_0000;
    });
    VISIBLE.store(true, Ordering::Release);
    PCI_ENUM.store(false, Ordering::Release);
    MARKER.store(false, Ordering::Release);
    true
}

pub fn pci_write_addr(addr: u32) {
    with_virtio(|v| v.pci_addr = addr);
}

pub fn pci_read_addr() -> u32 {
    with_virtio(|v| v.pci_addr)
}

fn config_dword(v: &VirtioPci, off: u8) -> u32 {
    match off {
        0x00 => u32::from(GUEST_VIRTIO_PCI_VENDOR) | (u32::from(GUEST_VIRTIO_PCI_DEVICE) << 16),
        0x04 => u32::from(v.pci_cmd) | 0x0010_0000, // CapList
        0x08 => 0x0100_0001,                        // SCSI mass-storage, rev 1
        0x0C => PCI_HEADER_MULTIFUNCTION,
        0x10 => v.bar0,
        0x2C => u32::from(GUEST_VIRTIO_PCI_VENDOR) | (u32::from(GUEST_VIRTIO_PCI_SUBSYS) << 16),
        0x34 => 0x0000_0040, // cap pointer
        0x3C => 0x0000_0100,
        // Vendor cap: virtio-pci common cfg (type 1) — enough for enum, not queues.
        0x40 => 0x0001_0010,
        _ => 0,
    }
}

pub fn pci_read_data(port: u16, size: u8) -> u32 {
    with_virtio(|v| {
        if !v.visible {
            return 0xFFFF_FFFF;
        }
        let addr = v.pci_addr;
        if !pci_addr_selects_virtio(addr) {
            return 0xFFFF_FFFF;
        }
        let off = pci_cfg_offset(addr, port);
        let aligned = off & 0xFC;
        if aligned == 0 {
            v.pci_enum = true;
            PCI_ENUM.store(true, Ordering::Release);
        }
        let dword = config_dword(v, aligned);
        let shift = (off & 3) * 8;
        let shifted = dword >> shift;
        match size {
            1 => shifted & 0xff,
            2 => shifted & 0xffff,
            _ => shifted,
        }
    })
}

pub fn pci_write_data(port: u16, size: u8, val: u32) {
    with_virtio(|v| {
        if !v.visible || !pci_addr_selects_virtio(v.pci_addr) {
            return;
        }
        let off = pci_cfg_offset(v.pci_addr, port);
        if off == 0x04 {
            v.pci_cmd = (val as u16) | 0x0002;
        } else if off == 0x10 {
            let mask = if size >= 4 { 0xFFFF_FFF0 } else { 0xFFFF };
            v.bar0 = val & mask;
        }
    });
}

pub fn take_marker() -> bool {
    if !virtio_disk_evidence(is_visible(), pci_enumerated(), boot_order_cd_then_disk()) {
        return false;
    }
    !MARKER.swap(true, Ordering::AcqRel)
}

#[cfg(test)]
#[path = "guest_virtio_blk_test.rs"]
mod guest_virtio_blk_test;
