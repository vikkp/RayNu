//! E5 Stage 3 — guest UEFI firmware envelope (outside Proven Core).
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** (ADR-002 / ADR-003 / ADR-014)
//! VERIFICATION: N/A
//!
//! Boxes a sized guest-firmware envelope under ADR-003 (lazy/zstd, 15 MB /
//! 20 MB). The PE section `.asguefw` (ADR-003 `.assets.guefw`) holds the
//! envelope plus a **stub payload** (identity-lazy, `RAYNUFD`). That is
//! **not** EDK2 OVMF and does **not** VMLAUNCH guest UEFI.
//! `attach_cdrom_uefi` stays `UnsupportedOnFirmware`.

use core::sync::atomic::{AtomicBool, Ordering};

use crate::audit::AuditEvent;
use crate::audit_log;

use super::api::{auth_allows, ApiReply, RestMethod, RestRequest, RestResponse};

/// PE section name (COFF 8-char alias for ADR-003 `.assets.guefw`).
pub const SECTION_GUEST_FW: &str = ".asguefw";
/// ADR-003 long name for the guest firmware envelope.
pub const ADR003_GUEST_FW: &str = ".assets.guefw";

/// Envelope magic (`RAYNUFW` + NUL).
pub const GUEST_FW_MAGIC: [u8; 8] = *b"RAYNUFW\0";
/// Header bytes before an optional payload.
pub const GUEST_FW_HEADER_LEN: usize = 32;
/// Embedded envelope length (header + stub payload).
pub const GUEST_FW_BLOB_LEN: usize = 64;
/// In-tree stub payload length (not OVMF).
pub const GUEST_FW_STUB_PAYLOAD_LEN: u32 = 32;
/// Stub payload magic (`RAYNUFD` + NUL).
pub const GUEST_FW_PAYLOAD_MAGIC: [u8; 8] = *b"RAYNUFD\0";
/// ADR-003 uncompressed envelope cap (real OVMF later, lazy/zstd).
pub const GUEST_FW_MAX_UNCOMPRESSED: u32 = 4 * 1024 * 1024;
/// ADR-003 compressed / lazy envelope cap.
pub const GUEST_FW_MAX_COMPRESSED: u32 = 1024 * 1024;
/// Flag bit 0: payload is lazy/zstd (required).
pub const GUEST_FW_FLAG_LAZY_ZSTD: u32 = 1;
/// UEFI Firmware Volume signature (PI spec Vol 3).
pub const OVMF_FV_SIGNATURE: [u8; 4] = *b"_FVH";
/// Byte offset of `_FVH` in `EFI_FIRMWARE_VOLUME_HEADER`.
pub const OVMF_FV_SIG_OFF: usize = 0x28;
/// ADR-003 split-mode path for a real OVMF image (not embedded).
pub const OVMF_ESP_PATH: &str = "\\EFI\\RayNu\\OVMF.fd";
/// Host mock FV size (header + empty block map). Not a 4 MiB EDK2 image.
pub const MOCK_OVMF_FV_BYTES: usize = 80;
/// Minimum size-floor FV (larger than the 80-byte mock). Not EDK2.
pub const MIN_LAUNCH_FV_BYTES: usize = 4096;
/// Host size-floor fixture. Not a 4 MiB EDK2 `OVMF.fd`.
pub const SIZE_FLOOR_FV_BYTES: usize = 4096;
/// Minimum real EDK2 OVMF size. Size-floor stays below this.
pub const MIN_EDK2_OVMF_BYTES: usize = 1024 * 1024;
/// Minimum live-sized ESP map. EDK2-sized fixture stays below this.
pub const MIN_LIVE_ESP_OVMF_BYTES: usize = 2 * 1024 * 1024;
/// Minimum firmware-alias image. Live-sized map stays below this.
pub const MIN_FIRMWARE_ALIAS_BYTES: usize = 4 * 1024 * 1024;

const _: () = assert!(SIZE_FLOOR_FV_BYTES > MOCK_OVMF_FV_BYTES);
const _: () = assert!(SIZE_FLOOR_FV_BYTES == MIN_LAUNCH_FV_BYTES);
const _: () = assert!(SIZE_FLOOR_FV_BYTES < MIN_EDK2_OVMF_BYTES);
const _: () = assert!(MIN_EDK2_OVMF_BYTES < MIN_LIVE_ESP_OVMF_BYTES);
const _: () = assert!(MIN_LIVE_ESP_OVMF_BYTES < MIN_FIRMWARE_ALIAS_BYTES);
const _: () = assert!(MIN_FIRMWARE_ALIAS_BYTES <= GUEST_FW_MAX_UNCOMPRESSED as usize);

/// Envelope kind. Only the UEFI envelope exists in Stage 3.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuestFwKind {
    UefiEnvelope = 1,
}

/// Parsed / boxed guest firmware envelope. Not a live firmware image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GuestFwBlob {
    pub kind: GuestFwKind,
    pub uncompressed_len: u32,
    pub compressed_len: u32,
    pub payload_len: u32,
    pub lazy_zstd: bool,
    pub boxed: bool,
}

/// Error from envelope parse / box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuestFwError {
    BadMagic,
    BadState,
    TooLarge,
    /// Load requested before the envelope was boxed.
    NotBoxed,
    /// OVMF FV probe requested before the stub was loaded.
    NotLoaded,
    /// ESP load requested before the FV header was probed.
    NotProbed,
    /// ESP load requested with no fixture / staged bytes.
    MissingEsp,
    /// Slot arm requested before ESP load.
    NotEspLoaded,
    /// Guest bind requested before the firmware slot was armed.
    NotSlotArmed,
    /// Launch-prepare requested before the firmware guest was bound.
    NotGuestBound,
    /// In-tree 80-byte mock FV is refused for guest UEFI VMLAUNCH.
    MockFirmwareRefused,
    /// Size-floor stage requested with fewer bytes than [`MIN_LAUNCH_FV_BYTES`].
    TooSmall,
    /// Size-floor is staged but is not EDK2-sized; VMLAUNCH stays refused.
    NotRealFirmware,
    /// EDK2-sized candidate is staged; guest UEFI VMLAUNCH is not wired.
    LaunchNotWired,
    /// Live-sized ESP map is recorded; VMLAUNCH instruction is not issued.
    LiveMappedNotLaunched,
    /// Live map is present but the image has no JMP FAR reset-vector stub.
    NoResetVector,
    /// Reset-vector VMCS contract is recorded; VMLAUNCH instruction is not issued.
    ResetVectorNotLaunched,
    /// Firmware-alias contract is recorded; VMLAUNCH instruction is not issued.
    FirmwareAliasNotLaunched,
    /// Alias-EPT program contract is recorded; VMLAUNCH instruction is not issued.
    AliasEptNotLaunched,
    /// Private alias-EPT install is recorded; VMLAUNCH instruction is not issued.
    AliasEptInstalledNotLaunched,
    /// Real-ESP VMLAUNCH-ready contract is recorded; VMLAUNCH instruction is not issued.
    RealEspNotLaunched,
    /// Guest-UEFI VMLAUNCH insn path is armed; instruction is not issued.
    RealLaunchNotIssued,
    /// Live ESP `\EFI\RayNu\OVMF.fd` bytes are required; instruction is not issued.
    LiveEspRequired,
    /// Private guest-UEFI VMCS is selected (not E4 SHELL); instruction is not issued.
    PrivateVmcsNotLaunched,
    /// Live-ESP VMLAUNCH issue path is armed; live ESP bytes are still absent.
    LiveEspBytesNotPresent,
    /// Live ESP `\EFI\RayNu\OVMF.fd` bytes were probed; they are still absent.
    LiveEspBytesAbsent,
    /// A real ESP `\EFI\RayNu\OVMF.fd` is required; the heap fixture is not that file.
    LiveEspFdAbsent,
    /// Real ESP `\EFI\RayNu\OVMF.fd` bytes were presented; they are still absent.
    LiveEspPresentAbsent,
    /// Real ESP `\EFI\RayNu\OVMF.fd` bytes were admitted; they are still absent.
    LiveEspAdmitAbsent,
    /// Real ESP `\EFI\RayNu\OVMF.fd` bytes were read-attempted; they are still absent.
    LiveEspReadAbsent,
    /// Real ESP `\EFI\RayNu\OVMF.fd` bytes were copy-attempted; they are still absent.
    LiveEspCopyAbsent,
    /// Real ESP `\EFI\RayNu\OVMF.fd` bytes were place-attempted; they are still absent.
    LiveEspPlaceAbsent,
    /// Real ESP `\EFI\RayNu\OVMF.fd` bytes were apply-attempted; they are still absent.
    LiveEspApplyAbsent,
    /// Real ESP `\EFI\RayNu\OVMF.fd` bytes were commit-attempted; they are still absent.
    LiveEspCommitAbsent,
    /// Real ESP `\EFI\RayNu\OVMF.fd` bytes were latch-attempted; they are still absent.
    LiveEspLatchAbsent,
}

/// ESP-path launch bookkeeping. Not a live OVMF mapping / not VMLAUNCH.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OvmfEspLaunch {
    pub guest_id: u8,
    pub slot_id: u8,
}

/// Probed UEFI Firmware Volume (host mock or ESP image). Not VMLAUNCH.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OvmfFv {
    pub fv_len: u64,
    pub header_len: u16,
}

/// Guest firmware slot bookkeeping. Not a live VMCS / not VMLAUNCH.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OvmfSlot {
    pub slot_id: u8,
}

/// Host slot id for the ESP-loaded OVMF fixture (single guest).
pub const OVMF_FW_SLOT_ID: u8 = 1;
/// Guest id bound to the armed firmware slot (bookkeeping). Not VMLAUNCH.
pub const OVMF_FW_GUEST_ID: u8 = 1;

/// Firmware-to-guest bind. Not a live UEFI VMLAUNCH.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OvmfBind {
    pub guest_id: u8,
    pub slot_id: u8,
}

/// Firmware launch-prepare bookkeeping. Not a live UEFI VMLAUNCH.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OvmfLaunchPrep {
    pub guest_id: u8,
    pub slot_id: u8,
}

/// Size-floor FV bookkeeping. Not EDK2 and not a live UEFI VMLAUNCH.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OvmfFloor {
    pub bytes_len: u64,
}

/// EDK2-sized FV bookkeeping. Not a shipped `OVMF.fd` and not VMLAUNCH.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OvmfEdk2 {
    pub bytes_len: u64,
}

/// Live-sized ESP map bookkeeping. Not a shipped `OVMF.fd` and not VMLAUNCH.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OvmfLiveMap {
    pub bytes_len: u64,
}

/// Reset-vector VMCS bookkeeping. Not a shipped `OVMF.fd` and not VMLAUNCH.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OvmfResetVec {
    pub bytes_len: u64,
}

/// Firmware-alias bookkeeping. Not a shipped `OVMF.fd` and not VMLAUNCH.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OvmfAlias {
    pub bytes_len: u64,
}

/// Alias-EPT program bookkeeping. Not a live EPT write and not VMLAUNCH.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OvmfAliasEpt {
    pub bytes_len: u64,
    pub gpa: u64,
}

/// Private alias-EPT install bookkeeping. Not a live E4 SHELL EPT write and not VMLAUNCH.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OvmfAliasEptInstall {
    pub bytes_len: u64,
    pub gpa: u64,
}

/// Real-ESP VMLAUNCH-ready bookkeeping. Not a shipped `OVMF.fd` and not VMLAUNCH.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OvmfRealEsp {
    pub bytes_len: u64,
    pub gpa: u64,
}

/// Guest-UEFI VMLAUNCH insn-path bookkeeping. Not a shipped `OVMF.fd` and not VMLAUNCH.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OvmfRealLaunch {
    pub bytes_len: u64,
    pub gpa: u64,
}

/// Live-ESP execute bookkeeping. Not a shipped `OVMF.fd` and not VMLAUNCH.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OvmfLiveExec {
    pub bytes_len: u64,
    pub gpa: u64,
}

/// Private guest-UEFI VMCS bookkeeping. Not the E4 SHELL VMCS and not VMLAUNCH.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OvmfPrivateVmcs {
    pub bytes_len: u64,
    pub gpa: u64,
}

/// Live-ESP VMLAUNCH issue bookkeeping. Not a shipped `OVMF.fd` and not VMLAUNCH.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OvmfLiveIssue {
    pub bytes_len: u64,
    pub gpa: u64,
}

/// Live-ESP bytes probe bookkeeping. Not a shipped `OVMF.fd` and not VMLAUNCH.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OvmfLiveBytes {
    pub bytes_len: u64,
    pub gpa: u64,
}

/// Live-ESP FD require bookkeeping. Not a shipped `OVMF.fd` and not VMLAUNCH.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OvmfLiveFd {
    pub bytes_len: u64,
    pub gpa: u64,
}

/// Live-ESP present bookkeeping. Not a shipped `OVMF.fd` and not VMLAUNCH.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OvmfLivePresent {
    pub bytes_len: u64,
    pub gpa: u64,
}

/// Live-ESP admit bookkeeping. Not a shipped `OVMF.fd` and not VMLAUNCH.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OvmfLiveAdmit {
    pub bytes_len: u64,
    pub gpa: u64,
}

/// Live-ESP read bookkeeping. Not a shipped `OVMF.fd` and not VMLAUNCH.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OvmfLiveRead {
    pub bytes_len: u64,
    pub gpa: u64,
}

/// Live-ESP copy bookkeeping. Not a shipped `OVMF.fd` and not VMLAUNCH.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OvmfLiveCopy {
    pub bytes_len: u64,
    pub gpa: u64,
}

/// Live-ESP place bookkeeping. Not a shipped `OVMF.fd` and not VMLAUNCH.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OvmfLivePlace {
    pub bytes_len: u64,
    pub gpa: u64,
}

/// Live-ESP apply bookkeeping. Not a shipped `OVMF.fd` and not VMLAUNCH.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OvmfLiveApply {
    pub bytes_len: u64,
    pub gpa: u64,
}

/// Live-ESP commit bookkeeping. Not a shipped `OVMF.fd` and not VMLAUNCH.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OvmfLiveCommit {
    pub bytes_len: u64,
    pub gpa: u64,
}

/// Live-ESP latch bookkeeping. Not a shipped `OVMF.fd` and not VMLAUNCH.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OvmfLiveLatch {
    pub bytes_len: u64,
    pub gpa: u64,
}

// `include_bytes!(…).len()` is const — keeps the PE section sized to the asset.
#[link_section = ".asguefw"]
#[used]
static PE_GUEST_FW: [u8; include_bytes!("../assets/guest_fw.bin").len()] =
    *include_bytes!("../assets/guest_fw.bin");

/// Embedded envelope bytes (header + stub payload; not OVMF).
pub fn guest_fw_bytes() -> &'static [u8] {
    &PE_GUEST_FW[..]
}

/// JUSTIFICATION (global state): HTTP listen loops must not grow another
/// argument (HOST-NIC FIN-close). Boxing / load are process-local flags.
static GUEST_FW_BOXED: AtomicBool = AtomicBool::new(false);
static GUEST_FW_LOADED: AtomicBool = AtomicBool::new(false);
static GUEST_FW_OVMF_PROBED: AtomicBool = AtomicBool::new(false);
static GUEST_FW_OVMF_ESP_LOADED: AtomicBool = AtomicBool::new(false);
static GUEST_FW_OVMF_SLOT_ARMED: AtomicBool = AtomicBool::new(false);
static GUEST_FW_OVMF_GUEST_BOUND: AtomicBool = AtomicBool::new(false);
static GUEST_FW_OVMF_LAUNCH_PREPPED: AtomicBool = AtomicBool::new(false);
static GUEST_FW_OVMF_FLOOR_STAGED: AtomicBool = AtomicBool::new(false);
static GUEST_FW_OVMF_EDK2_STAGED: AtomicBool = AtomicBool::new(false);
static GUEST_FW_OVMF_ESP_LAUNCH_ARMED: AtomicBool = AtomicBool::new(false);
static GUEST_FW_OVMF_LIVE_MAPPED: AtomicBool = AtomicBool::new(false);
static GUEST_FW_OVMF_RESET_ARMED: AtomicBool = AtomicBool::new(false);
static GUEST_FW_OVMF_ALIAS_ARMED: AtomicBool = AtomicBool::new(false);
static GUEST_FW_OVMF_ALIAS_EPT_PROGRAMMED: AtomicBool = AtomicBool::new(false);
static GUEST_FW_OVMF_ALIAS_EPT_INSTALLED: AtomicBool = AtomicBool::new(false);
static GUEST_FW_OVMF_REAL_ESP_QUALIFIED: AtomicBool = AtomicBool::new(false);
static GUEST_FW_OVMF_REAL_LAUNCH_ARMED: AtomicBool = AtomicBool::new(false);
static GUEST_FW_OVMF_LIVE_ESP_REQUIRED: AtomicBool = AtomicBool::new(false);
static GUEST_FW_OVMF_PRIVATE_VMCS_ARMED: AtomicBool = AtomicBool::new(false);
static GUEST_FW_OVMF_LIVE_ISSUE_ARMED: AtomicBool = AtomicBool::new(false);
static GUEST_FW_OVMF_LIVE_BYTES_PROBED: AtomicBool = AtomicBool::new(false);
static GUEST_FW_OVMF_LIVE_FD_REQUIRED: AtomicBool = AtomicBool::new(false);
static GUEST_FW_OVMF_LIVE_ESP_PRESENTED: AtomicBool = AtomicBool::new(false);
static GUEST_FW_OVMF_LIVE_ESP_ADMITTED: AtomicBool = AtomicBool::new(false);
static GUEST_FW_OVMF_LIVE_ESP_READ: AtomicBool = AtomicBool::new(false);
static GUEST_FW_OVMF_LIVE_ESP_COPIED: AtomicBool = AtomicBool::new(false);
static GUEST_FW_OVMF_LIVE_ESP_PLACED: AtomicBool = AtomicBool::new(false);
static GUEST_FW_OVMF_LIVE_ESP_APPLIED: AtomicBool = AtomicBool::new(false);
static GUEST_FW_OVMF_LIVE_ESP_COMMITTED: AtomicBool = AtomicBool::new(false);
static GUEST_FW_OVMF_LIVE_ESP_LATCHED: AtomicBool = AtomicBool::new(false);

