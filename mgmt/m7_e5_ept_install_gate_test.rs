use super::{
    ept_install_surface_present, prop_ept_install_after_program, prop_ept_install_rejects,
    prop_rest_ept_install, run_m7_e5_ept_install_gate, E5_EPT_INSTALL_RESIDUAL_NOTE,
    M7_E5_EPT_INSTALL_OK_MARKER,
};

#[test]
fn m7_e5_ept_install_gate_passes() {
    assert_eq!(M7_E5_EPT_INSTALL_OK_MARKER, "RAYNU-V-M7-E5-EPT-INSTALL-OK");
    assert!(E5_EPT_INSTALL_RESIDUAL_NOTE.contains("UnsupportedOnFirmware"));
    assert!(E5_EPT_INSTALL_RESIDUAL_NOTE.contains("not a shipped OVMF.fd"));
    assert!(E5_EPT_INSTALL_RESIDUAL_NOTE.contains("VMLAUNCH insn not issued"));
    assert!(E5_EPT_INSTALL_RESIDUAL_NOTE.contains("live EPT is not written"));
    assert!(prop_ept_install_after_program());
    assert!(prop_ept_install_rejects());
    assert!(prop_rest_ept_install());
    assert!(ept_install_surface_present());
    assert!(
        run_m7_e5_ept_install_gate(),
        "E5 Stage 17 private alias-EPT install contract must hold"
    );
}
