//! E5 / M7.7 ISO install-to-disk path (outside Proven Core).
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** (ADR-002 / ADR-009)
//! VERIFICATION: N/A
//!
//! Builds on M7.3 (`IsoDeployPlan`): extract-boot bind + virtio-blk install
//! size → a phased **install-to-disk** contract (write disk → reboot → boot
//! from disk). Host/CI closes only the **scaffold**; iron close is
//! [`M7_ISO_BOOTED_FROM_DISK_MARKER`] on COM2 (documented equivalent of
//! [`M7_ISO_INSTALL_OK_MARKER`]; host/CI must **never** print the iron OK).

use super::api::{
    auth_allows, ApiReply, RestMethod, RestRequest, RestResponse, BRINGUP_AUTH_TOKEN,
};
use super::datastore::{ImageKind, ImageTable};
use super::iso::{
    bind_extract_boot, configure_install_disk, extract_boot_surface_present,
    install_disk_surface_present, register_iso, IsoDeployPlan, IsoError,
    DEFAULT_INSTALL_DISK_BYTES,
};

/// Iron marker — firmware/COM2 after install-to-disk + reboot-to-disk on R640.
/// Host/CI smoke must **never** print this.
pub const M7_ISO_INSTALL_OK_MARKER: &str = "RAYNU-V-M7-ISO-INSTALL-OK";

/// Host / CI scaffold marker when runbook + package gate pass.
pub const M7_ISO_INSTALL_SCAFFOLD_MARKER: &str = "RAYNU-V-M7-ISO-INSTALL-SCAFFOLD-OK";

/// Closed on iron 2026-08-16: persist-detect + prefix-copy → BOOTED-FROM-DISK.
pub const ISO_INSTALL_GAP_NOTE: &str =
    "GAP(CLOSED M7.7): ISO install-to-disk + reboot-to-disk (LBA stamp persist)";

/// Documented MVP: extract-boot + empty virtio-blk → guest writes → reboot from disk.
pub const ISO_INSTALL_MVP_NOTE: &str =
    "MVP: extract-boot + virtio-blk install disk → guest write → reboot-to-disk (El Torito deferred)";

/// Honesty: scaffold cannot invent iron install evidence.
pub const ISO_INSTALL_HOST_LIMIT_NOTE: &str =
    "Latitude/QEMU host smoke cannot close RAYNU-V-M7-ISO-INSTALL-OK; real install proof required";

/// Stage 46: El Torito on a product CD continues; not install-OK.
///
/// INVARIANTS:
/// - `true` only when El Torito evidence ran **and** the CD is not the lab stub
/// - Never implies [`M7_ISO_INSTALL_OK_MARKER`]
pub fn product_iso_continues_past_eltorito(lab_stub: bool, eltorito_ran: bool) -> bool {
    eltorito_ran && !lab_stub
}

/// Host/CI must never print the iron Everest E5 marker.
pub fn stage46_host_never_prints_iso_install_ok() -> bool {
    M7_ISO_INSTALL_OK_MARKER == "RAYNU-V-M7-ISO-INSTALL-OK"
        && M7_ISO_INSTALL_SCAFFOLD_MARKER != M7_ISO_INSTALL_OK_MARKER
        && M7_ISO_BOOTED_FROM_DISK_MARKER != M7_ISO_INSTALL_OK_MARKER
}

/// True when `len` arms the product ISO ATAPI window (not the 72 KiB lab stub).
pub fn product_iso_len_is_window(len: usize) -> bool {
    len > crate::devices::ide_cdrom::GUEST_CD_ISO_CAP && len <= PRODUCT_ISO_MAX_BYTES
}

/// After guest-UEFI, stay there instead of packed-bzImage E4.
///
/// INVARIANTS:
/// - `true` only when the product ISO window is armed
/// - Lab 73728 stub / `iso=0` stays `false` (E4 `LINUX-EARLY` still runs)
pub fn stage46_hold_e4_shell() -> bool {
    crate::devices::ide_cdrom::product_iso_window_armed()
}

/// ESP paths probed PRE-EBS for a distro ISO (not the 1 KiB persist stamp).
pub const PRODUCT_ISO_ESP_PATHS: &[&str] = &[
    "\\EFI\\RayNu\\linux.iso",
    "\\linux.iso",
    "\\EFI\\RayNu\\install.iso",
];

/// Cap a product ISO so PRE-EBS AllocatePages cannot consume the map.
pub const PRODUCT_ISO_MAX_BYTES: usize = 0x8000_0000;

/// Serial when ESP retained a window-sized ISO. Not `ISO-INSTALL-OK`.
pub const M7_STAGE46_PRODUCT_ISO_ESP_NOTE: &str =
    "boot: Stage 46 product ISO retained from ESP (not ISO-INSTALL-OK)";

/// Serial when ESP has no window-sized ISO — lab El Torito stub.
pub const M7_STAGE46_PRODUCT_ISO_MISSING_NOTE: &str =
    "boot: Stage 46 no product ISO on ESP — lab El Torito stub";

/// Serial when product ISO is armed and E4 SHELL is not entered.
pub const M7_STAGE46_HOLD_E4_NOTE: &str =
    "boot: Stage 46 product ISO hold (not ISO-INSTALL-OK); not E4 SHELL";

static mut PRODUCT_ISO_PTR: *const u8 = core::ptr::null();
static mut PRODUCT_ISO_LEN: usize = 0;

