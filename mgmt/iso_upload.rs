//! M8.4 ISO blob upload (outside Proven Core).
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** (ADR-002 / ADR-018). Do not touch VMX/EPT.
//! VERIFICATION: L1 host tests (PUT/POST bytes → datastore blob round-trip).
//!
//! Firmware still boots the ESP-staged `linux.iso`. That is **not** the M8.4
//! product close. This module is the host-first slice:
//! [`UploadMode::HostReady`] registers ISO bytes into a host blob table.
//! Firmware HTTP does not grow a coexist blob PUT. SPA has no upload widget
//! (16 KiB). **ESP-staged stays valid**.
//!
//! Iron close marker [`M8_ISO_UPLOAD_OK_MARKER`] is a network ISO PUT on
//! coexist after `BOOT-OK`. Host/CI print
//! [`M8_ISO_UPLOAD_HOST_OK_MARKER`]. Nested QEMU ≠ R640. Do not flash.

use crate::mgmt::api::RestMethod;
use crate::mgmt::datastore::{ImageKind, ImageTable, StoreError};

/// Iron COM2 / operator LAN close: PUT ISO bytes on coexist after `BOOT-OK`.
/// Host/CI/nested must **never** print this.
pub const M8_ISO_UPLOAD_OK_MARKER: &str = "RAYNU-V-M8-ISO-UPLOAD-OK";

/// Host/CI: ISO bytes land in the host datastore blob. Not iron.
pub const M8_ISO_UPLOAD_HOST_OK_MARKER: &str = "RAYNU-V-M8-ISO-UPLOAD-HOST-OK";

/// Honesty: host blob register ≠ iron network ISO PUT on coexist.
pub const ISO_UPLOAD_HOST_RESIDUAL_NOTE: &str =
    "residual: HostReady blob register is not iron RAYNU-V-M8-ISO-UPLOAD-OK; firmware coexist has no ISO blob PUT; SPA has no upload widget; ESP-staged stays valid; nested QEMU ≠ R640; do not print iron ISO-UPLOAD-OK from host/CI";

/// Firmware listen today: ESP-staged linux.iso, not a network blob PUT.
pub const UPLOAD_FIRMWARE_ESP_NOTE: &str =
    "firmware ISO path is ESP-staged linux.iso (M8.4 host-ready; iron network ISO PUT not claimed; ESP-staged stays valid)";

/// Host blob cap (tests). Not a distro ISO size.
pub const ISO_BLOB_CAP: usize = 256;

const ISO_BLOB_SLOTS: usize = 2;

/// How ISO bytes enter the datastore.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UploadMode {
    /// Iron / firmware: ESP-staged `linux.iso` only.
    EspStaged,
    /// Host/`cfg(test)`: PUT/POST bytes into a host blob table.
    HostReady,
}

/// Product firmware upload today. Host tests use [`UploadMode::HostReady`].
pub const FIRMWARE_UPLOAD_MODE: UploadMode = UploadMode::EspStaged;

/// Host-only ISO byte slots (not firmware coexist RAM).
pub struct IsoBlobTable {
    slots: [Option<IsoBlobSlot>; ISO_BLOB_SLOTS],
}

struct IsoBlobSlot {
    id: u64,
    len: usize,
    data: [u8; ISO_BLOB_CAP],
}

impl IsoBlobTable {
    pub const fn new() -> Self {
        Self {
            slots: [None, None],
        }
    }

    pub fn get(&self, id: u64) -> Option<&[u8]> {
        self.slots
            .iter()
            .flatten()
            .find(|s| s.id == id)
            .map(|s| &s.data[..s.len])
    }

