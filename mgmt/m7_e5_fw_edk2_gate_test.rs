use super::{
    fw_edk2_surface_present, prop_fw_edk2_after_floor, prop_fw_edk2_rejects, prop_rest_fw_edk2,
    run_m7_e5_fw_edk2_gate, E5_FW_EDK2_RESIDUAL_NOTE, M7_E5_FW_EDK2_OK_MARKER,
};

#[test]
fn m7_e5_fw_edk2_gate_passes() {
    assert_eq!(M7_E5_FW_EDK2_OK_MARKER, "RAYNU-V-M7-E5-FW-EDK2-OK");
    assert!(E5_FW_EDK2_RESIDUAL_NOTE.contains("UnsupportedOnFirmware"));
    assert!(E5_FW_EDK2_RESIDUAL_NOTE.contains("not a shipped"));
    assert!(prop_fw_edk2_after_floor());
    assert!(prop_fw_edk2_rejects());
    assert!(prop_rest_fw_edk2());
    assert!(fw_edk2_surface_present());
    assert!(
        run_m7_e5_fw_edk2_gate(),
        "E5 Stage 11 firmware EDK2-sized stage must hold"
    );
}
