use super::{
    guest_fw_load_surface_present, prop_guest_fw_load_after_box, prop_guest_fw_load_rejects,
    prop_rest_guest_fw_load, run_m7_e5_guest_fw_load_gate, E5_GUEST_FW_LOAD_RESIDUAL_NOTE,
    M7_E5_GUEST_FW_LOAD_OK_MARKER,
};

#[test]
fn m7_e5_guest_fw_load_gate_passes() {
    assert_eq!(
        M7_E5_GUEST_FW_LOAD_OK_MARKER,
        "RAYNU-V-M7-E5-GUEST-FW-LOAD-OK"
    );
    assert!(E5_GUEST_FW_LOAD_RESIDUAL_NOTE.contains("UnsupportedOnFirmware"));
    assert!(prop_guest_fw_load_after_box());
    assert!(prop_guest_fw_load_rejects());
    assert!(prop_rest_guest_fw_load());
    assert!(guest_fw_load_surface_present());
    assert!(
        run_m7_e5_guest_fw_load_gate(),
        "E5 Stage 4 guest firmware load must hold"
    );
}