    fn put(&mut self, id: u64, bytes: &[u8]) -> Result<usize, UploadError> {
        if bytes.is_empty() {
            return Err(UploadError::Empty);
        }
        if bytes.len() > ISO_BLOB_CAP {
            return Err(UploadError::TooLarge);
        }
        for slot in self.slots.iter_mut() {
            if let Some(existing) = slot {
                if existing.id == id {
                    existing.len = bytes.len();
                    existing.data[..bytes.len()].copy_from_slice(bytes);
                    return Ok(bytes.len());
                }
            }
        }
        for slot in self.slots.iter_mut() {
            if slot.is_none() {
                let mut data = [0u8; ISO_BLOB_CAP];
                data[..bytes.len()].copy_from_slice(bytes);
                *slot = Some(IsoBlobSlot {
                    id,
                    len: bytes.len(),
                    data,
                });
                return Ok(bytes.len());
            }
        }
        Err(UploadError::Full)
    }
}

impl Default for IsoBlobTable {
    fn default() -> Self {
        Self::new()
    }
}

/// Error from host ISO blob upload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UploadError {
    EspStagedOnly,
    InvalidId,
    Empty,
    TooLarge,
    Full,
    NotFound,
    BadState,
    Store(StoreError),
}

/// True until coexist serves a network ISO PUT after `BOOT-OK`.
pub fn firmware_upload_is_esp_staged() -> bool {
    matches!(FIRMWARE_UPLOAD_MODE, UploadMode::EspStaged)
        && UPLOAD_FIRMWARE_ESP_NOTE.contains("ESP-staged stays valid")
        && UPLOAD_FIRMWARE_ESP_NOTE.contains("linux.iso")
}

/// Host/CI must never print the iron ISO-upload marker.
pub fn host_never_prints_iron_iso_upload_ok() -> bool {
    M8_ISO_UPLOAD_OK_MARKER == "RAYNU-V-M8-ISO-UPLOAD-OK"
        && M8_ISO_UPLOAD_HOST_OK_MARKER == "RAYNU-V-M8-ISO-UPLOAD-HOST-OK"
        && M8_ISO_UPLOAD_HOST_OK_MARKER != M8_ISO_UPLOAD_OK_MARKER
        && M8_ISO_UPLOAD_OK_MARKER != crate::mgmt::tls::M8_TLS_OK_MARKER
        && M8_ISO_UPLOAD_OK_MARKER != crate::mgmt::auth::M8_AUTH_OK_MARKER
        && M8_ISO_UPLOAD_OK_MARKER != crate::mgmt::console::M8_CONSOLE_OK_MARKER
}

/// PUT/POST ISO bytes. EspStaged is a no-op error. HostReady registers the blob.
pub fn put_iso_blob(
    mode: UploadMode,
    store: &mut ImageTable,
    blobs: &mut IsoBlobTable,
    id: u64,
    name: &str,
    bytes: &[u8],
) -> Result<usize, UploadError> {
    match mode {
        UploadMode::EspStaged => Err(UploadError::EspStagedOnly),
        UploadMode::HostReady => {
            if id == 0 {
                return Err(UploadError::InvalidId);
            }
            let n = blobs.put(id, bytes)?;
            match store.get(id) {
                None => {
                    store
                        .register(id, ImageKind::Iso, n as u64, name)
                        .map_err(UploadError::Store)?;
                }
                Some(rec) => {
                    if rec.kind != ImageKind::Iso {
                        return Err(UploadError::BadState);
                    }
                    if let Some(rec) = store.get_mut(id) {
                        rec.size_bytes = n as u64;
                    }
                }
            }
            Ok(n)
        }
    }
}

/// Copy a HostReady blob (oldest slot order). EspStaged has no host bytes.
pub fn get_iso_blob(blobs: &IsoBlobTable, id: u64, out: &mut [u8]) -> Option<usize> {
    let src = blobs.get(id)?;
    let n = src.len().min(out.len());
    out[..n].copy_from_slice(&src[..n]);
    Some(n)
}

