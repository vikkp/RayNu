use super::{
    audit_soak_events_present, run_m6_soak_gate, soak_runbook_present, soak_scripts_present,
    soak_surface_present, M6_SOAK_GATE_MARKER,
};
use crate::mgmt::soak::prop_soak_72h_thresholds;

#[test]
fn m6_8_soak_gate_passes() {
    assert_eq!(M6_SOAK_GATE_MARKER, "RAYNU-V-M6-SOAK-OK");
    assert!(soak_surface_present(), "mgmt/soak must embed M6.8 suite");
    assert!(
        audit_soak_events_present(),
        "audit must carry SoakStarted/Completed/Failed"
    );
    assert!(soak_scripts_present(), "m6-soak-smoke.sh must be present");
    assert!(soak_runbook_present(), "docs/runbooks/soak.md must be present");
    assert!(
        prop_soak_72h_thresholds(),
        "72h soak threshold prop must hold"
    );
    assert!(run_m6_soak_gate());
    println!("RAYNU-V-M6-SOAK-OK");
}
