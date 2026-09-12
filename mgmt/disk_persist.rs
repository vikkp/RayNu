//! M8.0 install-disk persist across a **hypervisor** reboot (outside Proven Core).
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** (ADR-002 / ADR-018). Do not touch VMX/EPT/allocator
//! unless a new ADR says otherwise.
//! VERIFICATION: L0/L1 host tests (GPT+ESP+ext4 byte round-trip).
//!
//! Guest F7 already keeps leftover DRAM (`reset_keep_disk`, ADR-017). A
//! RayNu-V reboot zeros that RAM. ADR-018's first action: virtio-blk HPA *is*
//! a durable device (nested: a QEMU file; iron: a USB partition or NVMe LUN
//! that is not PERC Ubuntu). Leftover DRAM is the fallback when no persist
//! media exists. Rejected: copy 1 GiB DRAM ↔ ESP on every HV stop.
//!
//! ADR-004: persist backing HPAs are virtio-blk / BlockIo only — the guest
//! never sees them as RAM.
//!
//! Iron close marker [`M8_DISK_PERSIST_OK_MARKER`] is COM2-only. Host/CI
//! print [`M8_DISK_PERSIST_HOST_OK_MARKER`]. Nested harness may print
//! [`M8_DISK_PERSIST_NESTED_OK_MARKER`]. Never `ISO-INSTALL-OK`.

use crate::raynu_f::fat::{self, VolumeRead};
use crate::raynu_f::gpt::{find_esp, ESP_TYPE_GUID};
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// Iron COM2 close: Force Off / reboot RayNu-V, installed disk still there.
/// Host/CI/nested must **never** print this.
pub const M8_DISK_PERSIST_OK_MARKER: &str = "RAYNU-V-M8-DISK-PERSIST-OK";

/// Host/CI: file-backed GPT+ESP+ext4 round-trip after an in-process HV reboot.
/// Not nested. Not iron.
pub const M8_DISK_PERSIST_HOST_OK_MARKER: &str = "RAYNU-V-M8-DISK-PERSIST-HOST-OK";

/// Nested QEMU: install → kill/restart HV process → second Linux without
/// `setup-disk`. Printed only by `tools/m8-persist-nested.sh` on success.
/// Host cargo tests and iron COM2 must **never** print this.
pub const M8_DISK_PERSIST_NESTED_OK_MARKER: &str = "RAYNU-V-M8-DISK-PERSIST-NESTED-OK";

/// Why we do not copy leftover DRAM onto the Cruzer ESP.
pub const ESP_COPY_REJECT_NOTE: &str = "rejected: copy 1 GiB DRAM ↔ ESP on every HV stop (too slow; this ESP is too small for the Alpine GPT/ext4 disk; installdisk.bin is the 1 MiB LBA-stamp, not the guest disk)";

/// 4 GB UDisk already holds alpine-extended; cannot also hold a 1 GiB image.
pub const UDISK_TOO_SMALL_NOTE: &str = "the 4 GB UDisk already holds ~994 MiB alpine-extended; it cannot also hold a 1 GiB disk image next to the ISO";

/// Iron durable media is not the standing Ubuntu install.
pub const PERC_UBUNTU_UNTOUCHED_NOTE: &str = "Do not format the R640 PERC (Ubuntu). Iron durable LUN is a USB partition or NVMe that is not PERC Ubuntu.";

/// ADR-004: persist HPAs are exclusive disk backing, not guest RAM.
pub const PERSIST_EXCLUSIVE_OWNERSHIP_NOTE: &str = "ADR-004 exclusive ownership: persist backing HPAs (file / durable LUN / leftover carve) are virtio-blk / BlockIo only; the guest never sees those HPAs as RAM";

