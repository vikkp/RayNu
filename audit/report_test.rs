use super::*;

#[test]
fn schemas_embedded() {
    assert!(schemas_present());
    assert!(!SCHEMA_SOX.is_empty());
    assert!(!SCHEMA_ISO.is_empty());
    assert_eq!(SECTION_SCHEMAS, ".aschema");
}

#[test]
fn reports_deterministic() {
    assert!(prop_reports_deterministic());
}

#[test]
fn sox_json_mentions_controls() {
    let ring = sample_ring();
    let snap = RingSnapshot::from_ring(&ring);
    let mut buf = [0u8; 512];
    let n = render_report(
        ReportKind::SoxAccessControl,
        ReportFormat::Json,
        &snap,
        &mut buf,
    )
    .unwrap();
    let s = core::str::from_utf8(&buf[..n]).unwrap();
    assert!(s.contains("vmcs_created"));
    assert!(s.contains("lifecycle_mutations"));
    assert!(s.contains("msr_blocks"));
}

#[test]
fn iso_csv_has_header() {
    let ring = sample_ring();
    let snap = RingSnapshot::from_ring(&ring);
    let mut buf = [0u8; 512];
    let n = render_report(
        ReportKind::IsoEventInventory,
        ReportFormat::Csv,
        &snap,
        &mut buf,
    )
    .unwrap();
    let s = core::str::from_utf8(&buf[..n]).unwrap();
    assert!(s.starts_with("report,schema,tip_hash,total,"));
    assert!(s.contains("iso_event_inventory"));
}

#[test]
fn pdf_is_gap() {
    let ring = sample_ring();
    let snap = RingSnapshot::from_ring(&ring);
    let mut buf = [0u8; 64];
    assert_eq!(
        render_report(ReportKind::SoxAccessControl, ReportFormat::Pdf, &snap, &mut buf),
        Err(ReportError::PdfNotImplemented)
    );
    assert!(PDF_GAP_NOTE.contains("M6"));
}
