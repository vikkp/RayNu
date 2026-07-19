use super::*;

#[test]
fn m3_16_l3_link_gate_passes() {
    assert!(
        ept_model_opts_into_verus(),
        "ept_model must set package.metadata.verus.verify = true"
    );
    assert!(
        ept_model_is_linked(),
        "ept_model must contain verus! GhostEptMap + exclusivity lemmas"
    );
    assert!(l3_link_scripts_present(), "tools/verus-link-smoke.sh missing");
    assert!(run_l3_link_gate(), "M3.16 L3-link gate failed");
    assert_eq!(M3_L3_LINK_OK_MARKER, "RAYNU-V-M3-L3-LINK-OK");
    println!("{M3_L3_LINK_OK_MARKER}");
}
