//! M8.4 host ISO upload gate (outside Proven Core).
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** (ADR-018)
//!
//! Proves HostReady ISO bytes land in a datastore blob, firmware stays
//! ESP-staged, and the iron marker is never printed from host/CI.
//! Nested QEMU is not this gate. Iron network ISO PUT after `BOOT-OK`
//! is not this gate.

use crate::mgmt::iso_upload::{
    firmware_upload_is_esp_staged, host_never_prints_iron_iso_upload_ok,
    prop_iso_upload_host_package, ISO_UPLOAD_HOST_RESIDUAL_NOTE, M8_ISO_UPLOAD_HOST_OK_MARKER,
    M8_ISO_UPLOAD_OK_MARKER, UPLOAD_FIRMWARE_ESP_NOTE,
};

/// Host / CI marker when the M8.4 upload package passes.
pub const M8_ISO_UPLOAD_GATE_MARKER: &str = M8_ISO_UPLOAD_HOST_OK_MARKER;

/// True when plan, markers, ESP-staged firmware, and HostReady blob hold.
pub fn iso_upload_surface_present() -> bool {
    let upload = include_str!("iso_upload.rs");
    let plan = include_str!("../docs/m8_plan.md");
    let smoke = include_str!("../tools/m8-iso-upload-smoke.sh");
    let runbook = include_str!("../docs/runbooks/iso.md");
    let forbidden = concat!("println!(", "\"RAYNU-V-M8-ISO-UPLOAD-OK\")");
    upload.contains("enum UploadMode")
        && upload.contains("EspStaged")
        && upload.contains("HostReady")
        && upload.contains("fn firmware_upload_is_esp_staged(")
        && upload.contains("fn host_never_prints_iron_iso_upload_ok(")
        && upload.contains("fn prop_iso_upload_host_package(")
        && upload.contains("fn put_iso_blob(")
        && upload.contains(M8_ISO_UPLOAD_OK_MARKER)
        && upload.contains(M8_ISO_UPLOAD_HOST_OK_MARKER)
        && !upload.contains(forbidden)
        && !include_str!("http.rs").contains(forbidden)
        && !include_str!("http.rs").contains("/blob")
        && !include_str!("../assets/webui.html").contains("type=\"file\"")
        && !include_str!("../assets/webui.html").contains("/blob")
        && ISO_UPLOAD_HOST_RESIDUAL_NOTE.contains("not iron")
        && ISO_UPLOAD_HOST_RESIDUAL_NOTE.contains("ESP-staged stays valid")
        && UPLOAD_FIRMWARE_ESP_NOTE.contains("ESP-staged stays valid")
        && plan.contains("M8.4")
        && plan.contains("ESP-staged stays valid")
        && smoke.contains(M8_ISO_UPLOAD_HOST_OK_MARKER)
        && smoke.contains("m8_iso_upload_host_gate_passes")
        && runbook.contains("M8.4")
        && runbook.contains("ESP-staged stays valid")
}

/// Full M8.4 host artifact gate (not iron network ISO PUT).
pub fn run_m8_iso_upload_host_gate() -> bool {
    iso_upload_surface_present()
        && prop_iso_upload_host_package()
        && firmware_upload_is_esp_staged()
        && host_never_prints_iron_iso_upload_ok()
        && M8_ISO_UPLOAD_OK_MARKER == "RAYNU-V-M8-ISO-UPLOAD-OK"
        && M8_ISO_UPLOAD_GATE_MARKER == "RAYNU-V-M8-ISO-UPLOAD-HOST-OK"
}

#[cfg(test)]
#[path = "m8_iso_upload_gate_test.rs"]
mod m8_iso_upload_gate_test;
