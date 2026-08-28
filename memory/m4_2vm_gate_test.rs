use super::*;

#[test]
fn marker_stable() {
    assert_eq!(M4_2VM_OK_MARKER, "RAYNU-V-M4-2VM-OK");
}

#[test]
fn m4_2vm_gate_passes() {
    assert!(run_m4_2vm_gate(), "M4.0 2VM gate failed");
}

#[test]
fn g1_slab_cr3_matches_iron_ept_gpa() {
    use crate::memory::ept_hw::{G1_SLAB_OFF_PML4, TWO_MIB};
    assert_eq!(G1_SLAB_OFF_PML4, 0x3000);
    let slab = 0x1040_0000u64;
    assert_eq!(slab & (TWO_MIB - 1), 0);
    assert_eq!(slab + G1_SLAB_OFF_PML4, 0x1040_3000);
}
