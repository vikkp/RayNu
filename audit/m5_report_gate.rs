//! M5.4 host verification gate (SOX / ISO-style audit reports).
//!
//! Pillar: [A]
//! Proven Core: outside (report templates — companion to `audit/report`).
//!
//! Checks embedded schemas, deterministic JSON/CSV render from a frozen ring
//! snapshot, and smoke script presence.

use super::report::{
    prop_reports_deterministic, schemas_present, M5_REPORT_OK_MARKER, PDF_GAP_NOTE, SCHEMA_ISO,
    SCHEMA_SOX, SECTION_SCHEMAS,
};

/// True when report module exposes schemas, renderer, and marker.
pub fn report_surface_present() -> bool {
    let s = include_str!("report.rs");
    s.contains("fn render_report(")
        && s.contains("fn prop_reports_deterministic(")
        && s.contains("struct RingSnapshot")
        && s.contains("SoxAccessControl")
        && s.contains("IsoEventInventory")
        && s.contains("link_section = \".aschema\"")
        && s.contains(M5_REPORT_OK_MARKER)
        && s.contains(PDF_GAP_NOTE)
        && s.contains(SECTION_SCHEMAS)
}

/// True when on-disk schemas match the embedded constants.
pub fn schema_assets_present() -> bool {
    schemas_present()
        && SCHEMA_SOX.contains("\"id\": \"sox_access_control\"")
        && SCHEMA_ISO.contains("\"id\": \"iso_event_inventory\"")
}

/// True when the M5.4 smoke script is present.
pub fn report_scripts_present() -> bool {
    let smoke = include_str!("../tools/m5-report-smoke.sh");
    smoke.contains(M5_REPORT_OK_MARKER)
        && smoke.contains("m5_4_report_gate_passes")
        && smoke.contains("reports_deterministic")
}

/// Full M5.4 artifact + determinism gate.
pub fn run_m5_report_gate() -> bool {
    report_surface_present()
        && schema_assets_present()
        && report_scripts_present()
        && prop_reports_deterministic()
}

#[cfg(test)]
#[path = "m5_report_gate_test.rs"]
mod m5_report_gate_test;
