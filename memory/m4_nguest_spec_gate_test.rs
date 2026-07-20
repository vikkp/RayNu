use super::*;

#[test]
fn m4_6_nguest_spec_gate_passes() {
    assert!(
        ept_model_opts_into_verus(),
        "ept_model must set package.metadata.verus.verify = true"
    );
    assert!(
        ept_model_has_n_guest_spec(),
        "ept_model must contain theorem_n_guest_4k_map_unmap_exclusive + marker"
    );
    assert!(
        ept_spec_closes_n_guest_todo(),
        "ept_spec/ept_proof must close N-guest TODO/GAP (spec side)"
    );
    assert!(
        nguest_spec_scripts_present(),
        "tools/verus-nguest-spec-smoke.sh missing or incomplete"
    );
    assert!(
        prop_n_guest_hpa_exclusive(),
        "live EptMap must reject HPA sharing across two guests"
    );
    assert!(run_m4_nguest_spec_gate(), "M4.6 N-guest-spec gate failed");
    assert_eq!(M4_NGUEST_SPEC_OK_MARKER, "RAYNU-V-M4-NGUEST-SPEC-OK");
    println!("{M4_NGUEST_SPEC_OK_MARKER}");
}
