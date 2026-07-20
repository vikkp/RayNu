use super::*;

#[test]
fn m4_9_nguest_refine_gate_passes() {
    assert!(
        ept_model_opts_into_verus(),
        "ept_model must set package.metadata.verus.verify = true"
    );
    assert!(
        ept_model_has_n_guest_refine(),
        "ept_model must contain theorem_concrete_n_guest_4k_refine + marker"
    );
    assert!(
        ept_spec_closes_n_guest_refine(),
        "ept_spec/ept_proof must close N-guest refine TODO/GAP"
    );
    assert!(
        nguest_refine_scripts_present(),
        "tools/verus-nguest-refine-smoke.sh missing or incomplete"
    );
    assert!(
        prop_live_n_guest_map_unmap_refines(),
        "live EptMap must refine exclusivity under multi-guest map/unmap"
    );
    assert!(run_m4_nguest_refine_gate(), "M4.9 N-guest-refine gate failed");
    assert_eq!(M4_REFINE_OK_MARKER, "RAYNU-V-M4-REFINE-OK");
    println!("{M4_REFINE_OK_MARKER}");
}
