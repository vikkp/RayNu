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

/// Reset the process-local boxed / loaded flags (host tests).
pub fn reset_guest_fw() {
    GUEST_FW_BOXED.store(false, Ordering::Release);
    GUEST_FW_LOADED.store(false, Ordering::Release);
}

/// True after a successful [`box_guest_firmware`].
pub fn guest_fw_is_boxed() -> bool {
    GUEST_FW_BOXED.load(Ordering::Acquire)
}

/// True after a successful [`load_guest_firmware`].
pub fn guest_fw_is_loaded() -> bool {
    GUEST_FW_LOADED.load(Ordering::Acquire)
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

/// True when `path` is the guest-firmware envelope REST surface.
pub fn is_guest_fw_path(path: &str) -> bool {
    let path = path.trim().trim_end_matches('/');
    path == "/fw" || path == "/fw/box" || path == "/fw/load"
}

enum GuestFwOp {
    Status,
    Box,
    LoadStatus,
    Load,
}

fn route_guest_fw(method: RestMethod, path: &str) -> Result<GuestFwOp, ()> {
    let path = path.trim().trim_end_matches('/');
    match (method, path) {
        (RestMethod::Get, "/fw") => Ok(GuestFwOp::Status),
        (RestMethod::Post, "/fw/box") => Ok(GuestFwOp::Box),
        (RestMethod::Get, "/fw/load") => Ok(GuestFwOp::LoadStatus),
        (RestMethod::Post, "/fw/load") => Ok(GuestFwOp::Load),
        _ => Err(()),
    }
}

fn guest_fw_err_status(e: GuestFwError) -> u16 {
    match e {
        GuestFwError::BadMagic
        | GuestFwError::BadState
        | GuestFwError::TooLarge
        | GuestFwError::NotBoxed => 409,
    }
}

/// REST: `POST /fw/box` boxes the envelope. `POST /fw/load` lazy-loads the
/// stub payload after box. `GET /fw` / `GET /fw/load` return counts.
/// Not OVMF. Not VMLAUNCH.
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
        Err(()) => RestResponse {
            status: 400,
            reply: None,
        },
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