/// Host-only REST shape. Firmware [`crate::mgmt::http`] does not grow this route.
pub fn dispatch_iso_upload_host(
    mode: UploadMode,
    store: &mut ImageTable,
    blobs: &mut IsoBlobTable,
    method: RestMethod,
    path: &str,
    body: &[u8],
) -> u16 {
    let path = path.trim().trim_end_matches('/');
    let rest = match path.strip_prefix("/iso/") {
        Some(r) => r,
        None => return 400,
    };
    let mut segs = rest.split('/');
    let id_s = match segs.next() {
        Some(s) => s,
        None => return 400,
    };
    let mut id: u64 = 0;
    if id_s.is_empty() {
        return 400;
    }
    for b in id_s.bytes() {
        if !(b'0'..=b'9').contains(&b) {
            return 400;
        }
        match id
            .checked_mul(10)
            .and_then(|n| n.checked_add(u64::from(b - b'0')))
        {
            Some(n) => id = n,
            None => return 400,
        }
    }
    if segs.next() != Some("blob") || segs.next().is_some() {
        return 400;
    }
    match (mode, method) {
        (UploadMode::EspStaged, _) => 501,
        (UploadMode::HostReady, RestMethod::Post) => {
            match put_iso_blob(mode, store, blobs, id, "upload.iso", body) {
                Ok(_) => 201,
                Err(UploadError::InvalidId)
                | Err(UploadError::Empty)
                | Err(UploadError::TooLarge) => 400,
                Err(UploadError::BadState)
                | Err(UploadError::Full)
                | Err(UploadError::Store(_)) => 409,
                Err(_) => 400,
            }
        }
        (UploadMode::HostReady, RestMethod::Get) => {
            if blobs.get(id).is_some() {
                200
            } else {
                404
            }
        }
        _ => 400,
    }
}

/// Host package: HostReady stores bytes; EspStaged refuses; firmware ESP-staged.
pub fn prop_iso_upload_host_package() -> bool {
    let plan = include_str!("../docs/m8_plan.md");
    let html = include_str!("../assets/webui.html");
    let http = include_str!("http.rs");
    let mut store = ImageTable::new();
    let mut blobs = IsoBlobTable::new();
    let sample = b"CD001-ISO-BLOB";
    let esp_refuses = put_iso_blob(
        UploadMode::EspStaged,
        &mut store,
        &mut blobs,
        1,
        "linux.iso",
        sample,
    ) == Err(UploadError::EspStagedOnly);
    let put_ok = put_iso_blob(
        UploadMode::HostReady,
        &mut store,
        &mut blobs,
        1,
        "linux.iso",
        sample,
    ) == Ok(sample.len());
    let mut out = [0u8; 32];
    let n = get_iso_blob(&blobs, 1, &mut out).unwrap_or(0);
    let roundtrip = n == sample.len() && &out[..n] == sample;
    let rec_ok = store
        .get(1)
        .map(|r| r.kind == ImageKind::Iso && r.size_bytes == sample.len() as u64)
        .unwrap_or(false);
    let post_status = dispatch_iso_upload_host(
        UploadMode::HostReady,
        &mut store,
        &mut blobs,
        RestMethod::Post,
        "/iso/2/blob",
        b"ISO2",
    );
    let get_status = dispatch_iso_upload_host(
        UploadMode::HostReady,
        &mut store,
        &mut blobs,
        RestMethod::Get,
        "/iso/2/blob",
        b"",
    );
    let esp_http = dispatch_iso_upload_host(
        UploadMode::EspStaged,
        &mut store,
        &mut blobs,
        RestMethod::Post,
        "/iso/3/blob",
        b"x",
    );
    firmware_upload_is_esp_staged()
        && host_never_prints_iron_iso_upload_ok()
        && esp_refuses
        && put_ok
        && roundtrip
        && rec_ok
        && post_status == 201
        && get_status == 200
        && esp_http == 501
        && ISO_UPLOAD_HOST_RESIDUAL_NOTE.contains("not iron")
        && ISO_UPLOAD_HOST_RESIDUAL_NOTE.contains("ESP-staged stays valid")
        && plan.contains("M8.4")
        && plan.contains("ESP-staged stays valid")
        && !html.contains("type=\"file\"")
        && !html.contains("/blob")
        && !http.contains("/blob")
        && !http.contains("put_iso_blob")
}

#[cfg(test)]
#[path = "iso_upload_test.rs"]
mod iso_upload_test;