/// Host-test GPT image size (not the iron 1 GiB leftover).
pub const HOST_PERSIST_DISK_BYTES: usize = 1024 * 1024;
/// First ESP LBA on the host-test image (after protective MBR + GPT header + entries).
pub const HOST_PERSIST_ESP_START_LBA: u64 = 34;
/// Last ESP LBA (128 sectors = 64 KiB FAT12).
pub const HOST_PERSIST_ESP_END_LBA: u64 = 161;
/// First Linux-filesystem LBA (ext4 superblock + `root=UUID=`).
pub const HOST_PERSIST_DATA_START_LBA: u64 = 162;
/// Last Linux-filesystem LBA on the 1 MiB host image.
pub const HOST_PERSIST_DATA_END_LBA: u64 = 200;
/// Planted root UUID (host fixture — not an iron COM2 UUID).
pub const HOST_PERSIST_ROOT_UUID: &str = "c0ffee00-0d15-4c00-9e51-0000000008a0";
/// GRUB-style cmdline that must survive HV reboot.
pub const HOST_PERSIST_ROOT_CMDLINE: &str = "root=UUID=c0ffee00-0d15-4c00-9e51-0000000008a0";
/// ext4 superblock magic (little-endian `0xEF53` at partition offset `0x438`).
pub const EXT4_SUPER_MAGIC: u16 = 0xEF53;
/// Offset of `s_magic` from the start of the Linux filesystem partition.
pub const EXT4_MAGIC_OFF: usize = 0x438;

/// Linux filesystem GUID `0FC63DAF-8483-4772-8E79-3D69D8477DE4` (mixed-endian).
pub const LINUX_FS_GUID: [u8; 16] = [
    0xAF, 0x3D, 0xC6, 0x0F, 0x83, 0x84, 0x72, 0x47, 0x8E, 0x79, 0x3D, 0x69, 0xD8, 0x47, 0x7D, 0xE4,
];

/// How virtio-blk is backed for M8.0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PersistKind {
    /// Nested: virtio-blk HPA is a QEMU file. Host tests use `std::fs`.
    File,
    /// Iron: USB partition or NVMe LUN that is not PERC Ubuntu.
    DurableLun,
    /// Leftover DRAM above PRECISE. Fallback when no persist media exists.
    /// Survives guest F7; dies on HV reboot.
    LeftoverDram,
}

/// Pick the backend. Durable media wins; leftover DRAM is the fallback.
///
/// Nested with a QEMU file → [`PersistKind::File`]. Iron with a USB/NVMe LUN
/// → [`PersistKind::DurableLun`]. No persist media → leftover DRAM.
pub fn select_persist_kind(nested: bool, has_durable_media: bool) -> PersistKind {
    if !has_durable_media {
        return PersistKind::LeftoverDram;
    }
    if nested {
        PersistKind::File
    } else {
        PersistKind::DurableLun
    }
}

/// File and durable LUN survive a hypervisor reboot. Leftover DRAM does not.
pub fn kind_survives_hv_reboot(kind: PersistKind) -> bool {
    matches!(kind, PersistKind::File | PersistKind::DurableLun)
}

/// Copying 1 GiB through the Cruzer ESP is not a backend.
pub fn esp_copy_is_rejected() -> bool {
    ESP_COPY_REJECT_NOTE.contains("rejected")
        && ESP_COPY_REJECT_NOTE.contains("1 GiB")
        && UDISK_TOO_SMALL_NOTE.contains("994 MiB")
}

/// Host/CI must never print the Everest iron install marker, the M8 iron
/// persist marker, or the nested persist marker.
pub fn host_never_prints_iso_install_ok() -> bool {
    M8_DISK_PERSIST_OK_MARKER != "RAYNU-V-M7-ISO-INSTALL-OK"
        && M8_DISK_PERSIST_HOST_OK_MARKER != "RAYNU-V-M7-ISO-INSTALL-OK"
        && M8_DISK_PERSIST_NESTED_OK_MARKER != "RAYNU-V-M7-ISO-INSTALL-OK"
        && M8_DISK_PERSIST_HOST_OK_MARKER != M8_DISK_PERSIST_OK_MARKER
        && M8_DISK_PERSIST_NESTED_OK_MARKER != M8_DISK_PERSIST_OK_MARKER
        && M8_DISK_PERSIST_NESTED_OK_MARKER != M8_DISK_PERSIST_HOST_OK_MARKER
        && crate::mgmt::iso_install::M7_ISO_INSTALL_OK_MARKER == "RAYNU-V-M7-ISO-INSTALL-OK"
}

struct SliceDisk<'a>(&'a [u8]);

impl VolumeRead for SliceDisk<'_> {
    fn read_at(&self, off: u64, buf: &mut [u8]) -> bool {
        let Ok(start) = usize::try_from(off) else {
            return false;
        };
        let Some(end) = start.checked_add(buf.len()) else {
            return false;
        };
        if end > self.0.len() {
            return false;
        }
        buf.copy_from_slice(&self.0[start..end]);
        true
    }
}

