use super::{
    audit_fault_events_present, fault_hooks_present, fault_runbook_present, fault_scripts_present,
    fault_surface_present, run_m6_fault_gate, M6_FAULT_GATE_MARKER,
};
use crate::mgmt::fault::prop_fault_suite;

#[test]
fn m6_7_fault_gate_passes() {
    assert_eq!(M6_FAULT_GATE_MARKER, "RAYNU-V-M6-FAULT-OK");
    assert!(fault_surface_present(), "mgmt/fault must embed M6.7 suite");
    assert!(
        audit_fault_events_present(),
        "audit must carry FaultInjected/Recovered/FailClosed"
    );
    assert!(
        fault_hooks_present(),
        "vcpu tear_down + vswitch partition required"
    );
    assert!(fault_scripts_present(), "m6-fault-smoke.sh must be present");
    assert!(fault_runbook_present(), "docs/runbooks/fault.md must be present");
    assert!(prop_fault_suite(), "fault suite prop must hold");
    assert!(run_m6_fault_gate());
    println!("RAYNU-V-M6-FAULT-OK");
}
