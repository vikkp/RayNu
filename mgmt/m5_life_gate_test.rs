use super::*;

#[test]
fn m5_0_life_gate_passes() {
    assert!(
        mgmt_lifecycle_api_present(),
        "mgmt must expose VmTable create/start/stop/destroy + marker"
    );
    assert!(
        audit_lifecycle_events_present(),
        "audit must define VmCreated/Started/Stopped/Destroyed"
    );
    assert!(
        life_scripts_present(),
        "tools/m5-life-smoke.sh missing or incomplete"
    );
    assert!(
        prop_lifecycle_roundtrip(),
        "lifecycle round-trip Defined→Running→Stopped→Destroyed failed"
    );
    assert!(run_m5_life_gate(), "M5.0 lifecycle gate failed");
    assert_eq!(M5_LIFE_OK_MARKER, "RAYNU-V-M5-LIFE-OK");
    println!("{M5_LIFE_OK_MARKER}");
}
