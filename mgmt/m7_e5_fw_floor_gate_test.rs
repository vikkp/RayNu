use super::{
    fw_floor_surface_present, prop_fw_floor_after_prep, prop_fw_floor_rejects, prop_rest_fw_floor,
    run_m7_e5_fw_floor_gate, E5_FW_FLOOR_RESIDUAL_NOTE, M7_E5_FW_FLOOR_OK_MARKER,
};

#[test]
fn m7_e5_fw_floor_gate_passes() {
    assert_eq!(M7_E5_FW_FLOOR_OK_MARKER, "RAYNU-V-M7-E5-FW-FLOOR-OK");
    assert!(E5_FW_FLOOR_RESIDUAL_NOTE.contains("UnsupportedOnFirmware"));
    assert!(E5_FW_FLOOR_RESIDUAL_NOTE.contains("not EDK2"));
    assert!(prop_fw_floor_after_prep());
    assert!(prop_fw_floor_rejects());
    assert!(prop_rest_fw_floor());
    assert!(fw_floor_surface_present());
    assert!(
        run_m7_e5_fw_floor_gate(),
        "E5 Stage 10 firmware size-floor must hold"
    );
}
