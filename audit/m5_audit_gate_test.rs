use super::*;

#[test]
fn m5_3_audit_gate_passes() {
    assert!(
        audit_integrity_surface_present(),
        "audit/integrity must expose ring/verify/tamper + marker"
    );
    assert!(
        mandatory_events_wired(),
        "VMCS/EPT/MSR/lifecycle audit_log call sites missing"
    );
    assert!(
        audit_scripts_present(),
        "tools/m5-audit-smoke.sh missing or incomplete"
    );
    assert!(
        prop_mandatory_events_chain(),
        "mandatory event chain property failed"
    );
    assert!(prop_tamper_detected(), "tamper detection property failed");
    assert!(run_m5_audit_gate(), "M5.3 audit gate failed");
    assert_eq!(M5_AUDIT_OK_MARKER, "RAYNU-V-M5-AUDIT-OK");
    println!("{M5_AUDIT_OK_MARKER}");
}
