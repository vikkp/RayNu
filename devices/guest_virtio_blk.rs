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
//! PIIX `00:01.1` is the same CD. iso=0 boot order is CD then disk.
//! product ISO fw_cfg bootorder virtio-iso scsi@3 first (empty scsi@2
//! last). product ISO fw_cfg bootorder El Torito ide@ first.
//! ConnectDevicesFromQemu listing is not enough: iron COM2
//! `d61dc7e` still ConnectAll-Started IdeBus. product ISO hides PIIX IDE.
//! Do **not**
//! move virtio to `00:00.0`.
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

/// Virtio 1.0 DEVICE_STATUS DRIVER_OK (bit 2 / value 4).
pub const VIRTIO_STATUS_DRIVER_OK: u8 = 4;

/// Linux kworker / `msleep` need PIT to beat UART until both product-ISO
/// virtio-blk functions (`00:02.0` install disk, `00:03.0` ISO) reach
/// DRIVER_OK. After that, UART must beat PIT so ttyS0 TX and Alpine
/// serial auto-answer work. Lab / iso=0 (queues off) is false.
/// linux PIT prefer until DRIVER_OK. Not `ISO-INSTALL-OK`.
pub fn virtio_needs_pit_over_uart() -> bool {
    with_box(|b| {
        fn pending(v: &VirtioPci) -> bool {
            v.queues_armed && (v.status & VIRTIO_STATUS_DRIVER_OK) == 0
        }
        pending(&b.disk) || pending(&b.iso)
    })
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

/// Disk then ISO BAR GPAs. 0 when that device is off / queues unarmed.
pub fn mmio_programmed_bar_gpas() -> [u64; 2] {
    [mmio_bar_base(), mmio_iso_bar_base()]
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

/// One little-endian byte of virtio-pci common cfg (QEMU packed layout).
///
/// Iron `deefa7c`: Linux ioread after BAR trap. A 32-bit load at 0x18 must
/// return `queue_msix_vector=0xFFFF` in the high half — 0 looks like MSI-X
/// vector 0 and the guest waits for a vector we never inject.
/// packed virtio common cfg. Not `ISO-INSTALL-OK`.
fn common_cfg_byte(v: &VirtioPci, off: u16) -> u8 {
    let qsize = if v.queue_sel == 0 { v.queue_size } else { 0 };
    let feat = features_for(v, v.feat_sel);
    let drv = if v.drv_feat_sel == 0 {
        v.drv_feat as u32
    } else {
        (v.drv_feat >> 32) as u32
    };
    match off {
        0x00..=0x03 => (v.feat_sel >> (8 * (off - 0x00))) as u8,
        0x04..=0x07 => (feat >> (8 * (off - 0x04))) as u8,
        0x08..=0x0B => (v.drv_feat_sel >> (8 * (off - 0x08))) as u8,
        0x0C..=0x0F => (drv >> (8 * (off - 0x0C))) as u8,
        0x10 | 0x11 => 0xFF,
        0x12 => 1,
        0x13 => 0,
        0x14 => v.status,
        0x15 => 0,
        0x16 => v.queue_sel as u8,
        0x17 => (v.queue_sel >> 8) as u8,
        0x18 => qsize as u8,
        0x19 => (qsize >> 8) as u8,
        0x1A | 0x1B => 0xFF,
        0x1C => v.queue_enable as u8,
        0x1D => (v.queue_enable >> 8) as u8,
        0x1E | 0x1F => 0,
        0x20..=0x27 => (v.queue_desc >> (8 * (off - 0x20))) as u8,
        0x28..=0x2F => (v.queue_driver >> (8 * (off - 0x28))) as u8,
        0x30..=0x37 => (v.queue_device >> (8 * (off - 0x30))) as u8,
        _ => 0,
    }
}

fn common_cfg_read(v: &VirtioPci, off: u16, size: u8) -> u64 {
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
        val |= u64::from(common_cfg_byte(v, off.wrapping_add(i))) << (8 * i);
        i += 1;
    }
    val
}

fn mmio_read_locked(v: &mut VirtioPci, off: u16, size: u8) -> u64 {
    if !v.queues_armed {
        return 0;
    }
    let cap = capacity_sectors_for(v);
    if off < 0x38 {
        return common_cfg_read(v, off, size);
    }
    let val = match off {
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

/// One little-endian byte of a virtio-pci common-cfg store (QEMU packed).
///
/// Iron `deefa7c`: Linux iowrite after BAR trap. A 32-bit store at 0x14
/// used to match only `device_status` and drop `queue_select`; a 1-byte
/// store at 0x00 zeroed `device_feature_select` high bytes. Packed writes
/// RMW each field. packed virtio common cfg write. Not `ISO-INSTALL-OK`.
fn common_cfg_write_byte(v: &mut VirtioPci, off: u16, b: u8) {
    match off {
        0x00..=0x03 => {
            let sh = 8 * (off - 0x00);
            v.feat_sel = (v.feat_sel & !(0xffu32 << sh)) | (u32::from(b) << sh);
        }
        0x08..=0x0B => {
            let sh = 8 * (off - 0x08);
            v.drv_feat_sel = (v.drv_feat_sel & !(0xffu32 << sh)) | (u32::from(b) << sh);
        }
        0x0C..=0x0F => {
            let sh = 8 * (off - 0x0C);
            if v.drv_feat_sel == 0 {
                let lo = (v.drv_feat as u32 & !(0xffu32 << sh)) | (u32::from(b) << sh);
                v.drv_feat = (v.drv_feat & !0xFFFF_FFFF) | u64::from(lo);
            } else {
                let hi = ((v.drv_feat >> 32) as u32 & !(0xffu32 << sh)) | (u32::from(b) << sh);
                v.drv_feat = (v.drv_feat & 0xFFFF_FFFF) | (u64::from(hi) << 32);
            }
        }
        0x14 => {
            v.status = b;
            if v.status == 0 {
                v.queue_enable = 0;
                v.last_avail = 0;
                v.notify_pending = false;
                v.isr = 0;
            }
        }
        0x16 | 0x17 => {
            let sh = 8 * (off - 0x16);
            v.queue_sel = (v.queue_sel & !(0xffu16 << sh)) | (u16::from(b) << sh);
        }
        0x18 | 0x19 => {
            if v.queue_sel == 0 {
                let sh = 8 * (off - 0x18);
                let n = (v.queue_size & !(0xffu16 << sh)) | (u16::from(b) << sh);
                if n > 0 && n <= QUEUE_MAX {
                    v.queue_size = n;
                }
            }
        }
        0x1C | 0x1D => {
            let sh = 8 * (off - 0x1C);
            v.queue_enable = (v.queue_enable & !(0xffu16 << sh)) | (u16::from(b) << sh);
        }
        0x20..=0x27 => {
            let sh = 8 * (off - 0x20);
            v.queue_desc = (v.queue_desc & !(0xffu64 << sh)) | (u64::from(b) << sh);
        }
        0x28..=0x2F => {
            let sh = 8 * (off - 0x28);
            v.queue_driver = (v.queue_driver & !(0xffu64 << sh)) | (u64::from(b) << sh);
        }
        0x30..=0x37 => {
            let sh = 8 * (off - 0x30);
            v.queue_device = (v.queue_device & !(0xffu64 << sh)) | (u64::from(b) << sh);
        }
        _ => {}
    }
}

fn mmio_write_locked(v: &mut VirtioPci, off: u16, size: u8, val: u64) {
    if !v.queues_armed {
        return;
    }
    if off < 0x38 {
        let n = match size {
            1 => 1u16,
            2 => 2,
            4 => 4,
            8 => 8,
            _ => 4,
        };
        let mut i = 0u16;
        while i < n {
            common_cfg_write_byte(v, off.wrapping_add(i), (val >> (8 * i)) as u8);
            i += 1;
        }
        return;
    }
    if off == OFF_NOTIFY {
        v.notify_pending = true;
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

/// One virtio-blk sector of OUT. The gate is a GPT/MBR write, not a 16KiB
/// floor: Alpine `setup-disk` with `BOOT_SIZE=48` on a 64MiB disk can write
/// the table then fail apk before 16KiB OUT. ISO-INSTALL-OK on GPT not 16KiB.
pub const ISO_INSTALL_OK_MIN_OUT: u64 = 512;

/// Iron-only: product queues + partition table + at least one sector OUT.
/// Caller prints [`crate::mgmt::iso_install::M7_ISO_INSTALL_OK_MARKER`].
/// Host/CI / nested must not call this print path.
pub fn take_iso_install_ok() -> bool {
    if ISO_OK.load(Ordering::Acquire) {
        return false;
    }
    if !queues_armed() || disk_bytes_written() < ISO_INSTALL_OK_MIN_OUT {
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
    /// 10=ROL, 11=ROR, 12=RCL, 13=RCR, 14=SHL, 15=SHR, 16=SAR, 17=BSF, 18=BSR,
    /// 19=hint (PREFETCH/NOP/CLFLUSH: skip, do not touch the BAR), 20=IMUL
    /// dest-reg, 21=MUL DX:AX, 22=one-operand IMUL DX:AX, 23=DIV, 24=IDIV,
    /// 25=SHLD, 26=SHRD, 27=TZCNT, 28=LZCNT, 29=POPCNT, 30=PUSH r/m, 31=POP r/m,
    /// 32=MOVS, 33=STOS, 34=LODS (`has_imm` is F3 REP), 35=CALL r/m (`FF /2`),
    /// 36=JMP r/m (`FF /4`). Far CALLF/JMPF (`/3` `/5`) stay decode-fail.
    /// 37=CMPS, 38=SCAS (`has_imm` is REP; `imm!=0` is F2 REPNE).
    /// 39=SSE MOVUPS/MOVUPD/MOVSS/MOVSD/MOVDQU/MOVDQA/MOVAPS/MOVAPD (`reg` is xmm).
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
    /// 0=none, 1=CMPXCHG, 2=XADD, 3=CMPXCHG8B (`0F C7 /1`, m64).
    pub atomic: u8,
    /// 0=none; `0x40..=0x4F` CMOVcc r, r/m; `0x90..=0x9F` SETcc r/m8.
    pub cc: u8,
}

pub const MMIO_CMPXCHG: u8 = 1;
pub const MMIO_XADD: u8 = 2;
/// CMPXCHG8B m64 (`0F C7 /1`). Not CMPXCHG16B (REX.W).
pub const MMIO_CMPXCHG8B: u8 = 3;

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
pub const MMIO_ALU_BSF: u8 = 17;
pub const MMIO_ALU_BSR: u8 = 18;
/// PREFETCH / multi-byte NOP / CLFLUSH: skip the insn, do not access MMIO.
pub const MMIO_ALU_HINT: u8 = 19;
/// IMUL r, r/m (`0F AF`) or IMUL r, r/m, imm (`69`/`6B`). Dest is the GPR.
pub const MMIO_ALU_IMUL: u8 = 20;
/// F6/F7 /4 MUL r/m: product in AX (byte) or DX:AX.
pub const MMIO_ALU_MUL: u8 = 21;
/// F6/F7 /5 IMUL r/m (one-operand): product in AX or DX:AX.
pub const MMIO_ALU_IMUL1: u8 = 22;
/// F6/F7 /6 DIV r/m: quotient AX, remainder DX (byte: AH:AL).
pub const MMIO_ALU_DIV: u8 = 23;
/// F6/F7 /7 IDIV r/m.
pub const MMIO_ALU_IDIV: u8 = 24;
/// SHLD r/m, r, imm8/CL (`0F A4`/`A5`). Dest is MMIO.
pub const MMIO_ALU_SHLD: u8 = 25;
/// SHRD r/m, r, imm8/CL (`0F AC`/`AD`). Dest is MMIO.
pub const MMIO_ALU_SHRD: u8 = 26;
/// TZCNT r, r/m (`F3 0F BC`). Dest is the GPR. Src 0 writes bitwidth, CF=1.
pub const MMIO_ALU_TZCNT: u8 = 27;
/// LZCNT r, r/m (`F3 0F BD`). Dest is the GPR. Src 0 writes bitwidth, CF=1.
pub const MMIO_ALU_LZCNT: u8 = 28;
/// POPCNT r, r/m (`F3 0F B8`). Dest is the GPR.
pub const MMIO_ALU_POPCNT: u8 = 29;
/// PUSH r/m (`FF /6`). Reads MMIO, pushes onto the guest stack.
pub const MMIO_ALU_PUSH: u8 = 30;
/// POP r/m (`8F /0`). Pops the guest stack into MMIO.
pub const MMIO_ALU_POP: u8 = 31;

pub fn mmio_alu_is_shift(alu: u8) -> bool {
    (MMIO_ALU_ROL..=MMIO_ALU_SAR).contains(&alu)
}

pub fn mmio_alu_is_double_shift(alu: u8) -> bool {
    alu == MMIO_ALU_SHLD || alu == MMIO_ALU_SHRD
}

pub fn mmio_alu_is_scan(alu: u8) -> bool {
    alu == MMIO_ALU_BSF || alu == MMIO_ALU_BSR
}

pub fn mmio_alu_is_count_zero(alu: u8) -> bool {
    alu == MMIO_ALU_TZCNT || alu == MMIO_ALU_LZCNT
}

pub fn mmio_alu_is_popcnt(alu: u8) -> bool {
    alu == MMIO_ALU_POPCNT
}

pub fn mmio_alu_is_push(alu: u8) -> bool {
    alu == MMIO_ALU_PUSH
}

pub fn mmio_alu_is_pop(alu: u8) -> bool {
    alu == MMIO_ALU_POP
}

/// MOVS (`A4`/`A5`). Src [RSI] or dest [RDI] is MMIO; EPT qual says which.
pub const MMIO_ALU_MOVS: u8 = 32;
/// STOS (`AA`/`AB`). Stores AL/AX/EAX/RAX to [RDI] MMIO.
pub const MMIO_ALU_STOS: u8 = 33;
/// LODS (`AC`/`AD`). Loads MMIO [RSI] into AL/AX/EAX/RAX.
pub const MMIO_ALU_LODS: u8 = 34;

pub fn mmio_alu_is_string(alu: u8) -> bool {
    (MMIO_ALU_MOVS..=MMIO_ALU_LODS).contains(&alu)
        || alu == MMIO_ALU_CMPS
        || alu == MMIO_ALU_SCAS
}

pub fn mmio_alu_is_movs(alu: u8) -> bool {
    alu == MMIO_ALU_MOVS
}

pub fn mmio_alu_is_stos(alu: u8) -> bool {
    alu == MMIO_ALU_STOS
}

pub fn mmio_alu_is_lods(alu: u8) -> bool {
    alu == MMIO_ALU_LODS
}

/// Near CALL r/m (`FF /2`). Reads MMIO as the target; handler pushes RIP+len.
pub const MMIO_ALU_CALL: u8 = 35;
/// Near JMP r/m (`FF /4`). Reads MMIO as the target; handler sets RIP.
pub const MMIO_ALU_JMP: u8 = 36;

pub fn mmio_alu_is_call(alu: u8) -> bool {
    alu == MMIO_ALU_CALL
}

pub fn mmio_alu_is_jmp(alu: u8) -> bool {
    alu == MMIO_ALU_JMP
}

/// CMPS (`A6`/`A7`). Compares [RSI] to [RDI]; one side is MMIO.
pub const MMIO_ALU_CMPS: u8 = 37;
/// SCAS (`AE`/`AF`). Compares AL/AX/EAX/RAX to [RDI] MMIO.
pub const MMIO_ALU_SCAS: u8 = 38;

pub fn mmio_alu_is_cmps(alu: u8) -> bool {
    alu == MMIO_ALU_CMPS
}

pub fn mmio_alu_is_scas(alu: u8) -> bool {
    alu == MMIO_ALU_SCAS
}

/// SSE packed/scalar move targeting MMIO. `reg` is xmm0–15, not a GPR.
pub const MMIO_ALU_SSE: u8 = 39;

pub fn mmio_alu_is_sse(alu: u8) -> bool {
    alu == MMIO_ALU_SSE
}

/// MOVSS/MOVSD from mem zero the rest of the XMM. Packed 16-byte keeps all bits.
pub fn mmio_sse_from_mem(mem: u128, size: u8) -> u128 {
    match size {
        4 => mem & 0xffff_ffff,
        8 => mem & u128::from(u64::MAX),
        _ => mem,
    }
}

/// Read 4/8/16 bytes of virtio BAR MMIO as an XMM payload.
pub fn mmio_read_sse_at(gpa: u64, size: u8) -> u128 {
    match size {
        4 => u128::from(mmio_read_at(gpa, 4)),
        8 => u128::from(mmio_read_at(gpa, 8)),
        _ => {
            let lo = u128::from(mmio_read_at(gpa, 8));
            let hi = u128::from(mmio_read_at(gpa.wrapping_add(8), 8));
            lo | (hi << 64)
        }
    }
}

/// Write the low 4/8/16 bytes of an XMM payload into virtio BAR MMIO.
pub fn mmio_write_sse_at(gpa: u64, val: u128, size: u8) {
    match size {
        4 => mmio_write_at(gpa, 4, val as u64),
        8 => mmio_write_at(gpa, 8, val as u64),
        _ => {
            mmio_write_at(gpa, 8, val as u64);
            mmio_write_at(gpa.wrapping_add(8), 8, (val >> 64) as u64);
        }
    }
}

/// PUSH/POP width: 66h → 16-bit; long mode → 64-bit; else 32-bit.
pub fn mmio_stack_width(decoded_size: u8, long: bool) -> u8 {
    if decoded_size == 2 {
        2
    } else if long {
        8
    } else {
        4
    }
}

pub fn mmio_alu_is_hint(alu: u8) -> bool {
    alu == MMIO_ALU_HINT
}

pub fn mmio_alu_is_imul(alu: u8) -> bool {
    alu == MMIO_ALU_IMUL
}

pub fn mmio_alu_is_mul_pair(alu: u8) -> bool {
    alu == MMIO_ALU_MUL || alu == MMIO_ALU_IMUL1
}

pub fn mmio_alu_is_div_pair(alu: u8) -> bool {
    alu == MMIO_ALU_DIV || alu == MMIO_ALU_IDIV
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

/// CMOVcc / SETcc: `cc` is 1..=16 for opcodes `0x40..=0x4F` / `0x90..=0x9F`.
pub fn mmio_cc_taken(cc: u8, flags: u64) -> bool {
    if cc == 0 || cc > 16 {
        return false;
    }
    let n = cc - 1;
    let cf = (flags & 1) != 0;
    let pf = (flags & (1 << 2)) != 0;
    let zf = (flags & (1 << 6)) != 0;
    let sf = (flags & (1 << 7)) != 0;
    let of = (flags & (1 << 11)) != 0;
    match n {
        0 => of,
        1 => !of,
        2 => cf,
        3 => !cf,
        4 => zf,
        5 => !zf,
        6 => cf || zf,
        7 => !cf && !zf,
        8 => sf,
        9 => !sf,
        10 => pf,
        11 => !pf,
        12 => sf != of,
        13 => sf == of,
        14 => zf || sf != of,
        _ => !zf && sf == of,
    }
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

/// SHLD/SHRD: dest is MMIO, `src` fills vacated bits, `count` is imm8 or CL.
pub fn mmio_double_shift_apply(dest: u64, src: u64, count: u64, alu: u8, size: u8) -> u64 {
    let mask = mmio_size_mask(size);
    let a = dest & mask;
    let s = src & mask;
    let n = mmio_shift_amt(count, size);
    if n == 0 {
        return a;
    }
    let w = mmio_bit_width(size);
    let concat = if alu == MMIO_ALU_SHLD {
        (u128::from(a) << w) | u128::from(s)
    } else {
        (u128::from(s) << w) | u128::from(a)
    };
    let shifted = if alu == MMIO_ALU_SHLD {
        concat << n
    } else {
        concat >> n
    };
    let result = if alu == MMIO_ALU_SHLD {
        (shifted >> w) as u64
    } else {
        shifted as u64
    };
    result & mask
}

/// SHLD/SHRD RFLAGS. Count 0 leaves flags. OF defined only for count==1.
pub fn mmio_double_shift_rflags(
    old: u64,
    dest: u64,
    src: u64,
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
    let a = dest & mask;
    let s = src & mask;
    let r = result & mask;
    let w = mmio_bit_width(size);
    let concat = if alu == MMIO_ALU_SHLD {
        (u128::from(a) << w) | u128::from(s)
    } else {
        (u128::from(s) << w) | u128::from(a)
    };
    let cf = if alu == MMIO_ALU_SHLD {
        ((concat >> (2 * w - n)) & 1) != 0
    } else {
        ((concat >> (n - 1)) & 1) != 0
    };
    let mut f = mmio_test_rflags(old, r, size);
    if cf {
        f |= 1 << 0;
    } else {
        f &= !1;
    }
    if n == 1 {
        let sign = mmio_sign_bit(size);
        let of = if alu == MMIO_ALU_SHLD {
            ((r & sign) != 0) != cf
        } else {
            ((a & sign) != 0) != ((r & sign) != 0)
        };
        f &= !(1 << 11);
        if of {
            f |= 1 << 11;
        }
    }
    f
}

/// CMPXCHG8B: match stores `ecx_ebx`, else mem is unchanged. Returns (mem_out, matched).
pub fn mmio_cmpxchg8b_apply(mem: u64, edx_eax: u64, ecx_ebx: u64) -> (u64, bool) {
    if mem == edx_eax {
        (ecx_ebx, true)
    } else {
        (mem, false)
    }
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

/// BSF/BSR of `src`. Returns (bit_index, src_was_zero). Dest is unchanged when
/// `src_was_zero` (AMD; Intel dest undefined — Linux only inspects ZF).
pub fn mmio_scan_apply(src: u64, size: u8, bsr: bool) -> (u64, bool) {
    let v = src & mmio_size_mask(size);
    if v == 0 {
        return (0, true);
    }
    let idx = if bsr {
        63u32.saturating_sub(v.leading_zeros())
    } else {
        v.trailing_zeros()
    };
    (u64::from(idx), false)
}

/// TZCNT: trailing zeros, or operand bitwidth when `src` is 0.
pub fn mmio_tzcnt_apply(src: u64, size: u8) -> (u64, bool) {
    let w = mmio_bit_width(size);
    let v = src & mmio_size_mask(size);
    if v == 0 {
        (u64::from(w), true)
    } else {
        (u64::from(v.trailing_zeros()), false)
    }
}

/// LZCNT: leading zeros in the operand width, or bitwidth when `src` is 0.
pub fn mmio_lzcnt_apply(src: u64, size: u8) -> (u64, bool) {
    let w = mmio_bit_width(size);
    let v = src & mmio_size_mask(size);
    if v == 0 {
        (u64::from(w), true)
    } else {
        (u64::from(v.leading_zeros() - (64u32 - w)), false)
    }
}

/// TZCNT/LZCNT: CF = (src == 0), ZF = (result == 0). Other flags undefined.
pub fn mmio_tzcnt_rflags(old: u64, result: u64, src_zero: bool) -> u64 {
    const CF: u64 = 1;
    const ZF: u64 = 1 << 6;
    let mut f = (old | 2) & !(CF | ZF);
    if src_zero {
        f |= CF;
    }
    if result == 0 {
        f |= ZF;
    }
    f
}

/// POPCNT of `src` truncated to `size`.
pub fn mmio_popcnt_apply(src: u64, size: u8) -> u64 {
    u64::from((src & mmio_size_mask(size)).count_ones())
}

/// POPCNT: ZF = (src == 0). Other flags undefined — leave them.
pub fn mmio_popcnt_rflags(old: u64, src_zero: bool) -> u64 {
    const ZF: u64 = 1 << 6;
    if src_zero {
        old | ZF
    } else {
        old & !ZF
    }
}

/// BSF/BSR: ZF = (src == 0). Other status flags undefined — leave them.
pub fn mmio_scan_rflags(old: u64, src_zero: bool) -> u64 {
    const ZF: u64 = 1 << 6;
    if src_zero {
        old | ZF
    } else {
        old & !ZF
    }
}

/// Signed IMUL truncated to `size`. Returns (result, CF/OF overflow).
pub fn mmio_imul_apply(a: u64, b: u64, size: u8) -> (u64, bool) {
    let prod = mmio_as_signed(a, size) * mmio_as_signed(b, size);
    let result = (prod as u64) & mmio_size_mask(size);
    let fit = mmio_as_signed(result, size) == prod;
    (result, !fit)
}

/// IMUL: CF = OF = truncated. Other status flags undefined — leave them.
pub fn mmio_imul_rflags(old: u64, overflow: bool) -> u64 {
    const CF: u64 = 1;
    const OF: u64 = 1 << 11;
    let mut f = (old | 2) & !(CF | OF);
    if overflow {
        f |= CF | OF;
    }
    f
}

/// One-operand MUL/IMUL. Returns (lo AX, hi DX, overflow). Byte form uses only `lo` as AX.
pub fn mmio_mul_pair_apply(ax: u64, mem: u64, size: u8, signed: bool) -> (u64, u64, bool) {
    let mask = mmio_size_mask(size);
    let a = ax & mask;
    let b = mem & mask;
    if size == 1 {
        if signed {
            let prod = i16::from(a as u8 as i8).wrapping_mul(i16::from(b as u8 as i8));
            let lo = prod as u16 as u64;
            let ah = (lo >> 8) & 0xff;
            let sign = if (lo & 0x80) != 0 { 0xff } else { 0 };
            (lo, 0, ah != sign)
        } else {
            let prod = a.wrapping_mul(b);
            (prod & 0xffff, 0, prod > 0xff)
        }
    } else {
        let width = u32::from(size) * 8;
        if signed {
            let prod = mmio_as_signed(a, size) * mmio_as_signed(b, size);
            let raw = prod as u128;
            let lo = (raw as u64) & mask;
            let hi = ((raw >> width) as u64) & mask;
            (lo, hi, mmio_as_signed(lo, size) != prod)
        } else {
            let prod = u128::from(a) * u128::from(b);
            let lo = (prod as u64) & mask;
            let hi = ((prod >> width) as u64) & mask;
            (lo, hi, hi != 0)
        }
    }
}

/// Unsigned DIV or signed IDIV. `None` = #DE (divisor 0 or quotient overflow).
/// Byte: dividend AX, quot AL, rem AH packed in `lo`. Wider: dividend DX:AX.
pub fn mmio_div_apply(
    ax: u64,
    dx: u64,
    mem: u64,
    size: u8,
    signed: bool,
) -> Option<(u64, u64)> {
    match size {
        1 => {
            let dividend = ax & 0xffff;
            let divisor = mem & 0xff;
            if divisor == 0 {
                return None;
            }
            if signed {
                let d = divisor as i8 as i16;
                let n = dividend as i16;
                if d == -1 && n == i16::MIN {
                    return None;
                }
                let q = n / d;
                let r = n % d;
                if q < i8::MIN as i16 || q > i8::MAX as i16 {
                    return None;
                }
                Some(((q as u8 as u64) | ((r as u8 as u64) << 8), 0))
            } else {
                let q = dividend / divisor;
                let r = dividend % divisor;
                if q > 0xff {
                    return None;
                }
                Some((q | (r << 8), 0))
            }
        }
        2 => {
            let dividend = ((dx & 0xffff) << 16) | (ax & 0xffff);
            let divisor = mem & 0xffff;
            if divisor == 0 {
                return None;
            }
            if signed {
                let d = divisor as i16 as i32;
                let n = dividend as i32;
                if d == -1 && n == i32::MIN {
                    return None;
                }
                let q = n / d;
                let r = n % d;
                if q < i16::MIN as i32 || q > i16::MAX as i32 {
                    return None;
                }
                Some((q as u16 as u64, r as u16 as u64))
            } else {
                let q = dividend / divisor;
                let r = dividend % divisor;
                if q > 0xffff {
                    return None;
                }
                Some((q, r))
            }
        }
        4 => {
            let dividend = ((dx & 0xffff_ffff) << 32) | (ax & 0xffff_ffff);
            let divisor = mem & 0xffff_ffff;
            if divisor == 0 {
                return None;
            }
            if signed {
                let d = divisor as i32 as i64;
                let n = dividend as i64;
                if d == -1 && n == i64::MIN {
                    return None;
                }
                let q = n / d;
                let r = n % d;
                if q < i32::MIN as i64 || q > i32::MAX as i64 {
                    return None;
                }
                Some((q as u32 as u64, r as u32 as u64))
            } else {
                let q = dividend / divisor;
                let r = dividend % divisor;
                if q > 0xffff_ffff {
                    return None;
                }
                Some((q, r))
            }
        }
        8 => {
            if mem == 0 {
                return None;
            }
            if signed {
                if mem as i64 == -1 && dx == 0x8000_0000_0000_0000 && ax == 0 {
                    return None;
                }
                let n = (i128::from(dx as i64) << 64) | i128::from(ax as u64);
                let d = i128::from(mem as i64);
                let q = n / d;
                let r = n % d;
                if q < i64::MIN as i128 || q > i64::MAX as i128 {
                    return None;
                }
                Some((q as u64, r as u64))
            } else {
                let n = (u128::from(dx) << 64) | u128::from(ax);
                let d = u128::from(mem);
                let q = n / d;
                let r = n % d;
                if q > u64::MAX as u128 {
                    return None;
                }
                Some((q as u64, r as u64))
            }
        }
        _ => None,
    }
}

/// #DE VM-entry interruption info: vector 0, type 3 (hw exception), valid bit 31.
/// No error code (bit 11 = 0). SDM Vol. 3C 24.8.3.
pub const MMIO_DIV_DE_INTR_INFO: u64 = 0 | (3 << 8) | (1 << 31);

fn mmio_hint() -> MmioInsn {
    MmioInsn {
        is_write: false,
        size: 1,
        reg: 0,
        has_imm: false,
        imm: 0,
        zero_ext: false,
        sign_ext: false,
        xchg: false,
        alu: MMIO_ALU_HINT,
        rex: false,
        test: false,
        cmp: false,
        cmp_reg_left: false,
        alu_reg_left: false,
        bt: 0,
        atomic: 0,
        cc: 0,
    }
}

/// MOVUPS (`0F 10`/`11`), MOVSS (`F3 0F 10`/`11`), MOVSD (`F2 0F 10`/`11`),
/// MOVDQU (`F3 0F 6F`/`7F`), MOVDQA (`66 0F 6F`/`7F`), MOVAPS (`0F 28`/`29`).
fn mmio_sse_op(
    op2: u8,
    f2: bool,
    f3: bool,
    operand16: bool,
    rex: bool,
    rex_r: u8,
    bytes: &[u8],
    i: usize,
    insn_len: usize,
) -> Option<MmioInsn> {
    if f2 && f3 {
        return None;
    }
    let (is_write, size) = match op2 {
        0x10 | 0x11 => {
            let size = if f2 {
                8
            } else if f3 {
                4
            } else {
                16
            };
            (op2 == 0x11, size)
        }
        0x6F | 0x7F => {
            if f2 || (!f3 && !operand16) {
                return None;
            }
            (op2 == 0x7F, 16)
        }
        0x28 | 0x29 => {
            if f2 || f3 {
                return None;
            }
            (op2 == 0x29, 16)
        }
        _ => return None,
    };
    if i >= insn_len {
        return None;
    }
    let m = bytes[i];
    let xmm = ((m >> 3) & 7) | rex_r;
    Some(MmioInsn {
        is_write,
        size,
        reg: xmm,
        has_imm: false,
        imm: 0,
        zero_ext: false,
        sign_ext: false,
        xchg: false,
        alu: MMIO_ALU_SSE,
        rex,
        test: false,
        cmp: false,
        cmp_reg_left: false,
        alu_reg_left: false,
        bt: 0,
        atomic: 0,
        cc: 0,
    })
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
        cc: 0,
    }
}

/// Bytes fetchable from `gpa` without crossing a 4 KiB page. MMIO emulate
/// must loop when the instruction straddles a page.
pub fn mmio_insn_bytes_this_page(gpa: u64, want: usize) -> usize {
    want.min(page_left(gpa))
}

/// ModR/M + optional SIB + displacement. Not the opcode.
pub fn mmio_modrm_span(rest: &[u8], addr16: bool) -> Option<usize> {
    if rest.is_empty() {
        return None;
    }
    let m = rest[0];
    let md = m >> 6;
    let rm = m & 7;
    if addr16 {
        let disp = match (md, rm) {
            (3, _) => 0,
            (0, 6) => 2,
            (1, _) => 1,
            (2, _) => 2,
            (0, _) => 0,
            _ => return None,
        };
        let n = 1 + disp;
        return (rest.len() >= n).then_some(n);
    }
    let sib = md != 3 && rm == 4;
    if sib && rest.len() < 2 {
        return None;
    }
    let disp = match md {
        3 => 0,
        1 => 1,
        2 => 4,
        0 => {
            if rm == 5 || (sib && (rest[1] & 7) == 5) {
                4
            } else {
                0
            }
        }
        _ => return None,
    };
    let n = 1 + if sib { 1 } else { 0 } + disp;
    (rest.len() >= n).then_some(n)
}

fn mmio_len_after_modrm(rest: &[u8], addr16: bool, imm: usize) -> Option<usize> {
    let n = mmio_modrm_span(rest, addr16)?;
    let total = n.checked_add(imm)?;
    (rest.len() >= total).then_some(total)
}

/// Instruction length from fetched bytes when VMCS `insn_len` is 0.
///
/// Iron COM2 may fetch flash bytes at `rip=0xfffcfc86` while the VMCS length
/// field is undefined. Do not skip `fetched_n` (that is a 16-byte peek).
pub fn mmio_decoded_len(bytes: &[u8], long64: bool) -> Option<usize> {
    let max = bytes.len().min(15);
    if max == 0 {
        return None;
    }
    let b = &bytes[..max];
    let mut i = 0usize;
    let mut operand16 = false;
    let mut addr16 = false;
    let mut rex_w = false;
    while i < max {
        match b[i] {
            0x66 => {
                operand16 = true;
                i += 1;
            }
            0x67 => {
                addr16 = !long64;
                i += 1;
            }
            0xF2 | 0xF3 | 0x26 | 0x2E | 0x36 | 0x3E | 0x64 | 0x65 | 0xF0 => i += 1,
            r if long64 && (0x40..=0x4F).contains(&r) => {
                rex_w = (r & 8) != 0;
                i += 1;
            }
            _ => break,
        }
    }
    if i >= max {
        return None;
    }
    let op = b[i];
    i += 1;
    let opsz = if rex_w {
        4
    } else if operand16 {
        2
    } else {
        4
    };
    let moffs = if long64 && !addr16 {
        8
    } else if addr16 {
        2
    } else {
        4
    };
    let rest = &b[i..];
    let body = if op == 0x0F {
        if rest.is_empty() {
            return None;
        }
        let op2 = rest[0];
        let after = &rest[1..];
        let extra = match op2 {
            0xA4 | 0xAC | 0xBA => 1,
            _ => 0,
        };
        1 + mmio_len_after_modrm(after, addr16, extra)?
    } else {
        match op {
            0xA4 | 0xA5 | 0xA6 | 0xA7 | 0xAA | 0xAB | 0xAC | 0xAD | 0xAE | 0xAF => 0,
            0xA0 | 0xA1 | 0xA2 | 0xA3 => {
                if rest.len() < moffs {
                    return None;
                }
                moffs
            }
            0xC6 => mmio_len_after_modrm(rest, addr16, 1)?,
            0xC7 => mmio_len_after_modrm(rest, addr16, opsz)?,
            0x80 | 0x82 | 0x83 | 0xC0 | 0xC1 | 0x6B => mmio_len_after_modrm(rest, addr16, 1)?,
            0x81 | 0x69 => mmio_len_after_modrm(rest, addr16, opsz)?,
            0xF6 => {
                if rest.is_empty() {
                    return None;
                }
                let ext = (rest[0] >> 3) & 7;
                mmio_len_after_modrm(rest, addr16, if ext <= 1 { 1 } else { 0 })?
            }
            0xF7 => {
                if rest.is_empty() {
                    return None;
                }
                let ext = (rest[0] >> 3) & 7;
                mmio_len_after_modrm(rest, addr16, if ext <= 1 { opsz } else { 0 })?
            }
            0x88 | 0x89 | 0x8A | 0x8B | 0x86 | 0x87 | 0x84 | 0x85 | 0x8F | 0xFE | 0xFF
            | 0x00 | 0x01 | 0x02 | 0x03 | 0x08 | 0x09 | 0x0A | 0x0B | 0x10 | 0x11 | 0x12
            | 0x13 | 0x18 | 0x19 | 0x1A | 0x1B | 0x20 | 0x21 | 0x22 | 0x23 | 0x28 | 0x29
            | 0x2A | 0x2B | 0x30 | 0x31 | 0x32 | 0x33 | 0x38 | 0x39 | 0x3A | 0x3B
            | 0xD0 | 0xD1 | 0xD2 | 0xD3 => mmio_len_after_modrm(rest, addr16, 0)?,
            _ => return None,
        }
    };
    let n = i.checked_add(body)?;
    (n >= 1 && n <= 15).then_some(n)
}

/// VMCS length when it is a valid 1–15, else length decoded from fetched bytes.
/// Do not use `fetched_n` (that is a 16-byte peek).
pub fn mmio_effective_len(bytes: &[u8], vmcs_len: u64, long64: bool) -> u64 {
    if vmcs_len >= 1 && vmcs_len <= 15 {
        vmcs_len
    } else {
        mmio_decoded_len(bytes, long64).unwrap_or(0) as u64
    }
}

/// Decode MOV/MOVZX/MOVSX/XCHG/ALU RMW (mem or dest-reg)/TEST/CMP/INC/DEC/NOT/NEG/BT family/CMPXCHG/XADD/CMPXCHG8B/CMOV/SETCC/BSF/BSR/TZCNT/LZCNT/POPCNT/IMUL/MUL/DIV/IDIV/MOVNTI/SHLD/SHRD/PUSH/POP/MOVS/STOS/LODS/CMPS/SCAS/MOVUPS/MOVDQU/PREFETCH/NOP/CLFLUSH that OVMF IoLib and Linux ioread use for virtio-pci BAR and xAPIC MMIO.
pub fn decode_mmio_insn(bytes: &[u8], insn_len: usize) -> Option<MmioInsn> {
    if bytes.is_empty() || insn_len == 0 || insn_len > bytes.len() || insn_len > 15 {
        return None;
    }
    let mut i = 0usize;
    let mut operand16 = false;
    let mut rex_w = false;
    let mut rex_r = 0u8;
    let mut rex = false;
    let mut f3 = false;
    let mut f2 = false;
    while i < insn_len {
        match bytes[i] {
            0x66 => {
                operand16 = true;
                i += 1;
            }
            0xF3 => {
                f3 = true;
                i += 1;
            }
            0xF2 => {
                f2 = true;
                i += 1;
            }
            0x26 | 0x2E | 0x36 | 0x3E | 0x64 | 0x65 | 0x67 | 0xF0 => i += 1,
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
        if let Some(sse) = mmio_sse_op(op2, f2, f3, operand16, rex, rex_r, bytes, i, insn_len) {
            return Some(sse);
        }
        if (0x40..=0x4F).contains(&op2) || (0x90..=0x9F).contains(&op2) {
            if i >= insn_len {
                return None;
            }
            let m = bytes[i];
            let setcc = op2 >= 0x90;
            let reg = ((m >> 3) & 7) | rex_r;
            let size = if setcc {
                1
            } else if rex_w {
                8
            } else if operand16 {
                2
            } else {
                4
            };
            return Some(MmioInsn {
                is_write: setcc,
                size,
                reg,
                has_imm: false,
                imm: 0,
                zero_ext: !setcc && size == 4,
                sign_ext: false,
                xchg: false,
                alu: 0,
                rex,
                test: false,
                cmp: false,
                cmp_reg_left: false,
                alu_reg_left: false,
                bt: 0,
                atomic: 0,
                cc: (op2 & 0xf) + 1,
            });
        }
        if op2 == 0x18 || op2 == 0x0D || op2 == 0x1F || op2 == 0x19 {
            if i >= insn_len {
                return None;
            }
            return Some(mmio_hint());
        }
        if op2 == 0xAE {
            if i >= insn_len {
                return None;
            }
            let m = bytes[i];
            if ((m >> 3) & 7) == 7 {
                return Some(mmio_hint());
            }
            return None;
        }
        if op2 == 0xBC || op2 == 0xBD {
            if i >= insn_len {
                return None;
            }
            let m = bytes[i];
            let reg = ((m >> 3) & 7) | rex_r;
            let size = if rex_w {
                8
            } else if operand16 {
                2
            } else {
                4
            };
            return Some(MmioInsn {
                is_write: false,
                size,
                reg,
                has_imm: false,
                imm: 0,
                zero_ext: size == 4,
                sign_ext: false,
                xchg: false,
                alu: if f3 {
                    if op2 == 0xBD {
                        MMIO_ALU_LZCNT
                    } else {
                        MMIO_ALU_TZCNT
                    }
                } else if op2 == 0xBD {
                    MMIO_ALU_BSR
                } else {
                    MMIO_ALU_BSF
                },
                rex,
                test: false,
                cmp: false,
                cmp_reg_left: false,
                alu_reg_left: true,
                bt: 0,
                atomic: 0,
                cc: 0,
            });
        }
        if f3 && op2 == 0xB8 {
            if i >= insn_len {
                return None;
            }
            let m = bytes[i];
            let reg = ((m >> 3) & 7) | rex_r;
            let size = if rex_w {
                8
            } else if operand16 {
                2
            } else {
                4
            };
            return Some(MmioInsn {
                is_write: false,
                size,
                reg,
                has_imm: false,
                imm: 0,
                zero_ext: size == 4,
                sign_ext: false,
                xchg: false,
                alu: MMIO_ALU_POPCNT,
                rex,
                test: false,
                cmp: false,
                cmp_reg_left: false,
                alu_reg_left: true,
                bt: 0,
                atomic: 0,
                cc: 0,
            });
        }
        if op2 == 0xAF {
            if i >= insn_len {
                return None;
            }
            let m = bytes[i];
            let reg = ((m >> 3) & 7) | rex_r;
            let size = if rex_w {
                8
            } else if operand16 {
                2
            } else {
                4
            };
            return Some(MmioInsn {
                is_write: false,
                size,
                reg,
                has_imm: false,
                imm: 0,
                zero_ext: size == 4,
                sign_ext: false,
                xchg: false,
                alu: MMIO_ALU_IMUL,
                rex,
                test: false,
                cmp: false,
                cmp_reg_left: false,
                alu_reg_left: true,
                bt: 0,
                atomic: 0,
                cc: 0,
            });
        }
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
                cc: 0,
            });
        }
        // SHLD/SHRD r/m, r, imm8 or CL (`0F A4`/`A5`/`AC`/`AD`). Dest is r/m.
        if op2 == 0xA4 || op2 == 0xA5 || op2 == 0xAC || op2 == 0xAD {
            if i >= insn_len {
                return None;
            }
            let m = bytes[i];
            let reg = ((m >> 3) & 7) | rex_r;
            let size = if rex_w {
                8
            } else if operand16 {
                2
            } else {
                4
            };
            let has_imm = op2 == 0xA4 || op2 == 0xAC;
            let imm = if has_imm {
                if i + 1 >= insn_len {
                    return None;
                }
                u64::from(bytes[insn_len - 1])
            } else {
                0
            };
            return Some(MmioInsn {
                is_write: true,
                size,
                reg,
                has_imm,
                imm,
                zero_ext: false,
                sign_ext: false,
                xchg: false,
                alu: if op2 == 0xA4 || op2 == 0xA5 {
                    MMIO_ALU_SHLD
                } else {
                    MMIO_ALU_SHRD
                },
                rex,
                test: false,
                cmp: false,
                cmp_reg_left: false,
                alu_reg_left: false,
                bt: 0,
                atomic: 0,
                cc: 0,
            });
        }
        // MOVNTI m32/m64, r32/r64 (`0F C3`). No 16-bit form (ignore 66h).
        if op2 == 0xC3 {
            if i >= insn_len {
                return None;
            }
            let m = bytes[i];
            let reg = ((m >> 3) & 7) | rex_r;
            let size = if rex_w { 8 } else { 4 };
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
                atomic: 0,
                cc: 0,
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
                cc: 0,
            });
        }
        // CMPXCHG8B m64 (`0F C7 /1`). REX.W is CMPXCHG16B (not emulated).
        if op2 == 0xC7 {
            if rex_w || i >= insn_len {
                return None;
            }
            let m = bytes[i];
            if ((m >> 3) & 7) != 1 {
                return None;
            }
            return Some(MmioInsn {
                is_write: true,
                size: 8,
                reg: 0,
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
                bt: 0,
                atomic: MMIO_CMPXCHG8B,
                cc: 0,
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
                cc: 0,
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
            cc: 0,
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
                cc: 0,
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
                cc: 0,
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
                cc: 0,
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
                cc: 0,
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
        0x69 | 0x6B => {
            if i >= insn_len {
                return None;
            }
            let m = bytes[i];
            let reg = ((m >> 3) & 7) | rex_r;
            let size = if rex_w {
                8
            } else if operand16 {
                2
            } else {
                4
            };
            let imm_n = if op == 0x6B {
                1usize
            } else if size == 2 {
                2
            } else {
                4
            };
            if insn_len < imm_n {
                return None;
            }
            let imm_off = insn_len - imm_n;
            let mut raw = 0u64;
            let mut k = 0usize;
            while k < imm_n {
                raw |= u64::from(bytes[imm_off + k]) << (8 * k);
                k += 1;
            }
            let imm = if op == 0x6B {
                raw as u8 as i8 as i64 as u64
            } else if size == 8 {
                raw as u32 as i32 as i64 as u64
            } else {
                raw
            };
            Some(MmioInsn {
                is_write: false,
                size,
                reg,
                has_imm: true,
                imm,
                zero_ext: size == 4,
                sign_ext: false,
                xchg: false,
                alu: MMIO_ALU_IMUL,
                rex,
                test: false,
                cmp: false,
                cmp_reg_left: false,
                alu_reg_left: true,
                bt: 0,
                atomic: 0,
                cc: 0,
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
                cc: 0,
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
                cc: 0,
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
            } else if rex_w {
                8
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
                        cc: 0,
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
                    cc: 0,
                }),
                4 | 5 | 6 | 7 => Some(MmioInsn {
                    is_write: false,
                    size,
                    reg: 0,
                    has_imm: false,
                    imm: 0,
                    zero_ext: size == 4,
                    sign_ext: false,
                    xchg: false,
                    alu: match ext {
                        4 => MMIO_ALU_MUL,
                        5 => MMIO_ALU_IMUL1,
                        6 => MMIO_ALU_DIV,
                        _ => MMIO_ALU_IDIV,
                    },
                    rex,
                    test: false,
                    cmp: false,
                    cmp_reg_left: false,
                    alu_reg_left: true,
                    bt: 0,
                    atomic: 0,
                    cc: 0,
                }),
                _ => None,
            }
        }
        0x8F => {
            if i >= insn_len {
                return None;
            }
            let m = bytes[i];
            if ((m >> 3) & 7) != 0 {
                return None;
            }
            let size = if rex_w {
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
                has_imm: false,
                imm: 0,
                zero_ext: false,
                sign_ext: false,
                xchg: false,
                alu: MMIO_ALU_POP,
                rex,
                test: false,
                cmp: false,
                cmp_reg_left: false,
                alu_reg_left: false,
                bt: 0,
                atomic: 0,
                cc: 0,
            })
        }
        0xFE | 0xFF => {
            if i >= insn_len {
                return None;
            }
            let m = bytes[i];
            let ext = (m >> 3) & 7;
            if op == 0xFF && (ext == 2 || ext == 4 || ext == 6) {
                let alu = match ext {
                    2 => MMIO_ALU_CALL,
                    4 => MMIO_ALU_JMP,
                    _ => MMIO_ALU_PUSH,
                };
                let size = if rex_w {
                    8
                } else if operand16 {
                    2
                } else {
                    4
                };
                return Some(MmioInsn {
                    is_write: false,
                    size,
                    reg: 0,
                    has_imm: false,
                    imm: 0,
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
                    cc: 0,
                });
            }
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
                cc: 0,
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
                cc: 0,
            })
        }
        0xA4 | 0xA5 | 0xA6 | 0xA7 | 0xAA | 0xAB | 0xAC | 0xAD | 0xAE | 0xAF => {
            let size = if op == 0xA4 || op == 0xAA || op == 0xAC || op == 0xA6 || op == 0xAE {
                1
            } else if rex_w {
                8
            } else if operand16 {
                2
            } else {
                4
            };
            let alu = match op {
                0xA4 | 0xA5 => MMIO_ALU_MOVS,
                0xAA | 0xAB => MMIO_ALU_STOS,
                0xAC | 0xAD => MMIO_ALU_LODS,
                0xA6 | 0xA7 => MMIO_ALU_CMPS,
                _ => MMIO_ALU_SCAS,
            };
            let rep = if alu == MMIO_ALU_CMPS || alu == MMIO_ALU_SCAS {
                f3 || f2
            } else {
                f3
            };
            Some(MmioInsn {
                is_write: alu == MMIO_ALU_STOS || alu == MMIO_ALU_MOVS,
                size,
                reg: 0,
                has_imm: rep,
                imm: if f2 { 1 } else { 0 },
                zero_ext: size == 4,
                sign_ext: false,
                xchg: false,
                alu,
                rex,
                test: false,
                cmp: alu == MMIO_ALU_CMPS || alu == MMIO_ALU_SCAS,
                cmp_reg_left: false,
                alu_reg_left: alu == MMIO_ALU_LODS || alu == MMIO_ALU_SCAS,
                bt: 0,
                atomic: 0,
                cc: 0,
            })
        }
        _ => None,
    }
}

#[cfg(test)]
#[path = "guest_virtio_blk_test.rs"]
mod guest_virtio_blk_test;
