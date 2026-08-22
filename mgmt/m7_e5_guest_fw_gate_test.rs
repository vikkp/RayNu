use super::{
    guest_fw_surface_present, prop_guest_fw_box_embedded, prop_guest_fw_box_rejects,
    prop_rest_guest_fw_box, run_m7_e5_guest_fw_gate, E5_GUEST_FW_RESIDUAL_NOTE,
    M7_E5_GUEST_FW_OK_MARKER,
};

#[test]
fn m7_e5_guest_fw_gate_passes() {
    assert_eq!(M7_E5_GUEST_FW_OK_MARKER, "RAYNU-V-M7-E5-GUEST-FW-OK");
    assert!(E5_GUEST_FW_RESIDUAL_NOTE.contains("UnsupportedOnFirmware"));
    assert!(prop_guest_fw_box_embedded());
    assert!(prop_guest_fw_box_rejects());
    assert!(prop_rest_guest_fw_box());
    assert!(guest_fw_surface_present());
    assert!(
        run_m7_e5_guest_fw_gate(),
        "E5 Stage 3 guest firmware envelope must hold"
    );
}