/// Same-length swap so Alpine/GRUB gets serial + virtio-blk, and still loads squashfs.
/// `squashfs,sd-mod,usb-storage quiet` → `squashfs,virtio_blk console=ttyS0`
/// `modules=loop,squashfs,virtio_blk` stays a valid list (Alpine mkinitfs needs
/// squashfs in `modules=` to mount the live root / modloop; virtio_blk so
/// `/dev/vda` and `/dev/vdb` appear when the virt drivers are modules).
/// `console=ttyS0` is a kernel param. Product ISO xAPIC is trap-and-emulate
/// (`lapic_virt` CUR_COUNT + EOI), so `nolapic` is no longer required.
/// Device IRQs stay on IOAPIC GSI 17/18 and PCI line 11. Optional
/// `console=tty0` → `noapic` when that string exists. alpine-virt media is
/// `/dev/vdb`. Does not grow the ISO. Does not print [`M7_ISO_INSTALL_OK_MARKER`].
pub const ISO_SERIAL_CONSOLE_FROM: &[u8] = b"squashfs,sd-mod,usb-storage quiet";
pub const ISO_SERIAL_CONSOLE_TO: &[u8] = b"squashfs,virtio_blk console=ttyS0";
const _: () = assert!(ISO_SERIAL_CONSOLE_FROM.len() == ISO_SERIAL_CONSOLE_TO.len());
/// Drop VGA console and request PIC-only IRQs when the ISO still has tty0.
pub const ISO_TTY0_FROM: &[u8] = b"console=tty0";
pub const ISO_TTY0_TO: &[u8] = b"noapic      ";
const _: () = assert!(ISO_TTY0_FROM.len() == ISO_TTY0_TO.len());
pub const ISO_GRUB_TIMEOUT_FROM: &[u8] = b"timeout=10";
pub const ISO_GRUB_TIMEOUT_TO: &[u8] = b"timeout=0 ";
const _: () = assert!(ISO_GRUB_TIMEOUT_FROM.len() == ISO_GRUB_TIMEOUT_TO.len());
/// Modern alpine-virt GRUB is `set timeout=1`, not `timeout=10`.
/// Apply **after** `timeout=10` so `set timeout=10` does not become `set timeout=00`.
pub const ISO_GRUB_TIMEOUT1_FROM: &[u8] = b"set timeout=1";
pub const ISO_GRUB_TIMEOUT1_TO: &[u8] = b"set timeout=0";
const _: () = assert!(ISO_GRUB_TIMEOUT1_FROM.len() == ISO_GRUB_TIMEOUT1_TO.len());
/// After an sr-mod swap (other ISOs): load PIIX IDE so `/dev/sr0` can attach.
/// 0 hits on alpine-virt after the `noapic` swap — that path uses virtio-iso.
pub const ISO_ATA_PIIX_FROM: &[u8] = b"loop,squashfs,sr-mod";
pub const ISO_ATA_PIIX_TO: &[u8] = b"ata_piix,loop,sr-mod";
const _: () = assert!(ISO_ATA_PIIX_FROM.len() == ISO_ATA_PIIX_TO.len());
/// GRUB EFI often binds GOP (`gfxterm`) and never prints `GNU GRUB` on COM1.
/// Same-length swap onto `serial` when those strings exist (0 hits is fine).
pub const ISO_GRUB_GFXTERM_FROM: &[u8] = b"terminal_output gfxterm";
pub const ISO_GRUB_GFXTERM_TO: &[u8] = b"terminal_output serial ";
/// Some alpine-virt GRUB cfg uses `console` rather than `gfxterm`. 0 hits OK.
pub const ISO_GRUB_TERM_CONSOLE_FROM: &[u8] = b"terminal_output console";
pub const ISO_GRUB_TERM_CONSOLE_TO: &[u8] = b"terminal_output serial ";
const _: () = assert!(ISO_GRUB_TERM_CONSOLE_FROM.len() == ISO_GRUB_TERM_CONSOLE_TO.len());
const _: () = assert!(ISO_GRUB_GFXTERM_FROM.len() == ISO_GRUB_GFXTERM_TO.len());
pub const ISO_GRUB_INSMOD_GFX_FROM: &[u8] = b"insmod gfxterm";
pub const ISO_GRUB_INSMOD_GFX_TO: &[u8] = b"insmod serial ";
const _: () = assert!(ISO_GRUB_INSMOD_GFX_FROM.len() == ISO_GRUB_INSMOD_GFX_TO.len());
/// GRUB EFI `load_video` pulls GOP/UGA. Guest-UEFI has no GOP; insmod can stall.
/// 0 hits is fine. Same length as `insmod gfxterm`.
pub const ISO_GRUB_INSMOD_GOP_FROM: &[u8] = b"insmod efi_gop";
pub const ISO_GRUB_INSMOD_GOP_TO: &[u8] = b"insmod serial ";
const _: () = assert!(ISO_GRUB_INSMOD_GOP_FROM.len() == ISO_GRUB_INSMOD_GOP_TO.len());
pub const ISO_GRUB_INSMOD_UGA_FROM: &[u8] = b"insmod efi_uga";
pub const ISO_GRUB_INSMOD_UGA_TO: &[u8] = b"insmod serial ";
const _: () = assert!(ISO_GRUB_INSMOD_UGA_FROM.len() == ISO_GRUB_INSMOD_UGA_TO.len());
/// GRUB `load_video` may `insmod all_video` instead of efi_gop. 0 hits OK.
pub const ISO_GRUB_INSMOD_ALLVID_FROM: &[u8] = b"insmod all_video";
pub const ISO_GRUB_INSMOD_ALLVID_TO: &[u8] = b"insmod serial   ";
const _: () = assert!(ISO_GRUB_INSMOD_ALLVID_FROM.len() == ISO_GRUB_INSMOD_ALLVID_TO.len());
/// alpine-virt `nlplug-findfs -b cdrom` waits for ATAPI. Point it at virtio
/// ISO `/dev/vdb`. 0 hits is fine when the string is absent.
pub const ISO_ALPINE_DEV_FROM: &[u8] = b"alpine_dev=cdrom";
pub const ISO_ALPINE_DEV_TO: &[u8] = b"alpine_dev=vdb  ";
const _: () = assert!(ISO_ALPINE_DEV_FROM.len() == ISO_ALPINE_DEV_TO.len());

/// Patch a product ISO so the installer kernel uses `console=ttyS0`, PIC, and virtio media.
///
/// INVARIANTS:
/// - Replacements are the same length as the originals
/// - Returns the number of replacements (0 = nothing patched)
pub fn patch_iso_linux_serial_console(bytes: &mut [u8]) -> u32 {
    patch_same(bytes, ISO_SERIAL_CONSOLE_FROM, ISO_SERIAL_CONSOLE_TO)
        .saturating_add(patch_same(bytes, ISO_ATA_PIIX_FROM, ISO_ATA_PIIX_TO))
        .saturating_add(patch_same(bytes, ISO_GRUB_TIMEOUT_FROM, ISO_GRUB_TIMEOUT_TO))
        .saturating_add(patch_same(bytes, ISO_GRUB_TIMEOUT1_FROM, ISO_GRUB_TIMEOUT1_TO))
        .saturating_add(patch_same(bytes, ISO_GRUB_GFXTERM_FROM, ISO_GRUB_GFXTERM_TO))
        .saturating_add(patch_same(
            bytes,
            ISO_GRUB_TERM_CONSOLE_FROM,
            ISO_GRUB_TERM_CONSOLE_TO,
        ))
        .saturating_add(patch_same(bytes, ISO_GRUB_INSMOD_GFX_FROM, ISO_GRUB_INSMOD_GFX_TO))
        .saturating_add(patch_same(bytes, ISO_GRUB_INSMOD_GOP_FROM, ISO_GRUB_INSMOD_GOP_TO))
        .saturating_add(patch_same(bytes, ISO_GRUB_INSMOD_UGA_FROM, ISO_GRUB_INSMOD_UGA_TO))
        .saturating_add(patch_same(bytes, ISO_GRUB_INSMOD_ALLVID_FROM, ISO_GRUB_INSMOD_ALLVID_TO))
        .saturating_add(patch_same(bytes, ISO_ALPINE_DEV_FROM, ISO_ALPINE_DEV_TO))
        .saturating_add(patch_same(bytes, ISO_TTY0_FROM, ISO_TTY0_TO))
}

fn patch_same(bytes: &mut [u8], from: &[u8], to: &[u8]) -> u32 {
    if from.len() != to.len() || from.is_empty() {
        return 0;
    }
    let mut n = 0u32;
    let mut i = 0usize;
    while i + from.len() <= bytes.len() {
        if bytes[i..i + from.len()] == *from {
            bytes[i..i + to.len()].copy_from_slice(to);
            n = n.saturating_add(1);
            i = i.saturating_add(to.len());
        } else {
            i = i.saturating_add(1);
        }
    }
    n
}

/// Remember a window-sized ISO. Caller keeps `bytes` alive across EBS.
///
/// INVARIANTS:
/// - Rejects lab stub size (`<= GUEST_CD_ISO_CAP`) and oversize
/// - Does not print [`M7_ISO_INSTALL_OK_MARKER`]
pub fn retain_product_iso_bytes(bytes: &[u8]) -> bool {
    if !product_iso_len_is_window(bytes.len()) {
        return false;
    }
    // SAFETY: single-threaded boot / host-test lock.
    unsafe {
        PRODUCT_ISO_PTR = bytes.as_ptr();
        PRODUCT_ISO_LEN = bytes.len();
    }
    true
}