/// Reset the process-local boxed / loaded / probed / ESP / slot / bind / prep / floor / EDK2 / ESP-launch / live-map / reset-vector / alias / alias-EPT / EPT-install / real-ESP / live-exec / private-VMCS / live-issue / live-bytes / live-fd / live-present / live-admit / live-read / live-copy / live-place / live-apply / live-commit / live-latch flags.
pub fn reset_guest_fw() {
    GUEST_FW_BOXED.store(false, Ordering::Release);
    GUEST_FW_LOADED.store(false, Ordering::Release);
    GUEST_FW_OVMF_PROBED.store(false, Ordering::Release);
    GUEST_FW_OVMF_ESP_LOADED.store(false, Ordering::Release);
    GUEST_FW_OVMF_SLOT_ARMED.store(false, Ordering::Release);
    GUEST_FW_OVMF_GUEST_BOUND.store(false, Ordering::Release);
    GUEST_FW_OVMF_LAUNCH_PREPPED.store(false, Ordering::Release);
    GUEST_FW_OVMF_FLOOR_STAGED.store(false, Ordering::Release);
    GUEST_FW_OVMF_EDK2_STAGED.store(false, Ordering::Release);
    GUEST_FW_OVMF_ESP_LAUNCH_ARMED.store(false, Ordering::Release);
    GUEST_FW_OVMF_LIVE_MAPPED.store(false, Ordering::Release);
    GUEST_FW_OVMF_RESET_ARMED.store(false, Ordering::Release);
    GUEST_FW_OVMF_ALIAS_ARMED.store(false, Ordering::Release);
    GUEST_FW_OVMF_ALIAS_EPT_PROGRAMMED.store(false, Ordering::Release);
    GUEST_FW_OVMF_ALIAS_EPT_INSTALLED.store(false, Ordering::Release);
    GUEST_FW_OVMF_REAL_ESP_QUALIFIED.store(false, Ordering::Release);
    GUEST_FW_OVMF_REAL_LAUNCH_ARMED.store(false, Ordering::Release);
    GUEST_FW_OVMF_LIVE_ESP_REQUIRED.store(false, Ordering::Release);
    GUEST_FW_OVMF_PRIVATE_VMCS_ARMED.store(false, Ordering::Release);
    GUEST_FW_OVMF_LIVE_ISSUE_ARMED.store(false, Ordering::Release);
    GUEST_FW_OVMF_LIVE_BYTES_PROBED.store(false, Ordering::Release);
    GUEST_FW_OVMF_LIVE_FD_REQUIRED.store(false, Ordering::Release);
    GUEST_FW_OVMF_LIVE_ESP_PRESENTED.store(false, Ordering::Release);
    GUEST_FW_OVMF_LIVE_ESP_ADMITTED.store(false, Ordering::Release);
    GUEST_FW_OVMF_LIVE_ESP_READ.store(false, Ordering::Release);
    GUEST_FW_OVMF_LIVE_ESP_COPIED.store(false, Ordering::Release);
    GUEST_FW_OVMF_LIVE_ESP_PLACED.store(false, Ordering::Release);
    GUEST_FW_OVMF_LIVE_ESP_APPLIED.store(false, Ordering::Release);
    GUEST_FW_OVMF_LIVE_ESP_COMMITTED.store(false, Ordering::Release);
    GUEST_FW_OVMF_LIVE_ESP_LATCHED.store(false, Ordering::Release);
    crate::vmx::launch::reset_live_esp_ovmf_mapping();
}

/// True after a successful [`box_guest_firmware`].
pub fn guest_fw_is_boxed() -> bool {
    GUEST_FW_BOXED.load(Ordering::Acquire)
}

/// True after a successful [`load_guest_firmware`].
pub fn guest_fw_is_loaded() -> bool {
    GUEST_FW_LOADED.load(Ordering::Acquire)
}

/// True after a successful [`probe_ovmf_firmware`].
pub fn ovmf_fv_is_probed() -> bool {
    GUEST_FW_OVMF_PROBED.load(Ordering::Acquire)
}

/// True after a successful [`load_ovmf_from_esp`].
pub fn ovmf_esp_is_loaded() -> bool {
    GUEST_FW_OVMF_ESP_LOADED.load(Ordering::Acquire)
}

/// True after a successful [`arm_ovmf_firmware_slot`].
pub fn ovmf_slot_is_armed() -> bool {
    GUEST_FW_OVMF_SLOT_ARMED.load(Ordering::Acquire)
}

/// True after a successful [`bind_ovmf_firmware_guest`].
pub fn ovmf_guest_is_bound() -> bool {
    GUEST_FW_OVMF_GUEST_BOUND.load(Ordering::Acquire)
}

/// True after a successful [`prepare_ovmf_firmware_launch`].
pub fn ovmf_launch_is_prepared() -> bool {
    GUEST_FW_OVMF_LAUNCH_PREPPED.load(Ordering::Acquire)
}

/// True after a successful [`stage_ovmf_firmware_floor`].
pub fn ovmf_floor_is_staged() -> bool {
    GUEST_FW_OVMF_FLOOR_STAGED.load(Ordering::Acquire)
}

/// True after a successful [`stage_edk2_ovmf_firmware`].
pub fn ovmf_edk2_is_staged() -> bool {
    GUEST_FW_OVMF_EDK2_STAGED.load(Ordering::Acquire)
}

/// True after a successful [`arm_ovmf_esp_launch`].
pub fn ovmf_esp_launch_is_armed() -> bool {
    GUEST_FW_OVMF_ESP_LAUNCH_ARMED.load(Ordering::Acquire)
}

/// True after a successful [`map_live_esp_ovmf`].
pub fn ovmf_live_esp_is_mapped() -> bool {
    GUEST_FW_OVMF_LIVE_MAPPED.load(Ordering::Acquire)
}

/// True after a successful [`arm_ovmf_reset_vector`].
pub fn ovmf_reset_vector_is_armed() -> bool {
    GUEST_FW_OVMF_RESET_ARMED.load(Ordering::Acquire)
}

/// True after a successful [`arm_ovmf_firmware_alias`].
pub fn ovmf_firmware_alias_is_armed() -> bool {
    GUEST_FW_OVMF_ALIAS_ARMED.load(Ordering::Acquire)
}

/// True after a successful [`program_ovmf_alias_ept`].
pub fn ovmf_alias_ept_is_programmed() -> bool {
    GUEST_FW_OVMF_ALIAS_EPT_PROGRAMMED.load(Ordering::Acquire)
}

/// True after a successful [`install_ovmf_alias_ept`].
pub fn ovmf_alias_ept_is_installed() -> bool {
    GUEST_FW_OVMF_ALIAS_EPT_INSTALLED.load(Ordering::Acquire)
}

/// True after a successful [`qualify_real_esp_ovmf`].
pub fn ovmf_real_esp_is_qualified() -> bool {
    GUEST_FW_OVMF_REAL_ESP_QUALIFIED.load(Ordering::Acquire)
}

/// True after a successful [`arm_ovmf_real_launch`].
pub fn ovmf_real_launch_is_armed() -> bool {
    GUEST_FW_OVMF_REAL_LAUNCH_ARMED.load(Ordering::Acquire)
}

/// True after a successful [`require_ovmf_live_esp`].
pub fn ovmf_live_esp_is_required() -> bool {
    GUEST_FW_OVMF_LIVE_ESP_REQUIRED.load(Ordering::Acquire)
}

/// True after a successful [`arm_ovmf_private_vmcs`].
pub fn ovmf_private_vmcs_is_armed() -> bool {
    GUEST_FW_OVMF_PRIVATE_VMCS_ARMED.load(Ordering::Acquire)
}

/// True after a successful [`arm_ovmf_live_issue`].
pub fn ovmf_live_issue_is_armed() -> bool {
    GUEST_FW_OVMF_LIVE_ISSUE_ARMED.load(Ordering::Acquire)
}

/// True after a successful [`probe_ovmf_live_bytes`].
pub fn ovmf_live_bytes_is_probed() -> bool {
    GUEST_FW_OVMF_LIVE_BYTES_PROBED.load(Ordering::Acquire)
}

/// True after a successful [`require_ovmf_live_fd`].
pub fn ovmf_live_fd_is_required() -> bool {
    GUEST_FW_OVMF_LIVE_FD_REQUIRED.load(Ordering::Acquire)
}

/// True after a successful [`present_ovmf_live_esp`].
pub fn ovmf_live_esp_is_presented() -> bool {
    GUEST_FW_OVMF_LIVE_ESP_PRESENTED.load(Ordering::Acquire)
}

/// True after a successful [`admit_ovmf_live_esp`].
pub fn ovmf_live_esp_is_admitted() -> bool {
    GUEST_FW_OVMF_LIVE_ESP_ADMITTED.load(Ordering::Acquire)
}

/// True after a successful [`read_ovmf_live_esp`].
pub fn ovmf_live_esp_is_read() -> bool {
    GUEST_FW_OVMF_LIVE_ESP_READ.load(Ordering::Acquire)
}

/// True after a successful [`copy_ovmf_live_esp`].
pub fn ovmf_live_esp_is_copied() -> bool {
    GUEST_FW_OVMF_LIVE_ESP_COPIED.load(Ordering::Acquire)
}

/// True after a successful [`place_ovmf_live_esp`].
pub fn ovmf_live_esp_is_placed() -> bool {
    GUEST_FW_OVMF_LIVE_ESP_PLACED.load(Ordering::Acquire)
}

/// True after a successful [`apply_ovmf_live_esp`].
pub fn ovmf_live_esp_is_applied() -> bool {
    GUEST_FW_OVMF_LIVE_ESP_APPLIED.load(Ordering::Acquire)
}

/// True after a successful [`commit_ovmf_live_esp`].
pub fn ovmf_live_esp_is_committed() -> bool {
    GUEST_FW_OVMF_LIVE_ESP_COMMITTED.load(Ordering::Acquire)
}

/// True after a successful [`latch_ovmf_live_esp`].
pub fn ovmf_live_esp_is_latched() -> bool {
    GUEST_FW_OVMF_LIVE_ESP_LATCHED.load(Ordering::Acquire)
}

