use super::{
    audit_ha_events_present, ha_runbook_present, ha_scripts_present, ha_surface_present,
    run_m6_ha_gate, M6_HA_GATE_MARKER,
};
use crate::mgmt::ha::{prop_ha_failover_restart, prop_security_harden_checklist};

#[test]
fn m6_6_ha_gate_passes() {
    assert_eq!(M6_HA_GATE_MARKER, "RAYNU-V-M6-HA-OK");
    assert!(ha_surface_present(), "mgmt/ha must embed M6.6 HA");
    assert!(
        audit_ha_events_present(),
        "audit must carry HaFailoverStarted/Completed"
    );
    assert!(ha_scripts_present(), "m6-ha-smoke.sh must be present");
    assert!(ha_runbook_present(), "docs/runbooks/ha.md must be present");
    assert!(
        prop_ha_failover_restart(),
        "HA failover prop must hold"
    );
    assert!(
        prop_security_harden_checklist(),
        "security harden checklist must hold"
    );
    assert!(run_m6_ha_gate());
    println!("RAYNU-V-M6-HA-OK");
}