/// Bytes retained by [`retain_product_iso_bytes`] / PRE-EBS ESP probe.
pub fn product_iso_retained_bytes() -> Option<&'static [u8]> {
    // SAFETY: single-threaded boot / host-test lock; written once PRE-EBS.
    unsafe {
        if PRODUCT_ISO_LEN == 0 || PRODUCT_ISO_PTR.is_null() {
            None
        } else {
            Some(core::slice::from_raw_parts(PRODUCT_ISO_PTR, PRODUCT_ISO_LEN))
        }
    }
}

/// Clear ESP retain (host tests).
pub fn clear_product_iso_retain() {
    // SAFETY: single-threaded boot / host-test lock.
    unsafe {
        PRODUCT_ISO_PTR = core::ptr::null();
        PRODUCT_ISO_LEN = 0;
    }
}

/// Present retained product ISO on the guest ATAPI function (no placeholder).
pub fn present_product_iso_if_retained() -> bool {
    let Some(bytes) = product_iso_retained_bytes() else {
        return false;
    };
    crate::devices::ide_cdrom::present(bytes, 1)
}

/// Iron product-ISO virtio-blk (1 GiB). Nested stays [`LAB_INSTALL_DISK_BYTES`].
pub const PRODUCT_ISO_INSTALL_DISK_IRON_BYTES: usize = 1024 * 1024 * 1024;

/// Install-disk size for the guest-UEFI virtio-pci backend when the window is armed.
pub fn product_iso_install_disk_bytes(host_hypervisor: bool) -> usize {
    if host_hypervisor {
        LAB_INSTALL_DISK_BYTES as usize
    } else {
        PRODUCT_ISO_INSTALL_DISK_IRON_BYTES
    }
}

/// Phases toward E5 close (management-plane bookkeeping).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallPhase {
    /// No install plan yet.
    Idle,
    /// Deploy plan ready; launch contract issued (extract-boot + disk size).
    ContractReady,
    /// Guest (or host selftest) wrote install marker to virtio-blk.
    DiskWritten,
    /// Operator/host requested reboot-from-disk.
    RebootPending,
    /// Second boot observed from install disk (iron/QEMU close).
    BootedFromDisk,
}

/// Install-to-disk plan layered on [`IsoDeployPlan`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InstallToDiskPlan {
    pub deploy: IsoDeployPlan,
    pub phase: InstallPhase,
}

impl InstallToDiskPlan {
    pub const fn empty() -> Self {
        Self {
            deploy: IsoDeployPlan::empty(),
            phase: InstallPhase::Idle,
        }
    }

    pub fn is_contract_ready(&self) -> bool {
        self.deploy.is_ready() && self.phase != InstallPhase::Idle
    }

    pub fn is_install_complete(&self) -> bool {
        self.phase == InstallPhase::BootedFromDisk
    }
}

/// What the guest launch path must honor for extract-boot install MVP.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InstallLaunchContract {
    pub iso_id: u64,
    pub extract_bound: bool,
    pub install_disk_bytes: u64,
}

/// Probe-sized virtio-blk backing when no install contract is armed (M4.3).
pub const PROBE_DISK_BYTES: usize = 4096;

/// QEMU / lab install disk (1 MiB) — keeps contiguous alloc cheap under `-m 512M`.
/// Iron / REST default remains [`DEFAULT_INSTALL_DISK_BYTES`] (64 MiB).
pub const LAB_INSTALL_DISK_BYTES: u64 = 1024 * 1024;

/// Serial / CI marker when lab ESP flag armed a 1 MiB install disk.
pub const M7_ISO_INSTALL_LAB_ARM_NOTE: &str = "boot: E5 lab isoinstall.txt armed (1MiB)";

/// Host writeback proved install-sized disk (LBA1 marker) — not reboot-to-disk.
pub const M7_ISO_DISK_WRITTEN_MARKER: &str = "RAYNU-V-M7-ISO-DISK-WRITTEN";

/// QEMU lab package close (sized disk + BLK-OK + disk written). Not iron E5.
pub const M7_ISO_INSTALL_LAB_OK_MARKER: &str = "RAYNU-V-M7-ISO-INSTALL-LAB-OK";

/// Lab honesty: install disk written; reboot-to-disk requested but not executed.
pub const M7_ISO_REBOOT_PENDING_MARKER: &str = "RAYNU-V-M7-ISO-REBOOT-PENDING";

/// QEMU lab: second boot detected persisted install marker (not iron).
pub const M7_ISO_BOOTED_FROM_DISK_MARKER: &str = "RAYNU-V-M7-ISO-BOOTED-FROM-DISK";

/// Serial note when isoreboot.txt arms persist detect.
pub const M7_ISO_REBOOT_LAB_ARM_NOTE: &str = "boot: E5 lab isoreboot.txt armed (1MiB persist)";

/// PRE-EBS ESP persist of LBA stamps so the next boot can preload without host synth.
pub const M7_ISO_PERSIST_WRITE_NOTE: &str = "boot: E5 persist wrote installdisk.bin";

/// ESP write failed (common on read-only iDRAC Virtual Floppy).
pub const M7_ISO_PERSIST_FAIL_NOTE: &str = "boot: WARN — E5 persist ESP write failed";

/// Second boot: `installdisk.bin` present, no `isoinstall.txt` → persist-detect.
pub const M7_ISO_PERSIST_DETECT_NOTE: &str = "boot: E5 persist-detect armed (installdisk.bin)";

/// LBA0+LBA1 only — iron 64 MiB disk must not be copied into the EFI or floppy.
pub const INSTALL_MARKER_PERSIST_BYTES: usize = 1024;

/// Max staged install disk from ESP (lab 1 MiB).
pub const INSTALL_DISK_STAGE_CAP: usize = LAB_INSTALL_DISK_BYTES as usize;

/// Armed across ExitBootServices: set by PRE-EBS `POST /iso/{id}/install` or lab ESP flag.
static mut ARMED_INSTALL: Option<InstallLaunchContract> = None;
static mut LAB_ARMED: bool = false;
static mut REBOOT_LAB_ARMED: bool = false;
static mut DISK_WRITTEN_NOTED: bool = false;
static mut REBOOT_PENDING_NOTED: bool = false;
static mut BOOT_INSTALL_PLAN: InstallToDiskPlan = InstallToDiskPlan::empty();
static mut INSTALL_DISK_BUF: [u8; INSTALL_DISK_STAGE_CAP] = [0; INSTALL_DISK_STAGE_CAP];
static mut INSTALL_DISK_LEN: usize = 0;
/// Stash launch contract so post-EBS `virtio_blk::init` can size the install disk.
pub fn arm_install_launch_contract(contract: InstallLaunchContract) {
    // SAFETY: single-threaded boot / mgmt path.
    unsafe {
        ARMED_INSTALL = Some(contract);
        BOOT_INSTALL_PLAN = InstallToDiskPlan {
            deploy: IsoDeployPlan {
                iso_id: contract.iso_id,
                extract_bound: contract.extract_bound,
                install_disk_bytes: contract.install_disk_bytes,
            },
            phase: InstallPhase::ContractReady,
        };
        DISK_WRITTEN_NOTED = false;
        REBOOT_PENDING_NOTED = false;
    }
}

