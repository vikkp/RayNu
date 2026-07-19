use super::*;

#[test]
fn m3_17_l3_verify_gate_passes() {
    assert!(
        ept_model_opts_into_verus(),
        "ept_model must set package.metadata.verus.verify = true"
    );
    assert!(
        ept_model_is_verified_l3(),
        "ept_model must discharge exclusivity lemmas with no admit("
    );
    assert!(
        l3_verify_scripts_present(),
        "tools/verus-verify-smoke.sh missing or incomplete"
    );
    assert!(run_l3_verify_gate(), "M3.17 L3-verify gate failed");
    assert_eq!(M3_L3_VERIFY_OK_MARKER, "RAYNU-V-M3-L3-VERIFY-OK");
    println!("{M3_L3_VERIFY_OK_MARKER}");
}
