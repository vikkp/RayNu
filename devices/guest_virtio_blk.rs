//! Guest-visible PCI virtio-blk for the private guest-UEFI VMCS.
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** (ADR-002 / ADR-014)
//! VERIFICATION: L1 (runtime + host tests; QEMU is the enum gate)
//!
//! Empty virtio 1.0 block function at `00:02.0` (Red Hat `1AF4:1042`).
//! Nested VT-x: this OVMF PEI only `inw` Device ID of `00:00.0` into
//! `HostBridgeDevId`. Iron `c1476d3` served virtio `0x1042` there, so PEI
//! skipped the stock QEMU map (`PlatformMemMapInitialization` IoMemory HOB
//! at `0xA0000–1MiB`) and left GCD as a merged `[0, LowMemory)` SystemMemory
//! range covering VGA UC. PEI therefore sees i440FX `8086:1237` at `00:00.0`.
//! Iron `2cbf9e8` `retcmp=` is CpuDxe `AcpiTimerLibConstructor`:
//! `cmp ax, 0x1237` / `0x29C0` / `0x0D57` then store PIIX4 `0xB000` /
//! ICH9 `0x0600`; default `ASSERT(FALSE)`. Latch used to rewrite `00:00.0`
//! to virtio `0x1042`, so DXE `PciRead16(OVMF_HOSTBRIDGE_DID)` missed the
//! switch. `00:00.0` stays i440FX; latch reveals virtio at `00:02.0`.
//! Header Type on slot 0 stays multifunction so a walk finds IDE fn1.
//! PIIX `00:01.1` is the same CD. Boot order is CD then disk (fw_cfg
//! `bootorder`).
//! Lab stub: vendor cap `0x0001_0010` (enum only, not queues). Product ISO
//! window: virtio-pci caps type 1/2/3/4 + trap-and-emulate BAR MMIO + split
//! virtqueue IN/OUT/FLUSH. Not the M4.3 virtio-mmio probe. Not ISO-INSTALL-OK.

use crate::devices::guest_platform::{
    boot_order_cd_then_disk, pci_bdf, pci_cfg_offset, HOST_BRIDGE_DEVICE, HOST_BRIDGE_VENDOR,
    PCI_HEADER_MULTIFUNCTION,
};
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// QEMU / serial marker when guest-UEFI sees virtio-blk + CD→disk order.
pub const M7_E5_OVMF_VIRTIO_OK_MARKER: &str = "RAYNU-V-M7-E5-OVMF-VIRTIO-OK";

pub const GUEST_VIRTIO_PCI_BUS: u8 = 0;
/// QEMU-compatible slot. Not `00:00.0` — that DID is the host bridge
/// (`OVMF_HOSTBRIDGE_DID`) for `AcpiTimerLibConstructor`.
pub const GUEST_VIRTIO_PCI_DEV: u8 = 2;
pub const GUEST_VIRTIO_PCI_FN: u8 = 0;
/// Slot 0 is always the i440FX host-bridge DID PEI/DXE probe.
pub const GUEST_SLOT0_PCI_DEV: u8 = 0;
pub const GUEST_SLOT0_PCI_FN: u8 = 0;
/// Virtio 1.0 PCI vendor (Red Hat).
pub const GUEST_VIRTIO_PCI_VENDOR: u16 = 0x1AF4;
/// Virtio 1.0 block device id.
pub const GUEST_VIRTIO_PCI_DEVICE: u16 = 0x1042;
/// Virtio blk subsystem device (legacy id 2).
pub const GUEST_VIRTIO_PCI_SUBSYS: u16 = 0x0002;

/// Default BAR0 (4 KiB MMIO) when the product ISO window arms queues.
pub const GUEST_VIRTIO_BAR0_DEFAULT: u32 = 0xFE00_0000;
pub const GUEST_VIRTIO_BAR0_SIZE: u32 = 0x1000;
/// PCI BAR size probe result for a 4 KiB memory BAR.
pub const GUEST_VIRTIO_BAR0_SIZE_MASK: u32 = 0xFFFF_F000;

pub const VIRTIO_PCI_CAP_VNDR: u8 = 0x09;
pub const VIRTIO_PCI_CAP_COMMON: u8 = 1;
pub const VIRTIO_PCI_CAP_NOTIFY: u8 = 2;
pub const VIRTIO_PCI_CAP_ISR: u8 = 3;
pub const VIRTIO_PCI_CAP_DEVICE: u8 = 4;

pub const VIRTIO_F_VERSION_1: u64 = 1 << 32;
pub const VIRTIO_BLK_F_FLUSH: u64 = 1 << 9;
pub const VIRTIO_BLK_DEVICE_FEATURES: u64 = VIRTIO_F_VERSION_1 | VIRTIO_BLK_F_FLUSH;