fn read_u32_le(bytes: &[u8], off: usize) -> Option<u32> {
    let slice = bytes.get(off..off.saturating_add(4))?;
    Some(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

/// Parse an ADR-003 guest firmware envelope. Does not box and does not VMLAUNCH.
///
/// INVARIANTS:
/// - Requires `GUEST_FW_MAGIC` and version 1
/// - `uncompressed_len` / `compressed_len` stay inside ADR-003 caps
/// - `payload_len` must fit in `bytes` after the header
/// - Payload, when present, must fit after the header (not treated as OVMF)
pub fn parse_guest_fw(bytes: &[u8]) -> Result<GuestFwBlob, GuestFwError> {
    if bytes.len() < GUEST_FW_HEADER_LEN {
        return Err(GuestFwError::BadMagic);
    }
    if bytes[..8] != GUEST_FW_MAGIC {
        return Err(GuestFwError::BadMagic);
    }
    let version = read_u32_le(bytes, 8).ok_or(GuestFwError::BadMagic)?;
    let kind_n = read_u32_le(bytes, 12).ok_or(GuestFwError::BadMagic)?;
    let uncompressed = read_u32_le(bytes, 16).ok_or(GuestFwError::BadMagic)?;
    let compressed = read_u32_le(bytes, 20).ok_or(GuestFwError::BadMagic)?;
    let flags = read_u32_le(bytes, 24).ok_or(GuestFwError::BadMagic)?;
    let payload_len = read_u32_le(bytes, 28).ok_or(GuestFwError::BadMagic)?;
    if version != 1 || kind_n != GuestFwKind::UefiEnvelope as u32 {
        return Err(GuestFwError::BadState);
    }
    if (flags & GUEST_FW_FLAG_LAZY_ZSTD) == 0 {
        return Err(GuestFwError::BadState);
    }
    if uncompressed == 0
        || uncompressed > GUEST_FW_MAX_UNCOMPRESSED
        || compressed == 0
        || compressed > GUEST_FW_MAX_COMPRESSED
        || compressed > uncompressed
    {
        return Err(GuestFwError::TooLarge);
    }
    let need = GUEST_FW_HEADER_LEN.saturating_add(payload_len as usize);
    if need > bytes.len() {
        return Err(GuestFwError::BadState);
    }
    if payload_len > uncompressed {
        return Err(GuestFwError::TooLarge);
    }
    Ok(GuestFwBlob {
        kind: GuestFwKind::UefiEnvelope,
        uncompressed_len: uncompressed,
        compressed_len: compressed,
        payload_len,
        lazy_zstd: true,
        boxed: false,
    })
}

/// Validate and box a guest firmware envelope (ADR-014 Stage 3).
///
/// INVARIANTS:
/// - On success the process-local boxed flag is set
/// - Returned `boxed` is true
/// - Does not change [`crate::mgmt::iso::attach_cdrom_uefi`]
/// - Does not VMLAUNCH and does not load OVMF
pub fn box_guest_firmware(bytes: &[u8]) -> Result<GuestFwBlob, GuestFwError> {
    let parsed = parse_guest_fw(bytes)?;
    audit_log!(AuditEvent::GuestFirmwareBoxed {
        uncompressed_len: u64::from(parsed.uncompressed_len),
        compressed_len: u64::from(parsed.compressed_len),
    });
    GUEST_FW_BOXED.store(true, Ordering::Release);
    Ok(GuestFwBlob {
        boxed: true,
        ..parsed
    })
}

/// Slice the stub payload from a parsed envelope. Identity-lazy (no zstd crate).
///
/// INVARIANTS:
/// - Requires `payload_len > 0` and `RAYNUFD` magic
/// - Does not allocate
/// - Does not VMLAUNCH and does not treat the stub as OVMF
pub fn guest_fw_payload(bytes: &[u8]) -> Result<&[u8], GuestFwError> {
    let parsed = parse_guest_fw(bytes)?;
    if parsed.payload_len == 0 {
        return Err(GuestFwError::BadState);
    }
    let start = GUEST_FW_HEADER_LEN;
    let end = start.saturating_add(parsed.payload_len as usize);
    let payload = bytes.get(start..end).ok_or(GuestFwError::BadState)?;
    if payload.len() < 8 || payload[..8] != GUEST_FW_PAYLOAD_MAGIC {
        return Err(GuestFwError::BadMagic);
    }
    Ok(payload)
}

/// Lazy-load the boxed stub payload (ADR-014 Stage 4).
///
/// INVARIANTS:
/// - Requires a prior successful [`box_guest_firmware`]
/// - On success the process-local loaded flag is set
/// - Does not change [`crate::mgmt::iso::attach_cdrom_uefi`]
/// - Does not VMLAUNCH and does not load OVMF
pub fn load_guest_firmware(bytes: &[u8]) -> Result<GuestFwBlob, GuestFwError> {
    if !guest_fw_is_boxed() {
        return Err(GuestFwError::NotBoxed);
    }
    let payload = guest_fw_payload(bytes)?;
    let mut parsed = parse_guest_fw(bytes)?;
    parsed.boxed = true;
    audit_log!(AuditEvent::GuestFirmwareLoaded {
        payload_len: u64::from(parsed.payload_len),
    });
    let _ = payload;
    GUEST_FW_LOADED.store(true, Ordering::Release);
    Ok(parsed)
}

fn read_u16_le(bytes: &[u8], off: usize) -> Option<u16> {
    let slice = bytes.get(off..off.saturating_add(2))?;
    Some(u16::from_le_bytes([slice[0], slice[1]]))
}

fn read_u64_le(bytes: &[u8], off: usize) -> Option<u64> {
    let slice = bytes.get(off..off.saturating_add(8))?;
    Some(u64::from_le_bytes([
        slice[0], slice[1], slice[2], slice[3], slice[4], slice[5], slice[6], slice[7],
    ]))
}

/// Probe a UEFI Firmware Volume header (PI Vol 3). Not VMLAUNCH.
///
/// INVARIANTS:
/// - Requires `_FVH` at offset `0x28`
/// - `HeaderLength >= 0x38` and `FvLength` inside ADR-003 uncompressed cap
/// - `FvLength` must be fully present in `bytes` (host mock is 80 bytes)
/// - Does not treat the probe as an embedded EDK2 OVMF ship image
pub fn probe_ovmf_fv(bytes: &[u8]) -> Result<OvmfFv, GuestFwError> {
    let sig_end = OVMF_FV_SIG_OFF.saturating_add(4);
    let sig = bytes
        .get(OVMF_FV_SIG_OFF..sig_end)
        .ok_or(GuestFwError::BadMagic)?;
    if sig != OVMF_FV_SIGNATURE {
        return Err(GuestFwError::BadMagic);
    }
    let fv_len = read_u64_le(bytes, 0x20).ok_or(GuestFwError::BadMagic)?;
    let header_len = read_u16_le(bytes, 0x30).ok_or(GuestFwError::BadMagic)?;
    if header_len < 0x38 || u64::from(header_len) > fv_len {
        return Err(GuestFwError::BadState);
    }
    if fv_len == 0 || fv_len > u64::from(GUEST_FW_MAX_UNCOMPRESSED) {
        return Err(GuestFwError::TooLarge);
    }
    if fv_len > bytes.len() as u64 {
        return Err(GuestFwError::BadState);
    }
    Ok(OvmfFv { fv_len, header_len })
}

/// Probe an OVMF-style FV after the stub is loaded (ADR-014 Stage 5).
///
/// INVARIANTS:
/// - Requires a prior successful [`load_guest_firmware`]
/// - Host REST uses [`write_mock_ovmf_fv`], not a 4 MiB EDK2 image
/// - Real bytes stay on ESP [`OVMF_ESP_PATH`] (ADR-003 split-mode)
/// - Does not VMLAUNCH and does not flip [`crate::mgmt::iso::attach_cdrom_uefi`]
pub fn probe_ovmf_firmware(bytes: &[u8]) -> Result<OvmfFv, GuestFwError> {
    if !guest_fw_is_loaded() {
        return Err(GuestFwError::NotLoaded);
    }
    let probed = probe_ovmf_fv(bytes)?;
    audit_log!(AuditEvent::OvmfFirmwareProbed {
        fv_len: probed.fv_len,
    });
    GUEST_FW_OVMF_PROBED.store(true, Ordering::Release);
    Ok(probed)
}

/// Load ESP split-mode OVMF bytes after the FV header was probed (ADR-014 Stage 6).
///
/// INVARIANTS:
/// - Requires a prior successful [`probe_ovmf_firmware`]
/// - Host REST uses [`write_mock_ovmf_fv`] as the ESP fixture (not a 4 MiB EDK2 image)
/// - Real bytes stay on ESP [`OVMF_ESP_PATH`] (ADR-003 split-mode)
/// - Does not VMLAUNCH and does not flip [`crate::mgmt::iso::attach_cdrom_uefi`]
pub fn load_ovmf_from_esp(bytes: &[u8]) -> Result<OvmfFv, GuestFwError> {
    if !ovmf_fv_is_probed() {
        return Err(GuestFwError::NotProbed);
    }
    if bytes.is_empty() {
        return Err(GuestFwError::MissingEsp);
    }
    let loaded = probe_ovmf_fv(bytes)?;
    audit_log!(AuditEvent::OvmfFirmwareEspLoaded {
        bytes_len: bytes.len() as u64,
        fv_len: loaded.fv_len,
    });
    GUEST_FW_OVMF_ESP_LOADED.store(true, Ordering::Release);
    Ok(loaded)
}

/// Arm guest firmware slot 1 after ESP load (ADR-014 Stage 7).
///
/// INVARIANTS:
/// - Requires a prior successful [`load_ovmf_from_esp`]
/// - Records slot bookkeeping only
/// - Does not VMLAUNCH and does not flip [`crate::mgmt::iso::attach_cdrom_uefi`]
pub fn arm_ovmf_firmware_slot() -> Result<OvmfSlot, GuestFwError> {
    if !ovmf_esp_is_loaded() {
        return Err(GuestFwError::NotEspLoaded);
    }
    audit_log!(AuditEvent::OvmfFirmwareSlotArmed {
        slot_id: u64::from(OVMF_FW_SLOT_ID),
    });
    GUEST_FW_OVMF_SLOT_ARMED.store(true, Ordering::Release);
    Ok(OvmfSlot {
        slot_id: OVMF_FW_SLOT_ID,
    })
}

/// Bind the armed firmware slot to guest 1 (ADR-014 Stage 8).
///
/// INVARIANTS:
/// - Requires a prior successful [`arm_ovmf_firmware_slot`]
/// - Records bind bookkeeping only
/// - Does not VMLAUNCH and does not flip [`crate::mgmt::iso::attach_cdrom_uefi`]
pub fn bind_ovmf_firmware_guest() -> Result<OvmfBind, GuestFwError> {
    if !ovmf_slot_is_armed() {
        return Err(GuestFwError::NotSlotArmed);
    }
    audit_log!(AuditEvent::OvmfFirmwareGuestBound {
        guest_id: u64::from(OVMF_FW_GUEST_ID),
        slot_id: u64::from(OVMF_FW_SLOT_ID),
    });
    GUEST_FW_OVMF_GUEST_BOUND.store(true, Ordering::Release);
    Ok(OvmfBind {
        guest_id: OVMF_FW_GUEST_ID,
        slot_id: OVMF_FW_SLOT_ID,
    })
}

/// Prepare guest UEFI launch after bind (ADR-014 Stage 9).
///
/// INVARIANTS:
/// - Requires a prior successful [`bind_ovmf_firmware_guest`]
/// - Records launch-prepare bookkeeping only
/// - Does not VMLAUNCH and does not flip [`crate::mgmt::iso::attach_cdrom_uefi`]
pub fn prepare_ovmf_firmware_launch() -> Result<OvmfLaunchPrep, GuestFwError> {
    if !ovmf_guest_is_bound() {
        return Err(GuestFwError::NotGuestBound);
    }
    audit_log!(AuditEvent::OvmfFirmwareLaunchPrepared {
        guest_id: u64::from(OVMF_FW_GUEST_ID),
        slot_id: u64::from(OVMF_FW_SLOT_ID),
    });
    GUEST_FW_OVMF_LAUNCH_PREPPED.store(true, Ordering::Release);
    Ok(OvmfLaunchPrep {
        guest_id: OVMF_FW_GUEST_ID,
        slot_id: OVMF_FW_SLOT_ID,
    })
}

/// Refuse guest UEFI VMLAUNCH unless a live ESP `OVMF.fd` is mapped.
///
/// INVARIANTS:
/// - Requires a prior successful [`prepare_ovmf_firmware_launch`]
/// - Mock (no floor) → [`GuestFwError::MockFirmwareRefused`]
/// - Size-floor staged (no EDK2) → [`GuestFwError::NotRealFirmware`]
/// - EDK2-sized staged (no ESP launch arm) → [`GuestFwError::LaunchNotWired`]
/// - ESP launch armed → [`crate::vmx::launch::try_vmlaunch_guest_uefi_ovmf`]
///   (unmapped → [`GuestFwError::MissingEsp`]; live-sized map →
///   [`GuestFwError::LiveMappedNotLaunched`]; reset-vector armed →
///   [`GuestFwError::ResetVectorNotLaunched`]; alias armed →
///   [`GuestFwError::FirmwareAliasNotLaunched`]; alias-EPT programmed →
///   [`GuestFwError::AliasEptNotLaunched`]; private EPT installed →
///   [`GuestFwError::AliasEptInstalledNotLaunched`]; real-ESP qualified →
///   [`GuestFwError::RealEspNotLaunched`]; insn path armed →
///   [`GuestFwError::RealLaunchNotIssued`]; live-ESP required →
///   [`GuestFwError::LiveEspRequired`]; private VMCS selected →
///   [`GuestFwError::PrivateVmcsNotLaunched`]; live-issue armed →
///   [`GuestFwError::LiveEspBytesNotPresent`]; live bytes probed →
///   [`GuestFwError::LiveEspBytesAbsent`]; live FD required →
///   [`GuestFwError::LiveEspFdAbsent`]; live ESP presented →
///   [`GuestFwError::LiveEspPresentAbsent`]; live ESP admitted →
///   [`GuestFwError::LiveEspAdmitAbsent`]; live ESP read-attempted →
///   [`GuestFwError::LiveEspReadAbsent`]; live ESP copy-attempted →
///   [`GuestFwError::LiveEspCopyAbsent`]; live ESP place-attempted →
///   [`GuestFwError::LiveEspPlaceAbsent`]; live ESP apply-attempted →
///   [`GuestFwError::LiveEspApplyAbsent`]; live ESP commit-attempted →
///   [`GuestFwError::LiveEspCommitAbsent`]; live ESP latch-attempted →
///   [`GuestFwError::LiveEspLatchAbsent`])
/// - Does not VMLAUNCH the 1 MiB, 2 MiB, or 4 MiB fixture, does not write
///   the E4 SHELL EPT, and does not flip attach_cdrom_uefi
pub fn try_vmlaunch_ovmf_firmware() -> Result<(), GuestFwError> {
    if !ovmf_launch_is_prepared() {
        return Err(GuestFwError::NotGuestBound);
    }
    if ovmf_esp_launch_is_armed() {
        return match crate::vmx::launch::try_vmlaunch_guest_uefi_ovmf() {
            Ok(()) => Ok(()),
            Err(crate::vmx::launch::GuestUefiLaunchError::MissingEspFirmware) => {
                Err(GuestFwError::MissingEsp)
            }
            Err(crate::vmx::launch::GuestUefiLaunchError::LiveMappedNotLaunched) => {
                Err(GuestFwError::LiveMappedNotLaunched)
            }
            Err(crate::vmx::launch::GuestUefiLaunchError::NoResetVector) => {
                Err(GuestFwError::NoResetVector)
            }
            Err(crate::vmx::launch::GuestUefiLaunchError::ResetVectorNotLaunched) => {
                Err(GuestFwError::ResetVectorNotLaunched)
            }
            Err(crate::vmx::launch::GuestUefiLaunchError::FirmwareAliasNotLaunched) => {
                Err(GuestFwError::FirmwareAliasNotLaunched)
            }
            Err(crate::vmx::launch::GuestUefiLaunchError::AliasEptNotLaunched) => {
                Err(GuestFwError::AliasEptNotLaunched)
            }
            Err(crate::vmx::launch::GuestUefiLaunchError::AliasEptInstalledNotLaunched) => {
                Err(GuestFwError::AliasEptInstalledNotLaunched)
            }
            Err(crate::vmx::launch::GuestUefiLaunchError::RealEspNotLaunched) => {
                Err(GuestFwError::RealEspNotLaunched)
            }
            Err(crate::vmx::launch::GuestUefiLaunchError::RealLaunchNotIssued) => {
                Err(GuestFwError::RealLaunchNotIssued)
            }
            Err(crate::vmx::launch::GuestUefiLaunchError::LiveEspRequired) => {
                Err(GuestFwError::LiveEspRequired)
            }
            Err(crate::vmx::launch::GuestUefiLaunchError::PrivateVmcsNotLaunched) => {
                Err(GuestFwError::PrivateVmcsNotLaunched)
            }
            Err(crate::vmx::launch::GuestUefiLaunchError::LiveEspBytesNotPresent) => {
                Err(GuestFwError::LiveEspBytesNotPresent)
            }
            Err(crate::vmx::launch::GuestUefiLaunchError::LiveEspBytesAbsent) => {
                Err(GuestFwError::LiveEspBytesAbsent)
            }
            Err(crate::vmx::launch::GuestUefiLaunchError::LiveEspFdAbsent) => {
                Err(GuestFwError::LiveEspFdAbsent)
            }
            Err(crate::vmx::launch::GuestUefiLaunchError::LiveEspPresentAbsent) => {
                Err(GuestFwError::LiveEspPresentAbsent)
            }
            Err(crate::vmx::launch::GuestUefiLaunchError::LiveEspAdmitAbsent) => {
                Err(GuestFwError::LiveEspAdmitAbsent)
            }
            Err(crate::vmx::launch::GuestUefiLaunchError::LiveEspReadAbsent) => {
                Err(GuestFwError::LiveEspReadAbsent)
            }
            Err(crate::vmx::launch::GuestUefiLaunchError::LiveEspCopyAbsent) => {
                Err(GuestFwError::LiveEspCopyAbsent)
            }
            Err(crate::vmx::launch::GuestUefiLaunchError::LiveEspPlaceAbsent) => {
                Err(GuestFwError::LiveEspPlaceAbsent)
            }
            Err(crate::vmx::launch::GuestUefiLaunchError::LiveEspApplyAbsent) => {
                Err(GuestFwError::LiveEspApplyAbsent)
            }
            Err(crate::vmx::launch::GuestUefiLaunchError::LiveEspCommitAbsent) => {
                Err(GuestFwError::LiveEspCommitAbsent)
            }
            Err(crate::vmx::launch::GuestUefiLaunchError::LiveEspLatchAbsent) => {
                Err(GuestFwError::LiveEspLatchAbsent)
            }
        };
    }
    if ovmf_edk2_is_staged() {
        return Err(GuestFwError::LaunchNotWired);
    }
    if ovmf_floor_is_staged() {
        return Err(GuestFwError::NotRealFirmware);
    }
    Err(GuestFwError::MockFirmwareRefused)
}

/// Arm the ESP-path guest UEFI VMLAUNCH contract after EDK2 (ADR-014 Stage 12).
///
/// INVARIANTS:
/// - Requires a prior successful [`stage_edk2_ovmf_firmware`]
/// - Records [`OVMF_ESP_PATH`] as the only allowed firmware source
/// - Does not map the 1 MiB fixture and does not VMLAUNCH
/// - Does not flip [`crate::mgmt::iso::attach_cdrom_uefi`]
pub fn arm_ovmf_esp_launch() -> Result<OvmfEspLaunch, GuestFwError> {
    if !ovmf_edk2_is_staged() {
        return Err(GuestFwError::LaunchNotWired);
    }
    audit_log!(AuditEvent::OvmfEspLaunchArmed {
        guest_id: u64::from(OVMF_FW_GUEST_ID),
        slot_id: u64::from(OVMF_FW_SLOT_ID),
    });
    GUEST_FW_OVMF_ESP_LAUNCH_ARMED.store(true, Ordering::Release);
    Ok(OvmfEspLaunch {
        guest_id: OVMF_FW_GUEST_ID,
        slot_id: OVMF_FW_SLOT_ID,
    })
}

/// Map a live-sized ESP `OVMF.fd` after ESP launch arm (ADR-014 Stage 13).
///
/// INVARIANTS:
/// - Requires a prior successful [`arm_ovmf_esp_launch`]
/// - `bytes.len()` and `_FVH` `FvLength` must be `>= MIN_LIVE_ESP_OVMF_BYTES`
/// - Accepts a live-sized map only — not a shipped EDK2 `OVMF.fd`
/// - Records the map in `vmx/launch.rs`; does not issue VMLAUNCH
/// - Does not flip [`crate::mgmt::iso::attach_cdrom_uefi`]
pub fn map_live_esp_ovmf(bytes: &[u8]) -> Result<OvmfLiveMap, GuestFwError> {
    if !ovmf_esp_launch_is_armed() {
        return Err(GuestFwError::LaunchNotWired);
    }
    if bytes.len() < MIN_LIVE_ESP_OVMF_BYTES {
        return Err(GuestFwError::TooSmall);
    }
    if bytes.len() > GUEST_FW_MAX_UNCOMPRESSED as usize {
        return Err(GuestFwError::TooLarge);
    }
    let probed = probe_ovmf_fv(bytes)?;
    if probed.fv_len < MIN_LIVE_ESP_OVMF_BYTES as u64 {
        return Err(GuestFwError::TooSmall);
    }
    crate::vmx::launch::arm_live_esp_ovmf_mapping(bytes.len() as u64)
        .map_err(|_| GuestFwError::TooSmall)?;
    audit_log!(AuditEvent::OvmfEspLiveMapped {
        bytes_len: bytes.len() as u64,
    });
    GUEST_FW_OVMF_LIVE_MAPPED.store(true, Ordering::Release);
    Ok(OvmfLiveMap {
        bytes_len: bytes.len() as u64,
    })
}

/// Arm the reset-vector VMCS contract after a live-sized map (ADR-014 Stage 14).
///
/// INVARIANTS:
/// - Requires a prior successful [`map_live_esp_ovmf`]
/// - Last 16 bytes must start with JMP FAR (`0xEA`)
/// - A synthetic stub is not a shipped EDK2 `OVMF.fd`
/// - Records the contract in `vmx/launch.rs`; does not issue VMLAUNCH
/// - Does not flip [`crate::mgmt::iso::attach_cdrom_uefi`]
pub fn arm_ovmf_reset_vector(bytes: &[u8]) -> Result<OvmfResetVec, GuestFwError> {
    if !ovmf_live_esp_is_mapped() {
        return Err(GuestFwError::LaunchNotWired);
    }
    match crate::vmx::launch::arm_guest_uefi_reset_vector(bytes) {
        Ok(()) => {}
        Err(crate::vmx::launch::GuestUefiLaunchError::MissingEspFirmware) => {
            return Err(GuestFwError::MissingEsp);
        }
        Err(crate::vmx::launch::GuestUefiLaunchError::NoResetVector) => {
            return Err(GuestFwError::NoResetVector);
        }
        Err(_) => return Err(GuestFwError::BadState),
    }
    audit_log!(AuditEvent::OvmfResetVectorArmed {
        bytes_len: bytes.len() as u64,
    });
    GUEST_FW_OVMF_RESET_ARMED.store(true, Ordering::Release);
    Ok(OvmfResetVec {
        bytes_len: bytes.len() as u64,
    })
}

/// Arm the 4 GiB firmware-alias contract after reset-vector (ADR-014 Stage 15).
///
/// INVARIANTS:
/// - Requires a prior successful [`arm_ovmf_reset_vector`]
/// - `bytes.len()` and `_FVH` `FvLength` must be `>= MIN_FIRMWARE_ALIAS_BYTES`
/// - Last 16 bytes must start with JMP FAR (`0xEA`)
/// - A 4 MiB heap fixture is not a shipped EDK2 `OVMF.fd`
/// - Records the contract in `vmx/launch.rs`; does not issue VMLAUNCH
/// - Does not flip [`crate::mgmt::iso::attach_cdrom_uefi`]
pub fn arm_ovmf_firmware_alias(bytes: &[u8]) -> Result<OvmfAlias, GuestFwError> {
    if !ovmf_reset_vector_is_armed() {
        return Err(GuestFwError::LaunchNotWired);
    }
    if bytes.len() < MIN_FIRMWARE_ALIAS_BYTES {
        return Err(GuestFwError::TooSmall);
    }
    if bytes.len() > GUEST_FW_MAX_UNCOMPRESSED as usize {
        return Err(GuestFwError::TooLarge);
    }
    let probed = probe_ovmf_fv(bytes)?;
    if probed.fv_len < MIN_FIRMWARE_ALIAS_BYTES as u64 {
        return Err(GuestFwError::TooSmall);
    }
    match crate::vmx::launch::probe_guest_uefi_reset_vector(bytes) {
        Ok(()) => {}
        Err(crate::vmx::launch::GuestUefiLaunchError::NoResetVector) => {
            return Err(GuestFwError::NoResetVector);
        }
        Err(_) => return Err(GuestFwError::BadState),
    }
    crate::vmx::launch::arm_guest_uefi_firmware_alias(bytes.len() as u64)
        .map_err(|_| GuestFwError::TooSmall)?;
    audit_log!(AuditEvent::OvmfFirmwareAliasArmed {
        bytes_len: bytes.len() as u64,
    });
    GUEST_FW_OVMF_ALIAS_ARMED.store(true, Ordering::Release);
    Ok(OvmfAlias {
        bytes_len: bytes.len() as u64,
    })
}

/// Program the alias-EPT window after firmware-alias (ADR-014 Stage 16).
///
/// INVARIANTS:
/// - Requires a prior successful [`arm_ovmf_firmware_alias`]
/// - `bytes.len()` and `_FVH` `FvLength` must be `>= MIN_FIRMWARE_ALIAS_BYTES`
/// - Reset vector GPA must sit inside the alias window
/// - Records the contract in `vmx/launch.rs`; does not write live EPT
/// - Does not issue VMLAUNCH and does not flip [`crate::mgmt::iso::attach_cdrom_uefi`]
pub fn program_ovmf_alias_ept(bytes: &[u8]) -> Result<OvmfAliasEpt, GuestFwError> {
    if !ovmf_firmware_alias_is_armed() {
        return Err(GuestFwError::LaunchNotWired);
    }
    if bytes.len() < MIN_FIRMWARE_ALIAS_BYTES {
        return Err(GuestFwError::TooSmall);
    }
    if bytes.len() > GUEST_FW_MAX_UNCOMPRESSED as usize {
        return Err(GuestFwError::TooLarge);
    }
    let probed = probe_ovmf_fv(bytes)?;
    if probed.fv_len < MIN_FIRMWARE_ALIAS_BYTES as u64 {
        return Err(GuestFwError::TooSmall);
    }
    match crate::vmx::launch::probe_guest_uefi_reset_vector(bytes) {
        Ok(()) => {}
        Err(crate::vmx::launch::GuestUefiLaunchError::NoResetVector) => {
            return Err(GuestFwError::NoResetVector);
        }
        Err(_) => return Err(GuestFwError::BadState),
    }
    crate::vmx::launch::program_guest_uefi_alias_ept(bytes.len() as u64)
        .map_err(|_| GuestFwError::TooSmall)?;
    let gpa = crate::vmx::launch::firmware_alias_gpa(bytes.len() as u64).unwrap_or(0);
    audit_log!(AuditEvent::OvmfAliasEptProgrammed {
        bytes_len: bytes.len() as u64,
        gpa,
    });
    GUEST_FW_OVMF_ALIAS_EPT_PROGRAMMED.store(true, Ordering::Release);
    Ok(OvmfAliasEpt {
        bytes_len: bytes.len() as u64,
        gpa,
    })
}

/// Install a private guest-UEFI alias-EPT after program (ADR-014 Stage 17).
///
/// INVARIANTS:
/// - Requires a prior successful [`program_ovmf_alias_ept`]
/// - `bytes.len()` and `_FVH` `FvLength` must be `>= MIN_FIRMWARE_ALIAS_BYTES`
/// - Reset vector GPA must sit inside the alias window
/// - Records a private install in `vmx/launch.rs`; does not write the E4 SHELL EPT
/// - Does not issue VMLAUNCH and does not flip [`crate::mgmt::iso::attach_cdrom_uefi`]
pub fn install_ovmf_alias_ept(bytes: &[u8]) -> Result<OvmfAliasEptInstall, GuestFwError> {
    if !ovmf_alias_ept_is_programmed() {
        return Err(GuestFwError::LaunchNotWired);
    }
    if bytes.len() < MIN_FIRMWARE_ALIAS_BYTES {
        return Err(GuestFwError::TooSmall);
    }
    if bytes.len() > GUEST_FW_MAX_UNCOMPRESSED as usize {
        return Err(GuestFwError::TooLarge);
    }
    let probed = probe_ovmf_fv(bytes)?;
    if probed.fv_len < MIN_FIRMWARE_ALIAS_BYTES as u64 {
        return Err(GuestFwError::TooSmall);
    }
    match crate::vmx::launch::probe_guest_uefi_reset_vector(bytes) {
        Ok(()) => {}
        Err(crate::vmx::launch::GuestUefiLaunchError::NoResetVector) => {
            return Err(GuestFwError::NoResetVector);
        }
        Err(_) => return Err(GuestFwError::BadState),
    }
    crate::vmx::launch::install_guest_uefi_alias_ept(bytes.len() as u64)
        .map_err(|_| GuestFwError::TooSmall)?;
    let gpa = crate::vmx::launch::firmware_alias_gpa(bytes.len() as u64).unwrap_or(0);
    audit_log!(AuditEvent::OvmfAliasEptInstalled {
        bytes_len: bytes.len() as u64,
        gpa,
    });
    GUEST_FW_OVMF_ALIAS_EPT_INSTALLED.store(true, Ordering::Release);
    Ok(OvmfAliasEptInstall {
        bytes_len: bytes.len() as u64,
        gpa,
    })
}

/// Qualify real-ESP VMLAUNCH-ready after private install (ADR-014 Stage 18).
///
/// INVARIANTS:
/// - Requires a prior successful [`install_ovmf_alias_ept`]
/// - `bytes.len()` and `_FVH` `FvLength` must be `>= MIN_FIRMWARE_ALIAS_BYTES`
/// - Reset vector GPA must sit inside the alias window
/// - Records the contract in `vmx/launch.rs`; does not write the E4 SHELL EPT
/// - Does not issue VMLAUNCH and does not flip [`crate::mgmt::iso::attach_cdrom_uefi`]
/// - A heap fixture is not a shipped EDK2 `OVMF.fd`
pub fn qualify_real_esp_ovmf(bytes: &[u8]) -> Result<OvmfRealEsp, GuestFwError> {
    if !ovmf_alias_ept_is_installed() {
        return Err(GuestFwError::LaunchNotWired);
    }
    if bytes.len() < MIN_FIRMWARE_ALIAS_BYTES {
        return Err(GuestFwError::TooSmall);
    }
    if bytes.len() > GUEST_FW_MAX_UNCOMPRESSED as usize {
        return Err(GuestFwError::TooLarge);
    }
    let probed = probe_ovmf_fv(bytes)?;
    if probed.fv_len < MIN_FIRMWARE_ALIAS_BYTES as u64 {
        return Err(GuestFwError::TooSmall);
    }
    match crate::vmx::launch::probe_guest_uefi_reset_vector(bytes) {
        Ok(()) => {}
        Err(crate::vmx::launch::GuestUefiLaunchError::NoResetVector) => {
            return Err(GuestFwError::NoResetVector);
        }
        Err(_) => return Err(GuestFwError::BadState),
    }
    crate::vmx::launch::qualify_guest_uefi_real_esp(bytes.len() as u64)
        .map_err(|_| GuestFwError::TooSmall)?;
    let gpa = crate::vmx::launch::firmware_alias_gpa(bytes.len() as u64).unwrap_or(0);
    audit_log!(AuditEvent::OvmfRealEspQualified {
        bytes_len: bytes.len() as u64,
        gpa,
    });
    GUEST_FW_OVMF_REAL_ESP_QUALIFIED.store(true, Ordering::Release);
    Ok(OvmfRealEsp {
        bytes_len: bytes.len() as u64,
        gpa,
    })
}

/// Arm the guest-UEFI VMLAUNCH insn path after real-ESP qualify (ADR-014 Stage 19).
///
/// INVARIANTS:
/// - Requires a prior successful [`qualify_real_esp_ovmf`]
/// - `bytes.len()` and `_FVH` `FvLength` must be `>= MIN_FIRMWARE_ALIAS_BYTES`
/// - Reset vector GPA must sit inside the alias window
/// - Selects the VMLAUNCH opcode in `vmx/launch.rs`; does not write the E4 SHELL EPT
/// - Does not issue VMLAUNCH and does not flip [`crate::mgmt::iso::attach_cdrom_uefi`]
/// - A heap fixture is not a shipped EDK2 `OVMF.fd`
pub fn arm_ovmf_real_launch(bytes: &[u8]) -> Result<OvmfRealLaunch, GuestFwError> {
    if !ovmf_real_esp_is_qualified() {
        return Err(GuestFwError::LaunchNotWired);
    }
    if bytes.len() < MIN_FIRMWARE_ALIAS_BYTES {
        return Err(GuestFwError::TooSmall);
    }
    if bytes.len() > GUEST_FW_MAX_UNCOMPRESSED as usize {
        return Err(GuestFwError::TooLarge);
    }
    let probed = probe_ovmf_fv(bytes)?;
    if probed.fv_len < MIN_FIRMWARE_ALIAS_BYTES as u64 {
        return Err(GuestFwError::TooSmall);
    }
    match crate::vmx::launch::probe_guest_uefi_reset_vector(bytes) {
        Ok(()) => {}
        Err(crate::vmx::launch::GuestUefiLaunchError::NoResetVector) => {
            return Err(GuestFwError::NoResetVector);
        }
        Err(_) => return Err(GuestFwError::BadState),
    }
    crate::vmx::launch::arm_guest_uefi_real_launch(bytes.len() as u64)
        .map_err(|_| GuestFwError::TooSmall)?;
    let gpa = crate::vmx::launch::firmware_alias_gpa(bytes.len() as u64).unwrap_or(0);
    audit_log!(AuditEvent::OvmfRealLaunchArmed {
        bytes_len: bytes.len() as u64,
        gpa,
    });
    GUEST_FW_OVMF_REAL_LAUNCH_ARMED.store(true, Ordering::Release);
    Ok(OvmfRealLaunch {
        bytes_len: bytes.len() as u64,
        gpa,
    })
}

/// Require live ESP `\EFI\RayNu\OVMF.fd` bytes before VMLAUNCH (ADR-014 Stage 20).
///
/// INVARIANTS:
/// - Requires a prior successful [`arm_ovmf_real_launch`]
/// - `bytes.len()` and `_FVH` `FvLength` must be `>= MIN_FIRMWARE_ALIAS_BYTES`
/// - Reset vector GPA must sit inside the alias window
/// - Does not write the E4 SHELL EPT and does not issue VMLAUNCH
/// - Does not flip [`crate::mgmt::iso::attach_cdrom_uefi`]
/// - A heap fixture is not a shipped EDK2 `OVMF.fd`
pub fn require_ovmf_live_esp(bytes: &[u8]) -> Result<OvmfLiveExec, GuestFwError> {
    if !ovmf_real_launch_is_armed() {
        return Err(GuestFwError::LaunchNotWired);
    }
    if bytes.len() < MIN_FIRMWARE_ALIAS_BYTES {
        return Err(GuestFwError::TooSmall);
    }
    if bytes.len() > GUEST_FW_MAX_UNCOMPRESSED as usize {
        return Err(GuestFwError::TooLarge);
    }
    let probed = probe_ovmf_fv(bytes)?;
    if probed.fv_len < MIN_FIRMWARE_ALIAS_BYTES as u64 {
        return Err(GuestFwError::TooSmall);
    }
    match crate::vmx::launch::probe_guest_uefi_reset_vector(bytes) {
        Ok(()) => {}
        Err(crate::vmx::launch::GuestUefiLaunchError::NoResetVector) => {
            return Err(GuestFwError::NoResetVector);
        }
        Err(_) => return Err(GuestFwError::BadState),
    }
    crate::vmx::launch::require_guest_uefi_live_esp(bytes.len() as u64)
        .map_err(|_| GuestFwError::TooSmall)?;
    let gpa = crate::vmx::launch::firmware_alias_gpa(bytes.len() as u64).unwrap_or(0);
    audit_log!(AuditEvent::OvmfLiveEspRequired {
        bytes_len: bytes.len() as u64,
        gpa,
    });
    GUEST_FW_OVMF_LIVE_ESP_REQUIRED.store(true, Ordering::Release);
    Ok(OvmfLiveExec {
        bytes_len: bytes.len() as u64,
        gpa,
    })
}

/// Select a private guest-UEFI VMCS after live-ESP require (ADR-014 Stage 21).
///
/// INVARIANTS:
/// - Requires a prior successful [`require_ovmf_live_esp`]
/// - `bytes.len()` and `_FVH` `FvLength` must be `>= MIN_FIRMWARE_ALIAS_BYTES`
/// - Reset vector GPA must sit inside the alias window
/// - Selects a private VMCS (not the E4 SHELL); does not allocate or VMWRITE
/// - Does not write the E4 SHELL EPT and does not issue VMLAUNCH
/// - Does not flip [`crate::mgmt::iso::attach_cdrom_uefi`]
/// - A heap fixture is not a shipped EDK2 `OVMF.fd`
pub fn arm_ovmf_private_vmcs(bytes: &[u8]) -> Result<OvmfPrivateVmcs, GuestFwError> {
    if !ovmf_live_esp_is_required() {
        return Err(GuestFwError::LaunchNotWired);
    }
    if bytes.len() < MIN_FIRMWARE_ALIAS_BYTES {
        return Err(GuestFwError::TooSmall);
    }
    if bytes.len() > GUEST_FW_MAX_UNCOMPRESSED as usize {
        return Err(GuestFwError::TooLarge);
    }
    let probed = probe_ovmf_fv(bytes)?;
    if probed.fv_len < MIN_FIRMWARE_ALIAS_BYTES as u64 {
        return Err(GuestFwError::TooSmall);
    }
    match crate::vmx::launch::probe_guest_uefi_reset_vector(bytes) {
        Ok(()) => {}
        Err(crate::vmx::launch::GuestUefiLaunchError::NoResetVector) => {
            return Err(GuestFwError::NoResetVector);
        }
        Err(_) => return Err(GuestFwError::BadState),
    }
    crate::vmx::launch::arm_guest_uefi_private_vmcs(bytes.len() as u64)
        .map_err(|_| GuestFwError::TooSmall)?;
    let gpa = crate::vmx::launch::firmware_alias_gpa(bytes.len() as u64).unwrap_or(0);
    audit_log!(AuditEvent::OvmfPrivateVmcsArmed {
        bytes_len: bytes.len() as u64,
        gpa,
    });
    GUEST_FW_OVMF_PRIVATE_VMCS_ARMED.store(true, Ordering::Release);
    Ok(OvmfPrivateVmcs {
        bytes_len: bytes.len() as u64,
        gpa,
    })
}

/// Arm the live-ESP VMLAUNCH issue path after private VMCS (ADR-014 Stage 22).
///
/// INVARIANTS:
/// - Requires a prior successful [`arm_ovmf_private_vmcs`]
/// - `bytes.len()` and `_FVH` `FvLength` must be `>= MIN_FIRMWARE_ALIAS_BYTES`
/// - Reset vector GPA must sit inside the alias window
/// - Does not flip [`crate::vmx::launch::guest_uefi_live_esp_bytes_present`]
/// - Does not write the E4 SHELL EPT and does not issue VMLAUNCH
/// - Does not flip [`crate::mgmt::iso::attach_cdrom_uefi`]
/// - A heap fixture is not a shipped EDK2 `OVMF.fd`
pub fn arm_ovmf_live_issue(bytes: &[u8]) -> Result<OvmfLiveIssue, GuestFwError> {
    if !ovmf_private_vmcs_is_armed() {
        return Err(GuestFwError::LaunchNotWired);
    }
    if bytes.len() < MIN_FIRMWARE_ALIAS_BYTES {
        return Err(GuestFwError::TooSmall);
    }
    if bytes.len() > GUEST_FW_MAX_UNCOMPRESSED as usize {
        return Err(GuestFwError::TooLarge);
    }
    let probed = probe_ovmf_fv(bytes)?;
    if probed.fv_len < MIN_FIRMWARE_ALIAS_BYTES as u64 {
        return Err(GuestFwError::TooSmall);
    }
    match crate::vmx::launch::probe_guest_uefi_reset_vector(bytes) {
        Ok(()) => {}
        Err(crate::vmx::launch::GuestUefiLaunchError::NoResetVector) => {
            return Err(GuestFwError::NoResetVector);
        }
        Err(_) => return Err(GuestFwError::BadState),
    }
    crate::vmx::launch::arm_guest_uefi_live_issue(bytes.len() as u64)
        .map_err(|_| GuestFwError::TooSmall)?;
    let gpa = crate::vmx::launch::firmware_alias_gpa(bytes.len() as u64).unwrap_or(0);
    audit_log!(AuditEvent::OvmfLiveIssueArmed {
        bytes_len: bytes.len() as u64,
        gpa,
    });
    GUEST_FW_OVMF_LIVE_ISSUE_ARMED.store(true, Ordering::Release);
    Ok(OvmfLiveIssue {
        bytes_len: bytes.len() as u64,
        gpa,
    })
}

/// Probe for live ESP `\EFI\RayNu\OVMF.fd` bytes after live-issue (ADR-014 Stage 23).
///
/// INVARIANTS:
/// - Requires a prior successful [`arm_ovmf_live_issue`]
/// - `bytes.len()` and `_FVH` `FvLength` must be `>= MIN_FIRMWARE_ALIAS_BYTES`
/// - Reset vector GPA must sit inside the alias window
/// - Does not flip [`crate::vmx::launch::guest_uefi_live_esp_bytes_present`]
/// - Does not write the E4 SHELL EPT and does not issue VMLAUNCH
/// - Does not flip [`crate::mgmt::iso::attach_cdrom_uefi`]
/// - A heap fixture is not a shipped EDK2 `OVMF.fd`
pub fn probe_ovmf_live_bytes(bytes: &[u8]) -> Result<OvmfLiveBytes, GuestFwError> {
    if !ovmf_live_issue_is_armed() {
        return Err(GuestFwError::LaunchNotWired);
    }
    if bytes.len() < MIN_FIRMWARE_ALIAS_BYTES {
        return Err(GuestFwError::TooSmall);
    }
    if bytes.len() > GUEST_FW_MAX_UNCOMPRESSED as usize {
        return Err(GuestFwError::TooLarge);
    }
    let probed = probe_ovmf_fv(bytes)?;
    if probed.fv_len < MIN_FIRMWARE_ALIAS_BYTES as u64 {
        return Err(GuestFwError::TooSmall);
    }
    match crate::vmx::launch::probe_guest_uefi_reset_vector(bytes) {
        Ok(()) => {}
        Err(crate::vmx::launch::GuestUefiLaunchError::NoResetVector) => {
            return Err(GuestFwError::NoResetVector);
        }
        Err(_) => return Err(GuestFwError::BadState),
    }
    crate::vmx::launch::probe_guest_uefi_live_bytes(bytes.len() as u64)
        .map_err(|_| GuestFwError::TooSmall)?;
    let gpa = crate::vmx::launch::firmware_alias_gpa(bytes.len() as u64).unwrap_or(0);
    audit_log!(AuditEvent::OvmfLiveBytesProbed {
        bytes_len: bytes.len() as u64,
        gpa,
    });
    GUEST_FW_OVMF_LIVE_BYTES_PROBED.store(true, Ordering::Release);
    Ok(OvmfLiveBytes {
        bytes_len: bytes.len() as u64,
        gpa,
    })
}

/// Require a real ESP `\EFI\RayNu\OVMF.fd` after live-bytes probe (ADR-014 Stage 24).
///
/// INVARIANTS:
/// - Requires a prior successful [`probe_ovmf_live_bytes`]
/// - `bytes.len()` and `_FVH` `FvLength` must be `>= MIN_FIRMWARE_ALIAS_BYTES`
/// - Reset vector GPA must sit inside the alias window
/// - Does not flip [`crate::vmx::launch::guest_uefi_live_esp_bytes_present`]
/// - Does not write the E4 SHELL EPT and does not issue VMLAUNCH
/// - Does not flip [`crate::mgmt::iso::attach_cdrom_uefi`]
/// - A heap fixture is not a shipped EDK2 `OVMF.fd`
pub fn require_ovmf_live_fd(bytes: &[u8]) -> Result<OvmfLiveFd, GuestFwError> {
    if !ovmf_live_bytes_is_probed() {
        return Err(GuestFwError::LaunchNotWired);
    }
    if bytes.len() < MIN_FIRMWARE_ALIAS_BYTES {
        return Err(GuestFwError::TooSmall);
    }
    if bytes.len() > GUEST_FW_MAX_UNCOMPRESSED as usize {
        return Err(GuestFwError::TooLarge);
    }
    let probed = probe_ovmf_fv(bytes)?;
    if probed.fv_len < MIN_FIRMWARE_ALIAS_BYTES as u64 {
        return Err(GuestFwError::TooSmall);
    }
    match crate::vmx::launch::probe_guest_uefi_reset_vector(bytes) {
        Ok(()) => {}
        Err(crate::vmx::launch::GuestUefiLaunchError::NoResetVector) => {
            return Err(GuestFwError::NoResetVector);
        }
        Err(_) => return Err(GuestFwError::BadState),
    }
    crate::vmx::launch::require_guest_uefi_live_fd(bytes.len() as u64)
        .map_err(|_| GuestFwError::TooSmall)?;
    let gpa = crate::vmx::launch::firmware_alias_gpa(bytes.len() as u64).unwrap_or(0);
    audit_log!(AuditEvent::OvmfLiveFdRequired {
        bytes_len: bytes.len() as u64,
        gpa,
    });
    GUEST_FW_OVMF_LIVE_FD_REQUIRED.store(true, Ordering::Release);
    Ok(OvmfLiveFd {
        bytes_len: bytes.len() as u64,
        gpa,
    })
}

/// Present real ESP `\EFI\RayNu\OVMF.fd` bytes after live-FD require (ADR-014 Stage 25).
///
/// INVARIANTS:
/// - Requires a prior successful [`require_ovmf_live_fd`]
/// - `bytes.len()` and `_FVH` `FvLength` must be `>= MIN_FIRMWARE_ALIAS_BYTES`
/// - Reset vector GPA must sit inside the alias window
/// - Does not flip [`crate::vmx::launch::guest_uefi_live_esp_bytes_present`]
/// - Does not write the E4 SHELL EPT and does not issue VMLAUNCH
/// - Does not flip [`crate::mgmt::iso::attach_cdrom_uefi`]
/// - A heap fixture is not a shipped EDK2 `OVMF.fd`
pub fn present_ovmf_live_esp(bytes: &[u8]) -> Result<OvmfLivePresent, GuestFwError> {
    if !ovmf_live_fd_is_required() {
        return Err(GuestFwError::LaunchNotWired);
    }
    if bytes.len() < MIN_FIRMWARE_ALIAS_BYTES {
        return Err(GuestFwError::TooSmall);
    }
    if bytes.len() > GUEST_FW_MAX_UNCOMPRESSED as usize {
        return Err(GuestFwError::TooLarge);
    }
    let probed = probe_ovmf_fv(bytes)?;
    if probed.fv_len < MIN_FIRMWARE_ALIAS_BYTES as u64 {
        return Err(GuestFwError::TooSmall);
    }
    match crate::vmx::launch::probe_guest_uefi_reset_vector(bytes) {
        Ok(()) => {}
        Err(crate::vmx::launch::GuestUefiLaunchError::NoResetVector) => {
            return Err(GuestFwError::NoResetVector);
        }
        Err(_) => return Err(GuestFwError::BadState),
    }
    crate::vmx::launch::present_guest_uefi_live_esp(bytes.len() as u64)
        .map_err(|_| GuestFwError::TooSmall)?;
    let gpa = crate::vmx::launch::firmware_alias_gpa(bytes.len() as u64).unwrap_or(0);
    audit_log!(AuditEvent::OvmfLiveEspPresented {
        bytes_len: bytes.len() as u64,
        gpa,
    });
    GUEST_FW_OVMF_LIVE_ESP_PRESENTED.store(true, Ordering::Release);
    Ok(OvmfLivePresent {
        bytes_len: bytes.len() as u64,
        gpa,
    })
}

/// Admit real ESP `\EFI\RayNu\OVMF.fd` bytes after present-attempt (ADR-014 Stage 26).
///
/// INVARIANTS:
/// - Requires a prior successful [`present_ovmf_live_esp`]
/// - `bytes.len()` and `_FVH` `FvLength` must be `>= MIN_FIRMWARE_ALIAS_BYTES`
/// - Reset vector GPA must sit inside the alias window
/// - Does not flip [`crate::vmx::launch::guest_uefi_live_esp_bytes_present`]
/// - Does not write the E4 SHELL EPT and does not issue VMLAUNCH
/// - Does not flip [`crate::mgmt::iso::attach_cdrom_uefi`]
/// - A heap fixture is not a shipped EDK2 `OVMF.fd`
pub fn admit_ovmf_live_esp(bytes: &[u8]) -> Result<OvmfLiveAdmit, GuestFwError> {
    if !ovmf_live_esp_is_presented() {
        return Err(GuestFwError::LaunchNotWired);
    }
    if bytes.len() < MIN_FIRMWARE_ALIAS_BYTES {
        return Err(GuestFwError::TooSmall);
    }
    if bytes.len() > GUEST_FW_MAX_UNCOMPRESSED as usize {
        return Err(GuestFwError::TooLarge);
    }
    let probed = probe_ovmf_fv(bytes)?;
    if probed.fv_len < MIN_FIRMWARE_ALIAS_BYTES as u64 {
        return Err(GuestFwError::TooSmall);
    }
    match crate::vmx::launch::probe_guest_uefi_reset_vector(bytes) {
        Ok(()) => {}
        Err(crate::vmx::launch::GuestUefiLaunchError::NoResetVector) => {
            return Err(GuestFwError::NoResetVector);
        }
        Err(_) => return Err(GuestFwError::BadState),
    }
    crate::vmx::launch::admit_guest_uefi_live_esp(bytes.len() as u64)
        .map_err(|_| GuestFwError::TooSmall)?;
    let gpa = crate::vmx::launch::firmware_alias_gpa(bytes.len() as u64).unwrap_or(0);
    audit_log!(AuditEvent::OvmfLiveEspAdmitted {
        bytes_len: bytes.len() as u64,
        gpa,
    });
    GUEST_FW_OVMF_LIVE_ESP_ADMITTED.store(true, Ordering::Release);
    Ok(OvmfLiveAdmit {
        bytes_len: bytes.len() as u64,
        gpa,
    })
}

/// Read-attempt real ESP `\EFI\RayNu\OVMF.fd` bytes after admit (ADR-014 Stage 27).
///
/// INVARIANTS:
/// - Requires a prior successful [`admit_ovmf_live_esp`]
/// - `bytes.len()` and `_FVH` `FvLength` must be `>= MIN_FIRMWARE_ALIAS_BYTES`
/// - Reset vector GPA must sit inside the alias window
/// - Does not flip [`crate::vmx::launch::guest_uefi_live_esp_bytes_present`]
/// - Does not write the E4 SHELL EPT and does not issue VMLAUNCH
/// - Does not flip [`crate::mgmt::iso::attach_cdrom_uefi`]
/// - A heap fixture is not a shipped EDK2 `OVMF.fd`
pub fn read_ovmf_live_esp(bytes: &[u8]) -> Result<OvmfLiveRead, GuestFwError> {
    if !ovmf_live_esp_is_admitted() {
        return Err(GuestFwError::LaunchNotWired);
    }
    if bytes.len() < MIN_FIRMWARE_ALIAS_BYTES {
        return Err(GuestFwError::TooSmall);
    }
    if bytes.len() > GUEST_FW_MAX_UNCOMPRESSED as usize {
        return Err(GuestFwError::TooLarge);
    }
    let probed = probe_ovmf_fv(bytes)?;
    if probed.fv_len < MIN_FIRMWARE_ALIAS_BYTES as u64 {
        return Err(GuestFwError::TooSmall);
    }
    match crate::vmx::launch::probe_guest_uefi_reset_vector(bytes) {
        Ok(()) => {}
        Err(crate::vmx::launch::GuestUefiLaunchError::NoResetVector) => {
            return Err(GuestFwError::NoResetVector);
        }
        Err(_) => return Err(GuestFwError::BadState),
    }
    crate::vmx::launch::read_guest_uefi_live_esp(bytes.len() as u64)
        .map_err(|_| GuestFwError::TooSmall)?;
    let gpa = crate::vmx::launch::firmware_alias_gpa(bytes.len() as u64).unwrap_or(0);
    audit_log!(AuditEvent::OvmfLiveEspRead {
        bytes_len: bytes.len() as u64,
        gpa,
    });
    GUEST_FW_OVMF_LIVE_ESP_READ.store(true, Ordering::Release);
    Ok(OvmfLiveRead {
        bytes_len: bytes.len() as u64,
        gpa,
    })
}

/// Copy-attempt real ESP `\EFI\RayNu\OVMF.fd` bytes after read (ADR-014 Stage 28).
///
/// INVARIANTS:
/// - Requires a prior successful [`read_ovmf_live_esp`]
/// - `bytes.len()` and `_FVH` `FvLength` must be `>= MIN_FIRMWARE_ALIAS_BYTES`
/// - Reset vector GPA must sit inside the alias window
/// - Does not flip [`crate::vmx::launch::guest_uefi_live_esp_bytes_present`]
/// - Does not write the E4 SHELL EPT and does not issue VMLAUNCH
/// - Does not flip [`crate::mgmt::iso::attach_cdrom_uefi`]
/// - A heap fixture is not a shipped EDK2 `OVMF.fd`
pub fn copy_ovmf_live_esp(bytes: &[u8]) -> Result<OvmfLiveCopy, GuestFwError> {
    if !ovmf_live_esp_is_read() {
        return Err(GuestFwError::LaunchNotWired);
    }
    if bytes.len() < MIN_FIRMWARE_ALIAS_BYTES {
        return Err(GuestFwError::TooSmall);
    }
    if bytes.len() > GUEST_FW_MAX_UNCOMPRESSED as usize {
        return Err(GuestFwError::TooLarge);
    }
    let probed = probe_ovmf_fv(bytes)?;
    if probed.fv_len < MIN_FIRMWARE_ALIAS_BYTES as u64 {
        return Err(GuestFwError::TooSmall);
    }
    match crate::vmx::launch::probe_guest_uefi_reset_vector(bytes) {
        Ok(()) => {}
        Err(crate::vmx::launch::GuestUefiLaunchError::NoResetVector) => {
            return Err(GuestFwError::NoResetVector);
        }
        Err(_) => return Err(GuestFwError::BadState),
    }
    crate::vmx::launch::copy_guest_uefi_live_esp(bytes.len() as u64)
        .map_err(|_| GuestFwError::TooSmall)?;
    let gpa = crate::vmx::launch::firmware_alias_gpa(bytes.len() as u64).unwrap_or(0);
    audit_log!(AuditEvent::OvmfLiveEspCopied {
        bytes_len: bytes.len() as u64,
        gpa,
    });
    GUEST_FW_OVMF_LIVE_ESP_COPIED.store(true, Ordering::Release);
    Ok(OvmfLiveCopy {
        bytes_len: bytes.len() as u64,
        gpa,
    })
}

/// Place-attempt real ESP `\EFI\RayNu\OVMF.fd` bytes after copy (ADR-014 Stage 29).
///
/// INVARIANTS:
/// - Requires a prior successful [`copy_ovmf_live_esp`]
/// - `bytes.len()` and `_FVH` `FvLength` must be `>= MIN_FIRMWARE_ALIAS_BYTES`
/// - Reset vector GPA must sit inside the alias window
/// - Does not flip [`crate::vmx::launch::guest_uefi_live_esp_bytes_present`]
/// - Does not write the E4 SHELL EPT and does not issue VMLAUNCH
/// - Does not flip [`crate::mgmt::iso::attach_cdrom_uefi`]
/// - A heap fixture is not a shipped EDK2 `OVMF.fd`
pub fn place_ovmf_live_esp(bytes: &[u8]) -> Result<OvmfLivePlace, GuestFwError> {
    if !ovmf_live_esp_is_copied() {
        return Err(GuestFwError::LaunchNotWired);
    }
    if bytes.len() < MIN_FIRMWARE_ALIAS_BYTES {
        return Err(GuestFwError::TooSmall);
    }
    if bytes.len() > GUEST_FW_MAX_UNCOMPRESSED as usize {
        return Err(GuestFwError::TooLarge);
    }
    let probed = probe_ovmf_fv(bytes)?;
    if probed.fv_len < MIN_FIRMWARE_ALIAS_BYTES as u64 {
        return Err(GuestFwError::TooSmall);
    }
    match crate::vmx::launch::probe_guest_uefi_reset_vector(bytes) {
        Ok(()) => {}
        Err(crate::vmx::launch::GuestUefiLaunchError::NoResetVector) => {
            return Err(GuestFwError::NoResetVector);
        }
        Err(_) => return Err(GuestFwError::BadState),
    }
    crate::vmx::launch::place_guest_uefi_live_esp(bytes.len() as u64)
        .map_err(|_| GuestFwError::TooSmall)?;
    let gpa = crate::vmx::launch::firmware_alias_gpa(bytes.len() as u64).unwrap_or(0);
    audit_log!(AuditEvent::OvmfLiveEspPlaced {
        bytes_len: bytes.len() as u64,
        gpa,
    });
    GUEST_FW_OVMF_LIVE_ESP_PLACED.store(true, Ordering::Release);
    Ok(OvmfLivePlace {
        bytes_len: bytes.len() as u64,
        gpa,
    })
}

/// Apply-attempt real ESP `\EFI\RayNu\OVMF.fd` bytes after place (ADR-014 Stage 30).
///
/// INVARIANTS:
/// - Requires a prior successful [`place_ovmf_live_esp`]
/// - `bytes.len()` and `_FVH` `FvLength` must be `>= MIN_FIRMWARE_ALIAS_BYTES`
/// - Reset vector GPA must sit inside the alias window
/// - Does not flip [`crate::vmx::launch::guest_uefi_live_esp_bytes_present`]
/// - Does not write the E4 SHELL EPT and does not issue VMLAUNCH
/// - Does not flip [`crate::mgmt::iso::attach_cdrom_uefi`]
/// - A heap fixture is not a shipped EDK2 `OVMF.fd`
pub fn apply_ovmf_live_esp(bytes: &[u8]) -> Result<OvmfLiveApply, GuestFwError> {
    if !ovmf_live_esp_is_placed() {
        return Err(GuestFwError::LaunchNotWired);
    }
    if bytes.len() < MIN_FIRMWARE_ALIAS_BYTES {
        return Err(GuestFwError::TooSmall);
    }
    if bytes.len() > GUEST_FW_MAX_UNCOMPRESSED as usize {
        return Err(GuestFwError::TooLarge);
    }
    let probed = probe_ovmf_fv(bytes)?;
    if probed.fv_len < MIN_FIRMWARE_ALIAS_BYTES as u64 {
        return Err(GuestFwError::TooSmall);
    }
    match crate::vmx::launch::probe_guest_uefi_reset_vector(bytes) {
        Ok(()) => {}
        Err(crate::vmx::launch::GuestUefiLaunchError::NoResetVector) => {
            return Err(GuestFwError::NoResetVector);
        }
        Err(_) => return Err(GuestFwError::BadState),
    }
    crate::vmx::launch::apply_guest_uefi_live_esp(bytes.len() as u64)
        .map_err(|_| GuestFwError::TooSmall)?;
    let gpa = crate::vmx::launch::firmware_alias_gpa(bytes.len() as u64).unwrap_or(0);
    audit_log!(AuditEvent::OvmfLiveEspApplied {
        bytes_len: bytes.len() as u64,
        gpa,
    });
    GUEST_FW_OVMF_LIVE_ESP_APPLIED.store(true, Ordering::Release);
    Ok(OvmfLiveApply {
        bytes_len: bytes.len() as u64,
        gpa,
    })
}

/// Commit-attempt real ESP `\EFI\RayNu\OVMF.fd` bytes after apply (ADR-014 Stage 31).
///
/// INVARIANTS:
/// - Requires a prior successful [`apply_ovmf_live_esp`]
/// - `bytes.len()` and `_FVH` `FvLength` must be `>= MIN_FIRMWARE_ALIAS_BYTES`
/// - Reset vector GPA must sit inside the alias window
/// - Does not flip [`crate::vmx::launch::guest_uefi_live_esp_bytes_present`]
/// - Does not write the E4 SHELL EPT and does not issue VMLAUNCH
/// - Does not flip [`crate::mgmt::iso::attach_cdrom_uefi`]
/// - A heap fixture is not a shipped EDK2 `OVMF.fd`
pub fn commit_ovmf_live_esp(bytes: &[u8]) -> Result<OvmfLiveCommit, GuestFwError> {
    if !ovmf_live_esp_is_applied() {
        return Err(GuestFwError::LaunchNotWired);
    }
    if bytes.len() < MIN_FIRMWARE_ALIAS_BYTES {
        return Err(GuestFwError::TooSmall);
    }
    if bytes.len() > GUEST_FW_MAX_UNCOMPRESSED as usize {
        return Err(GuestFwError::TooLarge);
    }
    let probed = probe_ovmf_fv(bytes)?;
    if probed.fv_len < MIN_FIRMWARE_ALIAS_BYTES as u64 {
        return Err(GuestFwError::TooSmall);
    }
    match crate::vmx::launch::probe_guest_uefi_reset_vector(bytes) {
        Ok(()) => {}
        Err(crate::vmx::launch::GuestUefiLaunchError::NoResetVector) => {
            return Err(GuestFwError::NoResetVector);
        }
        Err(_) => return Err(GuestFwError::BadState),
    }
    crate::vmx::launch::commit_guest_uefi_live_esp(bytes.len() as u64)
        .map_err(|_| GuestFwError::TooSmall)?;
    let gpa = crate::vmx::launch::firmware_alias_gpa(bytes.len() as u64).unwrap_or(0);
    audit_log!(AuditEvent::OvmfLiveEspCommitted {
        bytes_len: bytes.len() as u64,
        gpa,
    });
    GUEST_FW_OVMF_LIVE_ESP_COMMITTED.store(true, Ordering::Release);
    Ok(OvmfLiveCommit {
        bytes_len: bytes.len() as u64,
        gpa,
    })
}

/// Latch-attempt real ESP `\EFI\RayNu\OVMF.fd` bytes after commit (ADR-014 Stage 32).
///
/// INVARIANTS:
/// - Requires a prior successful [`commit_ovmf_live_esp`]
/// - `bytes.len()` and `_FVH` `FvLength` must be `>= MIN_FIRMWARE_ALIAS_BYTES`
/// - Reset vector GPA must sit inside the alias window
/// - Does not flip [`crate::vmx::launch::guest_uefi_live_esp_bytes_present`]
/// - Does not write the E4 SHELL EPT and does not issue VMLAUNCH
/// - Does not flip [`crate::mgmt::iso::attach_cdrom_uefi`]
/// - A heap fixture is not a shipped EDK2 `OVMF.fd`
pub fn latch_ovmf_live_esp(bytes: &[u8]) -> Result<OvmfLiveLatch, GuestFwError> {
    if !ovmf_live_esp_is_committed() {
        return Err(GuestFwError::LaunchNotWired);
    }
    if bytes.len() < MIN_FIRMWARE_ALIAS_BYTES {
        return Err(GuestFwError::TooSmall);
    }
    if bytes.len() > GUEST_FW_MAX_UNCOMPRESSED as usize {
        return Err(GuestFwError::TooLarge);
    }
    let probed = probe_ovmf_fv(bytes)?;
    if probed.fv_len < MIN_FIRMWARE_ALIAS_BYTES as u64 {
        return Err(GuestFwError::TooSmall);
    }
    match crate::vmx::launch::probe_guest_uefi_reset_vector(bytes) {
        Ok(()) => {}
        Err(crate::vmx::launch::GuestUefiLaunchError::NoResetVector) => {
            return Err(GuestFwError::NoResetVector);
        }
        Err(_) => return Err(GuestFwError::BadState),
    }
    crate::vmx::launch::latch_guest_uefi_live_esp(bytes.len() as u64)
        .map_err(|_| GuestFwError::TooSmall)?;
    let gpa = crate::vmx::launch::firmware_alias_gpa(bytes.len() as u64).unwrap_or(0);
    audit_log!(AuditEvent::OvmfLiveEspLatched {
        bytes_len: bytes.len() as u64,
        gpa,
    });
    GUEST_FW_OVMF_LIVE_ESP_LATCHED.store(true, Ordering::Release);
    Ok(OvmfLiveLatch {
        bytes_len: bytes.len() as u64,
        gpa,
    })
}

/// Stage a size-floor FV after launch-prepare (ADR-014 Stage 10).
///
/// INVARIANTS:
/// - Requires a prior successful [`prepare_ovmf_firmware_launch`]
/// - `bytes.len()` and `_FVH` `FvLength` must be `>= MIN_LAUNCH_FV_BYTES`
/// - Still below [`MIN_EDK2_OVMF_BYTES`] — not embedded EDK2
/// - Does not VMLAUNCH and does not flip [`crate::mgmt::iso::attach_cdrom_uefi`]
pub fn stage_ovmf_firmware_floor(bytes: &[u8]) -> Result<OvmfFloor, GuestFwError> {
    if !ovmf_launch_is_prepared() {
        return Err(GuestFwError::NotGuestBound);
    }
    if bytes.len() < MIN_LAUNCH_FV_BYTES {
        return Err(GuestFwError::TooSmall);
    }
    if bytes.len() > GUEST_FW_MAX_UNCOMPRESSED as usize {
        return Err(GuestFwError::TooLarge);
    }
    let probed = probe_ovmf_fv(bytes)?;
    if probed.fv_len < MIN_LAUNCH_FV_BYTES as u64 {
        return Err(GuestFwError::TooSmall);
    }
    audit_log!(AuditEvent::OvmfFirmwareFloorStaged {
        bytes_len: bytes.len() as u64,
    });
    GUEST_FW_OVMF_FLOOR_STAGED.store(true, Ordering::Release);
    Ok(OvmfFloor {
        bytes_len: bytes.len() as u64,
    })
}

/// Stage an EDK2-sized FV after the size-floor (ADR-014 Stage 11).
///
/// INVARIANTS:
/// - Requires a prior successful [`stage_ovmf_firmware_floor`]
/// - `bytes.len()` and `_FVH` `FvLength` must be `>= MIN_EDK2_OVMF_BYTES`
/// - Accepts a size-qualified candidate only — not a shipped EDK2 `OVMF.fd`
/// - Does not VMLAUNCH and does not flip [`crate::mgmt::iso::attach_cdrom_uefi`]
pub fn stage_edk2_ovmf_firmware(bytes: &[u8]) -> Result<OvmfEdk2, GuestFwError> {
    if !ovmf_floor_is_staged() {
        return Err(GuestFwError::NotRealFirmware);
    }
    if bytes.len() < MIN_EDK2_OVMF_BYTES {
        return Err(GuestFwError::TooSmall);
    }
    if bytes.len() > GUEST_FW_MAX_UNCOMPRESSED as usize {
        return Err(GuestFwError::TooLarge);
    }
    let probed = probe_ovmf_fv(bytes)?;
    if probed.fv_len < MIN_EDK2_OVMF_BYTES as u64 {
        return Err(GuestFwError::TooSmall);
    }
    audit_log!(AuditEvent::OvmfFirmwareEdk2Staged {
        bytes_len: bytes.len() as u64,
    });
    GUEST_FW_OVMF_EDK2_STAGED.store(true, Ordering::Release);
    Ok(OvmfEdk2 {
        bytes_len: bytes.len() as u64,
    })
}

/// Write a tiny host mock FV (signature + lengths). Not EDK2 OVMF.
pub fn write_mock_ovmf_fv(buf: &mut [u8]) -> Result<(), GuestFwError> {
    if buf.len() < MOCK_OVMF_FV_BYTES {
        return Err(GuestFwError::BadState);
    }
    buf[..MOCK_OVMF_FV_BYTES].fill(0);
    buf[0x20..0x28].copy_from_slice(&(MOCK_OVMF_FV_BYTES as u64).to_le_bytes());
    buf[OVMF_FV_SIG_OFF..OVMF_FV_SIG_OFF + 4].copy_from_slice(&OVMF_FV_SIGNATURE);
    buf[0x30..0x32].copy_from_slice(&0x38u16.to_le_bytes());
    Ok(())
}

/// Write a 4 KiB size-floor FV (signature + lengths). Not EDK2 OVMF.
pub fn write_size_floor_ovmf_fv(buf: &mut [u8]) -> Result<(), GuestFwError> {
    if buf.len() < SIZE_FLOOR_FV_BYTES {
        return Err(GuestFwError::BadState);
    }
    buf[..SIZE_FLOOR_FV_BYTES].fill(0);
    buf[0x20..0x28].copy_from_slice(&(SIZE_FLOOR_FV_BYTES as u64).to_le_bytes());
    buf[OVMF_FV_SIG_OFF..OVMF_FV_SIG_OFF + 4].copy_from_slice(&OVMF_FV_SIGNATURE);
    buf[0x30..0x32].copy_from_slice(&0x38u16.to_le_bytes());
    Ok(())
}

/// Write an EDK2-sized `_FVH` into a caller-provided buffer. Not a shipped image.
///
/// Caller must supply at least [`MIN_EDK2_OVMF_BYTES`]. This writes the
/// header only (`FvLength` = 1 MiB). Do not VMLAUNCH this fixture.
pub fn write_edk2_sized_fv(buf: &mut [u8]) -> Result<(), GuestFwError> {
    if buf.len() < MIN_EDK2_OVMF_BYTES {
        return Err(GuestFwError::TooSmall);
    }
    buf[..MOCK_OVMF_FV_BYTES].fill(0);
    buf[0x20..0x28].copy_from_slice(&(MIN_EDK2_OVMF_BYTES as u64).to_le_bytes());
    buf[OVMF_FV_SIG_OFF..OVMF_FV_SIG_OFF + 4].copy_from_slice(&OVMF_FV_SIGNATURE);
    buf[0x30..0x32].copy_from_slice(&0x38u16.to_le_bytes());
    Ok(())
}

/// Write a live-sized `_FVH` into a caller-provided buffer. Not a shipped image.
///
/// Caller must supply at least [`MIN_LIVE_ESP_OVMF_BYTES`]. This writes the
/// header only (`FvLength` = 2 MiB). Do not VMLAUNCH this fixture.
pub fn write_live_esp_ovmf_fv(buf: &mut [u8]) -> Result<(), GuestFwError> {
    if buf.len() < MIN_LIVE_ESP_OVMF_BYTES {
        return Err(GuestFwError::TooSmall);
    }
    buf[..MOCK_OVMF_FV_BYTES].fill(0);
    buf[0x20..0x28].copy_from_slice(&(MIN_LIVE_ESP_OVMF_BYTES as u64).to_le_bytes());
    buf[OVMF_FV_SIG_OFF..OVMF_FV_SIG_OFF + 4].copy_from_slice(&OVMF_FV_SIGNATURE);
    buf[0x30..0x32].copy_from_slice(&0x38u16.to_le_bytes());
    Ok(())
}

/// Write a JMP FAR reset-vector stub at the end of a live-sized buffer.
///
/// Not EDK2 SEC and not a shipped `OVMF.fd`. Do not VMLAUNCH this stub.
pub fn write_reset_vector_stub(buf: &mut [u8]) -> Result<(), GuestFwError> {
    if buf.len() < MIN_LIVE_ESP_OVMF_BYTES {
        return Err(GuestFwError::TooSmall);
    }
    let off = buf.len() - crate::vmx::launch::GUEST_UEFI_RESET_VECTOR_LEN;
    buf[off] = crate::vmx::launch::GUEST_UEFI_RESET_VECTOR_OPCODE;
    buf[off + 1] = 0x00;
    buf[off + 2] = 0x00;
    buf[off + 3] = 0x00;
    buf[off + 4] = 0xF0;
    Ok(())
}

/// Write a firmware-alias `_FVH` + JMP FAR stub. Not a shipped `OVMF.fd`.
///
/// Caller must supply at least [`MIN_FIRMWARE_ALIAS_BYTES`]. This writes the
/// header (`FvLength` = 4 MiB) and reset stub only. Do not VMLAUNCH this fixture.
pub fn write_firmware_alias_fv(buf: &mut [u8]) -> Result<(), GuestFwError> {
    if buf.len() < MIN_FIRMWARE_ALIAS_BYTES {
        return Err(GuestFwError::TooSmall);
    }
    buf[..MOCK_OVMF_FV_BYTES].fill(0);
    buf[0x20..0x28].copy_from_slice(&(MIN_FIRMWARE_ALIAS_BYTES as u64).to_le_bytes());
    buf[OVMF_FV_SIG_OFF..OVMF_FV_SIG_OFF + 4].copy_from_slice(&OVMF_FV_SIGNATURE);
    buf[0x30..0x32].copy_from_slice(&0x38u16.to_le_bytes());
    write_reset_vector_stub(buf)
}

/// True when `path` is the guest-firmware envelope REST surface.
pub fn is_guest_fw_path(path: &str) -> bool {
    let path = path.trim().trim_end_matches('/');
    path == "/fw"
        || path == "/fw/box"
        || path == "/fw/load"
        || path == "/fw/ovmf"
        || path == "/fw/ovmf/esp"
        || path == "/fw/slot"
        || path == "/fw/bind"
        || path == "/fw/prepare"
        || path == "/fw/floor"
        || path == "/fw/edk2"
        || path == "/fw/esp-launch"
        || path == "/fw/esp-map"
        || path == "/fw/reset-vec"
        || path == "/fw/alias"
        || path == "/fw/alias-ept"
        || path == "/fw/ept-install"
        || path == "/fw/real-esp"
        || path == "/fw/real-launch"
        || path == "/fw/live-exec"
        || path == "/fw/priv-vmcs"
        || path == "/fw/live-issue"
        || path == "/fw/live-bytes"
        || path == "/fw/live-fd"
        || path == "/fw/live-present"
        || path == "/fw/live-admit"
        || path == "/fw/live-read"
        || path == "/fw/live-copy"
        || path == "/fw/live-place"
        || path == "/fw/live-apply"
        || path == "/fw/live-commit"
        || path == "/fw/live-latch"
        || path == "/fw/vmlaunch"
}

enum GuestFwOp {
    Status,
    Box,
    LoadStatus,
    Load,
    OvmfStatus,
    OvmfProbe,
    OvmfEspStatus,
    OvmfEspLoad,
    OvmfSlotStatus,
    OvmfSlotArm,
    OvmfBindStatus,
    OvmfBindGuest,
    OvmfPrepStatus,
    OvmfPrepLaunch,
    OvmfFloorStatus,
    OvmfFloorStage,
    OvmfEdk2Status,
    OvmfEdk2Stage,
    OvmfEspLaunchStatus,
    OvmfEspLaunchArm,
    OvmfEspMapStatus,
    OvmfEspMap,
    OvmfResetVecStatus,
    OvmfResetVecArm,
    OvmfAliasStatus,
    OvmfAliasArm,
    OvmfAliasEptStatus,
    OvmfAliasEptProgram,
    OvmfAliasEptInstallStatus,
    OvmfAliasEptInstall,
    OvmfRealEspStatus,
    OvmfRealEspQualify,
    OvmfRealLaunchStatus,
    OvmfRealLaunchArm,
    OvmfLiveExecStatus,
    OvmfLiveExecRequire,
    OvmfPrivateVmcsStatus,
    OvmfPrivateVmcsArm,
    OvmfLiveIssueStatus,
    OvmfLiveIssueArm,
    OvmfLiveBytesStatus,
    OvmfLiveBytesProbe,
    OvmfLiveFdStatus,
    OvmfLiveFdRequire,
    OvmfLivePresentStatus,
    OvmfLivePresent,
    OvmfLiveAdmitStatus,
    OvmfLiveAdmit,
    OvmfLiveReadStatus,
    OvmfLiveRead,
    OvmfLiveCopyStatus,
    OvmfLiveCopy,
    OvmfLivePlaceStatus,
    OvmfLivePlace,
    OvmfLiveApplyStatus,
    OvmfLiveApply,
    OvmfLiveCommitStatus,
    OvmfLiveCommit,
    OvmfLiveLatchStatus,
    OvmfLiveLatch,
    OvmfTryVmlaunch,
}

fn route_guest_fw(method: RestMethod, path: &str) -> Result<GuestFwOp, ()> {
    let path = path.trim().trim_end_matches('/');
    match (method, path) {
        (RestMethod::Get, "/fw") => Ok(GuestFwOp::Status),
        (RestMethod::Post, "/fw/box") => Ok(GuestFwOp::Box),
        (RestMethod::Get, "/fw/load") => Ok(GuestFwOp::LoadStatus),
        (RestMethod::Post, "/fw/load") => Ok(GuestFwOp::Load),
        (RestMethod::Get, "/fw/ovmf") => Ok(GuestFwOp::OvmfStatus),
        (RestMethod::Post, "/fw/ovmf") => Ok(GuestFwOp::OvmfProbe),
        (RestMethod::Get, "/fw/ovmf/esp") => Ok(GuestFwOp::OvmfEspStatus),
        (RestMethod::Post, "/fw/ovmf/esp") => Ok(GuestFwOp::OvmfEspLoad),
        (RestMethod::Get, "/fw/slot") => Ok(GuestFwOp::OvmfSlotStatus),
        (RestMethod::Post, "/fw/slot") => Ok(GuestFwOp::OvmfSlotArm),
        (RestMethod::Get, "/fw/bind") => Ok(GuestFwOp::OvmfBindStatus),
        (RestMethod::Post, "/fw/bind") => Ok(GuestFwOp::OvmfBindGuest),
        (RestMethod::Get, "/fw/prepare") => Ok(GuestFwOp::OvmfPrepStatus),
        (RestMethod::Post, "/fw/prepare") => Ok(GuestFwOp::OvmfPrepLaunch),
        (RestMethod::Get, "/fw/floor") => Ok(GuestFwOp::OvmfFloorStatus),
        (RestMethod::Post, "/fw/floor") => Ok(GuestFwOp::OvmfFloorStage),
        (RestMethod::Get, "/fw/edk2") => Ok(GuestFwOp::OvmfEdk2Status),
        (RestMethod::Post, "/fw/edk2") => Ok(GuestFwOp::OvmfEdk2Stage),
        (RestMethod::Get, "/fw/esp-launch") => Ok(GuestFwOp::OvmfEspLaunchStatus),
        (RestMethod::Post, "/fw/esp-launch") => Ok(GuestFwOp::OvmfEspLaunchArm),
        (RestMethod::Get, "/fw/esp-map") => Ok(GuestFwOp::OvmfEspMapStatus),
        (RestMethod::Post, "/fw/esp-map") => Ok(GuestFwOp::OvmfEspMap),
        (RestMethod::Get, "/fw/reset-vec") => Ok(GuestFwOp::OvmfResetVecStatus),
        (RestMethod::Post, "/fw/reset-vec") => Ok(GuestFwOp::OvmfResetVecArm),
        (RestMethod::Get, "/fw/alias") => Ok(GuestFwOp::OvmfAliasStatus),
        (RestMethod::Post, "/fw/alias") => Ok(GuestFwOp::OvmfAliasArm),
        (RestMethod::Get, "/fw/alias-ept") => Ok(GuestFwOp::OvmfAliasEptStatus),
        (RestMethod::Post, "/fw/alias-ept") => Ok(GuestFwOp::OvmfAliasEptProgram),
        (RestMethod::Get, "/fw/ept-install") => Ok(GuestFwOp::OvmfAliasEptInstallStatus),
        (RestMethod::Post, "/fw/ept-install") => Ok(GuestFwOp::OvmfAliasEptInstall),
        (RestMethod::Get, "/fw/real-esp") => Ok(GuestFwOp::OvmfRealEspStatus),
        (RestMethod::Post, "/fw/real-esp") => Ok(GuestFwOp::OvmfRealEspQualify),
        (RestMethod::Get, "/fw/real-launch") => Ok(GuestFwOp::OvmfRealLaunchStatus),
        (RestMethod::Post, "/fw/real-launch") => Ok(GuestFwOp::OvmfRealLaunchArm),
        (RestMethod::Get, "/fw/live-exec") => Ok(GuestFwOp::OvmfLiveExecStatus),
        (RestMethod::Post, "/fw/live-exec") => Ok(GuestFwOp::OvmfLiveExecRequire),
        (RestMethod::Get, "/fw/priv-vmcs") => Ok(GuestFwOp::OvmfPrivateVmcsStatus),
        (RestMethod::Post, "/fw/priv-vmcs") => Ok(GuestFwOp::OvmfPrivateVmcsArm),
        (RestMethod::Get, "/fw/live-issue") => Ok(GuestFwOp::OvmfLiveIssueStatus),
        (RestMethod::Post, "/fw/live-issue") => Ok(GuestFwOp::OvmfLiveIssueArm),
        (RestMethod::Get, "/fw/live-bytes") => Ok(GuestFwOp::OvmfLiveBytesStatus),
        (RestMethod::Post, "/fw/live-bytes") => Ok(GuestFwOp::OvmfLiveBytesProbe),
        (RestMethod::Get, "/fw/live-fd") => Ok(GuestFwOp::OvmfLiveFdStatus),
        (RestMethod::Post, "/fw/live-fd") => Ok(GuestFwOp::OvmfLiveFdRequire),
        (RestMethod::Get, "/fw/live-present") => Ok(GuestFwOp::OvmfLivePresentStatus),
        (RestMethod::Post, "/fw/live-present") => Ok(GuestFwOp::OvmfLivePresent),
        (RestMethod::Get, "/fw/live-admit") => Ok(GuestFwOp::OvmfLiveAdmitStatus),
        (RestMethod::Post, "/fw/live-admit") => Ok(GuestFwOp::OvmfLiveAdmit),
        (RestMethod::Get, "/fw/live-read") => Ok(GuestFwOp::OvmfLiveReadStatus),
        (RestMethod::Post, "/fw/live-read") => Ok(GuestFwOp::OvmfLiveRead),
        (RestMethod::Get, "/fw/live-copy") => Ok(GuestFwOp::OvmfLiveCopyStatus),
        (RestMethod::Post, "/fw/live-copy") => Ok(GuestFwOp::OvmfLiveCopy),
        (RestMethod::Get, "/fw/live-place") => Ok(GuestFwOp::OvmfLivePlaceStatus),
        (RestMethod::Post, "/fw/live-place") => Ok(GuestFwOp::OvmfLivePlace),
        (RestMethod::Get, "/fw/live-apply") => Ok(GuestFwOp::OvmfLiveApplyStatus),
        (RestMethod::Post, "/fw/live-apply") => Ok(GuestFwOp::OvmfLiveApply),
        (RestMethod::Get, "/fw/live-commit") => Ok(GuestFwOp::OvmfLiveCommitStatus),
        (RestMethod::Post, "/fw/live-commit") => Ok(GuestFwOp::OvmfLiveCommit),
        (RestMethod::Get, "/fw/live-latch") => Ok(GuestFwOp::OvmfLiveLatchStatus),
        (RestMethod::Post, "/fw/live-latch") => Ok(GuestFwOp::OvmfLiveLatch),
        (RestMethod::Post, "/fw/vmlaunch") => Ok(GuestFwOp::OvmfTryVmlaunch),
        _ => Err(()),
    }
}

fn guest_fw_err_status(e: GuestFwError) -> u16 {
    match e {
        GuestFwError::BadMagic
        | GuestFwError::BadState
        | GuestFwError::TooLarge
        | GuestFwError::NotBoxed
        | GuestFwError::NotLoaded
        | GuestFwError::NotProbed
        | GuestFwError::MissingEsp
        | GuestFwError::NotEspLoaded
        | GuestFwError::NotSlotArmed
        | GuestFwError::NotGuestBound
        | GuestFwError::MockFirmwareRefused
        | GuestFwError::TooSmall
        | GuestFwError::NotRealFirmware
        | GuestFwError::LaunchNotWired
        | GuestFwError::LiveMappedNotLaunched
        | GuestFwError::NoResetVector
        | GuestFwError::ResetVectorNotLaunched
        | GuestFwError::FirmwareAliasNotLaunched
        | GuestFwError::AliasEptNotLaunched
        | GuestFwError::AliasEptInstalledNotLaunched
        | GuestFwError::RealEspNotLaunched
        | GuestFwError::RealLaunchNotIssued
        | GuestFwError::LiveEspRequired
        | GuestFwError::PrivateVmcsNotLaunched
        | GuestFwError::LiveEspBytesNotPresent
        | GuestFwError::LiveEspBytesAbsent
        | GuestFwError::LiveEspFdAbsent
        | GuestFwError::LiveEspPresentAbsent
        | GuestFwError::LiveEspAdmitAbsent
        | GuestFwError::LiveEspReadAbsent
        | GuestFwError::LiveEspCopyAbsent
        | GuestFwError::LiveEspPlaceAbsent
        | GuestFwError::LiveEspApplyAbsent
        | GuestFwError::LiveEspCommitAbsent
        | GuestFwError::LiveEspLatchAbsent => 409,
    }
}

/// REST: `POST /fw/box` boxes the envelope. `POST /fw/load` lazy-loads the
/// stub payload after box. `POST /fw/ovmf` probes a host mock `_FVH` after
/// load. `POST /fw/ovmf/esp` loads the ESP fixture after probe.
/// `POST /fw/slot` arms guest firmware slot 1 after ESP load.
/// `POST /fw/bind` binds slot 1 to guest 1 after arm.
/// `POST /fw/prepare` records launch-prepare after bind.
/// `POST /fw/floor` stages a 4 KiB size-floor FV after prepare.
/// `POST /fw/edk2` stages an EDK2-sized candidate after floor (host test
/// heap fixture only). Production UEFI returns 409 (`MissingEsp`) — no
/// embedded 1 MiB image (ADR-003). `POST /fw/esp-launch` arms the ESP-path
/// VMLAUNCH contract after EDK2. `POST /fw/esp-map` records a live-sized
/// ESP map after launch-arm (host test heap fixture only). Production UEFI
/// returns 409 (`MissingEsp`) — no embedded 2 MiB (ADR-003).
/// `POST /fw/reset-vec` records the reset-vector VMCS contract after
/// the live map (host test heap stub only). Production UEFI returns 409
/// (`MissingEsp`) — no embedded 2 MiB. `POST /fw/alias` records the
/// unrestricted-guest + 4 GiB firmware-alias contract after reset-vector
/// (host test heap fixture only). Production UEFI returns 409
/// (`MissingEsp`) — no embedded 4 MiB. `POST /fw/alias-ept` records the
/// alias-EPT program contract after alias (host test heap fixture only).
/// `POST /fw/ept-install` records a private alias-EPT install after
/// program (host test heap fixture only). `POST /fw/real-esp` records
/// the real-ESP VMLAUNCH-ready contract after install (host test heap
/// fixture only). `POST /fw/real-launch` records the guest-UEFI
/// VMLAUNCH insn-path arm after qualify (host test heap fixture only).
/// `POST /fw/live-exec` records that live ESP `\EFI\RayNu\OVMF.fd`
/// bytes are required before VMLAUNCH (host test heap fixture only).
/// `POST /fw/priv-vmcs` records a private guest-UEFI VMCS (not E4 SHELL)
/// after live-ESP require (host test heap fixture only).
/// `POST /fw/live-issue` records the live-ESP VMLAUNCH issue path after
/// private VMCS (host test heap fixture only).
/// `POST /fw/live-bytes` records a live-ESP bytes probe after live-issue
/// (host test heap fixture only).
/// `POST /fw/live-fd` records that a real ESP `OVMF.fd` is required after
/// the live-bytes probe (host test heap fixture only).
/// `POST /fw/live-present` records a real-ESP present-attempt after
/// live-FD require (host test heap fixture only).
/// `POST /fw/live-admit` records a real-ESP admit-attempt after
/// live-present (host test heap fixture only).
/// `POST /fw/live-read` records a real-ESP read-attempt after
/// live-admit (host test heap fixture only).
/// `POST /fw/live-copy` records a real-ESP copy-attempt after
/// live-read (host test heap fixture only).
/// `POST /fw/live-place` records a real-ESP place-attempt after
/// live-copy (host test heap fixture only).
/// `POST /fw/live-apply` records a real-ESP apply-attempt after
/// live-place (host test heap fixture only).
/// `POST /fw/live-commit` records a real-ESP commit-attempt after
/// live-apply (host test heap fixture only).
/// `POST /fw/live-latch` records a real-ESP latch-attempt after
/// live-commit (host test heap fixture only).
/// Production UEFI returns 409 (`MissingEsp`) — no
/// embedded 4 MiB. `POST /fw/vmlaunch` then calls
/// `try_vmlaunch_guest_uefi_ovmf` (unmapped → 409 `MissingEsp`; mapped →
/// 409 `LiveMappedNotLaunched`; reset-vector armed → 409
/// `ResetVectorNotLaunched`; alias armed → 409 `FirmwareAliasNotLaunched`;
/// alias-EPT programmed → 409 `AliasEptNotLaunched`; private EPT
/// installed → 409 `AliasEptInstalledNotLaunched`; real-ESP qualified →
/// 409 `RealEspNotLaunched`; insn path armed → 409 `RealLaunchNotIssued`;
/// live-ESP required → 409 `LiveEspRequired`; private VMCS selected →
/// 409 `PrivateVmcsNotLaunched`; live-issue armed → 409
/// `LiveEspBytesNotPresent`; live bytes probed → 409
/// `LiveEspBytesAbsent`; live FD required → 409 `LiveEspFdAbsent`;
/// live ESP presented → 409 `LiveEspPresentAbsent`; live ESP admitted →
/// 409 `LiveEspAdmitAbsent`; live ESP read-attempted →
/// 409 `LiveEspReadAbsent`; live ESP copy-attempted →
/// 409 `LiveEspCopyAbsent`; live ESP place-attempted →
/// 409 `LiveEspPlaceAbsent`; live ESP apply-attempted →
/// 409 `LiveEspApplyAbsent`; live ESP commit-attempted →
/// 409 `LiveEspCommitAbsent`; live ESP latch-attempted →
/// 409 `LiveEspLatchAbsent`).
/// GET paths return counts. Not a shipped EDK2 `OVMF.fd`. Not VMLAUNCH.
/// Not a live E4 SHELL EPT write.
pub fn dispatch_guest_fw_rest(req: RestRequest<'_>) -> RestResponse {
    if !auth_allows(req.auth_token) {
        return RestResponse {
            status: 401,
            reply: None,
        };
    }
    match route_guest_fw(req.method, req.path) {
        Ok(GuestFwOp::Status) => RestResponse {
            status: 200,
            reply: Some(ApiReply::Listed {
                count: if guest_fw_is_boxed() { 1 } else { 0 },
            }),
        },
        Ok(GuestFwOp::Box) => match box_guest_firmware(guest_fw_bytes()) {
            Ok(_) => RestResponse {
                status: 201,
                reply: Some(ApiReply::Ok),
            },
            Err(e) => RestResponse {
                status: guest_fw_err_status(e),
                reply: None,
            },
        },
        Ok(GuestFwOp::LoadStatus) => RestResponse {
            status: 200,
            reply: Some(ApiReply::Listed {
                count: if guest_fw_is_loaded() { 1 } else { 0 },
            }),
        },
        Ok(GuestFwOp::Load) => match load_guest_firmware(guest_fw_bytes()) {
            Ok(_) => RestResponse {
                status: 201,
                reply: Some(ApiReply::Ok),
            },
            Err(e) => RestResponse {
                status: guest_fw_err_status(e),
                reply: None,
            },
        },
        Ok(GuestFwOp::OvmfStatus) => RestResponse {
            status: 200,
            reply: Some(ApiReply::Listed {
                count: if ovmf_fv_is_probed() { 1 } else { 0 },
            }),
        },
        Ok(GuestFwOp::OvmfProbe) => {
            let mut fv = [0u8; MOCK_OVMF_FV_BYTES];
            if write_mock_ovmf_fv(&mut fv).is_err() {
                return RestResponse {
                    status: 500,
                    reply: None,
                };
            }
            match probe_ovmf_firmware(&fv) {
                Ok(_) => RestResponse {
                    status: 201,
                    reply: Some(ApiReply::Ok),
                },
                Err(e) => RestResponse {
                    status: guest_fw_err_status(e),
                    reply: None,
                },
            }
        }
        Ok(GuestFwOp::OvmfEspStatus) => RestResponse {
            status: 200,
            reply: Some(ApiReply::Listed {
                count: if ovmf_esp_is_loaded() { 1 } else { 0 },
            }),
        },
        Ok(GuestFwOp::OvmfEspLoad) => {
            let mut fv = [0u8; MOCK_OVMF_FV_BYTES];
            if write_mock_ovmf_fv(&mut fv).is_err() {
                return RestResponse {
                    status: 500,
                    reply: None,
                };
            }
            match load_ovmf_from_esp(&fv) {
                Ok(_) => RestResponse {
                    status: 201,
                    reply: Some(ApiReply::Ok),
                },
                Err(e) => RestResponse {
                    status: guest_fw_err_status(e),
                    reply: None,
                },
            }
        }
        Ok(GuestFwOp::OvmfSlotStatus) => RestResponse {
            status: 200,
            reply: Some(ApiReply::Listed {
                count: if ovmf_slot_is_armed() { 1 } else { 0 },
            }),
        },
        Ok(GuestFwOp::OvmfSlotArm) => match arm_ovmf_firmware_slot() {
            Ok(_) => RestResponse {
                status: 201,
                reply: Some(ApiReply::Ok),
            },
            Err(e) => RestResponse {
                status: guest_fw_err_status(e),
                reply: None,
            },
        },
        Ok(GuestFwOp::OvmfBindStatus) => RestResponse {
            status: 200,
            reply: Some(ApiReply::Listed {
                count: if ovmf_guest_is_bound() { 1 } else { 0 },
            }),
        },
        Ok(GuestFwOp::OvmfBindGuest) => match bind_ovmf_firmware_guest() {
            Ok(_) => RestResponse {
                status: 201,
                reply: Some(ApiReply::Ok),
            },
            Err(e) => RestResponse {
                status: guest_fw_err_status(e),
                reply: None,
            },
        },
        Ok(GuestFwOp::OvmfPrepStatus) => RestResponse {
            status: 200,
            reply: Some(ApiReply::Listed {
                count: if ovmf_launch_is_prepared() { 1 } else { 0 },
            }),
        },
        Ok(GuestFwOp::OvmfPrepLaunch) => match prepare_ovmf_firmware_launch() {
            Ok(_) => RestResponse {
                status: 201,
                reply: Some(ApiReply::Ok),
            },
            Err(e) => RestResponse {
                status: guest_fw_err_status(e),
                reply: None,
            },
        },
        Ok(GuestFwOp::OvmfFloorStatus) => RestResponse {
            status: 200,
            reply: Some(ApiReply::Listed {
                count: if ovmf_floor_is_staged() { 1 } else { 0 },
            }),
        },
        Ok(GuestFwOp::OvmfFloorStage) => {
            let mut fv = [0u8; SIZE_FLOOR_FV_BYTES];
            if write_size_floor_ovmf_fv(&mut fv).is_err() {
                return RestResponse {
                    status: 500,
                    reply: None,
                };
            }
            match stage_ovmf_firmware_floor(&fv) {
                Ok(_) => RestResponse {
                    status: 201,
                    reply: Some(ApiReply::Ok),
                },
                Err(e) => RestResponse {
                    status: guest_fw_err_status(e),
                    reply: None,
                },
            }
        }
        Ok(GuestFwOp::OvmfEdk2Status) => RestResponse {
            status: 200,
            reply: Some(ApiReply::Listed {
                count: if ovmf_edk2_is_staged() { 1 } else { 0 },
            }),
        },
        Ok(GuestFwOp::OvmfEdk2Stage) => edk2_stage_rest(),
        Ok(GuestFwOp::OvmfEspLaunchStatus) => RestResponse {
            status: 200,
            reply: Some(ApiReply::Listed {
                count: if ovmf_esp_launch_is_armed() { 1 } else { 0 },
            }),
        },
        Ok(GuestFwOp::OvmfEspLaunchArm) => match arm_ovmf_esp_launch() {
            Ok(_) => RestResponse {
                status: 201,
                reply: Some(ApiReply::Ok),
            },
            Err(e) => RestResponse {
                status: guest_fw_err_status(e),
                reply: None,
            },
        },
        Ok(GuestFwOp::OvmfEspMapStatus) => RestResponse {
            status: 200,
            reply: Some(ApiReply::Listed {
                count: if ovmf_live_esp_is_mapped() { 1 } else { 0 },
            }),
        },
        Ok(GuestFwOp::OvmfEspMap) => live_esp_map_rest(),
        Ok(GuestFwOp::OvmfResetVecStatus) => RestResponse {
            status: 200,
            reply: Some(ApiReply::Listed {
                count: if ovmf_reset_vector_is_armed() { 1 } else { 0 },
            }),
        },
        Ok(GuestFwOp::OvmfResetVecArm) => reset_vec_rest(),
        Ok(GuestFwOp::OvmfAliasStatus) => RestResponse {
            status: 200,
            reply: Some(ApiReply::Listed {
                count: if ovmf_firmware_alias_is_armed() { 1 } else { 0 },
            }),
        },
        Ok(GuestFwOp::OvmfAliasArm) => firmware_alias_rest(),
        Ok(GuestFwOp::OvmfAliasEptStatus) => RestResponse {
            status: 200,
            reply: Some(ApiReply::Listed {
                count: if ovmf_alias_ept_is_programmed() { 1 } else { 0 },
            }),
        },
        Ok(GuestFwOp::OvmfAliasEptProgram) => alias_ept_rest(),
        Ok(GuestFwOp::OvmfAliasEptInstallStatus) => RestResponse {
            status: 200,
            reply: Some(ApiReply::Listed {
                count: if ovmf_alias_ept_is_installed() { 1 } else { 0 },
            }),
        },
        Ok(GuestFwOp::OvmfAliasEptInstall) => ept_install_rest(),
        Ok(GuestFwOp::OvmfRealEspStatus) => RestResponse {
            status: 200,
            reply: Some(ApiReply::Listed {
                count: if ovmf_real_esp_is_qualified() { 1 } else { 0 },
            }),
        },
        Ok(GuestFwOp::OvmfRealEspQualify) => real_esp_rest(),
        Ok(GuestFwOp::OvmfRealLaunchStatus) => RestResponse {
            status: 200,
            reply: Some(ApiReply::Listed {
                count: if ovmf_real_launch_is_armed() { 1 } else { 0 },
            }),
        },
        Ok(GuestFwOp::OvmfRealLaunchArm) => real_launch_rest(),
        Ok(GuestFwOp::OvmfLiveExecStatus) => RestResponse {
            status: 200,
            reply: Some(ApiReply::Listed {
                count: if ovmf_live_esp_is_required() { 1 } else { 0 },
            }),
        },
        Ok(GuestFwOp::OvmfLiveExecRequire) => live_exec_rest(),
        Ok(GuestFwOp::OvmfPrivateVmcsStatus) => RestResponse {
            status: 200,
            reply: Some(ApiReply::Listed {
                count: if ovmf_private_vmcs_is_armed() { 1 } else { 0 },
            }),
        },
        Ok(GuestFwOp::OvmfPrivateVmcsArm) => priv_vmcs_rest(),
        Ok(GuestFwOp::OvmfLiveIssueStatus) => RestResponse {
            status: 200,
            reply: Some(ApiReply::Listed {
                count: if ovmf_live_issue_is_armed() { 1 } else { 0 },
            }),
        },
        Ok(GuestFwOp::OvmfLiveIssueArm) => live_issue_rest(),
        Ok(GuestFwOp::OvmfLiveBytesStatus) => RestResponse {
            status: 200,
            reply: Some(ApiReply::Listed {
                count: if ovmf_live_bytes_is_probed() { 1 } else { 0 },
            }),
        },
        Ok(GuestFwOp::OvmfLiveBytesProbe) => live_bytes_rest(),
        Ok(GuestFwOp::OvmfLiveFdStatus) => RestResponse {
            status: 200,
            reply: Some(ApiReply::Listed {
                count: if ovmf_live_fd_is_required() { 1 } else { 0 },
            }),
        },
        Ok(GuestFwOp::OvmfLiveFdRequire) => live_fd_rest(),
        Ok(GuestFwOp::OvmfLivePresentStatus) => RestResponse {
            status: 200,
            reply: Some(ApiReply::Listed {
                count: if ovmf_live_esp_is_presented() { 1 } else { 0 },
            }),
        },
        Ok(GuestFwOp::OvmfLivePresent) => live_present_rest(),
        Ok(GuestFwOp::OvmfLiveAdmitStatus) => RestResponse {
            status: 200,
            reply: Some(ApiReply::Listed {
                count: if ovmf_live_esp_is_admitted() { 1 } else { 0 },
            }),
        },
        Ok(GuestFwOp::OvmfLiveAdmit) => live_admit_rest(),
        Ok(GuestFwOp::OvmfLiveReadStatus) => RestResponse {
            status: 200,
            reply: Some(ApiReply::Listed {
                count: if ovmf_live_esp_is_read() { 1 } else { 0 },
            }),
        },
        Ok(GuestFwOp::OvmfLiveRead) => live_read_rest(),
        Ok(GuestFwOp::OvmfLiveCopyStatus) => RestResponse {
            status: 200,
            reply: Some(ApiReply::Listed {
                count: if ovmf_live_esp_is_copied() { 1 } else { 0 },
            }),
        },
        Ok(GuestFwOp::OvmfLiveCopy) => live_copy_rest(),
        Ok(GuestFwOp::OvmfLivePlaceStatus) => RestResponse {
            status: 200,
            reply: Some(ApiReply::Listed {
                count: if ovmf_live_esp_is_placed() { 1 } else { 0 },
            }),
        },
        Ok(GuestFwOp::OvmfLivePlace) => live_place_rest(),
        Ok(GuestFwOp::OvmfLiveApplyStatus) => RestResponse {
            status: 200,
            reply: Some(ApiReply::Listed {
                count: if ovmf_live_esp_is_applied() { 1 } else { 0 },
            }),
        },
        Ok(GuestFwOp::OvmfLiveApply) => live_apply_rest(),
        Ok(GuestFwOp::OvmfLiveCommitStatus) => RestResponse {
            status: 200,
            reply: Some(ApiReply::Listed {
                count: if ovmf_live_esp_is_committed() { 1 } else { 0 },
            }),
        },
        Ok(GuestFwOp::OvmfLiveCommit) => live_commit_rest(),
        Ok(GuestFwOp::OvmfLiveLatchStatus) => RestResponse {
            status: 200,
            reply: Some(ApiReply::Listed {
                count: if ovmf_live_esp_is_latched() { 1 } else { 0 },
            }),
        },
        Ok(GuestFwOp::OvmfLiveLatch) => live_latch_rest(),
        Ok(GuestFwOp::OvmfTryVmlaunch) => match try_vmlaunch_ovmf_firmware() {
            Ok(()) => RestResponse {
                status: 500,
                reply: None,
            },
            Err(e) => RestResponse {
                status: guest_fw_err_status(e),
                reply: None,
            },
        },
        Err(()) => RestResponse {
            status: 400,
            reply: None,
        },
    }
}

/// Host tests: heap 1 MiB size fixture. Not a shipped EDK2 `OVMF.fd`.
/// Production UEFI: 409 `MissingEsp` — no embedded 1 MiB (ADR-003).
#[cfg(test)]
fn edk2_stage_rest() -> RestResponse {
    let mut fv = vec![0u8; MIN_EDK2_OVMF_BYTES];
    if write_edk2_sized_fv(&mut fv).is_err() {
        return RestResponse {
            status: 500,
            reply: None,
        };
    }
    match stage_edk2_ovmf_firmware(&fv) {
        Ok(_) => RestResponse {
            status: 201,
            reply: Some(ApiReply::Ok),
        },
        Err(e) => RestResponse {
            status: guest_fw_err_status(e),
            reply: None,
        },
    }
}

/// Production: no embedded 1 MiB EDK2 (ADR-003 split-mode / ESP only).
#[cfg(not(test))]
fn edk2_stage_rest() -> RestResponse {
    RestResponse {
        status: guest_fw_err_status(GuestFwError::MissingEsp),
        reply: None,
    }
}

/// Host tests: heap 2 MiB live-sized fixture. Not a shipped `OVMF.fd`.
/// Production UEFI: 409 `MissingEsp` — no embedded 2 MiB (ADR-003).
#[cfg(test)]
fn live_esp_map_rest() -> RestResponse {
    let mut fv = vec![0u8; MIN_LIVE_ESP_OVMF_BYTES];
    if write_live_esp_ovmf_fv(&mut fv).is_err() {
        return RestResponse {
            status: 500,
            reply: None,
        };
    }
    match map_live_esp_ovmf(&fv) {
        Ok(_) => RestResponse {
            status: 201,
            reply: Some(ApiReply::Ok),
        },
        Err(e) => RestResponse {
            status: guest_fw_err_status(e),
            reply: None,
        },
    }
}

/// Production: no embedded 2 MiB live map (ADR-003 split-mode / ESP only).
#[cfg(not(test))]
fn live_esp_map_rest() -> RestResponse {
    RestResponse {
        status: guest_fw_err_status(GuestFwError::MissingEsp),
        reply: None,
    }
}

/// Host tests: heap 2 MiB + JMP FAR stub. Not a shipped `OVMF.fd`.
/// Production UEFI: 409 `MissingEsp` — no embedded 2 MiB (ADR-003).
#[cfg(test)]
fn reset_vec_rest() -> RestResponse {
    let mut fv = vec![0u8; MIN_LIVE_ESP_OVMF_BYTES];
    if write_live_esp_ovmf_fv(&mut fv).is_err() || write_reset_vector_stub(&mut fv).is_err() {
        return RestResponse {
            status: 500,
            reply: None,
        };
    }
    match arm_ovmf_reset_vector(&fv) {
        Ok(_) => RestResponse {
            status: 201,
            reply: Some(ApiReply::Ok),
        },
        Err(e) => RestResponse {
            status: guest_fw_err_status(e),
            reply: None,
        },
    }
}

/// Production: no embedded reset-vector image (ADR-003 split-mode / ESP only).
#[cfg(not(test))]
fn reset_vec_rest() -> RestResponse {
    RestResponse {
        status: guest_fw_err_status(GuestFwError::MissingEsp),
        reply: None,
    }
}

/// Host tests: heap 4 MiB + JMP FAR stub. Not a shipped `OVMF.fd`.
/// Production UEFI: 409 `MissingEsp` — no embedded 4 MiB (ADR-003).
#[cfg(test)]
fn firmware_alias_rest() -> RestResponse {
    let mut fv = vec![0u8; MIN_FIRMWARE_ALIAS_BYTES];
    if write_firmware_alias_fv(&mut fv).is_err() {
        return RestResponse {
            status: 500,
            reply: None,
        };
    }
    match arm_ovmf_firmware_alias(&fv) {
        Ok(_) => RestResponse {
            status: 201,
            reply: Some(ApiReply::Ok),
        },
        Err(e) => RestResponse {
            status: guest_fw_err_status(e),
            reply: None,
        },
    }
}

/// Production: no embedded 4 MiB firmware alias (ADR-003 split-mode / ESP only).
#[cfg(not(test))]
fn firmware_alias_rest() -> RestResponse {
    RestResponse {
        status: guest_fw_err_status(GuestFwError::MissingEsp),
        reply: None,
    }
}

/// Host tests: heap 4 MiB + JMP FAR stub. Not a shipped `OVMF.fd`.
/// Production UEFI: 409 `MissingEsp` — no embedded 4 MiB (ADR-003).
#[cfg(test)]
fn alias_ept_rest() -> RestResponse {
    let mut fv = vec![0u8; MIN_FIRMWARE_ALIAS_BYTES];
    if write_firmware_alias_fv(&mut fv).is_err() {
        return RestResponse {
            status: 500,
            reply: None,
        };
    }
    match program_ovmf_alias_ept(&fv) {
        Ok(_) => RestResponse {
            status: 201,
            reply: Some(ApiReply::Ok),
        },
        Err(e) => RestResponse {
            status: guest_fw_err_status(e),
            reply: None,
        },
    }
}

/// Production: no embedded 4 MiB alias-EPT image (ADR-003 split-mode / ESP only).
#[cfg(not(test))]
fn alias_ept_rest() -> RestResponse {
    RestResponse {
        status: guest_fw_err_status(GuestFwError::MissingEsp),
        reply: None,
    }
}

/// Host tests: heap 4 MiB + JMP FAR stub. Not a shipped `OVMF.fd`.
/// Production UEFI: 409 `MissingEsp` — no embedded 4 MiB (ADR-003).
#[cfg(test)]
fn ept_install_rest() -> RestResponse {
    let mut fv = vec![0u8; MIN_FIRMWARE_ALIAS_BYTES];
    if write_firmware_alias_fv(&mut fv).is_err() {
        return RestResponse {
            status: 500,
            reply: None,
        };
    }
    match install_ovmf_alias_ept(&fv) {
        Ok(_) => RestResponse {
            status: 201,
            reply: Some(ApiReply::Ok),
        },
        Err(e) => RestResponse {
            status: guest_fw_err_status(e),
            reply: None,
        },
    }
}

/// Production: no embedded 4 MiB private EPT image (ADR-003 split-mode / ESP only).
#[cfg(not(test))]
fn ept_install_rest() -> RestResponse {
    RestResponse {
        status: guest_fw_err_status(GuestFwError::MissingEsp),
        reply: None,
    }
}

/// Host tests: heap 4 MiB + JMP FAR stub. Not a shipped `OVMF.fd`.
/// Production UEFI: 409 `MissingEsp` — no embedded 4 MiB (ADR-003).
#[cfg(test)]
fn real_esp_rest() -> RestResponse {
    let mut fv = vec![0u8; MIN_FIRMWARE_ALIAS_BYTES];
    if write_firmware_alias_fv(&mut fv).is_err() {
        return RestResponse {
            status: 500,
            reply: None,
        };
    }
    match qualify_real_esp_ovmf(&fv) {
        Ok(_) => RestResponse {
            status: 201,
            reply: Some(ApiReply::Ok),
        },
        Err(e) => RestResponse {
            status: guest_fw_err_status(e),
            reply: None,
        },
    }
}

/// Production: no embedded 4 MiB real-ESP image (ADR-003 split-mode / ESP only).
#[cfg(not(test))]
fn real_esp_rest() -> RestResponse {
    RestResponse {
        status: guest_fw_err_status(GuestFwError::MissingEsp),
        reply: None,
    }
}

/// Host tests: heap 4 MiB + JMP FAR stub. Not a shipped `OVMF.fd`.
/// Production UEFI: 409 `MissingEsp` — no embedded 4 MiB (ADR-003).
#[cfg(test)]
fn real_launch_rest() -> RestResponse {
    let mut fv = vec![0u8; MIN_FIRMWARE_ALIAS_BYTES];
    if write_firmware_alias_fv(&mut fv).is_err() {
        return RestResponse {
            status: 500,
            reply: None,
        };
    }
    match arm_ovmf_real_launch(&fv) {
        Ok(_) => RestResponse {
            status: 201,
            reply: Some(ApiReply::Ok),
        },
        Err(e) => RestResponse {
            status: guest_fw_err_status(e),
            reply: None,
        },
    }
}

/// Production: no embedded 4 MiB real-launch image (ADR-003 split-mode / ESP only).
#[cfg(not(test))]
fn real_launch_rest() -> RestResponse {
    RestResponse {
        status: guest_fw_err_status(GuestFwError::MissingEsp),
        reply: None,
    }
}

/// Host tests: heap 4 MiB + JMP FAR stub. Not a shipped `OVMF.fd`.
/// Production UEFI: 409 `MissingEsp` — no embedded 4 MiB (ADR-003).
#[cfg(test)]
fn live_exec_rest() -> RestResponse {
    let mut fv = vec![0u8; MIN_FIRMWARE_ALIAS_BYTES];
    if write_firmware_alias_fv(&mut fv).is_err() {
        return RestResponse {
            status: 500,
            reply: None,
        };
    }
    match require_ovmf_live_esp(&fv) {
        Ok(_) => RestResponse {
            status: 201,
            reply: Some(ApiReply::Ok),
        },
        Err(e) => RestResponse {
            status: guest_fw_err_status(e),
            reply: None,
        },
    }
}

/// Production: no embedded 4 MiB live-exec image (ADR-003 split-mode / ESP only).
#[cfg(not(test))]
fn live_exec_rest() -> RestResponse {
    RestResponse {
        status: guest_fw_err_status(GuestFwError::MissingEsp),
        reply: None,
    }
}

/// Host tests: heap 4 MiB + JMP FAR stub. Not a shipped `OVMF.fd`.
/// Production UEFI: 409 `MissingEsp` — no embedded 4 MiB (ADR-003).
#[cfg(test)]
fn priv_vmcs_rest() -> RestResponse {
    let mut fv = vec![0u8; MIN_FIRMWARE_ALIAS_BYTES];
    if write_firmware_alias_fv(&mut fv).is_err() {
        return RestResponse {
            status: 500,
            reply: None,
        };
    }
    match arm_ovmf_private_vmcs(&fv) {
        Ok(_) => RestResponse {
            status: 201,
            reply: Some(ApiReply::Ok),
        },
        Err(e) => RestResponse {
            status: guest_fw_err_status(e),
            reply: None,
        },
    }
}

/// Production: no embedded 4 MiB private-VMCS image (ADR-003 split-mode / ESP only).
#[cfg(not(test))]
fn priv_vmcs_rest() -> RestResponse {
    RestResponse {
        status: guest_fw_err_status(GuestFwError::MissingEsp),
        reply: None,
    }
}

/// Host tests: heap 4 MiB + JMP FAR stub. Not a shipped `OVMF.fd`.
/// Production UEFI: 409 `MissingEsp` — no embedded 4 MiB (ADR-003).
#[cfg(test)]
fn live_issue_rest() -> RestResponse {
    let mut fv = vec![0u8; MIN_FIRMWARE_ALIAS_BYTES];
    if write_firmware_alias_fv(&mut fv).is_err() {
        return RestResponse {
            status: 500,
            reply: None,
        };
    }
    match arm_ovmf_live_issue(&fv) {
        Ok(_) => RestResponse {
            status: 201,
            reply: Some(ApiReply::Ok),
        },
        Err(e) => RestResponse {
            status: guest_fw_err_status(e),
            reply: None,
        },
    }
}

/// Production: no embedded 4 MiB live-issue image (ADR-003 split-mode / ESP only).
#[cfg(not(test))]
fn live_issue_rest() -> RestResponse {
    RestResponse {
        status: guest_fw_err_status(GuestFwError::MissingEsp),
        reply: None,
    }
}

/// Host tests: heap 4 MiB + JMP FAR stub. Not a shipped `OVMF.fd`.
/// Production UEFI: 409 `MissingEsp` — no embedded 4 MiB (ADR-003).
#[cfg(test)]
fn live_bytes_rest() -> RestResponse {
    let mut fv = vec![0u8; MIN_FIRMWARE_ALIAS_BYTES];
    if write_firmware_alias_fv(&mut fv).is_err() {
        return RestResponse {
            status: 500,
            reply: None,
        };
    }
    match probe_ovmf_live_bytes(&fv) {
        Ok(_) => RestResponse {
            status: 201,
            reply: Some(ApiReply::Ok),
        },
        Err(e) => RestResponse {
            status: guest_fw_err_status(e),
            reply: None,
        },
    }
}

/// Production: no embedded 4 MiB live-bytes image (ADR-003 split-mode / ESP only).
#[cfg(not(test))]
fn live_bytes_rest() -> RestResponse {
    RestResponse {
        status: guest_fw_err_status(GuestFwError::MissingEsp),
        reply: None,
    }
}

/// Host tests: heap 4 MiB + JMP FAR stub. Not a shipped `OVMF.fd`.
/// Production UEFI: 409 `MissingEsp` — no embedded 4 MiB (ADR-003).
#[cfg(test)]
fn live_fd_rest() -> RestResponse {
    let mut fv = vec![0u8; MIN_FIRMWARE_ALIAS_BYTES];
    if write_firmware_alias_fv(&mut fv).is_err() {
        return RestResponse {
            status: 500,
            reply: None,
        };
    }
    match require_ovmf_live_fd(&fv) {
        Ok(_) => RestResponse {
            status: 201,
            reply: Some(ApiReply::Ok),
        },
        Err(e) => RestResponse {
            status: guest_fw_err_status(e),
            reply: None,
        },
    }
}

/// Production: no embedded 4 MiB live-FD image (ADR-003 split-mode / ESP only).
#[cfg(not(test))]
fn live_fd_rest() -> RestResponse {
    RestResponse {
        status: guest_fw_err_status(GuestFwError::MissingEsp),
        reply: None,
    }
}

/// Host tests: heap 4 MiB + JMP FAR stub. Not a shipped `OVMF.fd`.
/// Production UEFI: 409 `MissingEsp` — no embedded 4 MiB (ADR-003).
#[cfg(test)]
fn live_present_rest() -> RestResponse {
    let mut fv = vec![0u8; MIN_FIRMWARE_ALIAS_BYTES];
    if write_firmware_alias_fv(&mut fv).is_err() {
        return RestResponse {
            status: 500,
            reply: None,
        };
    }
    match present_ovmf_live_esp(&fv) {
        Ok(_) => RestResponse {
            status: 201,
            reply: Some(ApiReply::Ok),
        },
        Err(e) => RestResponse {
            status: guest_fw_err_status(e),
            reply: None,
        },
    }
}

/// Production: no embedded 4 MiB live-present image (ADR-003 split-mode / ESP only).
#[cfg(not(test))]
fn live_present_rest() -> RestResponse {
    RestResponse {
        status: guest_fw_err_status(GuestFwError::MissingEsp),
        reply: None,
    }
}

/// Host tests: heap 4 MiB + JMP FAR stub. Not a shipped `OVMF.fd`.
/// Production UEFI: 409 `MissingEsp` — no embedded 4 MiB (ADR-003).
#[cfg(test)]
fn live_admit_rest() -> RestResponse {
    let mut fv = vec![0u8; MIN_FIRMWARE_ALIAS_BYTES];
    if write_firmware_alias_fv(&mut fv).is_err() {
        return RestResponse {
            status: 500,
            reply: None,
        };
    }
    match admit_ovmf_live_esp(&fv) {
        Ok(_) => RestResponse {
            status: 201,
            reply: Some(ApiReply::Ok),
        },
        Err(e) => RestResponse {
            status: guest_fw_err_status(e),
            reply: None,
        },
    }
}

/// Production: no embedded 4 MiB live-admit image (ADR-003 split-mode / ESP only).
#[cfg(not(test))]
fn live_admit_rest() -> RestResponse {
    RestResponse {
        status: guest_fw_err_status(GuestFwError::MissingEsp),
        reply: None,
    }
}

/// Host tests: heap 4 MiB + JMP FAR stub. Not a shipped `OVMF.fd`.
/// Production UEFI: 409 `MissingEsp` — no embedded 4 MiB (ADR-003).
#[cfg(test)]
fn live_read_rest() -> RestResponse {
    let mut fv = vec![0u8; MIN_FIRMWARE_ALIAS_BYTES];
    if write_firmware_alias_fv(&mut fv).is_err() {
        return RestResponse {
            status: 500,
            reply: None,
        };
    }
    match read_ovmf_live_esp(&fv) {
        Ok(_) => RestResponse {
            status: 201,
            reply: Some(ApiReply::Ok),
        },
        Err(e) => RestResponse {
            status: guest_fw_err_status(e),
            reply: None,
        },
    }
}

/// Production: no embedded 4 MiB live-read image (ADR-003 split-mode / ESP only).
#[cfg(not(test))]
fn live_read_rest() -> RestResponse {
    RestResponse {
        status: guest_fw_err_status(GuestFwError::MissingEsp),
        reply: None,
    }
}

/// Host tests: heap 4 MiB + JMP FAR stub. Not a shipped `OVMF.fd`.
/// Production UEFI: 409 `MissingEsp` — no embedded 4 MiB (ADR-003).
#[cfg(test)]
fn live_copy_rest() -> RestResponse {
    let mut fv = vec![0u8; MIN_FIRMWARE_ALIAS_BYTES];
    if write_firmware_alias_fv(&mut fv).is_err() {
        return RestResponse {
            status: 500,
            reply: None,
        };
    }
    match copy_ovmf_live_esp(&fv) {
        Ok(_) => RestResponse {
            status: 201,
            reply: Some(ApiReply::Ok),
        },
        Err(e) => RestResponse {
            status: guest_fw_err_status(e),
            reply: None,
        },
    }
}

/// Production: no embedded 4 MiB live-copy image (ADR-003 split-mode / ESP only).
#[cfg(not(test))]
fn live_copy_rest() -> RestResponse {
    RestResponse {
        status: guest_fw_err_status(GuestFwError::MissingEsp),
        reply: None,
    }
}

/// Host tests: heap 4 MiB + JMP FAR stub. Not a shipped `OVMF.fd`.
/// Production UEFI: 409 `MissingEsp` — no embedded 4 MiB (ADR-003).
#[cfg(test)]
fn live_place_rest() -> RestResponse {
    let mut fv = vec![0u8; MIN_FIRMWARE_ALIAS_BYTES];
    if write_firmware_alias_fv(&mut fv).is_err() {
        return RestResponse {
            status: 500,
            reply: None,
        };
    }
    match place_ovmf_live_esp(&fv) {
        Ok(_) => RestResponse {
            status: 201,
            reply: Some(ApiReply::Ok),
        },
        Err(e) => RestResponse {
            status: guest_fw_err_status(e),
            reply: None,
        },
    }
}

/// Production: no embedded 4 MiB live-place image (ADR-003 split-mode / ESP only).
#[cfg(not(test))]
fn live_place_rest() -> RestResponse {
    RestResponse {
        status: guest_fw_err_status(GuestFwError::MissingEsp),
        reply: None,
    }
}

/// Host tests: heap 4 MiB + JMP FAR stub. Not a shipped `OVMF.fd`.
/// Production UEFI: 409 `MissingEsp` — no embedded 4 MiB (ADR-003).
#[cfg(test)]
fn live_apply_rest() -> RestResponse {
    let mut fv = vec![0u8; MIN_FIRMWARE_ALIAS_BYTES];
    if write_firmware_alias_fv(&mut fv).is_err() {
        return RestResponse {
            status: 500,
            reply: None,
        };
    }
    match apply_ovmf_live_esp(&fv) {
        Ok(_) => RestResponse {
            status: 201,
            reply: Some(ApiReply::Ok),
        },
        Err(e) => RestResponse {
            status: guest_fw_err_status(e),
            reply: None,
        },
    }
}

/// Production: no embedded 4 MiB live-apply image (ADR-003 split-mode / ESP only).
#[cfg(not(test))]
fn live_apply_rest() -> RestResponse {
    RestResponse {
        status: guest_fw_err_status(GuestFwError::MissingEsp),
        reply: None,
    }
}

/// Host tests: heap 4 MiB + JMP FAR stub. Not a shipped `OVMF.fd`.
/// Production UEFI: 409 `MissingEsp` — no embedded 4 MiB (ADR-003).
#[cfg(test)]
fn live_commit_rest() -> RestResponse {
    let mut fv = vec![0u8; MIN_FIRMWARE_ALIAS_BYTES];
    if write_firmware_alias_fv(&mut fv).is_err() {
        return RestResponse {
            status: 500,
            reply: None,
        };
    }
    match commit_ovmf_live_esp(&fv) {
        Ok(_) => RestResponse {
            status: 201,
            reply: Some(ApiReply::Ok),
        },
        Err(e) => RestResponse {
            status: guest_fw_err_status(e),
            reply: None,
        },
    }
}

/// Production: no embedded 4 MiB live-commit image (ADR-003 split-mode / ESP only).
#[cfg(not(test))]
fn live_commit_rest() -> RestResponse {
    RestResponse {
        status: guest_fw_err_status(GuestFwError::MissingEsp),
        reply: None,
    }
}

/// Host tests: heap 4 MiB + JMP FAR stub. Not a shipped `OVMF.fd`.
/// Production UEFI: 409 `MissingEsp` — no embedded 4 MiB (ADR-003).
#[cfg(test)]
fn live_latch_rest() -> RestResponse {
    let mut fv = vec![0u8; MIN_FIRMWARE_ALIAS_BYTES];
    if write_firmware_alias_fv(&mut fv).is_err() {
        return RestResponse {
            status: 500,
            reply: None,
        };
    }
    match latch_ovmf_live_esp(&fv) {
        Ok(_) => RestResponse {
            status: 201,
            reply: Some(ApiReply::Ok),
        },
        Err(e) => RestResponse {
            status: guest_fw_err_status(e),
            reply: None,
        },
    }
}

/// Production: no embedded 4 MiB live-latch image (ADR-003 split-mode / ESP only).
#[cfg(not(test))]
fn live_latch_rest() -> RestResponse {
    RestResponse {
        status: guest_fw_err_status(GuestFwError::MissingEsp),
        reply: None,
    }
}

/// Write a test envelope header into `buf` (at least [`GUEST_FW_HEADER_LEN`]).
pub fn write_guest_fw_header(
    buf: &mut [u8],
    uncompressed: u32,
    compressed: u32,
    payload_len: u32,
) -> Result<(), GuestFwError> {
    if buf.len() < GUEST_FW_HEADER_LEN {
        return Err(GuestFwError::BadState);
    }
    buf[..8].copy_from_slice(&GUEST_FW_MAGIC);
    buf[8..12].copy_from_slice(&1u32.to_le_bytes());
    buf[12..16].copy_from_slice(&(GuestFwKind::UefiEnvelope as u32).to_le_bytes());
    buf[16..20].copy_from_slice(&uncompressed.to_le_bytes());
    buf[20..24].copy_from_slice(&compressed.to_le_bytes());
    buf[24..28].copy_from_slice(&GUEST_FW_FLAG_LAZY_ZSTD.to_le_bytes());
    buf[28..32].copy_from_slice(&payload_len.to_le_bytes());
    Ok(())
}

#[cfg(test)]
#[path = "guest_fw_test.rs"]
mod guest_fw_test;