fn u16_at(b: &[u8], off: usize) -> u16 {
    u16::from_le_bytes([b[off], b[off + 1]])
}

fn u64_at(b: &[u8], off: usize) -> u64 {
    u64::from_le_bytes([
        b[off],
        b[off + 1],
        b[off + 2],
        b[off + 3],
        b[off + 4],
        b[off + 5],
        b[off + 6],
        b[off + 7],
    ])
}

struct OffsetVol<'a, R: VolumeRead> {
    inner: &'a R,
    base: u64,
}

impl<R: VolumeRead> VolumeRead for OffsetVol<'_, R> {
    fn read_at(&self, off: u64, buf: &mut [u8]) -> bool {
        self.inner.read_at(self.base.saturating_add(off), buf)
    }
}

/// Firmware-reported persist HPA (NVDIMM / durable LUN), not leftover DRAM.
struct PersistHpa {
    hpa: u64,
    len: u64,
}

impl VolumeRead for PersistHpa {
    fn read_at(&self, off: u64, buf: &mut [u8]) -> bool {
        if buf.is_empty() {
            return true;
        }
        let n = buf.len() as u64;
        let Some(end) = off.checked_add(n) else {
            return false;
        };
        if self.hpa == 0 || end > self.len {
            return false;
        }
        // SAFETY: caller of persist_hpa_looks_installed promised `hpa` is
        // readable for `len` (reserved persist region or a host-test Vec).
        unsafe {
            core::ptr::copy_nonoverlapping(
                (self.hpa + off) as *const u8,
                buf.as_mut_ptr(),
                buf.len(),
            );
        }
        true
    }
}

/// First Linux-filesystem partition start LBA, if the GPT array names one.
fn linux_fs_start_lba_vol<R: VolumeRead>(disk: &R) -> Option<u64> {
    let mut hdr = [0u8; 512];
    if !disk.read_at(512, &mut hdr) {
        return None;
    }
    let n = u32::from_le_bytes(hdr[80..84].try_into().ok()?) as usize;
    let es = u32::from_le_bytes(hdr[84..88].try_into().ok()?) as usize;
    if es < 128 || es > 4096 {
        return None;
    }
    let entry_lba = u64_at(&hdr, 72);
    let array_off = entry_lba.saturating_mul(512);
    for i in 0..n.min(128) {
        let mut ent = [0u8; 128];
        let take = es.min(ent.len());
        let off = array_off.saturating_add((i.checked_mul(es)?) as u64);
        if !disk.read_at(off, &mut ent[..take]) {
            return None;
        }
        if &ent[0..16] == ESP_TYPE_GUID {
            continue;
        }
        if &ent[0..16] != LINUX_FS_GUID {
            continue;
        }
        let start = u64_at(&ent, 32);
        if start == 0 {
            continue;
        }
        return Some(start);
    }
    None
}

fn linux_fs_start_lba(disk: &[u8]) -> Option<u64> {
    linux_fs_start_lba_vol(&SliceDisk(disk))
}

const ESP_KEEP_PREFIX: usize = 16384;
static mut ESP_KEEP_PREFIX_BUF: [u8; ESP_KEEP_PREFIX] = [0; ESP_KEEP_PREFIX];

fn disk_has_bootx64_from_esp<R: VolumeRead>(
    disk: &R,
    esp: crate::raynu_f::gpt::EspPartition,
) -> bool {
    let Some(base) = esp.start_lba.checked_mul(512) else {
        return false;
    };
    // Snapshot the FAT12 fixture prefix (BPB + FAT + root + BOOTX64
    // clusters). 32-byte dirent reads must not each be a BOT command.
    // SAFETY: BSP / test-threads=1; keep-detect is not re-entrant.
    let prefix = unsafe {
        core::slice::from_raw_parts_mut(
            core::ptr::addr_of_mut!(ESP_KEEP_PREFIX_BUF) as *mut u8,
            ESP_KEEP_PREFIX,
        )
    };
    if !disk.read_at(base, prefix) {
        return false;
    }
    let Ok(vol) = fat::parse_bpb(&prefix[..512]) else {
        return false;
    };
    let part = SliceDisk(prefix);
    match fat::resolve_path(&vol, &part, b"\\EFI\\BOOT\\BOOTX64.EFI") {
        Ok(e) => e.name_bytes() == b"BOOTX64.EFI" && e.size > 0 && !e.is_dir(),
        Err(_) => false,
    }
}