pub const VIRTIO_BLK_T_IN: u32 = 0;
pub const VIRTIO_BLK_T_OUT: u32 = 1;
pub const VIRTIO_BLK_T_FLUSH: u32 = 4;
pub const VIRTIO_BLK_S_OK: u8 = 0;
pub const VIRTIO_BLK_S_IOERR: u8 = 1;

const OFF_COMMON: u16 = 0x00;
const OFF_ISR: u16 = 0x100;
const OFF_DEVICE: u16 = 0x200;
const OFF_NOTIFY: u16 = 0x300;
const QUEUE_MAX: u16 = 128;
const SECTOR: usize = 512;
const VRING_DESC_F_NEXT: u16 = 1;
const VRING_DESC_F_WRITE: u16 = 2;

struct VirtioPci {
    visible: bool,
    pci_enum: bool,
    pci_addr: u32,
    pci_cmd: u16,
    bar0: u32,
    bar_sizing: bool,
    queues_armed: bool,
    feat_sel: u32,
    drv_feat_sel: u32,
    drv_feat: u64,
    status: u8,
    queue_sel: u16,
    queue_size: u16,
    queue_enable: u16,
    queue_desc: u64,
    queue_driver: u64,
    queue_device: u64,
    last_avail: u16,
    isr: u8,
    notify_pending: bool,
}

