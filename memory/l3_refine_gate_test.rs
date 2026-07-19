use super::*;

#[test]
fn m3_18_l3_refine_gate_passes() {
    assert!(
        ept_model_opts_into_verus(),
        "ept_model must set package.metadata.verus.verify = true"
    );
    assert!(
        ept_model_has_refine(),
        "ept_model must contain ConcreteEptMap + abs/refines + refine theorem"
    );
    assert!(
        l3_refine_scripts_present(),
        "tools/verus-refine-smoke.sh missing or incomplete"
    );
    assert!(
        prop_live_map_unmap_refines(),
        "live EptMap must refine exclusivity under 4K bring-up map/unmap"
    );
    assert!(run_l3_refine_gate(), "M3.18 L3-refine gate failed");
    assert_eq!(M3_L3_REFINE_OK_MARKER, "RAYNU-V-M3-L3-REFINE-OK");
    println!("{M3_L3_REFINE_OK_MARKER}");
}
