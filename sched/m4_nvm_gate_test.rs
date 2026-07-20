use super::*;
use crate::sched::scheduler::M4_NVM_OK_MARKER;

#[test]
fn marker_stable() {
    assert_eq!(M4_NVM_OK_MARKER, "RAYNU-V-M4-NVM-OK");
}

#[test]
fn m4_nvm_gate_passes() {
    assert!(run_m4_nvm_gate(), "M4.2 NVM gate failed");
}
