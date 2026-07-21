//! M7.2 host verification gate (datastore / image library).
//!
//! Pillar: [Z] [A]
//! Proven Core: outside (companion to `mgmt/datastore` — ADR-009).
//!
//! Checks image table, REST shapes, ESP-shaped host catalog, UEFI persist
//! stub honesty, runbook, and smoke/CI wiring.

use super::datastore::{
    persist_catalog_uefi, prop_datastore_package, ImageTable, StoreError, ESP_IMAGES_REL,
    M7_STORE_OK_MARKER, STORE_GAP_NOTE,
};

/// Host / CI marker when the M7.2 store gate passes.
pub const M7_STORE_GATE_MARKER: &str = M7_STORE_OK_MARKER;

/// True when datastore module exposes package props, closed GAP, ESP path, marker.
pub fn store_surface_present() -> bool {
    let s = include_str!("datastore.rs");
    s.contains("fn prop_datastore_package(")
        && s.contains("fn dispatch_store_rest(")
        && s.contains("fn persist_catalog_uefi(")
        && s.contains("persist_catalog_host")
        && s.contains("load_catalog_host")
        && s.contains("ImageTable")
        && s.contains("ImageKind")
        && s.contains(M7_STORE_OK_MARKER)
        && s.contains(STORE_GAP_NOTE)
        && s.contains(ESP_IMAGES_REL)
        && STORE_GAP_NOTE.contains("CLOSED M7.2")
}

/// True when UEFI persist stub is honest.
pub fn store_uefi_stub_honest() -> bool {
    let t = ImageTable::new();
    persist_catalog_uefi(&t) == Err(StoreError::UnsupportedOnFirmware)
}

/// True when runbook + smoke script exist with required phrases.
pub fn store_scripts_present() -> bool {
    let smoke = include_str!("../tools/m7-store-smoke.sh");
    let runbook = include_str!("../docs/runbooks/datastore.md");
    smoke.contains(M7_STORE_OK_MARKER)
        && smoke.contains("m7_2_store_gate_passes")
        && smoke.contains("prop_datastore_package")
        && smoke.contains("host_catalog_persist_roundtrip")
        && runbook.contains("RAYNU-V-M7-STORE-OK")
        && runbook.contains("EFI/RAYNU/images")
        && runbook.contains("/images")
        && runbook.contains("UnsupportedOnFirmware")
}

/// Full M7.2 artifact + package gate.
pub fn run_m7_store_gate() -> bool {
    store_surface_present()
        && store_uefi_stub_honest()
        && store_scripts_present()
        && prop_datastore_package()
}

#[cfg(test)]
#[path = "m7_store_gate_test.rs"]
mod m7_store_gate_test;
