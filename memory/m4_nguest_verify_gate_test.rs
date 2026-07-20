use super::*;

#[test]
fn m4_7_nguest_verify_gate_passes() {
    assert!(
        ept_model_opts_into_verus(),
        "ept_model must set package.metadata.verus.verify = true"
    );
    assert!(
        ept_model_is_verified_n_guest_l3(),
        "ept_model must discharge N-guest L3 with no admit("
    );
    assert!(
        ept_proof_closes_n_guest_l3(),
        "ept_proof/ept_spec must close N-guest L3 GAP (M4.7)"
    );
    assert!(
        nguest_verify_scripts_present(),
        "tools/verus-nguest-verify-smoke.sh missing or incomplete"
    );
    assert!(
        prop_two_guest_map_unmap_exclusive(),
        "live EptMap must preserve two-guest exclusivity"
    );
    assert!(run_m4_nguest_verify_gate(), "M4.7 N-guest-verify gate failed");
    assert_eq!(M4_NGUEST_VERIFY_OK_MARKER, "RAYNU-V-M4-NGUEST-VERIFY-OK");
    println!("{M4_NGUEST_VERIFY_OK_MARKER}");
}
