use super::{
    pdf_schemas_advertise_format, pdf_scripts_present, pdf_surface_present, run_m6_pdf_gate,
    M6_PDF_GATE_MARKER,
};
use crate::audit::report::prop_pdf_reports_deterministic;

#[test]
fn m6_5_pdf_gate_passes() {
    assert_eq!(M6_PDF_GATE_MARKER, "RAYNU-V-M6-PDF-OK");
    assert!(pdf_surface_present(), "audit/report must embed M6.5 PDF");
    assert!(
        pdf_schemas_advertise_format(),
        "schemas must list pdf format"
    );
    assert!(pdf_scripts_present(), "m6-pdf-smoke.sh must be present");
    assert!(
        prop_pdf_reports_deterministic(),
        "PDF determinism prop must hold"
    );
    assert!(run_m6_pdf_gate());
    println!("RAYNU-V-M6-PDF-OK");
}
