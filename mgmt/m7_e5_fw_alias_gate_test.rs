use super::{
    fw_alias_surface_present, prop_fw_alias_after_reset, prop_fw_alias_rejects, prop_rest_fw_alias,
    run_m7_e5_fw_alias_gate, E5_FW_ALIAS_RESIDUAL_NOTE, M7_E5_FW_ALIAS_OK_MARKER,
};

#[test]
fn m7_e5_fw_alias_gate_passes() {
    assert_eq!(M7_E5_FW_ALIAS_OK_MARKER, "RAYNU-V-M7-E5-FW-ALIAS-OK");
    assert!(E5_FW_ALIAS_RESIDUAL_NOTE.contains("UnsupportedOnFirmware"));
    assert!(E5_FW_ALIAS_RESIDUAL_NOTE.contains("not a shipped OVMF.fd"));
    assert!(E5_FW_ALIAS_RESIDUAL_NOTE.contains("VMLAUNCH insn not issued"));
    assert!(prop_fw_alias_after_reset());
    assert!(prop_fw_alias_rejects());
    assert!(prop_rest_fw_alias());
    assert!(fw_alias_surface_present());
    assert!(
        run_m7_e5_fw_alias_gate(),
        "E5 Stage 15 firmware-alias EPT contract must hold"
    );
}
