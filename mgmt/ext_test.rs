use super::*;

#[test]
fn auditor_pin_ready() {
    assert!(prop_auditor_pin_ready());
}

#[test]
fn spec_review_filed() {
    assert!(prop_spec_review_filed());
}

#[test]
fn findings_no_open_critical() {
    assert!(prop_findings_no_open_critical());
}

#[test]
fn proof_maintenance_dry_run() {
    assert!(prop_proof_maintenance_dry_run());
}

#[test]
fn external_audit_package() {
    assert!(prop_external_audit_package());
}

#[test]
fn gap_closed_and_marker() {
    assert!(EXT_GAP_NOTE.contains("CLOSED M6.9"));
    assert_eq!(M6_EXT_OK_MARKER, "RAYNU-V-M6-EXT-OK");
}