impl VirtioPci {
    const fn empty() -> Self {
        Self {
            visible: false,
            pci_enum: false,
            pci_addr: 0,
            pci_cmd: 0,
            bar0: 0,
            bar_sizing: false,
            queues_armed: false,
            feat_sel: 0,
            drv_feat_sel: 0,
            drv_feat: 0,
            status: 0,
            queue_sel: 0,
            queue_size: QUEUE_MAX,
            queue_enable: 0,
            queue_desc: 0,
            queue_driver: 0,
            queue_device: 0,
            last_avail: 0,
            isr: 0,
            notify_pending: false,
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
static QUEUES: AtomicBool = AtomicBool::new(false);
static DISK_HPA: AtomicU64 = AtomicU64::new(0);
static DISK_LEN: AtomicU64 = AtomicU64::new(0);
static BYTES_WRITTEN: AtomicU64 = AtomicU64::new(0);
static ISO_OK: AtomicBool = AtomicBool::new(false);
/// PEI `PciRead16(00:00.0 DID)` must be i440FX so `HostBridgeDevId==0x1237`.
/// Sticky-false after [`latch_dxe_virtio_did`] (virtio appears at `00:02.0`).
static PEI_I440FX_DID: AtomicBool = AtomicBool::new(true);

/// True until DXE / MiscInitialization has selected a BDF other than `00:00.0`.
///
/// INVARIANTS:
/// - PEI `InitializePlatform` stores `00:00.0` DID as `HostBridgeDevId`
/// - `00:00.0` VID/DID stays i440FX `8086:1237` after latch (AcpiTimerLib)
/// - After [`latch_dxe_virtio_did`], virtio `1AF4:1042` is at `00:02.0`
pub fn pei_host_bridge_did() -> bool {
    PEI_I440FX_DID.load(Ordering::Acquire)
}

/// DXE / MiscInitialization: first CF8 of a BDF other than `00:00.0`.
///
/// Returns true when this call performed the latch. Reveals virtio at
/// `00:02.0`. Does **not** rewrite `00:00.0` DID.
pub fn latch_dxe_virtio_did() -> bool {
    PEI_I440FX_DID.swap(false, Ordering::AcqRel)
}

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

pub fn pci_addr_selects_slot0(addr: u32) -> bool {
    if (addr & 0x8000_0000) == 0 {
        return false;
    }
    let (bus, dev, fun, _) = pci_bdf(addr);
    bus == GUEST_VIRTIO_PCI_BUS && dev == GUEST_SLOT0_PCI_DEV && fun == GUEST_SLOT0_PCI_FN
}

pub fn pci_addr_selects_virtio(addr: u32) -> bool {
    if (addr & 0x8000_0000) == 0 {
        return false;
    }
    let (bus, dev, fun, _) = pci_bdf(addr);
    bus == GUEST_VIRTIO_PCI_BUS && dev == GUEST_VIRTIO_PCI_DEV && fun == GUEST_VIRTIO_PCI_FN
}

/// Slot 0 host-bridge DID or latched virtio `00:02.0`.
pub fn pci_addr_selects_owned(addr: u32) -> bool {
    pci_addr_selects_slot0(addr) || pci_addr_selects_virtio(addr)
}

/// PCI config address for the guest virtio-blk function (`00:02.0`).
pub fn pci_config_addr() -> u32 {
    0x8000_0000
        | (u32::from(GUEST_VIRTIO_PCI_BUS) << 16)
        | (u32::from(GUEST_VIRTIO_PCI_DEV) << 11)
        | (u32::from(GUEST_VIRTIO_PCI_FN) << 8)
}

/// PCI config address for slot 0 (`00:00.0`, i440FX host-bridge DID).
pub fn pci_config_addr_slot0() -> u32 {
    0x8000_0000
        | (u32::from(GUEST_VIRTIO_PCI_BUS) << 16)
        | (u32::from(GUEST_SLOT0_PCI_DEV) << 11)
        | (u32::from(GUEST_SLOT0_PCI_FN) << 8)
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

/// True when product ISO armed virtio-pci queues (not the lab enum stub).
pub fn queues_armed() -> bool {
    QUEUES.load(Ordering::Acquire)
}

pub fn reset() {
    with_virtio(|v| *v = VirtioPci::empty());
    VISIBLE.store(false, Ordering::Release);
    PCI_ENUM.store(false, Ordering::Release);
    MARKER.store(false, Ordering::Release);
    QUEUES.store(false, Ordering::Release);
    DISK_HPA.store(0, Ordering::Release);
    DISK_LEN.store(0, Ordering::Release);
    BYTES_WRITTEN.store(0, Ordering::Release);
    ISO_OK.store(false, Ordering::Release);
    PEI_I440FX_DID.store(true, Ordering::Release);
}

/// Mark the empty virtio-blk function live on the private guest-UEFI VMCS.
///
/// Queues and modern virtio-pci caps arm only when the product ISO window
/// is already live. Lab 72 KiB stub stays enum-only (`0x0001_0010`).
pub fn present() -> bool {
    let queues = crate::devices::ide_cdrom::product_iso_window_armed();
    with_virtio(|v| {
        *v = VirtioPci::empty();
        v.visible = true;
        v.bar0 = GUEST_VIRTIO_BAR0_DEFAULT;
        v.queues_armed = queues;
        v.queue_size = QUEUE_MAX;
    });
    VISIBLE.store(true, Ordering::Release);
    PCI_ENUM.store(false, Ordering::Release);
    MARKER.store(false, Ordering::Release);
    QUEUES.store(queues, Ordering::Release);
    PEI_I440FX_DID.store(true, Ordering::Release);
    true
}

/// Host-owned install disk for the guest-UEFI virtio-pci backend.
///
/// SAFETY: `hpa` is writable for `bytes` (multiple of 512) until reset.
pub unsafe fn attach_disk(hpa: u64, bytes: usize) -> bool {
    if bytes == 0 || bytes % SECTOR != 0 || hpa == 0 {
        return false;
    }
    core::ptr::write_bytes(hpa as *mut u8, 0, bytes);
    DISK_HPA.store(hpa, Ordering::Release);
    DISK_LEN.store(bytes as u64, Ordering::Release);
    BYTES_WRITTEN.store(0, Ordering::Release);
    true
}

pub fn disk_bytes() -> u64 {
    DISK_LEN.load(Ordering::Acquire)
}

pub fn disk_bytes_written() -> u64 {
    BYTES_WRITTEN.load(Ordering::Acquire)
}

pub fn pci_write_addr(addr: u32) {
    with_virtio(|v| v.pci_addr = addr);
}

pub fn pci_read_addr() -> u32 {
    with_virtio(|v| v.pci_addr)
}

fn slot0_dword(off: u8) -> u32 {
    match off {
        0x00 => u32::from(HOST_BRIDGE_VENDOR) | (u32::from(HOST_BRIDGE_DEVICE) << 16),
        0x04 => 0x0000_0006,
        0x08 => 0x0600_0000, // host bridge (not virtio SCSI class)
        0x0C => PCI_HEADER_MULTIFUNCTION,
        _ => 0,
    }
}

fn bar0_read(v: &VirtioPci) -> u32 {
    if v.queues_armed && v.bar_sizing {
        GUEST_VIRTIO_BAR0_SIZE_MASK
    } else {
        v.bar0
    }
}

fn virtio_dword(v: &VirtioPci, off: u8) -> u32 {
    match off {
        0x00 => u32::from(GUEST_VIRTIO_PCI_VENDOR) | (u32::from(GUEST_VIRTIO_PCI_DEVICE) << 16),
        0x04 => u32::from(v.pci_cmd) | 0x0010_0000, // CapList
        0x08 => 0x0100_0001,                        // SCSI mass-storage, rev 1
        0x0C => PCI_HEADER_MULTIFUNCTION,
        0x10 => bar0_read(v),
        0x2C => u32::from(GUEST_VIRTIO_PCI_VENDOR) | (u32::from(GUEST_VIRTIO_PCI_SUBSYS) << 16),
        0x34 => 0x0000_0040, // cap pointer
        0x3C => 0x0000_0100,
        off if v.queues_armed => modern_cap_dword(off),
        // Vendor cap: virtio-pci common cfg (type 1) — enough for enum, not queues.
        0x40 => 0x0001_0010,
        _ => 0,
    }
}

/// Virtio-pci vendor caps (ID `0x09`) when the product ISO window is armed.
fn modern_cap_dword(off: u8) -> u32 {
    match off {
        0x40 => 0x0110_5009, // vndr=09 next=50 len=16 type=1
        0x44 => 0x0000_0000, // bar 0
        0x48 => u32::from(OFF_COMMON),
        0x4C => 0x0000_0038,
        0x50 => 0x0214_6409, // next=64 len=20 type=2
        0x54 => 0x0000_0000,
        0x58 => u32::from(OFF_NOTIFY),
        0x5C => 0x0000_0004,
        0x60 => 0x0000_0000, // notify_off_multiplier
        0x64 => 0x0310_7409, // next=74 len=16 type=3
        0x68 => 0x0000_0000,
        0x6C => u32::from(OFF_ISR),
        0x70 => 0x0000_0004,
        0x74 => 0x0410_0009, // next=00 len=16 type=4
        0x78 => 0x0000_0000,
        0x7C => u32::from(OFF_DEVICE),
        0x80 => 0x0000_0008,
        _ => 0,
    }
}

fn shift_cfg(dword: u32, off: u8, size: u8) -> u32 {
    let shift = (off & 3) * 8;
    let shifted = dword >> shift;
    match size {
        1 => shifted & 0xff,
        2 => shifted & 0xffff,
        _ => shifted,
    }
}

pub fn pci_read_data(port: u16, size: u8) -> u32 {
    with_virtio(|v| {
        if !v.visible {
            return 0xFFFF_FFFF;
        }
        let addr = v.pci_addr;
        let off = pci_cfg_offset(addr, port);
        let aligned = off & 0xFC;
        if pci_addr_selects_slot0(addr) {
            return shift_cfg(slot0_dword(aligned), off, size);
        }
        if !pci_addr_selects_virtio(addr) {
            return 0xFFFF_FFFF;
        }
        if pei_host_bridge_did() {
            return 0xFFFF_FFFF;
        }
        if aligned == 0 {
            v.pci_enum = true;
            PCI_ENUM.store(true, Ordering::Release);
        }
        shift_cfg(virtio_dword(v, aligned), off, size)
    })
}

pub fn pci_write_data(port: u16, size: u8, val: u32) {
    with_virtio(|v| {
        if !v.visible || !pci_addr_selects_virtio(v.pci_addr) || pei_host_bridge_did() {
            return;
        }
        let off = pci_cfg_offset(v.pci_addr, port);
        if off == 0x04 {
            v.pci_cmd = (val as u16) | 0x0002;
        } else if off == 0x10 {
            if v.queues_armed && val == 0xFFFF_FFFF {
                v.bar_sizing = true;
            } else {
                v.bar_sizing = false;
                let mask = if size >= 4 { 0xFFFF_F000 } else { 0xFFFF };
                let next = val & mask;
                v.bar0 = if next == 0 { GUEST_VIRTIO_BAR0_DEFAULT } else { next };
            }
        }
    });
}

pub fn take_marker() -> bool {
    if !virtio_disk_evidence(is_visible(), pci_enumerated(), boot_order_cd_then_disk()) {
        return false;
    }
    !MARKER.swap(true, Ordering::AcqRel)
}

fn mmio_bar_base_locked(v: &mut VirtioPci) -> u64 {
    if !v.queues_armed {
        return 0;
    }
    let b = v.bar0 & GUEST_VIRTIO_BAR0_SIZE_MASK;
    if v.bar_sizing || b == 0 || b == GUEST_VIRTIO_BAR0_SIZE_MASK {
        u64::from(GUEST_VIRTIO_BAR0_DEFAULT)
    } else {
        u64::from(b)
    }
}

/// Programmed (or default) BAR0 when queues are armed.
pub fn mmio_bar_base() -> u64 {
    with_virtio(mmio_bar_base_locked)
}

/// GPA in the 4 KiB virtio BAR. False for the lab enum stub.
pub fn is_virtio_bar_gpa(gpa: u64) -> bool {
    if !queues_armed() {
        return false;
    }
    let bar = mmio_bar_base();
    bar != 0 && gpa >= bar && gpa < bar + u64::from(GUEST_VIRTIO_BAR0_SIZE)
}

/// 2 MiB page containing the virtio BAR — must not be an EPT zero sink.
pub fn is_virtio_bar_2m_gpa(gpa: u64) -> bool {
    if !queues_armed() {
        return false;
    }
    let bar = mmio_bar_base();
    bar != 0 && (gpa & !0x1F_FFFF) == (bar & !0x1F_FFFF)
}

fn features_for_select(sel: u32) -> u32 {
    if sel == 0 {
        VIRTIO_BLK_DEVICE_FEATURES as u32
    } else if sel == 1 {
        (VIRTIO_BLK_DEVICE_FEATURES >> 32) as u32
    } else {
        0
    }
}

fn capacity_sectors() -> u64 {
    DISK_LEN.load(Ordering::Acquire) / SECTOR as u64
}

/// Read virtio-pci BAR MMIO (common / ISR / device / notify).
pub fn mmio_read(off: u16, size: u8) -> u64 {
    let val = with_virtio(|v| mmio_read_locked(v, off, size));
    if off == OFF_ISR {
        crate::devices::guest_irq::lower_virtio();
    }
    val
}

fn mmio_read_locked(v: &mut VirtioPci, off: u16, size: u8) -> u64 {
    if !v.queues_armed {
        return 0;
    }
    let val = match off {
        0x00 => v.feat_sel as u64,
        0x04 => features_for_select(v.feat_sel) as u64,
        0x08 => v.drv_feat_sel as u64,
        0x0C => {
            if v.drv_feat_sel == 0 {
                v.drv_feat as u32 as u64
            } else {
                (v.drv_feat >> 32) as u64
            }
        }
        0x10 => 0xFFFF, // msix_config = no MSI-X
        0x12 => 1,      // num_queues
        0x14 => u64::from(v.status),
        0x15 => 0,
        0x16 => u64::from(v.queue_sel),
        0x18 => {
            if v.queue_sel == 0 {
                u64::from(v.queue_size)
            } else {
                0
            }
        }
        0x1A => 0xFFFF,
        0x1C => u64::from(v.queue_enable),
        0x1E => 0,
        0x20 => v.queue_desc,
        0x24 => v.queue_desc >> 32,
        0x28 => v.queue_driver,
        0x2C => v.queue_driver >> 32,
        0x30 => v.queue_device,
        0x34 => v.queue_device >> 32,
        x if x == OFF_ISR => {
            let isr = v.isr;
            v.isr = 0;
            u64::from(isr)
        }
        x if x == OFF_DEVICE => capacity_sectors(),
        x if x == OFF_DEVICE + 4 => capacity_sectors() >> 32,
        x if x == OFF_NOTIFY => 0,
        _ => 0,
    };
    match size {
        1 => val & 0xff,
        2 => val & 0xffff,
        4 => val & 0xffff_ffff,
        _ => val,
    }
}

/// Write virtio-pci BAR MMIO. Notify sets a pending bit; call [`drain_queue`].
pub fn mmio_write(off: u16, _size: u8, val: u64) {
    with_virtio(|v| {
        if !v.queues_armed {
            return;
        }
        match off {
            0x00 => v.feat_sel = val as u32,
            0x08 => v.drv_feat_sel = val as u32,
            0x0C => {
                if v.drv_feat_sel == 0 {
                    v.drv_feat = (v.drv_feat & !0xFFFF_FFFF) | (val & 0xFFFF_FFFF);
                } else {
                    v.drv_feat = (v.drv_feat & 0xFFFF_FFFF) | (val << 32);
                }
            }
            0x14 => {
                v.status = val as u8;
                if v.status == 0 {
                    v.queue_enable = 0;
                    v.last_avail = 0;
                    v.notify_pending = false;
                }
            }
            0x16 => v.queue_sel = val as u16,
            0x18 => {
                if v.queue_sel == 0 {
                    let n = val as u16;
                    if n > 0 && n <= QUEUE_MAX {
                        v.queue_size = n;
                    }
                }
            }
            0x1C => v.queue_enable = val as u16,
            0x20 => v.queue_desc = (v.queue_desc & !0xFFFF_FFFF) | (val & 0xFFFF_FFFF),
            0x24 => v.queue_desc = (v.queue_desc & 0xFFFF_FFFF) | (val << 32),
            0x28 => v.queue_driver = (v.queue_driver & !0xFFFF_FFFF) | (val & 0xFFFF_FFFF),
            0x2C => v.queue_driver = (v.queue_driver & 0xFFFF_FFFF) | (val << 32),
            0x30 => v.queue_device = (v.queue_device & !0xFFFF_FFFF) | (val & 0xFFFF_FFFF),
            0x34 => v.queue_device = (v.queue_device & 0xFFFF_FFFF) | (val << 32),
            x if x == OFF_NOTIFY => v.notify_pending = true,
            _ => {}
        }
    });
}

/// Apply a virtio-blk sector request to `disk`. Host-testable.
pub fn blk_sector_rw(disk: &mut [u8], ty: u32, sector: u64, buf: &mut [u8]) -> u8 {
    if ty == VIRTIO_BLK_T_FLUSH {
        return VIRTIO_BLK_S_OK;
    }
    let off = match (sector as usize).checked_mul(SECTOR) {
        Some(o) => o,
        None => return VIRTIO_BLK_S_IOERR,
    };
    if buf.is_empty() || off >= disk.len() {
        return VIRTIO_BLK_S_IOERR;
    }
    let n = core::cmp::min(buf.len(), disk.len() - off);
    match ty {
        VIRTIO_BLK_T_IN => {
            buf[..n].copy_from_slice(&disk[off..off + n]);
            VIRTIO_BLK_S_OK
        }
        VIRTIO_BLK_T_OUT => {
            disk[off..off + n].copy_from_slice(&buf[..n]);
            VIRTIO_BLK_S_OK
        }
        _ => VIRTIO_BLK_S_IOERR,
    }
}

/// ESP type GUID on disk (EFI System Partition).
const GPT_ESP_TYPE: [u8; 16] = [
    0x28, 0x73, 0x2A, 0xC1, 0x1F, 0xF8, 0xD2, 0x11, 0xBA, 0x4B, 0x00, 0xA0, 0xC9, 0x3E, 0xC9,
    0x3B,
];
/// Linux filesystem GUID on disk.
const GPT_LINUX_FS_TYPE: [u8; 16] = [
    0xAF, 0x3D, 0xC6, 0x0F, 0x83, 0x84, 0x72, 0x47, 0x8E, 0x79, 0x3D, 0x69, 0xD8, 0x47, 0x7D,
    0xE4,
];

/// True when `disk` has a GPT or a non-empty MBR partition (installer wrote it).
///
/// INVARIANTS:
/// - Empty zeros is false
/// - Does not imply [`crate::mgmt::iso_install::M7_ISO_INSTALL_OK_MARKER`]
pub fn install_disk_has_partition_table(disk: &[u8]) -> bool {
    if disk.len() < 512 {
        return false;
    }
    let mbr = disk[510] == 0x55 && disk[511] == 0xAA;
    if disk.len() >= 1024 && &disk[512..520] == b"EFI PART" {
        if disk.len() >= 1024 + 128 {
            for i in 0..4 {
                let off = 1024 + i * 128;
                if disk.len() < off + 16 {
                    break;
                }
                let ty = &disk[off..off + 16];
                if ty == GPT_ESP_TYPE || ty == GPT_LINUX_FS_TYPE {
                    return true;
                }
            }
        }
        return true;
    }
    mbr && disk[0x1BE + 4] != 0
}

/// Iron-only: product queues + partition table + enough OUT bytes.
/// Caller prints [`crate::mgmt::iso_install::M7_ISO_INSTALL_OK_MARKER`].
/// Host/CI / nested must not call this print path.
pub fn take_iso_install_ok() -> bool {
    if !queues_armed() || disk_bytes_written() < 16 * 1024 {
        return false;
    }
    let hpa = DISK_HPA.load(Ordering::Acquire);
    let dlen = DISK_LEN.load(Ordering::Acquire) as usize;
    if hpa == 0 || dlen < 512 {
        return false;
    }
    // SAFETY: attach_disk installed exclusive disk frames.
    let disk = unsafe { core::slice::from_raw_parts(hpa as *const u8, dlen) };
    if !install_disk_has_partition_table(disk) {
        return false;
    }
    !ISO_OK.swap(true, Ordering::AcqRel)
}

/// Walk a split virtqueue in a flat GPA image (`guest_mem[gpa]`).
pub fn process_blk_queue_in(
    guest_mem: &mut [u8],
    disk: &mut [u8],
    qsize: u16,
    last_avail: &mut u16,
    desc_gpa: u64,
    avail_gpa: u64,
    used_gpa: u64,
) -> u32 {
    let base = guest_mem.as_mut_ptr() as u64;
    let len = guest_mem.len() as u64;
    let translate = |gpa: u64| {
        if gpa < len {
            Some(base + gpa)
        } else {
            None
        }
    };
    process_blk_queue(qsize, last_avail, desc_gpa, avail_gpa, used_gpa, disk, &translate)
}

fn read_bytes(translate: &impl Fn(u64) -> Option<u64>, gpa: u64, dst: &mut [u8]) -> bool {
    let Some(hpa) = translate(gpa) else {
        return false;
    };
    // SAFETY: translate returned a host pointer covering `dst.len()`.
    unsafe {
        core::ptr::copy_nonoverlapping(hpa as *const u8, dst.as_mut_ptr(), dst.len());
    }
    true
}

fn write_bytes(translate: &impl Fn(u64) -> Option<u64>, gpa: u64, src: &[u8]) -> bool {
    let Some(hpa) = translate(gpa) else {
        return false;
    };
    // SAFETY: translate returned a writable host pointer covering `src.len()`.
    unsafe {
        core::ptr::copy_nonoverlapping(src.as_ptr(), hpa as *mut u8, src.len());
    }
    true
}

fn read_u16(translate: &impl Fn(u64) -> Option<u64>, gpa: u64) -> Option<u16> {
    let mut b = [0u8; 2];
    if read_bytes(translate, gpa, &mut b) {
        Some(u16::from_le_bytes(b))
    } else {
        None
    }
}

fn write_u16(translate: &impl Fn(u64) -> Option<u64>, gpa: u64, val: u16) -> bool {
    write_bytes(translate, gpa, &val.to_le_bytes())
}

fn process_blk_queue(
    qsize: u16,
    last_avail: &mut u16,
    desc_gpa: u64,
    avail_gpa: u64,
    used_gpa: u64,
    disk: &mut [u8],
    translate: &impl Fn(u64) -> Option<u64>,
) -> u32 {
    if qsize == 0 {
        return 0;
    }
    let Some(avail_idx) = read_u16(translate, avail_gpa + 2) else {
        return 0;
    };
    let mut written = 0u32;
    while *last_avail != avail_idx {
        let slot = (*last_avail as usize) % (qsize as usize);
        let Some(head) = read_u16(translate, avail_gpa + 4 + (slot as u64) * 2) else {
            break;
        };
        let mut desc = head;
        let mut hdr = [0u8; 16];
        let mut data_gpa = 0u64;
        let mut data_len = 0u32;
        let mut data_write = false;
        let mut status_gpa = 0u64;
        let mut chain = 0u16;
        let mut got_hdr = false;
        loop {
            if chain > qsize {
                break;
            }
            let d_off = desc_gpa + u64::from(desc) * 16;
            let mut raw = [0u8; 16];
            if !read_bytes(translate, d_off, &mut raw) {
                break;
            }
            let addr = u64::from_le_bytes(raw[0..8].try_into().unwrap_or([0; 8]));
            let len = u32::from_le_bytes(raw[8..12].try_into().unwrap_or([0; 4]));
            let flags = u16::from_le_bytes(raw[12..14].try_into().unwrap_or([0; 2]));
            let next = u16::from_le_bytes(raw[14..16].try_into().unwrap_or([0; 2]));
            if !got_hdr && len >= 16 {
                if !read_bytes(translate, addr, &mut hdr) {
                    break;
                }
                got_hdr = true;
            } else if (flags & VRING_DESC_F_WRITE) != 0 && len == 1 && status_gpa == 0 {
                status_gpa = addr;
            } else if data_gpa == 0 && len > 1 {
                data_gpa = addr;
                data_len = len;
                data_write = (flags & VRING_DESC_F_WRITE) != 0;
            }
            if (flags & VRING_DESC_F_NEXT) == 0 {
                break;
            }
            desc = next;
            chain += 1;
        }
        let ty = u32::from_le_bytes(hdr[0..4].try_into().unwrap_or([0; 4]));
        let sector = u64::from_le_bytes(hdr[8..16].try_into().unwrap_or([0; 8]));
        let mut status = VIRTIO_BLK_S_IOERR;
        if ty == VIRTIO_BLK_T_FLUSH {
            status = VIRTIO_BLK_S_OK;
        } else if data_gpa != 0 && data_len > 0 {
            let n = data_len as usize;
            let mut buf = [0u8; 4096];
            let take = core::cmp::min(n, buf.len());
            if data_write {
                // IN: device writes guest buffer
                status = blk_sector_rw(disk, ty, sector, &mut buf[..take]);
                if status == VIRTIO_BLK_S_OK {
                    let _ = write_bytes(translate, data_gpa, &buf[..take]);
                }
            } else {
                if read_bytes(translate, data_gpa, &mut buf[..take]) {
                    status = blk_sector_rw(disk, ty, sector, &mut buf[..take]);
                    if status == VIRTIO_BLK_S_OK && ty == VIRTIO_BLK_T_OUT {
                        written = written.saturating_add(take as u32);
                    }
                }
            }
        }
        if status_gpa != 0 {
            let _ = write_bytes(translate, status_gpa, &[status]);
        }
        let used_idx = read_u16(translate, used_gpa + 2).unwrap_or(0);
        let uslot = (used_idx as usize) % (qsize as usize);
        let mut used_elt = [0u8; 8];
        used_elt[0..4].copy_from_slice(&(u32::from(head)).to_le_bytes());
        used_elt[4..8].copy_from_slice(&1u32.to_le_bytes());
        let _ = write_bytes(translate, used_gpa + 4 + (uslot as u64) * 8, &used_elt);
        let _ = write_u16(translate, used_gpa + 2, used_idx.wrapping_add(1));
        *last_avail = last_avail.wrapping_add(1);
    }
    written
}

/// Drain a pending notify using `translate` (GPA → HPA).
pub fn drain_queue(translate: fn(u64) -> Option<u64>) -> u32 {
    let (notified, pending, qsize, last, desc, avail, used) = with_virtio(|v| {
        let p = v.notify_pending;
        v.notify_pending = false;
        if p {
            v.isr = 1;
        }
        (
            p,
            p && v.queue_enable != 0,
            v.queue_size,
            v.last_avail,
            v.queue_desc,
            v.queue_driver,
            v.queue_device,
        )
    });
    if notified {
        crate::devices::guest_irq::raise_virtio();
    }
    if !pending {
        return 0;
    }
    let hpa = DISK_HPA.load(Ordering::Acquire);
    let dlen = DISK_LEN.load(Ordering::Acquire) as usize;
    if hpa == 0 || dlen == 0 {
        return 0;
    }
    // SAFETY: attach_disk installed exclusive disk frames.
    let disk = unsafe { core::slice::from_raw_parts_mut(hpa as *mut u8, dlen) };
    let mut last_avail = last;
    let n = process_blk_queue(qsize, &mut last_avail, desc, avail, used, disk, &translate);
    with_virtio(|v| v.last_avail = last_avail);
    if n > 0 {
        BYTES_WRITTEN.fetch_add(u64::from(n), Ordering::AcqRel);
    }
    n
}

/// Decoded guest MOV targeting BAR MMIO. GPA comes from the EPT violation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MmioInsn {
    pub is_write: bool,
    pub size: u8,
    pub reg: u8,
    pub has_imm: bool,
    pub imm: u64,
}

/// Decode a MOV/MOVZX that OVMF IoLib uses for virtio-pci BAR MMIO.
pub fn decode_mmio_insn(bytes: &[u8], insn_len: usize) -> Option<MmioInsn> {
    if bytes.is_empty() || insn_len == 0 || insn_len > bytes.len() || insn_len > 15 {
        return None;
    }
    let mut i = 0usize;
    let mut operand16 = false;
    let mut rex_w = false;
    let mut rex_r = 0u8;
    while i < insn_len {
        match bytes[i] {
            0x66 => {
                operand16 = true;
                i += 1;
            }
            0x67 | 0xF0 | 0xF2 | 0xF3 => i += 1,
            r if (0x40..=0x4F).contains(&r) => {
                rex_w = (r & 0x8) != 0;
                rex_r = (r & 0x4) << 1;
                i += 1;
            }
            _ => break,
        }
    }
    if i >= insn_len {
        return None;
    }
    let op = bytes[i];
    i += 1;
    if op == 0x0F {
        if i >= insn_len {
            return None;
        }
        let op2 = bytes[i];
        i += 1;
        if op2 != 0xB6 && op2 != 0xB7 {
            return None;
        }
        if i >= insn_len {
            return None;
        }
        let m = bytes[i];
        let reg = ((m >> 3) & 7) | rex_r;
        let size = if op2 == 0xB6 { 1 } else { 2 };
        return Some(MmioInsn {
            is_write: false,
            size,
            reg,
            has_imm: false,
            imm: 0,
        });
    }
    if i >= insn_len && op != 0xC6 && op != 0xC7 {
        // ModRM required except we still need it for C6/C7.
    }
    match op {
        0x88 | 0x89 | 0x8A | 0x8B => {
            if i >= insn_len {
                return None;
            }
            let m = bytes[i];
            let reg = ((m >> 3) & 7) | rex_r;
            let is_write = op == 0x88 || op == 0x89;
            let size = if op == 0x88 || op == 0x8A {
                1
            } else if rex_w {
                8
            } else if operand16 {
                2
            } else {
                4
            };
            Some(MmioInsn {
                is_write,
                size,
                reg,
                has_imm: false,
                imm: 0,
            })
        }
        0xC6 | 0xC7 => {
            let size = if op == 0xC6 {
                1
            } else if operand16 {
                2
            } else {
                4
            };
            if insn_len < size as usize {
                return None;
            }
            let imm_off = insn_len - size as usize;
            let mut imm = 0u64;
            let mut k = 0u32;
            while k < u32::from(size) {
                imm |= u64::from(bytes[imm_off + k as usize]) << (8 * k);
                k += 1;
            }
            Some(MmioInsn {
                is_write: true,
                size,
                reg: 0,
                has_imm: true,
                imm,
            })
        }
        _ => None,
    }
}

#[cfg(test)]
#[path = "guest_virtio_blk_test.rs"]
mod guest_virtio_blk_test;
