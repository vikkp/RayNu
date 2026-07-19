use super::*;

#[test]
fn m3_14_l3_gate_passes() {
    assert!(
        ept_proof_is_l3_attempt(),
        "ept_proof.rs must document the M3.14 L3 attempt + GAP list"
    );
    assert!(
        ept_proof_does_not_claim_l3_complete(),
        "ept_proof.rs must not claim machine-checked L3 before M3.17 green verify"
    );
    assert!(run_l3_gate(), "M3.14 L3-attempt property gate failed");
    assert_eq!(M3_L3_OK_MARKER, "RAYNU-V-M3-L3-OK");
    // Surface the marker in CI logs (host gate — not a serial boot marker).
    println!("{M3_L3_OK_MARKER}");
}

#[test]
fn single_guest_4k_prop() {
    assert!(prop_single_guest_4k_map_unmap_exclusive());
}