fn disk_has_bootx64_vol<R: VolumeRead>(disk: &R) -> bool {
    let Ok(esp) = find_esp(disk) else {
        return false;
    };
    disk_has_bootx64_from_esp(disk, esp)
}

fn disk_has_bootx64(disk: &[u8]) -> bool {
    disk_has_bootx64_vol(&SliceDisk(disk))
}

fn disk_has_ext4_vol<R: VolumeRead>(disk: &R) -> bool {
    let Some(start_lba) = linux_fs_start_lba_vol(disk) else {
        return false;
    };
    let base = start_lba.saturating_mul(512);
    let mut magic = [0u8; 2];
    if !disk.read_at(base.saturating_add(EXT4_MAGIC_OFF as u64), &mut magic) {
        return false;
    }
    u16::from_le_bytes(magic) == EXT4_SUPER_MAGIC
}

fn disk_has_ext4_and_root_uuid(disk: &[u8]) -> bool {
    let Some(start_lba) = linux_fs_start_lba(disk) else {
        return false;
    };
    let Some(base) = usize::try_from(start_lba.saturating_mul(512)).ok() else {
        return false;
    };
    let magic_at = base.saturating_add(EXT4_MAGIC_OFF);
    if magic_at + 2 > disk.len() {
        return false;
    }
    if u16_at(disk, magic_at) != EXT4_SUPER_MAGIC {
        return false;
    }
    disk[base..]
        .windows(HOST_PERSIST_ROOT_CMDLINE.len())
        .any(|w| w == HOST_PERSIST_ROOT_CMDLINE.as_bytes())
}

fn persist_media_looks_installed_vol<R: VolumeRead>(disk: &R) -> bool {
    let Ok(esp) = find_esp(disk) else {
        return false;
    };
    disk_has_bootx64_from_esp(disk, esp) && disk_has_ext4_vol(disk)
}

struct LunVol;

impl VolumeRead for LunVol {
    fn read_at(&self, off: u64, buf: &mut [u8]) -> bool {
        crate::mgmt::durable_lun::durable_lun_read_any(off, buf)
    }
}

static LUN_KEEP_STICKY: AtomicBool = AtomicBool::new(false);

/// Remember a live GPT+ESP+ext4 probe so TCG skip can keep after USB dies.
pub fn persist_lun_remember_keep(keep: bool) {
    if keep {
        LUN_KEEP_STICKY.store(true, Ordering::Release);
    }
}

/// Last successful DurableLun keep-detect (false until a live probe succeeds).
pub fn persist_lun_sticky_keep() -> bool {
    LUN_KEEP_STICKY.load(Ordering::Acquire)
}

/// Host tests / handoff reset.
pub fn persist_lun_clear_sticky() {
    LUN_KEEP_STICKY.store(false, Ordering::Release);
}

/// GPT / BOOTX64 / ext4 on the LUN (each may issue I/O). Remembers keep.
pub fn persist_lun_keep_parts() -> (bool, bool, bool) {
    let gpt = find_esp(&LunVol);
    let boot = match gpt {
        Ok(esp) => disk_has_bootx64_from_esp(&LunVol, esp),
        Err(_) => false,
    };
    let ext4 = disk_has_ext4_vol(&LunVol);
    persist_lun_remember_keep(gpt.is_ok() && boot && ext4);
    (gpt.is_ok(), boot, ext4)
}

/// Live probe, or a prior successful peek if the LUN path later fails.
pub fn persist_lun_keep() -> bool {
    let (gpt, boot, ext4) = persist_lun_keep_parts();
    (gpt && boot && ext4) || persist_lun_sticky_keep()
}

/// Peek the NVMe (or host-test) DurableLun without treating it as RAM.
pub fn persist_lun_looks_installed() -> bool {
    persist_media_looks_installed_vol(&LunVol)
}

/// True when media has GPT ESP + `\EFI\BOOT\BOOTX64.EFI` + ext4 magic.
///
/// Alpine's real `root=UUID=` is **not** required (host fixture UUID is only
/// for [`restored_disk_is_installed`]). Empty persist → false → first attach
/// zeros so the ISO wins.
pub fn persist_media_looks_installed(image: &[u8]) -> bool {
    persist_media_looks_installed_vol(&SliceDisk(image))
}

