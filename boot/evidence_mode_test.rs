//! Host unit tests for evidence mode (ADR-011).

use super::*;

#[test]
fn markers_are_stable() {
    assert_eq!(EVIDENCE_MODE_ON_MARKER, "RAYNU-V-EVIDENCE-MODE-ON");
    assert!(EVIDENCE_BUNDLE_BEGIN.contains("EVIDENCE BUNDLE BEGIN"));
    assert!(EVIDENCE_BUNDLE_END.contains("EVIDENCE BUNDLE END"));
}

#[test]
fn default_inactive() {
    clear_for_test();
    assert!(!is_active());
    assert_eq!(source(), SOURCE_NONE);
}

#[test]
fn force_active_sets_flag_and_source() {
    clear_for_test();
    force_active_for_test(SOURCE_ROOT);
    assert!(is_active());
    assert_eq!(source(), SOURCE_ROOT);

    clear_for_test();
    force_active_for_test(SOURCE_EFI_RAYNU);
    assert!(is_active());
    assert_eq!(source(), SOURCE_EFI_RAYNU);

    clear_for_test();
    assert!(!is_active());
}

#[test]
fn emit_bundle_header_is_noop_when_inactive() {
    clear_for_test();
    // Must not panic when inactive (no serial on host).
    emit_bundle_header("M0");
    assert!(!is_active());
}
