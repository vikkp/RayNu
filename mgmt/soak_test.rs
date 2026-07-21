use super::*;

#[test]
fn soak_72h_thresholds() {
    assert!(prop_soak_72h_thresholds());
}

#[test]
fn soak_metrics_artifact() {
    let m = run_soak_simulation();
    assert!(m.passed);
    assert_eq!(m.hours_completed, 72);
    let a = m.artifact_line();
    assert_eq!(a.hours, 72);
    assert!(a.ok);
    assert_eq!(a.live, 0);
    assert!(a.s0 >= 1 && a.s1 >= 1);
}

#[test]
fn gap_closed_and_marker() {
    assert!(SOAK_GAP_NOTE.contains("CLOSED M6.8"));
    assert_eq!(M6_SOAK_OK_MARKER, "RAYNU-V-M6-SOAK-OK");
    assert_eq!(SOAK_TARGET_HOURS, 72);
}