/// Peek persist HPA bytes without copying the whole disk.
///
/// SAFETY: `hpa` is readable for `bytes` until the caller drops the region
/// (UEFI PersistentMemory, or a live host-test allocation).
/// KANI-TARGET: persist peek (outside Proven Core).
pub unsafe fn persist_hpa_looks_installed(hpa: u64, bytes: u64) -> bool {
    persist_media_looks_installed_vol(&PersistHpa { hpa, len: bytes })
}

/// True when `image` is still an installed disk after an HV reboot restore:
/// GPT ESP, `\EFI\BOOT\BOOTX64.EFI`, ext4 magic, `root=UUID=` (host fixture).
pub fn restored_disk_is_installed(image: &[u8]) -> bool {
    persist_media_looks_installed(image) && disk_has_ext4_and_root_uuid(image)
}

/// Who backs virtio-blk on this HV boot, and whether to keep existing bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallDiskChoice {
    /// Persist media already looks installed — [`crate::devices::guest_virtio_blk::attach_disk_keep`].
    PersistKeep,
    /// Persist media is empty / not a GPT disk — zero so the ISO wins.
    PersistZero,
    /// Leftover DRAM fallback (dies on the next HV reboot).
    LeftoverZero,
    /// Precise-window pool ladder.
    PoolZero,
}

/// Distro OVMF ignores nvdimm and pc-dimm hotplug (no EFI type 14). Nested
/// QEMU file-backed initial RAM lands as conventional above PRECISE.
/// Promote that leftover carve to File persist. Iron (no hypervisor CPUID)
/// keeps leftover DRAM. Type 14, when present, already reserved persist.
pub fn nested_promotes_leftover_to_file_persist(
    host_hypervisor: bool,
    persist_already: bool,
) -> bool {
    host_hypervisor && !persist_already
}

/// Persist wins; leftover DRAM is the fallback; pool last.
pub fn choose_install_disk_attach(
    persist_reserved: bool,
    persist_looks_installed: bool,
    leftover_reserved: bool,
) -> InstallDiskChoice {
    if persist_reserved {
        if persist_looks_installed {
            InstallDiskChoice::PersistKeep
        } else {
            InstallDiskChoice::PersistZero
        }
    } else if leftover_reserved {
        InstallDiskChoice::LeftoverZero
    } else {
        InstallDiskChoice::PoolZero
    }
}

static PERSIST_DISK_HPA: AtomicU64 = AtomicU64::new(0);
static PERSIST_DISK_BYTES: AtomicU64 = AtomicU64::new(0);
static INSTALL_DISK_KEEP: AtomicBool = AtomicBool::new(false);

/// True when handoff reserved a persist region that attach has not taken.
pub fn persist_install_disk_reserved() -> bool {
    persist_install_disk_region().is_some()
}

/// Persist HPA/size without taking. Nested TCG skip-path peeks before keep-attach.
pub fn persist_install_disk_region() -> Option<(u64, u64)> {
    let hpa = PERSIST_DISK_HPA.load(Ordering::Acquire);
    let bytes = PERSIST_DISK_BYTES.load(Ordering::Acquire);
    if hpa == 0 || bytes == 0 {
        None
    } else {
        Some((hpa, bytes))
    }
}

/// Record persist backing for [`take_persist_install_disk`]. `bytes == 0` clears.
pub fn reserve_persist_install_disk(hpa: u64, bytes: u64) {
    PERSIST_DISK_HPA.store(hpa, Ordering::Release);
    PERSIST_DISK_BYTES.store(bytes, Ordering::Release);
}

/// One-shot: hand persist HPA to virtio-blk attach, or `None`.
pub fn take_persist_install_disk() -> Option<(u64, usize)> {
    let bytes = PERSIST_DISK_BYTES.swap(0, Ordering::AcqRel);
    let hpa = PERSIST_DISK_HPA.swap(0, Ordering::AcqRel);
    if hpa == 0 || bytes == 0 {
        return None;
    }
    Some((hpa, bytes as usize))
}

/// Stash whether [`crate::devices::guest_virtio_blk::attach_disk_keep`] should run.
pub fn set_install_disk_keep(keep: bool) {
    INSTALL_DISK_KEEP.store(keep, Ordering::Release);
}

/// One-shot keep flag for the product-ISO virtio attach.
pub fn take_install_disk_keep() -> bool {
    INSTALL_DISK_KEEP.swap(false, Ordering::AcqRel)
}

#[cfg(test)]
#[path = "disk_persist_test.rs"]
mod disk_persist_test;
