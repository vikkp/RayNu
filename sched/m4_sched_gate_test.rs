use super::*;
use crate::sched::scheduler::M4_SCHED_OK_MARKER;

#[test]
fn marker_stable() {
    assert_eq!(M4_SCHED_OK_MARKER, "RAYNU-V-M4-SCHED-OK");
}

#[test]
fn m4_sched_gate_passes() {
    assert!(run_m4_sched_gate(), "M4.1 sched gate failed");
}
