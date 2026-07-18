use super::*;

#[test]
fn m2_6_l2_gate_passes() {
    assert!(ept_spec_is_l2(), "ept_spec.rs must declare L2 ghost model");
    assert!(
        allocator_spec_is_l2(),
        "frame_allocator_spec.rs must declare L2 ghost model"
    );
    assert!(run_l2_gate(), "M2.6 L2 property gate failed");
    assert_eq!(M2_L2_OK_MARKER, "RAYNU-V-M2-L2-OK");
    // Surface the marker in CI logs (host gate — not a serial boot marker).
    println!("{M2_L2_OK_MARKER}");
}

#[test]
fn props_individually() {
    assert!(prop_no_hpa_alias());
    assert!(prop_allocator_integrity());
}
