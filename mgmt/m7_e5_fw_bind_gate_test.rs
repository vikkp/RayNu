use super::{
    fw_bind_surface_present, prop_fw_bind_after_slot, prop_fw_bind_rejects, prop_rest_fw_bind,
    run_m7_e5_fw_bind_gate, E5_FW_BIND_RESIDUAL_NOTE, M7_E5_FW_BIND_OK_MARKER,
};

#[test]
fn m7_e5_fw_bind_gate_passes() {
    assert_eq!(M7_E5_FW_BIND_OK_MARKER, "RAYNU-V-M7-E5-FW-BIND-OK");
    assert!(E5_FW_BIND_RESIDUAL_NOTE.contains("UnsupportedOnFirmware"));
    assert!(prop_fw_bind_after_slot());
    assert!(prop_fw_bind_rejects());
    assert!(prop_rest_fw_bind());
    assert!(fw_bind_surface_present());
    assert!(
        run_m7_e5_fw_bind_gate(),
        "E5 Stage 8 firmware guest bind must hold"
    );
}
