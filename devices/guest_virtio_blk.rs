//! Guest-visible PCI virtio-blk for the private guest-UEFI VMCS.
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** (ADR-002 / ADR-014)
//! VERIFICATION: L1 (runtime + host tests; QEMU is the enum gate)
//!
//! Empty virtio 1.0 block function at `00:02.0` (Red Hat `1AF4:1042`).
//! Product ISO window also reveals a **read-only** virtio-blk at `00:03.0`
//! backed by the same ISO bytes (alpine-virt initramfs has virtio, not
//! `ata_piix`; `/dev/vdb` is ISO9660 so `nlplug-findfs` can find media).
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
//! `bootorder`). Do **not** move virtio to `00:00.0`.
//! Lab stub: vendor cap `0x0001_0010` (enum only, not queues); slot 3 empty.
//! Product ISO window: virtio-pci caps type 1/2/3/4 + trap-and-emulate BAR
//! MMIO + split virtqueue IN/OUT/FLUSH (every data descriptor in the chain,
//! not only the first). GPA copies stop at 4 KiB so report-RAM 2 MiB slots
//! (non-contiguous HPA) are not overrun. Not the M4.3 virtio-mmio probe.
//! Not ISO-INSTALL-OK.

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
/// Read-only product ISO virtio-blk. Not `00:00.0`. Not slot 2 (install disk).
pub const GUEST_VIRTIO_ISO_PCI_DEV: u8 = 3;
pub const GUEST_VIRTIO_ISO_PCI_FN: u8 = 0;
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
/// Slot 3 ISO BAR — same 2 MiB trap page as [`GUEST_VIRTIO_BAR0_DEFAULT`].
pub const GUEST_VIRTIO_ISO_BAR0_DEFAULT: u32 = 0xFE00_1000;
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
pub const VIRTIO_BLK_F_RO: u64 = 1 << 5;
pub const VIRTIO_BLK_DEVICE_FEATURES: u64 = VIRTIO_F_VERSION_1 | VIRTIO_BLK_F_FLUSH;
pub const VIRTIO_BLK_ISO_FEATURES: u64 = VIRTIO_F_VERSION_1 | VIRTIO_BLK_F_FLUSH | VIRTIO_BLK_F_RO;

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
/// Linux blk-mq maps each bio_vec as a descriptor. Cap the chain.
const DATA_SEGS: usize = 16;

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
    readonly: bool,
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
            readonly: false,
        }
    }
}

struct VirtioBox {
    pci_addr: u32,
    disk: VirtioPci,
    iso: VirtioPci,
}

impl VirtioBox {
    const fn empty() -> Self {
        Self {
            pci_addr: 0,
            disk: VirtioPci::empty(),
            iso: VirtioPci::empty(),
        }
    }
}

// JUSTIFICATION: one guest-UEFI virtio-blk; firmware is single-threaded after EBS.
struct GuestVirtio(core::cell::UnsafeCell<VirtioBox>);

// SAFETY: exclusive access is enforced by `VIRTIO_LOCK`.
// KANI-TARGET: guest-UEFI virtio-blk mutex (outside Proven Core).
unsafe impl Sync for GuestVirtio {}

static VIRTIO: GuestVirtio = GuestVirtio(core::cell::UnsafeCell::new(VirtioBox::empty()));
static VIRTIO_LOCK: AtomicBool = AtomicBool::new(false);
static VISIBLE: AtomicBool = AtomicBool::new(false);
static PCI_ENUM: AtomicBool = AtomicBool::new(false);
static MARKER: AtomicBool = AtomicBool::new(false);
static QUEUES: AtomicBool = AtomicBool::new(false);
static DISK_HPA: AtomicU64 = AtomicU64::new(0);
static DISK_LEN: AtomicU64 = AtomicU64::new(0);
static BYTES_WRITTEN: AtomicU64 = AtomicU64::new(0);
static ISO_PTR: AtomicU64 = AtomicU64::new(0);
static ISO_LEN: AtomicU64 = AtomicU64::new(0);
static ISO_READ: AtomicU64 = AtomicU64::new(0);
static ISO_VISIBLE: AtomicBool = AtomicBool::new(false);
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

fn with_box<R>(f: impl FnOnce(&mut VirtioBox) -> R) -> R {
    while VIRTIO_LOCK.swap(true, Ordering::Acquire) {
        core::hint::spin_loop();
    }
    // SAFETY: lock held; exclusive mutable access.
    // KANI-TARGET: guest-UEFI virtio-blk mutex (outside Proven Core).
    let out = unsafe { f(&mut *VIRTIO.0.get()) };
    VIRTIO_LOCK.store(false, Ordering::Release);
    out
}

fn with_virtio<R>(f: impl FnOnce(&mut VirtioPci) -> R) -> R {
    with_box(|b| f(&mut b.disk))
}

fn with_iso<R>(f: impl FnOnce(&mut VirtioPci) -> R) -> R {
    with_box(|b| f(&mut b.iso))
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

pub fn pci_addr_selects_virtio_iso(addr: u32) -> bool {
    if (addr & 0x8000_0000) == 0 {
        return false;
    }
    let (bus, dev, fun, _) = pci_bdf(addr);
    bus == GUEST_VIRTIO_PCI_BUS && dev == GUEST_VIRTIO_ISO_PCI_DEV && fun == GUEST_VIRTIO_ISO_PCI_FN
}

/// Slot 0 host-bridge DID or latched virtio `00:02.0` / `00:03.0`.
pub fn pci_addr_selects_owned(addr: u32) -> bool {
    pci_addr_selects_slot0(addr)
        || pci_addr_selects_virtio(addr)
        || pci_addr_selects_virtio_iso(addr)
}

/// PCI config address for the guest virtio-blk function (`00:02.0`).
pub fn pci_config_addr() -> u32 {
    0x8000_0000
        | (u32::from(GUEST_VIRTIO_PCI_BUS) << 16)
        | (u32::from(GUEST_VIRTIO_PCI_DEV) << 11)
        | (u32::from(GUEST_VIRTIO_PCI_FN) << 8)
}

/// PCI config address for the read-only ISO virtio-blk (`00:03.0`).
pub fn pci_config_addr_iso() -> u32 {
    0x8000_0000
        | (u32::from(GUEST_VIRTIO_PCI_BUS) << 16)
        | (u32::from(GUEST_VIRTIO_ISO_PCI_DEV) << 11)
        | (u32::from(GUEST_VIRTIO_ISO_PCI_FN) << 8)
}
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
    with_box(|b| *b = VirtioBox::empty());
    VISIBLE.store(false, Ordering::Release);
    PCI_ENUM.store(false, Ordering::Release);
    MARKER.store(false, Ordering::Release);
    QUEUES.store(false, Ordering::Release);
    DISK_HPA.store(0, Ordering::Release);
    DISK_LEN.store(0, Ordering::Release);
    BYTES_WRITTEN.store(0, Ordering::Release);
    ISO_PTR.store(0, Ordering::Release);
    ISO_LEN.store(0, Ordering::Release);
    ISO_READ.store(0, Ordering::Release);
    ISO_VISIBLE.store(false, Ordering::Release);
    ISO_OK.store(false, Ordering::Release);
    PEI_I440FX_DID.store(true, Ordering::Release);
}