/// Peek armed contract without clearing (host tests / diagnostics).
pub fn peek_armed_install_contract() -> Option<InstallLaunchContract> {
    // SAFETY: single-threaded boot / mgmt path.
    unsafe { ARMED_INSTALL }
}

/// Take armed contract (clear after read). Prefer [`disk_bytes_for_virtio_launch`] at init.
pub fn take_armed_install_contract() -> Option<InstallLaunchContract> {
    // SAFETY: single-threaded boot / mgmt path.
    unsafe { ARMED_INSTALL.take() }
}

/// Host-test mutex: boot statics (`ARMED_INSTALL`, `BOOT_INSTALL_PLAN`) are
/// single-threaded on iron; `cargo test` default threads otherwise race.
#[cfg(test)]
pub(crate) fn iso_install_host_test_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|p| p.into_inner())
}

/// Clear armed contract (tests / cancel).
pub fn clear_armed_install_contract() {
    // SAFETY: single-threaded boot / mgmt path.
    unsafe {
        ARMED_INSTALL = None;
        LAB_ARMED = false;
        REBOOT_LAB_ARMED = false;
        DISK_WRITTEN_NOTED = false;
        REBOOT_PENDING_NOTED = false;
        BOOT_INSTALL_PLAN = InstallToDiskPlan::empty();
        INSTALL_DISK_LEN = 0;
    }
    crate::devices::virtio_blk::set_reboot_detect(false);
}

/// Arm a QEMU-friendly 1 MiB install contract (ESP `isoinstall.txt` lab path).
pub fn arm_lab_install_contract() {
    arm_install_launch_contract(InstallLaunchContract {
        iso_id: 1,
        extract_bound: true,
        install_disk_bytes: LAB_INSTALL_DISK_BYTES,
    });
    // SAFETY: single-threaded boot.
    unsafe {
        LAB_ARMED = true;
    }
    persist_armed_install_to_esp();
}

/// True when the lab ESP path armed this boot.
pub fn lab_install_armed() -> bool {
    // SAFETY: single-threaded boot.
    unsafe { LAB_ARMED }
}

/// True when reboot-detect lab (isoreboot.txt) armed this boot.
pub fn lab_reboot_armed() -> bool {
    unsafe { REBOOT_LAB_ARMED }
}

/// Staged install disk image bytes (from ESP `installdisk.bin`), if any.
pub fn install_disk_preload_bytes() -> Option<&'static [u8]> {
    unsafe {
        let len = INSTALL_DISK_LEN;
        if len == 0 || len > INSTALL_DISK_STAGE_CAP {
            None
        } else {
            Some(&INSTALL_DISK_BUF[..len])
        }
    }
}

/// Stage raw install disk image into the static buffer (tests / ESP probe).
pub fn stage_install_disk_image(bytes: &[u8]) -> Result<(), ()> {
    if bytes.is_empty() || bytes.len() > INSTALL_DISK_STAGE_CAP {
        return Err(());
    }
    unsafe {
        INSTALL_DISK_BUF[..bytes.len()].copy_from_slice(bytes);
        INSTALL_DISK_LEN = bytes.len();
    }
    Ok(())
}

/// Arm reboot-detect: install-sized contract + virtio reboot detect mode.
pub fn arm_reboot_contract(disk_bytes: u64) {
    arm_install_launch_contract(InstallLaunchContract {
        iso_id: 1,
        extract_bound: true,
        install_disk_bytes: disk_bytes,
    });
    unsafe {
        REBOOT_LAB_ARMED = true;
        LAB_ARMED = false; // reboot path, not write path
    }
    crate::devices::virtio_blk::set_reboot_detect(true);
    unsafe {
        BOOT_INSTALL_PLAN.phase = InstallPhase::RebootPending;
    }
}

/// Arm reboot-detect lab: 1 MiB contract + virtio reboot detect mode.
pub fn arm_lab_reboot_contract() {
    arm_reboot_contract(LAB_INSTALL_DISK_BYTES);
}

/// Record BootedFromDisk on the boot plan (lab close).
pub fn note_booted_from_disk_lab() -> bool {
    unsafe {
        if BOOT_INSTALL_PLAN.phase != InstallPhase::RebootPending {
            return false;
        }
        if mark_booted_from_disk(&mut BOOT_INSTALL_PLAN).is_err() {
            return false;
        }
        true
    }
}

/// Record install-disk write proof (host LBA1 marker after DRIVER_OK).
pub fn note_install_disk_written() -> bool {
    if !install_disk_armed_for_launch() {
        return false;
    }
    // SAFETY: single-threaded boot.
    unsafe {
        if DISK_WRITTEN_NOTED {
            return false;
        }
        DISK_WRITTEN_NOTED = true;
        let _ = mark_disk_written(&mut BOOT_INSTALL_PLAN);
    }
    true
}

/// After disk written: advance phase to RebootPending and latch serial emit.
///
/// Honest lab step — does **not** reboot or second-boot from disk.
pub fn note_reboot_pending_lab() -> bool {
    // SAFETY: single-threaded boot.
    unsafe {
        if REBOOT_PENDING_NOTED {
            return false;
        }
        if BOOT_INSTALL_PLAN.phase != InstallPhase::DiskWritten {
            // Accept direct advance if write was noted via virtio latch without
            // going through note_install_disk_written (launch path).
            if BOOT_INSTALL_PLAN.phase == InstallPhase::ContractReady {
                let _ = mark_disk_written(&mut BOOT_INSTALL_PLAN);
            }
        }
        if mark_reboot_pending(&mut BOOT_INSTALL_PLAN).is_err() {
            return false;
        }
        REBOOT_PENDING_NOTED = true;
        true
    }
}

/// Take-once COM1 emit for reboot-pending lab latch.
pub fn take_reboot_pending_latch() -> bool {
    // SAFETY: single-threaded boot.
    unsafe {
        if REBOOT_PENDING_NOTED {
            REBOOT_PENDING_NOTED = false;
            true
        } else {
            false
        }
    }
}

/// Peek boot install phase (tests / diagnostics).
pub fn boot_install_phase() -> InstallPhase {
    // SAFETY: single-threaded boot.
    unsafe { BOOT_INSTALL_PLAN.phase }
}

/// Take-once: install disk write was noted this boot.
pub fn take_install_disk_written_latch() -> bool {
    // SAFETY: single-threaded boot.
    unsafe {
        if DISK_WRITTEN_NOTED {
            DISK_WRITTEN_NOTED = false;
            true
        } else {
            false
        }
    }
}

/// Disk bytes for live `virtio_blk::init`: armed install size or M4.3 probe (4 KiB).
pub fn disk_bytes_for_virtio_launch() -> usize {
    match peek_armed_install_contract() {
        Some(c)
            if c.extract_bound
                && c.install_disk_bytes >= PROBE_DISK_BYTES as u64
                && c.install_disk_bytes % 512 == 0 =>
        {
            c.install_disk_bytes as usize
        }
        _ => PROBE_DISK_BYTES,
    }
}

/// True when launch will use an install-sized disk (not the 4 KiB probe).
pub fn install_disk_armed_for_launch() -> bool {
    disk_bytes_for_virtio_launch() > PROBE_DISK_BYTES
}

