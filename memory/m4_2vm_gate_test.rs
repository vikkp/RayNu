use super::*;

#[test]
fn marker_stable() {
    assert_eq!(M4_2VM_OK_MARKER, "RAYNU-V-M4-2VM-OK");
}

#[test]
fn m4_2vm_gate_passes() {
    assert!(run_m4_2vm_gate(), "M4.0 2VM gate failed");
}
