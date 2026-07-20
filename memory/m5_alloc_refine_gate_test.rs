use super::{
    ept_model_has_alloc_refine, ept_spec_closes_alloc_refine, prop_live_alloc_ept_coupled,
    prop_precise_identity_geometry, run_m5_alloc_refine_gate, M5_ALLOC_REFINE_OK_MARKER,
};

#[test]
fn m5_9_alloc_refine_gate_passes() {
    assert!(
        ept_model_has_alloc_refine(),
        "ept_model must carry alloc_ept_refines / theorem_alloc_map_unmap_refines / marker"
    );
    assert!(
        ept_spec_closes_alloc_refine(),
        "ept_spec/ept_proof must close allocator↔EPT GAP for M5.9"
    );
    assert!(
        prop_live_alloc_ept_coupled(),
        "live allocate→map→unmap must keep owned ⊆ allocated"
    );
    assert!(
        prop_precise_identity_geometry(),
        "precise identity geometry must match ghost PRECISE_IDENTITY_FRAMES"
    );
    assert!(run_m5_alloc_refine_gate());
    assert_eq!(M5_ALLOC_REFINE_OK_MARKER, "RAYNU-V-M5-ALLOC-REFINE-OK");
    println!("{M5_ALLOC_REFINE_OK_MARKER}");
}