/// Probe ESP for `isoinstall.txt` (lab arm without PRE-EBS curl).
///
/// Paths: `\\EFI\\RayNu\\isoinstall.txt` then `\\isoinstall.txt`.
/// Must run **before** ExitBootServices.
#[cfg(target_os = "uefi")]
pub fn probe_iso_install_lab_flag() {
    use crate::boot::serial;
    use uefi::boot;
    use uefi::fs::FileSystem;

    let image = boot::image_handle();
    let Ok(sfs) = boot::get_image_file_system(image) else {
        return;
    };
    let mut fs = FileSystem::new(sfs);
    let present = flag_present(&mut fs, "\\EFI\\RayNu\\isoinstall.txt")
        || flag_present(&mut fs, "\\isoinstall.txt");
    if present {
        arm_lab_install_contract();
        serial::write_line(M7_ISO_INSTALL_LAB_ARM_NOTE);
    }
}

/// Probe ESP for a window-sized Linux/distro ISO (PRE-EBS).
///
/// Paths: [`PRODUCT_ISO_ESP_PATHS`]. Rejects the 72 KiB lab stub.
/// Copies into `LOADER_DATA` so the bytes survive ExitBootServices
/// (handoff takes CONVENTIONAL; LoaderData is not in the FrameAllocator pool).
#[cfg(target_os = "uefi")]
pub fn probe_product_linux_iso() {
    use crate::boot::serial;
    use uefi::boot::{self, AllocateType, MemoryType};
    use uefi::fs::FileSystem;
    use uefi::CString16;

    let image = boot::image_handle();
    let Ok(sfs) = boot::get_image_file_system(image) else {
        serial::write_line(M7_STAGE46_PRODUCT_ISO_MISSING_NOTE);
        return;
    };
    let mut fs = FileSystem::new(sfs);
    for path in PRODUCT_ISO_ESP_PATHS {
        let Ok(p) = CString16::try_from(*path) else {
            continue;
        };
        let Ok(data) = fs.read(p.as_ref()) else {
            continue;
        };
        if !product_iso_len_is_window(data.len()) {
            continue;
        }
        let pages = data.len().div_ceil(4096);
        let Ok(ptr) =
            boot::allocate_pages(AllocateType::AnyPages, MemoryType::LOADER_DATA, pages)
        else {
            continue;
        };
        // SAFETY: exclusive LOADER_DATA pages; copy then drop the Vec.
        // Conventional leak would be reclaimed at ExitBootServices.
        let leaked: &'static mut [u8] = unsafe {
            core::ptr::copy_nonoverlapping(data.as_ptr(), ptr.as_ptr(), data.len());
            core::slice::from_raw_parts_mut(ptr.as_ptr(), data.len())
        };
        let patched = patch_iso_linux_serial_console(leaked);
        if retain_product_iso_bytes(leaked) {
            serial::write_line(M7_STAGE46_PRODUCT_ISO_ESP_NOTE);
            if patched != 0 {
                serial::write_line(
                    "boot: Stage 46 ISO serial console patched (not ISO-INSTALL-OK)",
                );
            }
            return;
        }
    }
    serial::write_line(M7_STAGE46_PRODUCT_ISO_MISSING_NOTE);
}

#[cfg(not(target_os = "uefi"))]
pub fn probe_product_linux_iso() {}

/// Second boot without `isoreboot.txt`: `installdisk.bin` and no write-flag.
#[cfg(target_os = "uefi")]
pub fn probe_iso_persist_reboot() {
    use crate::boot::serial;
    use uefi::boot;
    use uefi::fs::FileSystem;

    if peek_armed_install_contract().is_some() {
        return;
    }
    let image = boot::image_handle();
    let Ok(sfs) = boot::get_image_file_system(image) else {
        return;
    };
    let mut fs = FileSystem::new(sfs);
    if flag_present(&mut fs, "\\EFI\\RayNu\\isoinstall.txt")
        || flag_present(&mut fs, "\\isoinstall.txt")
    {
        return;
    }
    let staged = stage_from_esp(&mut fs, "\\EFI\\RayNu\\installdisk.bin")
        .or_else(|_| stage_from_esp(&mut fs, "\\installdisk.bin"));
    if staged.is_err() {
        return;
    }
    let disk_bytes = persist_size_from_esp(&mut fs).unwrap_or_else(infer_persist_disk_bytes);
    arm_reboot_contract(disk_bytes);
    serial::write_line(M7_ISO_PERSIST_DETECT_NOTE);
}

#[cfg(not(target_os = "uefi"))]
pub fn probe_iso_persist_reboot() {}

#[cfg(target_os = "uefi")]
fn flag_present(fs: &mut uefi::fs::FileSystem, path: &str) -> bool {
    use uefi::CString16;
    let Ok(p) = CString16::try_from(path) else {
        return false;
    };
    fs.read(p.as_ref()).is_ok()
}

#[cfg(not(target_os = "uefi"))]
pub fn probe_iso_install_lab_flag() {}

/// Probe ESP for `isoreboot.txt` + stage `installdisk.bin` (second-boot lab).
#[cfg(target_os = "uefi")]
pub fn probe_iso_reboot_lab_flag() {
    use crate::boot::serial;
    use uefi::boot;
    use uefi::fs::FileSystem;

    let image = boot::image_handle();
    let Ok(sfs) = boot::get_image_file_system(image) else {
        return;
    };
    let mut fs = FileSystem::new(sfs);
    let flag = flag_present(&mut fs, "\\EFI\\RayNu\\isoreboot.txt")
        || flag_present(&mut fs, "\\isoreboot.txt");
    if !flag {
        return;
    }
    // Prefer namespaced install image.
    let staged = stage_from_esp(&mut fs, "\\EFI\\RayNu\\installdisk.bin")
        .or_else(|_| stage_from_esp(&mut fs, "\\installdisk.bin"));
    if staged.is_err() {
        serial::write_line("boot: E5 isoreboot.txt present but installdisk.bin missing");
        return;
    }
    arm_lab_reboot_contract();
    serial::write_line(M7_ISO_REBOOT_LAB_ARM_NOTE);
}

#[cfg(target_os = "uefi")]
fn stage_from_esp(fs: &mut uefi::fs::FileSystem, path: &str) -> Result<(), ()> {
    use uefi::CString16;
    let Ok(p) = CString16::try_from(path) else {
        return Err(());
    };
    let Ok(data) = fs.read(p.as_ref()) else {
        return Err(());
    };
    stage_install_disk_image(&data)
}

#[cfg(not(target_os = "uefi"))]
pub fn probe_iso_reboot_lab_flag() {}

/// How many bytes to write to ESP (LBA0+LBA1 only — never the live 1 MiB/64 MiB disk).
///
/// [`crate::devices::virtio_blk::init_with_image`] copies this prefix into the
/// larger RAM disk on the next boot (length need not match `disk_bytes`).
pub fn persist_image_len_for_contract(_disk_bytes: u64) -> usize {
    INSTALL_MARKER_PERSIST_BYTES
}

/// Stamp LBA0 (`DISK_PATTERN`) + LBA1 (`INSTALL_DISK_PATTERN`) into `buf`.
pub fn fill_persist_image(buf: &mut [u8]) -> bool {
    use crate::devices::virtio_blk::{DISK_PATTERN, INSTALL_DISK_PATTERN};
    if buf.len() < INSTALL_MARKER_PERSIST_BYTES {
        return false;
    }
    buf.fill(0);
    let sector = 512;
    for i in 0..(sector / 4) {
        let v = (DISK_PATTERN ^ (i as u32)).to_le_bytes();
        let off = i * 4;
        buf[off..off + 4].copy_from_slice(&v);
    }
    buf[0..4].copy_from_slice(&DISK_PATTERN.to_le_bytes());
    for i in 0..(sector / 4) {
        let v = (INSTALL_DISK_PATTERN ^ (i as u32)).to_le_bytes();
        let off = sector + i * 4;
        buf[off..off + 4].copy_from_slice(&v);
    }
    buf[sector..sector + 4].copy_from_slice(&INSTALL_DISK_PATTERN.to_le_bytes());
    true
}

