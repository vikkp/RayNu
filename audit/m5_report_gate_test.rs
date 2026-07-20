use super::*;

#[test]
fn m5_4_report_gate_passes() {
    assert!(
        report_surface_present(),
        "audit/report must expose schemas + renderer + marker"
    );
    assert!(
        schema_assets_present(),
        "SOX/ISO schema assets missing or incomplete"
    );
    assert!(
        report_scripts_present(),
        "tools/m5-report-smoke.sh missing or incomplete"
    );
    assert!(
        prop_reports_deterministic(),
        "report determinism property failed"
    );
    assert!(run_m5_report_gate(), "M5.4 report gate failed");
    assert_eq!(M5_REPORT_OK_MARKER, "RAYNU-V-M5-REPORT-OK");
    println!("{M5_REPORT_OK_MARKER}");
}