/// Mark the empty virtio-blk function live on the private guest-UEFI VMCS.
///
/// Queues and modern virtio-pci caps arm only when the product ISO window
/// is already live. Lab 72 KiB stub stays enum-only (`0x0001_0010`).
pub fn present() -> bool {
    let queues = crate::devices::ide_cdrom::product_iso_window_armed();
    let iso_win = crate::devices::ide_cdrom::product_iso_window_ptr();
    let iso_on = with_box(|b| {
        *b = VirtioBox::empty();
        b.disk.visible = true;
        b.disk.bar0 = GUEST_VIRTIO_BAR0_DEFAULT;
        b.disk.queues_armed = queues;
        b.disk.queue_size = QUEUE_MAX;
        if queues {
            if let Some((ptr, len)) = iso_win {
                let n = len & !(SECTOR - 1);
                if n >= SECTOR && !ptr.is_null() {
                    ISO_PTR.store(ptr as u64, Ordering::Release);
                    ISO_LEN.store(n as u64, Ordering::Release);
                    b.iso.visible = true;
                    b.iso.bar0 = GUEST_VIRTIO_ISO_BAR0_DEFAULT;
                    b.iso.queues_armed = true;
                    b.iso.queue_size = QUEUE_MAX;
                    b.iso.readonly = true;
                }
            }
        }
        b.iso.visible
    });
    VISIBLE.store(true, Ordering::Release);
    PCI_ENUM.store(false, Ordering::Release);
    MARKER.store(false, Ordering::Release);
    QUEUES.store(queues, Ordering::Release);
    ISO_VISIBLE.store(iso_on, Ordering::Release);
    PEI_I440FX_DID.store(true, Ordering::Release);
    true
}

/// True when the product window revealed read-only ISO virtio at `00:03.0`.
pub fn iso_visible() -> bool {
    ISO_VISIBLE.load(Ordering::Acquire)
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
    with_box(|b| b.pci_addr = addr);
}

pub fn pci_read_addr() -> u32 {
    with_box(|b| b.pci_addr)
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
        0x3C => u32::from(crate::devices::guest_irq::VIRTIO_PIC_IRQ) | 0x0000_0100,
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
    with_box(|b| {
        if !b.disk.visible {
            return 0xFFFF_FFFF;
        }
        let addr = b.pci_addr;
        let off = pci_cfg_offset(addr, port);
        let aligned = off & 0xFC;
        if pci_addr_selects_slot0(addr) {
            return shift_cfg(slot0_dword(aligned), off, size);
        }
        if pci_addr_selects_virtio_iso(addr) {
            if !b.iso.visible || pei_host_bridge_did() {
                return 0xFFFF_FFFF;
            }
            return shift_cfg(virtio_dword(&b.iso, aligned), off, size);
        }
        if !pci_addr_selects_virtio(addr) {
            return 0xFFFF_FFFF;
        }
        if pei_host_bridge_did() {
            return 0xFFFF_FFFF;
        }
        if aligned == 0 {
            b.disk.pci_enum = true;
            PCI_ENUM.store(true, Ordering::Release);
        }
        shift_cfg(virtio_dword(&b.disk, aligned), off, size)
    })
}

fn apply_bar_write(v: &mut VirtioPci, default_bar: u32, size: u8, val: u32) {
    if v.queues_armed && val == 0xFFFF_FFFF {
        v.bar_sizing = true;
    } else {
        v.bar_sizing = false;
        let mask = if size >= 4 { 0xFFFF_F000 } else { 0xFFFF };
        let next = val & mask;
        v.bar0 = if next == 0 { default_bar } else { next };
    }
}

