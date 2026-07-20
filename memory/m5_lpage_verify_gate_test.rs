use super::*;

#[test]
fn m5_7_lpage_verify_gate_passes() {
    assert!(
        ept_model_opts_into_verus(),
        "ept_model must opt into Verus"
    );
    assert!(
        ept_model_is_verified_lpage_l3(),
        "ept_model must discharge large-page L3 with no admit("
    );
    assert!(
        ept_proof_closes_lpage_l3(),
        "ept_proof/ept_spec must close Large-page L3 GAP for M5.7"
    );
    assert!(
        lpage_verify_scripts_present(),
        "verus-lpage-verify-smoke.sh incomplete"
    );
    assert!(run_m5_lpage_verify_gate(), "M5.7 lpage-verify gate failed");
    assert_eq!(M5_LPAGE_VERIFY_OK_MARKER, "RAYNU-V-M5-LPAGE-VERIFY-OK");
    println!("{M5_LPAGE_VERIFY_OK_MARKER}");
}
