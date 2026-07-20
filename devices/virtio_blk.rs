//! Minimal virtio-mmio block device (M4.3).
//!
//! Pillar: [Z]
//! Proven Core: **outside** (ADR-002)
//!
//! Config-space + status handshake over an EPT MMIO hole. When the guest
//! reaches `DRIVER_OK`, the host write+readbacks a pattern on the in-memory
//! disk image and latches [`M4_BLK_OK_MARKER`]. Disk frames are host-owned
//! (allocator pool), not claimed into guest-exclusive precise ranges.

/// COM1 / gate marker when virtio-blk write+readback succeeds.
pub const M4_BLK_OK_MARKER: &str = "RAYNU-V-M4-BLK-OK";

/// Virtio-mmio magic ("virt").
pub const VIRTIO_MMIO_MAGIC: u32 = 0x7472_6976;
/// Virtio-mmio version 2.
pub const VIRTIO_MMIO_VERSION: u32 = 2;
/// Virtio block device id.
pub const VIRTIO_ID_BLOCK: u32 = 2;
pub const VIRTIO_VENDOR: u32 = 0x5241_594E; // "RAYN"

/// Status bits (virtio 1.1 §2.1).
pub const STATUS_ACKNOWLEDGE: u32 = 1;
pub const STATUS_DRIVER: u32 = 2;
pub const STATUS_DRIVER_OK: u32 = 4;
pub const STATUS_FEATURES_OK: u32 = 8;

/// Register offsets (virtio-mmio).
pub const OFF_MAGIC: u64 = 0x00;
pub const OFF_VERSION: u64 = 0x04;
pub const OFF_DEVICE_ID: u64 = 0x08;
pub const OFF_VENDOR_ID: u64 = 0x0c;
pub const OFF_STATUS: u64 = 0x70;
pub const OFF_CONFIG: u64 = 0x100; // capacity u64 LE (low dword at +0)

/// Probe pattern written to LBA 0 on DRIVER_OK.
pub(crate) const DISK_PATTERN: u32 = 0xB10C_0B01;
const SECTOR_BYTES: usize = 512;

static mut BAR_GPA: u64 = 0;
static mut DISK_BASE: u64 = 0;
static mut DISK_BYTES: usize = 0;
static mut STATUS: u32 = 0;
static mut CAPACITY_SECTORS: u64 = 0;
static mut BLK_OK: bool = false;
static mut BLK_MARKED: bool = false;

/// True after a successful write+readback latch.
pub fn blk_ok() -> bool {
    // SAFETY: single-threaded boot / VMX root.
    unsafe { BLK_OK }
}

/// Take-once COM1 emit helper.
pub fn take_blk_ok_latch() -> bool {
    // SAFETY: single-threaded.
    unsafe {
        if BLK_OK && !BLK_MARKED {
            BLK_MARKED = true;
            true
        } else {
            false
        }
    }
}

/// Install MMIO BAR + host-owned disk backing.
///
/// `bar_gpa` must be EPT-unmapped (hole). `disk_phys` is identity-mapped host
/// RAM owned by the HV allocator — not claimed for any guest.
///
/// SAFETY: `disk_phys` writable for `disk_bytes` (multiple of 512).
pub unsafe fn init(bar_gpa: u64, disk_phys: u64, disk_bytes: usize) {
    debug_assert_eq!(disk_bytes % SECTOR_BYTES, 0);
    BAR_GPA = bar_gpa;
    DISK_BASE = disk_phys;
    DISK_BYTES = disk_bytes;
    CAPACITY_SECTORS = (disk_bytes / SECTOR_BYTES) as u64;
    STATUS = 0;
    BLK_OK = false;
    BLK_MARKED = false;
    core::ptr::write_bytes(disk_phys as *mut u8, 0, disk_bytes);
}

pub fn bar_gpa() -> u64 {
    unsafe { BAR_GPA }
}

pub fn bar_contains(gpa: u64) -> bool {
    let base = unsafe { BAR_GPA };
    base != 0 && (base..base + 0x1000).contains(&gpa)
}

/// Handle a 32-bit MMIO access at `gpa`. Returns `Some(read_val)` on read,
/// `Some(None)` on write, `None` if not our BAR.
pub fn mmio_access(gpa: u64, is_write: bool, write_val: u32) -> Option<Option<u32>> {
    if !bar_contains(gpa) {
        return None;
    }
    let off = gpa - unsafe { BAR_GPA };
    if is_write {
        match off {
            OFF_STATUS => {
                // SAFETY: single-threaded device model.
                unsafe {
                    STATUS = write_val;
                    if write_val & STATUS_DRIVER_OK != 0 {
                        run_write_readback();
                    }
                }
            }
            _ => {}
        }
        Some(None)
    } else {
        let v = match off {
            OFF_MAGIC => VIRTIO_MMIO_MAGIC,
            OFF_VERSION => VIRTIO_MMIO_VERSION,
            OFF_DEVICE_ID => VIRTIO_ID_BLOCK,
            OFF_VENDOR_ID => VIRTIO_VENDOR,
            OFF_STATUS => unsafe { STATUS },
            OFF_CONFIG => unsafe { CAPACITY_SECTORS as u32 },
            x if x == OFF_CONFIG + 4 => unsafe { (CAPACITY_SECTORS >> 32) as u32 },
            _ => 0,
        };
        Some(Some(v))
    }
}

unsafe fn run_write_readback() {
    if DISK_BASE == 0 || DISK_BYTES < SECTOR_BYTES {
        return;
    }
    let p = DISK_BASE as *mut u32;
    core::ptr::write_volatile(p, DISK_PATTERN);
    // Fill rest of sector with a recognizable trail.
    for i in 1..(SECTOR_BYTES / 4) {
        core::ptr::write_volatile(p.add(i), DISK_PATTERN ^ (i as u32));
    }
    let mut ok = true;
    if core::ptr::read_volatile(p) != DISK_PATTERN {
        ok = false;
    }
    for i in 1..(SECTOR_BYTES / 4) {
        if core::ptr::read_volatile(p.add(i)) != (DISK_PATTERN ^ (i as u32)) {
            ok = false;
            break;
        }
    }
    if ok {
        BLK_OK = true;
    }
}

/// Host-only selftest (no guest): write+readback via [`run_write_readback`].
#[cfg(test)]
pub unsafe fn host_selftest(disk_phys: u64, disk_bytes: usize) -> bool {
    init(0xFEB0_0000, disk_phys, disk_bytes);
    STATUS = STATUS_DRIVER_OK;
    run_write_readback();
    BLK_OK
}

#[cfg(test)]
#[path = "virtio_blk_test.rs"]
mod virtio_blk_test;
