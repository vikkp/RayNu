use super::*;
use crate::sched::smp_probe::M4_SMP_OK_MARKER;

#[test]
fn marker_stable() {
    assert_eq!(M4_SMP_OK_MARKER, "RAYNU-V-M4-SMP-OK");
}

#[test]
fn m4_smp_gate_passes() {
    assert!(run_m4_smp_gate(), "M4.5 smp gate failed");
}
