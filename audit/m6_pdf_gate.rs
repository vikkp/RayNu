//! M6.5 host verification gate (deterministic PDF audit reports).
//!
//! Pillar: [A]
//! Proven Core: outside (report templates — companion to `audit/report`).
//!
//! Checks that PDF render from a frozen ring snapshot is deterministic, closes
//! the M5.4 PDF GAP, embeds `RAYNU-V-M6-PDF-OK`, and that smoke/CI wiring exists.

use super::report::{
    prop_pdf_reports_deterministic, prop_reports_deterministic, M6_PDF_OK_MARKER, PDF_GAP_NOTE,
};

/// Host / CI marker when the M6.5 PDF report gate passes.
pub const M6_PDF_GATE_MARKER: &str = M6_PDF_OK_MARKER;

/// True when report module exposes PDF render, closed GAP, and marker.
pub fn pdf_surface_present() -> bool {
    let s = include_str!("report.rs");
    s.contains("fn render_pdf(")
        && s.contains("fn prop_pdf_reports_deterministic(")
        && s.contains("fn write_pdf_content(")
        && s.contains("%PDF-1.4")
        && s.contains(M6_PDF_OK_MARKER)
        && s.contains(PDF_GAP_NOTE)
        && PDF_GAP_NOTE.contains("CLOSED M6.5")
        && !s.contains("PdfNotImplemented")
}

/// True when schemas advertise PDF as a supported format.
pub fn pdf_schemas_advertise_format() -> bool {
    let sox = include_str!("../assets/schemas/sox_access_control.json");
    let iso = include_str!("../assets/schemas/iso_event_inventory.json");
    sox.contains("\"pdf\"") && iso.contains("\"pdf\"")
}

/// True when the M6.5 smoke script is present.
pub fn pdf_scripts_present() -> bool {
    let smoke = include_str!("../tools/m6-pdf-smoke.sh");
    smoke.contains(M6_PDF_OK_MARKER)
        && smoke.contains("m6_5_pdf_gate_passes")
        && smoke.contains("pdf_reports_deterministic")
}

/// Full M6.5 artifact + PDF determinism gate.
pub fn run_m6_pdf_gate() -> bool {
    pdf_surface_present()
        && pdf_schemas_advertise_format()
        && pdf_scripts_present()
        && prop_reports_deterministic()
        && prop_pdf_reports_deterministic()
}

#[cfg(test)]
#[path = "m6_pdf_gate_test.rs"]
mod m6_pdf_gate_test;