/// Fill a 1 MiB buffer with host-stamped LBA0/LBA1 lab patterns (for persist image).
pub fn synthesize_lab_install_image(buf: &mut [u8]) -> bool {
    if buf.len() != LAB_INSTALL_DISK_BYTES as usize {
        return false;
    }
    fill_persist_image(buf)
}

fn infer_persist_disk_bytes() -> u64 {
    // Marker-only files are 1 KiB for both lab and iron. Prefer installsize.txt.
    // Default 1 MiB so QEMU `-m 512M` never allocates a surprise 64 MiB disk.
    LAB_INSTALL_DISK_BYTES
}

/// Write LBA stamps to ESP `installdisk.bin` (+ `installsize.txt`) while Boot Services live.
pub fn persist_armed_install_to_esp() {
    let Some(c) = peek_armed_install_contract() else {
        return;
    };
    let len = persist_image_len_for_contract(c.install_disk_bytes);
    if len == 0 || len > INSTALL_DISK_STAGE_CAP {
        return;
    }
    unsafe {
        if !fill_persist_image(&mut INSTALL_DISK_BUF[..len]) {
            return;
        }
    }
    #[cfg(target_os = "uefi")]
    {
        write_esp_persist_files(c.install_disk_bytes, unsafe { &INSTALL_DISK_BUF[..len] });
        unsafe {
            INSTALL_DISK_LEN = 0;
        }
    }
    #[cfg(not(target_os = "uefi"))]
    {
        let _ = len;
    }
}

#[cfg(target_os = "uefi")]
fn write_esp_persist_files(disk_bytes: u64, image: &[u8]) {
    use crate::boot::serial;
    use uefi::boot;
    use uefi::fs::FileSystem;
    use uefi::CString16;

    let image_handle = boot::image_handle();
    let Ok(sfs) = boot::get_image_file_system(image_handle) else {
        serial::write_line(M7_ISO_PERSIST_FAIL_NOTE);
        return;
    };
    let mut fs = FileSystem::new(sfs);
    let Ok(dir) = CString16::try_from("\\EFI\\RayNu") else {
        serial::write_line(M7_ISO_PERSIST_FAIL_NOTE);
        return;
    };
    let _ = fs.create_dir_all(dir.as_ref());
    let Ok(bin) = CString16::try_from("\\EFI\\RayNu\\installdisk.bin") else {
        serial::write_line(M7_ISO_PERSIST_FAIL_NOTE);
        return;
    };
    if fs.write(bin.as_ref(), image).is_err() {
        let Ok(alt) = CString16::try_from("\\installdisk.bin") else {
            serial::write_line(M7_ISO_PERSIST_FAIL_NOTE);
            return;
        };
        if fs.write(alt.as_ref(), image).is_err() {
            serial::write_line(M7_ISO_PERSIST_FAIL_NOTE);
            return;
        }
    }
    if let Ok(sz) = CString16::try_from("\\EFI\\RayNu\\installsize.txt") {
        let mut ascii = [0u8; 24];
        let n = write_u64_ascii(disk_bytes, &mut ascii);
        let _ = fs.write(sz.as_ref(), &ascii[..n]);
    }
    serial::write_str(M7_ISO_PERSIST_WRITE_NOTE);
    serial::write_str(" bytes=");
    write_serial_u64(image.len() as u64);
    serial::write_byte(b'\n');
}

fn write_u64_ascii(mut n: u64, out: &mut [u8]) -> usize {
    if out.is_empty() {
        return 0;
    }
    if n == 0 {
        out[0] = b'0';
        return 1;
    }
    let mut tmp = [0u8; 20];
    let mut i = 20;
    while n > 0 && i > 0 {
        i -= 1;
        tmp[i] = b'0' + (n % 10) as u8;
        n /= 10;
    }
    let len = 20 - i;
    let copy = core::cmp::min(len, out.len());
    out[..copy].copy_from_slice(&tmp[i..i + copy]);
    copy
}

#[cfg(target_os = "uefi")]
fn write_serial_u64(n: u64) {
    use crate::boot::serial;
    let mut ascii = [0u8; 24];
    let len = write_u64_ascii(n, &mut ascii);
    for &b in &ascii[..len] {
        serial::write_byte(b);
    }
}

#[cfg(target_os = "uefi")]
fn persist_size_from_esp(fs: &mut uefi::fs::FileSystem) -> Option<u64> {
    use uefi::CString16;
    let p = CString16::try_from("\\EFI\\RayNu\\installsize.txt").ok()?;
    let data = fs.read(p.as_ref()).ok()?;
    parse_decimal_u64(&data)
}

pub(crate) fn parse_decimal_u64(raw: &[u8]) -> Option<u64> {
    let mut n: u64 = 0;
    let mut seen = false;
    for &b in raw {
        if b == b'\n' || b == b'\r' || b == b' ' {
            if seen {
                break;
            }
            continue;
        }
        if !b.is_ascii_digit() {
            return None;
        }
        seen = true;
        n = n.saturating_mul(10).saturating_add((b - b'0') as u64);
    }
    if seen {
        Some(n)
    } else {
        None
    }
}

/// Host package: reboot lab preload + detect + BootedFromDisk phase.
pub fn prop_iso_reboot_lab_package() -> bool {
    clear_armed_install_contract();

    #[cfg(test)]
    {
        let mut img = alloc_lab_img();
        if !synthesize_lab_install_image(&mut img) {
            return false;
        }
        if stage_install_disk_image(&img).is_err() {
            return false;
        }
        if install_disk_preload_bytes().map(|b| b.len()) != Some(img.len()) {
            return false;
        }
        arm_lab_reboot_contract();
        if !lab_reboot_armed() || !install_disk_armed_for_launch() {
            return false;
        }
        if boot_install_phase() != InstallPhase::RebootPending {
            return false;
        }
        let mut disk = img.clone();
        // SAFETY: heap buffer as fake disk HPA for host package prop.
        unsafe {
            crate::devices::virtio_blk::init_with_image(
                0x1000_0000,
                disk.as_mut_ptr() as u64,
                disk.len(),
                Some(img.as_slice()),
            );
            crate::devices::virtio_blk::set_reboot_detect(true);
            let _ = crate::devices::virtio_blk::mmio_access(
                0x1000_0000 + crate::devices::virtio_blk::OFF_STATUS,
                true,
                crate::devices::virtio_blk::STATUS_DRIVER_OK,
            );
        }
        if !crate::devices::virtio_blk::booted_from_disk() {
            return false;
        }
        if !note_booted_from_disk_lab() {
            return false;
        }
        if boot_install_phase() != InstallPhase::BootedFromDisk {
            return false;
        }
    }

    #[cfg(not(test))]
    {
        arm_lab_reboot_contract();
        if !lab_reboot_armed() || boot_install_phase() != InstallPhase::RebootPending {
            clear_armed_install_contract();
            return false;
        }
    }

    let smoke = include_str!("../tools/m7-iso-install-qemu-smoke.sh");
    let runbook = include_str!("../docs/runbooks/iso_install.md");
    let ok = smoke.contains(M7_ISO_BOOTED_FROM_DISK_MARKER)
        && smoke.contains("ISO_REBOOT_LAB")
        && smoke.contains("isoreboot.txt")
        && smoke.contains("installdisk.bin")
        && smoke.contains("e5-lab-install.img")
        && smoke.contains("never print iron marker")
        && runbook.contains("isoreboot.txt")
        && runbook.contains("BOOTED-FROM-DISK")
        && M7_ISO_BOOTED_FROM_DISK_MARKER
            == crate::devices::virtio_blk::M7_ISO_BOOTED_FROM_DISK_MARKER
        && include_str!("../tools/run-qemu.sh").contains("ISO_REBOOT_LAB")
        && include_str!("../tools/synth-e5-lab-install-img.sh").contains("INSTALL_DISK_PATTERN")
        && include_str!("../src/main.rs").contains("probe_iso_reboot_lab_flag")
        && include_str!("../src/main.rs").contains("probe_iso_persist_reboot")
        && include_str!("../src/main.rs").contains("prefix_into=")
        && include_str!("../devices/virtio_blk.rs").contains("min(img.len(), disk_bytes)")
        && include_str!("../vmx/launch.rs").contains("M7_ISO_BOOTED_FROM_DISK_MARKER")
        && include_str!("../vmx/launch.rs").contains("note_booted_from_disk_lab")
        && smoke.contains("installdisk.bin")
        && runbook.contains("installdisk.bin");
    clear_armed_install_contract();
    ok
}