pub fn pci_write_data(port: u16, size: u8, val: u32) {
    with_box(|b| {
        if !b.disk.visible || pei_host_bridge_did() {
            return;
        }
        let addr = b.pci_addr;
        let off = pci_cfg_offset(addr, port);
        if pci_addr_selects_virtio(addr) {
            if off == 0x04 {
                b.disk.pci_cmd = (val as u16) | 0x0002;
            } else if off == 0x10 {
                apply_bar_write(&mut b.disk, GUEST_VIRTIO_BAR0_DEFAULT, size, val);
            }
            return;
        }
        if pci_addr_selects_virtio_iso(addr) && b.iso.visible {
            if off == 0x04 {
                b.iso.pci_cmd = (val as u16) | 0x0002;
            } else if off == 0x10 {
                apply_bar_write(&mut b.iso, GUEST_VIRTIO_ISO_BAR0_DEFAULT, size, val);
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

fn mmio_bar_base_locked(v: &VirtioPci) -> u64 {
    if !v.queues_armed {
        return 0;
    }
    let default = if v.readonly {
        GUEST_VIRTIO_ISO_BAR0_DEFAULT
    } else {
        GUEST_VIRTIO_BAR0_DEFAULT
    };
    let b = v.bar0 & GUEST_VIRTIO_BAR0_SIZE_MASK;
    if v.bar_sizing || b == 0 || b == GUEST_VIRTIO_BAR0_SIZE_MASK {
        u64::from(default)
    } else {
        u64::from(b)
    }
}

/// Programmed (or default) install-disk BAR0 when queues are armed.
pub fn mmio_bar_base() -> u64 {
    with_virtio(|v| mmio_bar_base_locked(v))
}

fn mmio_iso_bar_base() -> u64 {
    with_iso(|v| mmio_bar_base_locked(v))
}

fn bar_covers(bar: u64, gpa: u64) -> bool {
    bar != 0 && gpa >= bar && gpa < bar + u64::from(GUEST_VIRTIO_BAR0_SIZE)
}

/// BAR base that contains `gpa`, if any.
pub fn mmio_bar_base_for_gpa(gpa: u64) -> Option<u64> {
    let disk = mmio_bar_base();
    if bar_covers(disk, gpa) {
        return Some(disk);
    }
    let iso = mmio_iso_bar_base();
    if bar_covers(iso, gpa) {
        return Some(iso);
    }
    None
}

pub fn is_virtio_iso_bar_gpa(gpa: u64) -> bool {
    bar_covers(mmio_iso_bar_base(), gpa)
}

/// GPA in a 4 KiB virtio BAR. False for the lab enum stub.
pub fn is_virtio_bar_gpa(gpa: u64) -> bool {
    mmio_bar_base_for_gpa(gpa).is_some()
}

/// 2 MiB page containing a virtio BAR — must not be an EPT zero sink.
pub fn is_virtio_bar_2m_gpa(gpa: u64) -> bool {
    if !queues_armed() {
        return false;
    }
    let page = gpa & !0x1F_FFFF;
    let disk = mmio_bar_base();
    if disk != 0 && page == (disk & !0x1F_FFFF) {
        return true;
    }
    let iso = mmio_iso_bar_base();
    iso != 0 && page == (iso & !0x1F_FFFF)
}

fn features_for(v: &VirtioPci, sel: u32) -> u32 {
    let feat = if v.readonly {
        VIRTIO_BLK_ISO_FEATURES
    } else {
        VIRTIO_BLK_DEVICE_FEATURES
    };
    if sel == 0 {
        feat as u32
    } else if sel == 1 {
        (feat >> 32) as u32
    } else {
        0
    }
}

fn capacity_sectors_for(v: &VirtioPci) -> u64 {
    if v.readonly {
        ISO_LEN.load(Ordering::Acquire) / SECTOR as u64
    } else {
        DISK_LEN.load(Ordering::Acquire) / SECTOR as u64
    }
}

/// Read virtio-pci BAR MMIO (install disk).
pub fn mmio_read(off: u16, size: u8) -> u64 {
    mmio_read_dev(false, off, size)
}

pub fn mmio_read_iso(off: u16, size: u8) -> u64 {
    mmio_read_dev(true, off, size)
}

fn mmio_read_dev(iso: bool, off: u16, size: u8) -> u64 {
    let val = if iso {
        with_iso(|v| mmio_read_locked(v, off, size))
    } else {
        with_virtio(|v| mmio_read_locked(v, off, size))
    };
    if off == OFF_ISR {
        if iso {
            crate::devices::guest_irq::lower_virtio_iso();
        } else {
            crate::devices::guest_irq::lower_virtio();
        }
    }
    val
}

pub fn mmio_read_at(gpa: u64, size: u8) -> u64 {
    let Some(bar) = mmio_bar_base_for_gpa(gpa) else {
        return 0;
    };
    let off = (gpa.wrapping_sub(bar)) as u16;
    mmio_read_dev(is_virtio_iso_bar_gpa(gpa), off, size)
}

fn mmio_read_locked(v: &mut VirtioPci, off: u16, size: u8) -> u64 {
    if !v.queues_armed {
        return 0;
    }
    let cap = capacity_sectors_for(v);
    let val = match off {
        0x00 => v.feat_sel as u64,
        0x04 => features_for(v, v.feat_sel) as u64,
        0x08 => v.drv_feat_sel as u64,
        0x0C => {
            if v.drv_feat_sel == 0 {
                v.drv_feat as u32 as u64
            } else {
                (v.drv_feat >> 32) as u64
            }
        }
        // Packed 32-bit: msix_config=0xFFFF, num_queues=1 in the high half.
        0x10 => 0x0001_FFFF,
        0x12 => 1,
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
        x if x == OFF_DEVICE => cap,
        x if x == OFF_DEVICE + 4 => cap >> 32,
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

/// Write a virtqueue GPA. Linux `writeq` is one 8-byte store at the low half.
fn write_queue_ptr(field: &mut u64, rel: u16, size: u8, val: u64) {
    if size >= 8 && rel == 0 {
        *field = val;
        return;
    }
    if rel == 0 {
        *field = (*field & !0xFFFF_FFFF) | (val & 0xFFFF_FFFF);
    } else if rel == 4 {
        *field = (*field & 0xFFFF_FFFF) | ((val & 0xFFFF_FFFF) << 32);
    }
}

/// Write virtio-pci BAR MMIO. Notify sets a pending bit; call [`drain_queue`].
pub fn mmio_write(off: u16, size: u8, val: u64) {
    mmio_write_dev(false, off, size, val);
}

pub fn mmio_write_iso(off: u16, size: u8, val: u64) {
    mmio_write_dev(true, off, size, val);
}

pub fn mmio_write_at(gpa: u64, size: u8, val: u64) {
    let Some(bar) = mmio_bar_base_for_gpa(gpa) else {
        return;
    };
    let off = (gpa.wrapping_sub(bar)) as u16;
    mmio_write_dev(is_virtio_iso_bar_gpa(gpa), off, size, val);
}

fn mmio_write_dev(iso: bool, off: u16, size: u8, val: u64) {
    if iso {
        with_iso(|v| mmio_write_locked(v, off, size, val));
    } else {
        with_virtio(|v| mmio_write_locked(v, off, size, val));
    }
}

fn mmio_write_locked(v: &mut VirtioPci, off: u16, size: u8, val: u64) {
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
        0x20 => write_queue_ptr(&mut v.queue_desc, 0, size, val),
        0x24 => write_queue_ptr(&mut v.queue_desc, 4, size, val),
        0x28 => write_queue_ptr(&mut v.queue_driver, 0, size, val),
        0x2C => write_queue_ptr(&mut v.queue_driver, 4, size, val),
        0x30 => write_queue_ptr(&mut v.queue_device, 0, size, val),
        0x34 => write_queue_ptr(&mut v.queue_device, 4, size, val),
        x if x == OFF_NOTIFY => v.notify_pending = true,
        _ => {}
    }
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
    0x28, 0x73, 0x2A, 0xC1, 0x1F, 0xF8, 0xD2, 0x11, 0xBA, 0x4B, 0x00, 0xA0, 0xC9, 0x3E, 0xC9, 0x3B,
];
/// Linux filesystem GUID on disk.
const GPT_LINUX_FS_TYPE: [u8; 16] = [
    0xAF, 0x3D, 0xC6, 0x0F, 0x83, 0x84, 0x72, 0x47, 0x8E, 0x79, 0x3D, 0x69, 0xD8, 0x47, 0x7D, 0xE4,
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
    process_blk_queue(
        qsize, last_avail, desc_gpa, avail_gpa, used_gpa, disk, &translate, false,
    )
}

/// Walk a split virtqueue against a read-only backing (product ISO `/dev/vdb`).
pub fn process_iso_queue_in(
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
    process_blk_queue(
        qsize, last_avail, desc_gpa, avail_gpa, used_gpa, disk, &translate, true,
    )
}

fn page_left(gpa: u64) -> usize {
    (0x1000 - (gpa & 0xfff)) as usize
}

fn read_bytes(translate: &impl Fn(u64) -> Option<u64>, gpa: u64, dst: &mut [u8]) -> bool {
    let mut done = 0usize;
    while done < dst.len() {
        let g = gpa.wrapping_add(done as u64);
        let Some(hpa) = translate(g) else {
            return false;
        };
        let take = (dst.len() - done).min(page_left(g));
        if take == 0 {
            return false;
        }
        // SAFETY: translate returned a host pointer covering this 4 KiB page.
        // Report-RAM 2 MiB slots are not contiguous in HPA; do not copy past
        // the page. KANI-TARGET: virtio GPA read (outside Proven Core).
        unsafe {
            core::ptr::copy_nonoverlapping(hpa as *const u8, dst[done..].as_mut_ptr(), take);
        }
        done = done.saturating_add(take);
    }
    true
}

fn write_bytes(translate: &impl Fn(u64) -> Option<u64>, gpa: u64, src: &[u8]) -> bool {
    let mut done = 0usize;
    while done < src.len() {
        let g = gpa.wrapping_add(done as u64);
        let Some(hpa) = translate(g) else {
            return false;
        };
        let take = (src.len() - done).min(page_left(g));
        if take == 0 {
            return false;
        }
        // SAFETY: translate returned a writable host pointer covering this
        // 4 KiB page. KANI-TARGET: virtio GPA write (outside Proven Core).
        unsafe {
            core::ptr::copy_nonoverlapping(src[done..].as_ptr(), hpa as *mut u8, take);
        }
        done = done.saturating_add(take);
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

fn xfer_data_seg(
    disk: &mut [u8],
    translate: &impl Fn(u64) -> Option<u64>,
    ty: u32,
    sector: u64,
    byte_off: usize,
    gpa: u64,
    len: u32,
    device_write: bool,
) -> (u8, u32) {
    let n = len as usize;
    let mut buf = [0u8; 4096];
    let mut done = 0usize;
    let mut wrote = 0u32;
    while done < n {
        let take = core::cmp::min(n - done, buf.len());
        let sec = sector.saturating_add(((byte_off + done) / SECTOR) as u64);
        if device_write {
            let status = blk_sector_rw(disk, ty, sec, &mut buf[..take]);
            if status != VIRTIO_BLK_S_OK {
                return (status, wrote);
            }
            if !write_bytes(translate, gpa + done as u64, &buf[..take]) {
                return (VIRTIO_BLK_S_IOERR, wrote);
            }
        } else if read_bytes(translate, gpa + done as u64, &mut buf[..take]) {
            let status = blk_sector_rw(disk, ty, sec, &mut buf[..take]);
            if status != VIRTIO_BLK_S_OK {
                return (status, wrote);
            }
            if ty == VIRTIO_BLK_T_OUT {
                wrote = wrote.saturating_add(take as u32);
            }
        } else {
            return (VIRTIO_BLK_S_IOERR, wrote);
        }
        done = done.saturating_add(take);
    }
    (VIRTIO_BLK_S_OK, wrote)
}

fn process_blk_queue(
    qsize: u16,
    last_avail: &mut u16,
    desc_gpa: u64,
    avail_gpa: u64,
    used_gpa: u64,
    disk: &mut [u8],
    translate: &impl Fn(u64) -> Option<u64>,
    readonly: bool,
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
        let mut segs = [(0u64, 0u32, false); DATA_SEGS];
        let mut nseg = 0usize;
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
            } else if len > 0 && nseg < DATA_SEGS {
                segs[nseg] = (addr, len, (flags & VRING_DESC_F_WRITE) != 0);
                nseg += 1;
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
        let mut req_bytes = 0u32;
        if ty == VIRTIO_BLK_T_FLUSH {
            status = VIRTIO_BLK_S_OK;
        } else if readonly && ty == VIRTIO_BLK_T_OUT {
            status = VIRTIO_BLK_S_IOERR;
        } else if nseg > 0 {
            let mut byte_off = 0usize;
            status = VIRTIO_BLK_S_OK;
            for &(gpa, len, device_write) in segs[..nseg].iter() {
                let (st, w) = xfer_data_seg(
                    disk,
                    translate,
                    ty,
                    sector,
                    byte_off,
                    gpa,
                    len,
                    device_write,
                );
                status = st;
                req_bytes = req_bytes.saturating_add(w);
                if status != VIRTIO_BLK_S_OK {
                    break;
                }
                byte_off = byte_off.saturating_add(len as usize);
            }
            if readonly && ty == VIRTIO_BLK_T_IN && status == VIRTIO_BLK_S_OK {
                written = written.saturating_add(byte_off as u32);
            } else {
                written = written.saturating_add(req_bytes);
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

/// Bytes the guest read from the ISO virtio since last take (iron serial).
pub fn take_iso_read_note() -> Option<u64> {
    let n = ISO_READ.swap(0, Ordering::AcqRel);
    if n == 0 {
        None
    } else {
        Some(n)
    }
}

fn take_notify(v: &mut VirtioPci) -> (bool, bool, u16, u16, u64, u64, u64) {
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
}

/// Drain pending notifies using `translate` (GPA → HPA).
///
/// Returns install-disk OUT bytes. ISO IN is counted separately
/// ([`take_iso_read_note`]).
pub fn drain_queue(translate: fn(u64) -> Option<u64>) -> u32 {
    let disk_n = drain_disk(translate);
    drain_iso(translate);
    disk_n
}

fn drain_disk(translate: fn(u64) -> Option<u64>) -> u32 {
    let (notified, pending, qsize, last, desc, avail, used) = with_virtio(|v| take_notify(v));
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
    let n = process_blk_queue(
        qsize,
        &mut last_avail,
        desc,
        avail,
        used,
        disk,
        &translate,
        false,
    );
    with_virtio(|v| v.last_avail = last_avail);
    if n > 0 {
        BYTES_WRITTEN.fetch_add(u64::from(n), Ordering::AcqRel);
    }
    n
}

fn drain_iso(translate: fn(u64) -> Option<u64>) {
    let (notified, pending, qsize, last, desc, avail, used) = with_iso(|v| take_notify(v));
    if notified {
        crate::devices::guest_irq::raise_virtio_iso();
    }
    if !pending {
        return;
    }
    let ptr = ISO_PTR.load(Ordering::Acquire);
    let ilen = ISO_LEN.load(Ordering::Acquire) as usize;
    if ptr == 0 || ilen == 0 {
        return;
    }
    // SAFETY: product ISO window is retained until reset; readonly rejects OUT.
    // KANI-TARGET: virtio-iso IN from product window (outside Proven Core).
    let disk = unsafe { core::slice::from_raw_parts_mut(ptr as *mut u8, ilen) };
    let mut last_avail = last;
    let n = process_blk_queue(
        qsize,
        &mut last_avail,
        desc,
        avail,
        used,
        disk,
        &translate,
        true,
    );
    with_iso(|v| v.last_avail = last_avail);
    if n > 0 {
        ISO_READ.fetch_add(u64::from(n), Ordering::AcqRel);
    }
}

/// Decoded guest MOV targeting BAR MMIO. GPA comes from the EPT violation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MmioInsn {
    pub is_write: bool,
    pub size: u8,
    pub reg: u8,
    pub has_imm: bool,
    pub imm: u64,
    /// MOVZX / 32-bit MOV dest: do not keep high bits of the old GPR.
    pub zero_ext: bool,
    /// MOVSX: sign-extend into the dest GPR.
    pub sign_ext: bool,
    /// XCHG: swap GPR with MMIO (EPT may be read or write).
    pub xchg: bool,
    /// RMW: 0=none, 1=AND, 2=OR, 3=XOR, 4=ADD, 5=SUB, 6=NOT, 7=NEG, 8=ADC, 9=SBB,
    /// 10=ROL, 11=ROR, 12=RCL, 13=RCR, 14=SHL, 15=SHR, 16=SAR.
    pub alu: u8,
    /// Any REX prefix: 8-bit reg 4–7 are SPL/BPL/SIL/DIL, not AH/CH/DH/BH.
    pub rex: bool,
    /// TEST r/m: AND into RFLAGS, do not store.
    pub test: bool,
    /// CMP r/m: SUB into RFLAGS, do not store.
    pub cmp: bool,
    /// CMP r, r/m (`3A`/`3B`): flags are `reg - mem`, not `mem - reg`.
    pub cmp_reg_left: bool,
    /// ALU r, r/m (`02`/`03` ADD, `12`/`13` ADC, `0A`/`0B` OR, `1A`/`1B` SBB,
    /// `22`/`23` AND, `2A`/`2B` SUB, `32`/`33` XOR): dest is the GPR, not MMIO.
    pub alu_reg_left: bool,
    /// BT family: 0=none, 1=BT, 2=BTS, 3=BTR, 4=BTC. CF = old bit.
    pub bt: u8,
    /// 0=none, 1=CMPXCHG, 2=XADD.
    pub atomic: u8,
}

pub const MMIO_CMPXCHG: u8 = 1;
pub const MMIO_XADD: u8 = 2;

pub const MMIO_BT: u8 = 1;
pub const MMIO_BTS: u8 = 2;
pub const MMIO_BTR: u8 = 3;
pub const MMIO_BTC: u8 = 4;

pub const MMIO_ALU_AND: u8 = 1;
pub const MMIO_ALU_OR: u8 = 2;
pub const MMIO_ALU_XOR: u8 = 3;
pub const MMIO_ALU_ADD: u8 = 4;
pub const MMIO_ALU_SUB: u8 = 5;
pub const MMIO_ALU_NOT: u8 = 6;
pub const MMIO_ALU_NEG: u8 = 7;
pub const MMIO_ALU_ADC: u8 = 8;
pub const MMIO_ALU_SBB: u8 = 9;
pub const MMIO_ALU_ROL: u8 = 10;
pub const MMIO_ALU_ROR: u8 = 11;
pub const MMIO_ALU_RCL: u8 = 12;
pub const MMIO_ALU_RCR: u8 = 13;
pub const MMIO_ALU_SHL: u8 = 14;
pub const MMIO_ALU_SHR: u8 = 15;
pub const MMIO_ALU_SAR: u8 = 16;

pub fn mmio_alu_is_shift(alu: u8) -> bool {
    (MMIO_ALU_ROL..=MMIO_ALU_SAR).contains(&alu)
}

pub fn mmio_alu_apply(cur: u64, rhs: u64, alu: u8) -> u64 {
    mmio_alu_apply_cf(cur, rhs, alu, false)
}

/// ADC/SBB use `cf` (RFLAGS.CF). Other ops ignore it.
pub fn mmio_alu_apply_cf(cur: u64, rhs: u64, alu: u8, cf: bool) -> u64 {
    let c = if cf { 1 } else { 0 };
    match alu {
        MMIO_ALU_AND => cur & rhs,
        MMIO_ALU_OR => cur | rhs,
        MMIO_ALU_XOR => cur ^ rhs,
        MMIO_ALU_ADD => cur.wrapping_add(rhs),
        MMIO_ALU_ADC => cur.wrapping_add(rhs).wrapping_add(c),
        MMIO_ALU_SUB => cur.wrapping_sub(rhs),
        MMIO_ALU_SBB => cur.wrapping_sub(rhs).wrapping_sub(c),
        MMIO_ALU_NOT => !cur,
        MMIO_ALU_NEG => 0u64.wrapping_sub(cur),
        _ => rhs,
    }
}

pub fn mmio_eq(a: u64, b: u64, size: u8) -> bool {
    let m = mmio_size_mask(size);
    (a & m) == (b & m)
}

fn mmio_size_mask(size: u8) -> u64 {
    match size {
        1 => 0xff,
        2 => 0xffff,
        4 => 0xffff_ffff,
        _ => u64::MAX,
    }
}

fn mmio_sign_bit(size: u8) -> u64 {
    match size {
        1 => 0x80,
        2 => 0x8000,
        4 => 0x8000_0000,
        _ => 1u64 << 63,
    }
}

/// TEST: ZF/SF/PF from `result`; CF/OF/AF cleared. Bit 1 stays set.
pub fn mmio_test_rflags(old: u64, result: u64, size: u8) -> u64 {
    const CF: u64 = 1 << 0;
    const PF: u64 = 1 << 2;
    const AF: u64 = 1 << 4;
    const ZF: u64 = 1 << 6;
    const SF: u64 = 1 << 7;
    const OF: u64 = 1 << 11;
    let r = result & mmio_size_mask(size);
    let mut f = (old | 2) & !(CF | PF | AF | ZF | SF | OF);
    if r == 0 {
        f |= ZF;
    }
    if (r & mmio_sign_bit(size)) != 0 {
        f |= SF;
    }
    if (r as u8).count_ones() % 2 == 0 {
        f |= PF;
    }
    f
}

/// CMP: flags from `left - right` (unsigned CF, signed OF).
pub fn mmio_cmp_rflags(old: u64, left: u64, right: u64, size: u8) -> u64 {
    let mask = mmio_size_mask(size);
    let a = left & mask;
    let b = right & mask;
    let r = a.wrapping_sub(b) & mask;
    let mut f = mmio_test_rflags(old, r, size);
    if a < b {
        f |= 1 << 0;
    }
    let s = mmio_sign_bit(size);
    let sa = (a & s) != 0;
    let sb = (b & s) != 0;
    let sr = (r & s) != 0;
    if sa != sb && sr != sa {
        f |= 1 << 11;
    }
    f
}

/// ADD: flags from `left + right` (unsigned CF, signed OF).
pub fn mmio_add_rflags(old: u64, left: u64, right: u64, size: u8) -> u64 {
    let mask = mmio_size_mask(size);
    let a = left & mask;
    let b = right & mask;
    let r = a.wrapping_add(b) & mask;
    let mut f = mmio_test_rflags(old, r, size);
    let carry = if size == 8 {
        a.overflowing_add(b).1
    } else {
        a.wrapping_add(b) > mask
    };
    if carry {
        f |= 1 << 0;
    }
    let s = mmio_sign_bit(size);
    let sa = (a & s) != 0;
    let sb = (b & s) != 0;
    let sr = (r & s) != 0;
    if sa == sb && sr != sa {
        f |= 1 << 11;
    }
    f
}

fn mmio_as_signed(v: u64, size: u8) -> i128 {
    match size {
        1 => i128::from(v as u8 as i8),
        2 => i128::from(v as u16 as i16),
        4 => i128::from(v as u32 as i32),
        _ => i128::from(v as i64),
    }
}

/// ADC: flags from `left + right + CF` (unsigned CF, signed OF).
pub fn mmio_adc_rflags(old: u64, left: u64, right: u64, size: u8) -> u64 {
    let mask = mmio_size_mask(size);
    let a = left & mask;
    let b = right & mask;
    let c = old & 1;
    let sum = u128::from(a) + u128::from(b) + u128::from(c);
    let r = (sum as u64) & mask;
    let mut f = mmio_test_rflags(old, r, size);
    if sum > u128::from(mask) {
        f |= 1 << 0;
    }
    if mmio_as_signed(a, size) + mmio_as_signed(b, size) + i128::from(c)
        != mmio_as_signed(r, size)
    {
        f |= 1 << 11;
    }
    f
}

/// SBB: flags from `left - right - CF` (unsigned CF, signed OF).
pub fn mmio_sbb_rflags(old: u64, left: u64, right: u64, size: u8) -> u64 {
    let mask = mmio_size_mask(size);
    let a = left & mask;
    let b = right & mask;
    let c = old & 1;
    let r = a.wrapping_sub(b).wrapping_sub(c) & mask;
    let mut f = mmio_test_rflags(old, r, size);
    if u128::from(a) < u128::from(b) + u128::from(c) {
        f |= 1 << 0;
    }
    if mmio_as_signed(a, size) - mmio_as_signed(b, size) - i128::from(c)
        != mmio_as_signed(r, size)
    {
        f |= 1 << 11;
    }
    f
}

/// ALU into RFLAGS. NOT leaves flags; NEG is `0 - src`; AND/OR/XOR like TEST.
/// ADC/SBB consume CF from `old`.
pub fn mmio_alu_rflags(old: u64, left: u64, right: u64, result: u64, alu: u8, size: u8) -> u64 {
    match alu {
        MMIO_ALU_NOT => old,
        MMIO_ALU_AND | MMIO_ALU_OR | MMIO_ALU_XOR => mmio_test_rflags(old, result, size),
        MMIO_ALU_ADD => mmio_add_rflags(old, left, right, size),
        MMIO_ALU_ADC => mmio_adc_rflags(old, left, right, size),
        MMIO_ALU_SUB => mmio_cmp_rflags(old, left, right, size),
        MMIO_ALU_SBB => mmio_sbb_rflags(old, left, right, size),
        MMIO_ALU_NEG => mmio_cmp_rflags(old, 0, left, size),
        a if mmio_alu_is_shift(a) => mmio_shift_rflags(old, left, right, result, alu, size),
        _ => old,
    }
}

fn mmio_bit_width(size: u8) -> u32 {
    match size {
        1 => 8,
        2 => 16,
        4 => 32,
        _ => 64,
    }
}

fn mmio_shift_amt(count: u64, size: u8) -> u32 {
    let m = if size == 8 { 0x3f } else { 0x1f };
    (count as u32) & m
}

/// Group-2 shift/rotate. `count` is imm8 or CL; masked to 5/6 bits.
pub fn mmio_shift_apply(cur: u64, count: u64, alu: u8, size: u8, cf: bool) -> u64 {
    let mask = mmio_size_mask(size);
    let a = cur & mask;
    let n = mmio_shift_amt(count, size);
    if n == 0 {
        return a;
    }
    let w = mmio_bit_width(size);
    match alu {
        MMIO_ALU_SHL => {
            if n >= w {
                0
            } else {
                (a << n) & mask
            }
        }
        MMIO_ALU_SHR => {
            if n >= w {
                0
            } else {
                a >> n
            }
        }
        MMIO_ALU_SAR => {
            let s = mmio_as_signed(a, size);
            (s >> n.min(127)) as u64 & mask
        }
        MMIO_ALU_ROL => {
            let k = n % w;
            if k == 0 {
                a
            } else {
                ((a << k) | (a >> (w - k))) & mask
            }
        }
        MMIO_ALU_ROR => {
            let k = n % w;
            if k == 0 {
                a
            } else {
                ((a >> k) | (a << (w - k))) & mask
            }
        }
        MMIO_ALU_RCL => {
            let m = w + 1;
            let k = n % m;
            if k == 0 {
                a
            } else {
                let val = u128::from(a) | ((if cf { 1u128 } else { 0 }) << w);
                let rot = (val << k) | (val >> (m - k));
                (rot as u64) & mask
            }
        }
        MMIO_ALU_RCR => {
            let m = w + 1;
            let k = n % m;
            if k == 0 {
                a
            } else {
                let val = u128::from(a) | ((if cf { 1u128 } else { 0 }) << w);
                let rot = (val >> k) | (val << (m - k));
                (rot as u64) & mask
            }
        }
        _ => a,
    }
}

/// Group-2 RFLAGS. Count 0 leaves flags. OF defined only for count==1.
pub fn mmio_shift_rflags(
    old: u64,
    cur: u64,
    count: u64,
    result: u64,
    alu: u8,
    size: u8,
) -> u64 {
    let n = mmio_shift_amt(count, size);
    if n == 0 {
        return old;
    }
    let mask = mmio_size_mask(size);
    let a = cur & mask;
    let r = result & mask;
    let w = mmio_bit_width(size);
    let old_cf = (old & 1) != 0;
    let cf = match alu {
        MMIO_ALU_SHL => {
            if n > w {
                false
            } else if n == w {
                (a & 1) != 0
            } else {
                ((a >> (w - n)) & 1) != 0
            }
        }
        MMIO_ALU_SHR | MMIO_ALU_SAR => {
            if n > w {
                alu == MMIO_ALU_SAR && (a & mmio_sign_bit(size)) != 0
            } else {
                ((a >> (n - 1)) & 1) != 0
            }
        }
        MMIO_ALU_ROL => {
            let k = n % w;
            if k == 0 {
                (a & 1) != 0
            } else {
                (r & 1) != 0
            }
        }
        MMIO_ALU_ROR => {
            let k = n % w;
            if k == 0 {
                (a & mmio_sign_bit(size)) != 0
            } else {
                (r & mmio_sign_bit(size)) != 0
            }
        }
        MMIO_ALU_RCL => {
            let m = w + 1;
            let k = n % m;
            if k == 0 {
                old_cf
            } else {
                let val = u128::from(a) | ((if old_cf { 1u128 } else { 0 }) << w);
                let rot = (val << k) | (val >> (m - k));
                ((rot >> w) & 1) != 0
            }
        }
        MMIO_ALU_RCR => {
            let m = w + 1;
            let k = n % m;
            if k == 0 {
                old_cf
            } else {
                let val = u128::from(a) | ((if old_cf { 1u128 } else { 0 }) << w);
                let rot = (val >> k) | (val << (m - k));
                ((rot >> w) & 1) != 0
            }
        }
        _ => old_cf,
    };
    let mut f = match alu {
        MMIO_ALU_SHL | MMIO_ALU_SHR | MMIO_ALU_SAR => mmio_test_rflags(old, r, size),
        _ => (old | 2) & !(1 << 0),
    };
    if cf {
        f |= 1 << 0;
    } else {
        f &= !1;
    }
    if n == 1 {
        let of = match alu {
            MMIO_ALU_SHL | MMIO_ALU_ROL | MMIO_ALU_RCL => {
                ((r & mmio_sign_bit(size)) != 0) != cf
            }
            MMIO_ALU_SHR => (a & mmio_sign_bit(size)) != 0,
            MMIO_ALU_SAR => false,
            MMIO_ALU_ROR | MMIO_ALU_RCR => {
                let s = mmio_sign_bit(size);
                ((r & s) != 0) != ((r & (s >> 1)) != 0)
            }
            _ => false,
        };
        f &= !(1 << 11);
        if of {
            f |= 1 << 11;
        }
    }
    f
}

/// BT family: `bit` indexes `cur` of `size` bytes. Returns (new_value, old_bit).
pub fn mmio_bt_apply(cur: u64, bit: u64, size: u8, bt: u8) -> (u64, bool) {
    let width = u64::from(size) * 8;
    if width == 0 {
        return (cur, false);
    }
    let b = bit % width;
    let bitv = 1u64 << b;
    let was = (cur & bitv) != 0;
    let new = match bt {
        MMIO_BTS => cur | bitv,
        MMIO_BTR => cur & !bitv,
        MMIO_BTC => cur ^ bitv,
        _ => cur,
    };
    (new, was)
}

/// BT: CF = old bit; other flags unchanged (SDM: OF/SF/AF/PF undefined).
pub fn mmio_bt_rflags(old: u64, was_set: bool) -> u64 {
    if was_set {
        old | 1
    } else {
        old & !1
    }
}

fn mmio_mov(
    is_write: bool,
    size: u8,
    reg: u8,
    has_imm: bool,
    imm: u64,
    zero_ext: bool,
) -> MmioInsn {
    MmioInsn {
        is_write,
        size,
        reg,
        has_imm,
        imm,
        zero_ext,
        sign_ext: false,
        xchg: false,
        alu: 0,
        rex: false,
        test: false,
        cmp: false,
        cmp_reg_left: false,
        alu_reg_left: false,
        bt: 0,
        atomic: 0,
    }
}

/// Bytes fetchable from `gpa` without crossing a 4 KiB page. MMIO emulate
/// must loop when the instruction straddles a page.
pub fn mmio_insn_bytes_this_page(gpa: u64, want: usize) -> usize {
    want.min(page_left(gpa))
}

/// Decode MOV/MOVZX/MOVSX/XCHG/ALU RMW (mem or dest-reg)/TEST/CMP/INC/DEC/NOT/NEG/BT family/CMPXCHG/XADD that OVMF IoLib and Linux ioread use for virtio-pci BAR and xAPIC MMIO.
pub fn decode_mmio_insn(bytes: &[u8], insn_len: usize) -> Option<MmioInsn> {
    if bytes.is_empty() || insn_len == 0 || insn_len > bytes.len() || insn_len > 15 {
        return None;
    }
    let mut i = 0usize;
    let mut operand16 = false;
    let mut rex_w = false;
    let mut rex_r = 0u8;
    let mut rex = false;
    while i < insn_len {
        match bytes[i] {
            0x66 => {
                operand16 = true;
                i += 1;
            }
            0x26 | 0x2E | 0x36 | 0x3E | 0x64 | 0x65 | 0x67 | 0xF0 | 0xF2 | 0xF3 => i += 1,
            r if (0x40..=0x4F).contains(&r) => {
                rex = true;
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
        if op2 == 0xB6 || op2 == 0xB7 || op2 == 0xBE || op2 == 0xBF {
            if i >= insn_len {
                return None;
            }
            let m = bytes[i];
            let reg = ((m >> 3) & 7) | rex_r;
            let size = if op2 == 0xB6 || op2 == 0xBE { 1 } else { 2 };
            let sign_ext = op2 == 0xBE || op2 == 0xBF;
            return Some(MmioInsn {
                is_write: false,
                size,
                reg,
                has_imm: false,
                imm: 0,
                zero_ext: !sign_ext,
                sign_ext,
                xchg: false,
                alu: 0,
                rex,
                test: false,
                cmp: false,
                cmp_reg_left: false,
                alu_reg_left: false,
                bt: 0,
                atomic: 0,
            });
        }
        if op2 == 0xB0 || op2 == 0xB1 || op2 == 0xC0 || op2 == 0xC1 {
            if i >= insn_len {
                return None;
            }
            let m = bytes[i];
            let reg = ((m >> 3) & 7) | rex_r;
            let size = if op2 == 0xB0 || op2 == 0xC0 {
                1
            } else if rex_w {
                8
            } else if operand16 {
                2
            } else {
                4
            };
            return Some(MmioInsn {
                is_write: true,
                size,
                reg,
                has_imm: false,
                imm: 0,
                zero_ext: size == 4,
                sign_ext: false,
                xchg: false,
                alu: 0,
                rex,
                test: false,
                cmp: false,
                cmp_reg_left: false,
                alu_reg_left: false,
                bt: 0,
                atomic: if op2 == 0xC0 || op2 == 0xC1 {
                    MMIO_XADD
                } else {
                    MMIO_CMPXCHG
                },
            });
        }
        let size = if rex_w {
            8
        } else if operand16 {
            2
        } else {
            4
        };
        let bt = match op2 {
            0xA3 => MMIO_BT,
            0xAB => MMIO_BTS,
            0xB3 => MMIO_BTR,
            0xBB => MMIO_BTC,
            0xBA => {
                if i >= insn_len {
                    return None;
                }
                let m = bytes[i];
                match (m >> 3) & 7 {
                    4 => MMIO_BT,
                    5 => MMIO_BTS,
                    6 => MMIO_BTR,
                    7 => MMIO_BTC,
                    _ => return None,
                }
            }
            _ => return None,
        };
        if op2 == 0xBA {
            if insn_len < 1 {
                return None;
            }
            let imm = u64::from(bytes[insn_len - 1]);
            return Some(MmioInsn {
                is_write: bt != MMIO_BT,
                size,
                reg: 0,
                has_imm: true,
                imm,
                zero_ext: false,
                sign_ext: false,
                xchg: false,
                alu: 0,
                rex,
                test: false,
                cmp: false,
                cmp_reg_left: false,
                alu_reg_left: false,
                bt,
                atomic: 0,
            });
        }
        if i >= insn_len {
            return None;
        }
        let m = bytes[i];
        let reg = ((m >> 3) & 7) | rex_r;
        return Some(MmioInsn {
            is_write: bt != MMIO_BT,
            size,
            reg,
            has_imm: false,
            imm: 0,
            zero_ext: false,
            sign_ext: false,
            xchg: false,
            alu: 0,
            rex,
            test: false,
            cmp: false,
            cmp_reg_left: false,
            alu_reg_left: false,
            bt,
            atomic: 0,
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
            Some({
                let mut o = mmio_mov(is_write, size, reg, false, 0, size == 4);
                o.rex = rex;
                o
            })
        }
        0x86 | 0x87 => {
            if i >= insn_len {
                return None;
            }
            let m = bytes[i];
            let reg = ((m >> 3) & 7) | rex_r;
            let size = if op == 0x86 {
                1
            } else if rex_w {
                8
            } else if operand16 {
                2
            } else {
                4
            };
            Some(MmioInsn {
                is_write: true,
                size,
                reg,
                has_imm: false,
                imm: 0,
                zero_ext: size == 4,
                sign_ext: false,
                xchg: true,
                alu: 0,
                rex,
                test: false,
                cmp: false,
                cmp_reg_left: false,
                alu_reg_left: false,
                bt: 0,
                atomic: 0,
            })
        }
        0x84 | 0x85 => {
            if i >= insn_len {
                return None;
            }
            let m = bytes[i];
            let reg = ((m >> 3) & 7) | rex_r;
            let size = if op == 0x84 {
                1
            } else if rex_w {
                8
            } else if operand16 {
                2
            } else {
                4
            };
            Some(MmioInsn {
                is_write: false,
                size,
                reg,
                has_imm: false,
                imm: 0,
                zero_ext: false,
                sign_ext: false,
                xchg: false,
                alu: 0,
                rex,
                test: true,
                cmp: false,
                cmp_reg_left: false,
                alu_reg_left: false,
                bt: 0,
                atomic: 0,
            })
        }
        0x38 | 0x39 => {
            if i >= insn_len {
                return None;
            }
            let m = bytes[i];
            let reg = ((m >> 3) & 7) | rex_r;
            let size = if op == 0x38 {
                1
            } else if rex_w {
                8
            } else if operand16 {
                2
            } else {
                4
            };
            Some(MmioInsn {
                is_write: false,
                size,
                reg,
                has_imm: false,
                imm: 0,
                zero_ext: false,
                sign_ext: false,
                xchg: false,
                alu: 0,
                rex,
                test: false,
                cmp: true,
                cmp_reg_left: false,
                alu_reg_left: false,
                bt: 0,
                atomic: 0,
            })
        }
        0x3A | 0x3B => {
            if i >= insn_len {
                return None;
            }
            let m = bytes[i];
            let reg = ((m >> 3) & 7) | rex_r;
            let size = if op == 0x3A {
                1
            } else if rex_w {
                8
            } else if operand16 {
                2
            } else {
                4
            };
            Some(MmioInsn {
                is_write: false,
                size,
                reg,
                has_imm: false,
                imm: 0,
                zero_ext: false,
                sign_ext: false,
                xchg: false,
                alu: 0,
                rex,
                test: false,
                cmp: true,
                cmp_reg_left: true,
                alu_reg_left: false,
                bt: 0,
                atomic: 0,
            })
        }
        0xA0 | 0xA1 | 0xA2 | 0xA3 => {
            let is_write = op == 0xA2 || op == 0xA3;
            let size = if op == 0xA0 || op == 0xA2 {
                1
            } else if rex_w {
                8
            } else if operand16 {
                2
            } else {
                4
            };
            Some({
                let mut o = mmio_mov(is_write, size, 0, false, 0, size == 4 && !is_write);
                o.rex = rex;
                o
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
            Some({
                let mut o = mmio_mov(true, size, 0, true, imm, false);
                o.rex = rex;
                o
            })
        }
        0x00 | 0x01 | 0x02 | 0x03 | 0x08 | 0x09 | 0x0A | 0x0B | 0x10 | 0x11 | 0x12 | 0x13
        | 0x18 | 0x19 | 0x1A | 0x1B | 0x20 | 0x21 | 0x22 | 0x23 | 0x28 | 0x29 | 0x2A | 0x2B
        | 0x30 | 0x31 | 0x32 | 0x33 => {
            if i >= insn_len {
                return None;
            }
            let m = bytes[i];
            let alu = match op & 0x38 {
                0x20 => MMIO_ALU_AND,
                0x08 => MMIO_ALU_OR,
                0x10 => MMIO_ALU_ADC,
                0x18 => MMIO_ALU_SBB,
                0x28 => MMIO_ALU_SUB,
                0x30 => MMIO_ALU_XOR,
                _ => MMIO_ALU_ADD,
            };
            let dest_reg = (op & 2) != 0;
            let reg = ((m >> 3) & 7) | rex_r;
            let size = if (op & 1) == 0 {
                1
            } else if rex_w {
                8
            } else if operand16 {
                2
            } else {
                4
            };
            Some(MmioInsn {
                is_write: !dest_reg,
                size,
                reg,
                has_imm: false,
                imm: 0,
                zero_ext: dest_reg && size == 4,
                sign_ext: false,
                xchg: false,
                alu,
                rex,
                test: false,
                cmp: false,
                cmp_reg_left: false,
                alu_reg_left: dest_reg,
                bt: 0,
                atomic: 0,
            })
        }
        0x80 | 0x81 | 0x83 => {
            if i >= insn_len {
                return None;
            }
            let m = bytes[i];
            let ext = (m >> 3) & 7;
            let is_cmp = ext == 7;
            let alu = if is_cmp {
                0
            } else {
                match ext {
                    0 => MMIO_ALU_ADD,
                    1 => MMIO_ALU_OR,
                    2 => MMIO_ALU_ADC,
                    3 => MMIO_ALU_SBB,
                    4 => MMIO_ALU_AND,
                    5 => MMIO_ALU_SUB,
                    6 => MMIO_ALU_XOR,
                    _ => return None,
                }
            };
            let size = if op == 0x80 {
                1
            } else if operand16 {
                2
            } else {
                4
            };
            let imm_sz = if op == 0x83 { 1 } else { size };
            if insn_len < imm_sz as usize {
                return None;
            }
            let imm_off = insn_len - imm_sz as usize;
            let mut imm = 0u64;
            let mut k = 0u32;
            while k < u32::from(imm_sz) {
                imm |= u64::from(bytes[imm_off + k as usize]) << (8 * k);
                k += 1;
            }
            if op == 0x83 {
                let s = imm as i8 as i64;
                imm = match size {
                    1 => (s as u8) as u64,
                    2 => (s as i16 as u16) as u64,
                    _ => (s as i32 as u32) as u64,
                };
            }
            Some(MmioInsn {
                is_write: !is_cmp,
                size,
                reg: 0,
                has_imm: true,
                imm,
                zero_ext: false,
                sign_ext: false,
                xchg: false,
                alu,
                rex,
                test: false,
                cmp: is_cmp,
                cmp_reg_left: false,
                alu_reg_left: false,
                bt: 0,
                atomic: 0,
            })
        }
        0xF6 | 0xF7 => {
            if i >= insn_len {
                return None;
            }
            let m = bytes[i];
            let ext = (m >> 3) & 7;
            let size = if op == 0xF6 {
                1
            } else if operand16 {
                2
            } else {
                4
            };
            match ext {
                0 => {
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
                        is_write: false,
                        size,
                        reg: 0,
                        has_imm: true,
                        imm,
                        zero_ext: false,
                        sign_ext: false,
                        xchg: false,
                        alu: 0,
                        rex,
                        test: true,
                        cmp: false,
                        cmp_reg_left: false,
                        alu_reg_left: false,
                        bt: 0,
                        atomic: 0,
                    })
                }
                2 | 3 => Some(MmioInsn {
                    is_write: true,
                    size,
                    reg: 0,
                    has_imm: false,
                    imm: 0,
                    zero_ext: false,
                    sign_ext: false,
                    xchg: false,
                    alu: if ext == 2 { MMIO_ALU_NOT } else { MMIO_ALU_NEG },
                    rex,
                    test: false,
                    cmp: false,
                    cmp_reg_left: false,
                    alu_reg_left: false,
                    bt: 0,
                    atomic: 0,
                }),
                _ => None,
            }
        }
        0xFE | 0xFF => {
            if i >= insn_len {
                return None;
            }
            let m = bytes[i];
            let ext = (m >> 3) & 7;
            let alu = match ext {
                0 => MMIO_ALU_ADD,
                1 => MMIO_ALU_SUB,
                _ => return None,
            };
            let size = if op == 0xFE {
                1
            } else if rex_w {
                8
            } else if operand16 {
                2
            } else {
                4
            };
            Some(MmioInsn {
                is_write: true,
                size,
                reg: 0,
                has_imm: true,
                imm: 1,
                zero_ext: false,
                sign_ext: false,
                xchg: false,
                alu,
                rex,
                test: false,
                cmp: false,
                cmp_reg_left: false,
                alu_reg_left: false,
                bt: 0,
                atomic: 0,
            })
        }
        0xC0 | 0xC1 | 0xD0 | 0xD1 | 0xD2 | 0xD3 => {
            if i >= insn_len {
                return None;
            }
            let m = bytes[i];
            let ext = (m >> 3) & 7;
            let alu = match ext {
                0 => MMIO_ALU_ROL,
                1 => MMIO_ALU_ROR,
                2 => MMIO_ALU_RCL,
                3 => MMIO_ALU_RCR,
                4 | 6 => MMIO_ALU_SHL,
                5 => MMIO_ALU_SHR,
                _ => MMIO_ALU_SAR,
            };
            let size = if op == 0xC0 || op == 0xD0 || op == 0xD2 {
                1
            } else if rex_w {
                8
            } else if operand16 {
                2
            } else {
                4
            };
            let (has_imm, imm) = match op {
                0xD0 | 0xD1 => (true, 1u64),
                0xD2 | 0xD3 => (false, 0u64),
                _ => {
                    if insn_len < i + 2 {
                        return None;
                    }
                    (true, u64::from(bytes[insn_len - 1]))
                }
            };
            Some(MmioInsn {
                is_write: true,
                size,
                reg: 1,
                has_imm,
                imm,
                zero_ext: false,
                sign_ext: false,
                xchg: false,
                alu,
                rex,
                test: false,
                cmp: false,
                cmp_reg_left: false,
                alu_reg_left: false,
                bt: 0,
                atomic: 0,
            })
        }
        _ => None,
    }
}

#[cfg(test)]
#[path = "guest_virtio_blk_test.rs"]
mod guest_virtio_blk_test;
