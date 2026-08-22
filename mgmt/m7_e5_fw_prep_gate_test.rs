use super::{
    fw_prep_surface_present, prop_fw_prep_after_bind, prop_fw_prep_rejects, prop_rest_fw_prep,
    run_m7_e5_fw_prep_gate, E5_FW_PREP_RESIDUAL_NOTE, M7_E5_FW_PREP_OK_MARKER,
};

#[test]
fn m7_e5_fw_prep_gate_passes() {
    assert_eq!(M7_E5_FW_PREP_OK_MARKER, "RAYNU-V-M7-E5-FW-PREP-OK");
    assert!(E5_FW_PREP_RESIDUAL_NOTE.contains("UnsupportedOnFirmware"));
    assert!(E5_FW_PREP_RESIDUAL_NOTE.contains("refused"));
    assert!(prop_fw_prep_after_bind());
    assert!(prop_fw_prep_rejects());
    assert!(prop_rest_fw_prep());
    assert!(fw_prep_surface_present());
    assert!(
        run_m7_e5_fw_prep_gate(),
        "E5 Stage 9 firmware launch-prepare must hold"
    );
}