#[cfg(test)]
fn alloc_lab_img() -> Vec<u8> {
    vec![0u8; LAB_INSTALL_DISK_BYTES as usize]
}

/// Host package: ESP lab arm + DiskWritten → RebootPending (boot1).
pub fn prop_iso_install_lab_package() -> bool {
    clear_armed_install_contract();
    if disk_bytes_for_virtio_launch() != PROBE_DISK_BYTES {
        return false;
    }
    arm_lab_install_contract();
    if !lab_install_armed() {
        return false;
    }
    if disk_bytes_for_virtio_launch() != LAB_INSTALL_DISK_BYTES as usize {
        return false;
    }
    if !note_install_disk_written() {
        return false;
    }
    if boot_install_phase() != InstallPhase::DiskWritten {
        return false;
    }
    if !note_reboot_pending_lab() {
        return false;
    }
    if boot_install_phase() != InstallPhase::RebootPending {
        return false;
    }
    if !take_reboot_pending_latch() {
        return false;
    }
    if !take_install_disk_written_latch() {
        return false;
    }
    let smoke = include_str!("../tools/m7-iso-install-qemu-smoke.sh");
    let runbook = include_str!("../docs/runbooks/iso_install.md");
    let ok = smoke.contains(M7_ISO_INSTALL_LAB_OK_MARKER)
        && smoke.contains("ISO_INSTALL_LAB")
        && smoke.contains("isoinstall.txt")
        && smoke.contains(M7_ISO_DISK_WRITTEN_MARKER)
        && smoke.contains(M7_ISO_REBOOT_PENDING_MARKER)
        && smoke.contains(M7_ISO_BOOTED_FROM_DISK_MARKER)
        && smoke.contains("never print iron marker")
        && runbook.contains("isoinstall.txt")
        && runbook.contains("1MiB")
        && runbook.contains("REBOOT-PENDING")
        && LAB_INSTALL_DISK_BYTES == 1024 * 1024
        && M7_ISO_DISK_WRITTEN_MARKER == crate::devices::virtio_blk::M7_ISO_DISK_WRITTEN_MARKER
        && M7_ISO_INSTALL_LAB_OK_MARKER == crate::devices::virtio_blk::M7_ISO_INSTALL_LAB_OK_MARKER
        && M7_ISO_REBOOT_PENDING_MARKER == crate::devices::virtio_blk::M7_ISO_REBOOT_PENDING_MARKER
        && include_str!("../src/main.rs").contains("probe_iso_install_lab_flag")
        && include_str!("../src/main.rs").contains("probe_iso_persist_reboot")
        && include_str!("../tools/run-qemu.sh").contains("ISO_INSTALL_LAB")
        && smoke.contains("E5 persist")
        && runbook.contains("E5 persist")
        && include_str!("../vmx/launch.rs").contains("M7_ISO_REBOOT_PENDING_MARKER")
        && include_str!("../vmx/launch.rs").contains("note_reboot_pending_lab");
    clear_armed_install_contract();
    ok
}

/// Build launch contract from a ready M7.3 deploy plan.
pub fn launch_contract(deploy: &IsoDeployPlan) -> Result<InstallLaunchContract, IsoError> {
    if !deploy.is_ready() {
        return Err(IsoError::BadState);
    }
    Ok(InstallLaunchContract {
        iso_id: deploy.iso_id,
        extract_bound: deploy.extract_bound,
        install_disk_bytes: deploy.install_disk_bytes,
    })
}

/// Start install-to-disk from a ready deploy plan (or bind one first).
pub fn begin_install_to_disk(
    store: &ImageTable,
    install: &mut InstallToDiskPlan,
    iso_id: u64,
) -> Result<InstallLaunchContract, IsoError> {
    let _ = (
        M7_ISO_INSTALL_OK_MARKER,
        M7_ISO_INSTALL_SCAFFOLD_MARKER,
        ISO_INSTALL_GAP_NOTE,
        ISO_INSTALL_MVP_NOTE,
        ISO_INSTALL_HOST_LIMIT_NOTE,
    );
    if !install.deploy.is_ready() || install.deploy.iso_id != iso_id {
        bind_extract_boot(store, &mut install.deploy, iso_id)?;
        configure_install_disk(&mut install.deploy, DEFAULT_INSTALL_DISK_BYTES)?;
    }
    let contract = launch_contract(&install.deploy)?;
    install.phase = InstallPhase::ContractReady;
    arm_install_launch_contract(contract);
    persist_armed_install_to_esp();
    Ok(contract)
}

/// Record that the install disk received a guest/host write proof.
pub fn mark_disk_written(install: &mut InstallToDiskPlan) -> Result<(), IsoError> {
    if install.phase != InstallPhase::ContractReady && install.phase != InstallPhase::DiskWritten {
        return Err(IsoError::BadState);
    }
    install.phase = InstallPhase::DiskWritten;
    Ok(())
}

/// Request reboot-from-disk after a successful disk write.
pub fn mark_reboot_pending(install: &mut InstallToDiskPlan) -> Result<(), IsoError> {
    if install.phase != InstallPhase::DiskWritten {
        return Err(IsoError::BadState);
    }
    install.phase = InstallPhase::RebootPending;
    Ok(())
}

/// Record second boot from the install disk (QEMU/iron close step).
pub fn mark_booted_from_disk(install: &mut InstallToDiskPlan) -> Result<(), IsoError> {
    if install.phase != InstallPhase::RebootPending {
        return Err(IsoError::BadState);
    }
    install.phase = InstallPhase::BootedFromDisk;
    Ok(())
}

/// True when virtio-blk accepts the default install-disk size (capacity surface).
pub fn install_disk_capacity_ok(disk_bytes: u64) -> bool {
    disk_bytes > 0
        && disk_bytes % 512 == 0
        && disk_bytes >= DEFAULT_INSTALL_DISK_BYTES
        && devices_install_capacity_surface()
}

