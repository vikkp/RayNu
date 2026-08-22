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

const _: () = assert!(SIZE_FLOOR_FV_BYTES > MOCK_OVMF_FV_BYTES);
const _: () = assert!(SIZE_FLOOR_FV_BYTES == MIN_LAUNCH_FV_BYTES);
const _: () = assert!(SIZE_FLOOR_FV_BYTES < MIN_EDK2_OVMF_BYTES);

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

/// Reset the process-local boxed / loaded / probed / ESP / slot / bind / prep / floor / EDK2 flags.
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
    let sig = bytes.get(OVMF_FV_SIG_OFF..sig_end).ok_or(GuestFwError::BadMagic)?;
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

/// Refuse guest UEFI VMLAUNCH of the in-tree mock / size-floor / EDK2-sized fixtures.
///
/// INVARIANTS:
/// - Requires a prior successful [`prepare_ovmf_firmware_launch`]
/// - Mock (no floor) → [`GuestFwError::MockFirmwareRefused`]
/// - Size-floor staged (no EDK2) → [`GuestFwError::NotRealFirmware`]
/// - EDK2-sized staged → [`GuestFwError::LaunchNotWired`] (not a shipped image)
/// - Does not write VMCS and does not VMLAUNCH
pub fn try_vmlaunch_ovmf_firmware() -> Result<(), GuestFwError> {
    if !ovmf_launch_is_prepared() {
        return Err(GuestFwError::NotGuestBound);
    }
    if ovmf_edk2_is_staged() {
        return Err(GuestFwError::LaunchNotWired);
    }
    if ovmf_floor_is_staged() {
        return Err(GuestFwError::NotRealFirmware);
    }
    Err(GuestFwError::MockFirmwareRefused)
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
        | GuestFwError::LaunchNotWired => 409,
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
/// embedded 1 MiB image (ADR-003). `POST /fw/vmlaunch` refuses (409).
/// GET paths return counts. Not a shipped EDK2 `OVMF.fd`. Not VMLAUNCH.
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