fn devices_install_capacity_surface() -> bool {
    let blk = include_str!("../devices/virtio_blk.rs");
    blk.contains("fn capacity_sectors_for(")
        && blk.contains("DEFAULT_INSTALL_DISK_BYTES")
        && blk.contains("unsafe fn init(")
}

/// True when guest extract-boot + install-disk surfaces exist for the contract.
pub fn install_launch_surfaces_present() -> bool {
    extract_boot_surface_present()
        && install_disk_surface_present()
        && devices_install_capacity_surface()
        && include_str!("../guest/linux_boot.rs").contains("fn load_bzimage_guest")
}

enum InstallOp {
    Status,
    Begin { id: u64 },
}

fn parse_u64(s: &str) -> Option<u64> {
    let mut n: u64 = 0;
    if s.is_empty() {
        return None;
    }
    for b in s.bytes() {
        if !(b'0'..=b'9').contains(&b) {
            return None;
        }
        n = n.checked_mul(10)?.checked_add(u64::from(b - b'0'))?;
    }
    Some(n)
}

fn route_install(method: RestMethod, path: &str) -> Result<InstallOp, ()> {
    let path = path.trim().trim_end_matches('/');
    if path == "/iso/install" {
        return match method {
            RestMethod::Get => Ok(InstallOp::Status),
            _ => Err(()),
        };
    }
    let rest = path.strip_prefix("/iso/").ok_or(())?;
    let mut segs = rest.split('/');
    let id_s = segs.next().ok_or(())?;
    let id = parse_u64(id_s).ok_or(())?;
    let action = segs.next();
    if segs.next().is_some() {
        return Err(());
    }
    match (method, action) {
        (RestMethod::Post, Some("install")) => Ok(InstallOp::Begin { id }),
        _ => Err(()),
    }
}

/// REST: `POST /iso/{id}/install` begins install-to-disk contract;
/// `GET /iso/install` returns Listed count `1` when contract ready (not iron-closed).
pub fn dispatch_iso_install_rest(
    store: &mut ImageTable,
    install: &mut InstallToDiskPlan,
    req: RestRequest<'_>,
) -> RestResponse {
    if !auth_allows(req.auth_token) {
        return RestResponse {
            status: 401,
            reply: None,
        };
    }
    match route_install(req.method, req.path) {
        Ok(InstallOp::Status) => RestResponse {
            status: 200,
            reply: Some(ApiReply::Listed {
                count: if install.is_contract_ready() { 1 } else { 0 },
            }),
        },
        Ok(InstallOp::Begin { id }) => {
            if store.get(id).is_none() {
                if let Err(e) = register_iso(store, id, 0, "distro.iso") {
                    return match e {
                        IsoError::Store(_) => RestResponse {
                            status: 409,
                            reply: None,
                        },
                        _ => RestResponse {
                            status: 500,
                            reply: None,
                        },
                    };
                }
            } else if store.get(id).map(|r| r.kind) != Some(ImageKind::Iso) {
                return RestResponse {
                    status: 409,
                    reply: None,
                };
            }
            match begin_install_to_disk(store, install, id) {
                Ok(_) => RestResponse {
                    status: 201,
                    reply: Some(ApiReply::Ok),
                },
                Err(IsoError::NotFound) => RestResponse {
                    status: 404,
                    reply: None,
                },
                Err(IsoError::BadState) | Err(IsoError::InvalidId) => RestResponse {
                    status: 409,
                    reply: None,
                },
                Err(_) => RestResponse {
                    status: 500,
                    reply: None,
                },
            }
        }
        Err(()) => RestResponse {
            status: 400,
            reply: None,
        },
    }
}

/// Host-testable install-to-disk **scaffold** package (not iron close).
pub fn prop_iso_install_package() -> bool {
    let _ = BRINGUP_AUTH_TOKEN;
    clear_armed_install_contract();
    if !install_launch_surfaces_present() {
        return false;
    }
    if !install_disk_capacity_ok(DEFAULT_INSTALL_DISK_BYTES) {
        return false;
    }
    if disk_bytes_for_virtio_launch() != PROBE_DISK_BYTES {
        return false;
    }

    let mut store = ImageTable::new();
    let mut install = InstallToDiskPlan::empty();
    if register_iso(&mut store, 7, 800_000_000, "debian.iso").is_err() {
        return false;
    }
    let contract = match begin_install_to_disk(&store, &mut install, 7) {
        Ok(c) => c,
        Err(_) => return false,
    };
    if contract.iso_id != 7
        || !contract.extract_bound
        || contract.install_disk_bytes != DEFAULT_INSTALL_DISK_BYTES
    {
        return false;
    }
    if install.phase != InstallPhase::ContractReady {
        return false;
    }
    if peek_armed_install_contract() != Some(contract) {
        return false;
    }
    if disk_bytes_for_virtio_launch() != DEFAULT_INSTALL_DISK_BYTES as usize {
        return false;
    }
    if !install_disk_armed_for_launch() {
        return false;
    }
    // Live path must name the wire helper (include_str gate in scaffold).
    if !include_str!("../src/main.rs").contains("disk_bytes_for_virtio_launch") {
        return false;
    }
    if !include_str!("../src/main.rs").contains("allocate_contiguous") {
        return false;
    }
    if mark_disk_written(&mut install).is_err() {
        return false;
    }
    if mark_reboot_pending(&mut install).is_err() {
        return false;
    }
    if mark_booted_from_disk(&mut install).is_err() {
        return false;
    }
    if !install.is_install_complete() {
        return false;
    }

    // Phase machine must reject out-of-order advances.
    let mut bad = InstallToDiskPlan::empty();
    if mark_reboot_pending(&mut bad).is_ok() {
        return false;
    }

    clear_armed_install_contract();
    let tok = Some(BRINGUP_AUTH_TOKEN);
    let mut store2 = ImageTable::new();
    let mut install2 = InstallToDiskPlan::empty();
    let began = dispatch_iso_install_rest(
        &mut store2,
        &mut install2,
        RestRequest {
            method: RestMethod::Post,
            path: "/iso/21/install",
            auth_token: tok,
        },
    );
    if began.status != 201 || !install2.is_contract_ready() {
        return false;
    }
    if disk_bytes_for_virtio_launch() != DEFAULT_INSTALL_DISK_BYTES as usize {
        return false;
    }
    let status = dispatch_iso_install_rest(
        &mut store2,
        &mut install2,
        RestRequest {
            method: RestMethod::Get,
            path: "/iso/install",
            auth_token: tok,
        },
    );
    let ok = status.status == 200
        && status.reply == Some(ApiReply::Listed { count: 1 })
        && ISO_INSTALL_GAP_NOTE.contains("CLOSED M7.7")
        && ISO_INSTALL_MVP_NOTE.contains("reboot-to-disk")
        && ISO_INSTALL_HOST_LIMIT_NOTE.contains("cannot close")
        && M7_ISO_INSTALL_SCAFFOLD_MARKER == "RAYNU-V-M7-ISO-INSTALL-SCAFFOLD-OK"
        && M7_ISO_INSTALL_OK_MARKER == "RAYNU-V-M7-ISO-INSTALL-OK";
    clear_armed_install_contract();
    ok
}

#[cfg(test)]
#[path = "iso_install_test.rs"]
mod iso_install_test;
