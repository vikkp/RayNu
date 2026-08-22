use super::{
    priv_vmcs_surface_present, prop_priv_vmcs_after_require, prop_priv_vmcs_rejects,
    prop_rest_priv_vmcs, run_m7_e5_priv_vmcs_gate, E5_PRIV_VMCS_RESIDUAL_NOTE,
    M7_E5_PRIV_VMCS_OK_MARKER,
};

#[test]
fn m7_e5_priv_vmcs_gate_passes() {
    assert_eq!(M7_E5_PRIV_VMCS_OK_MARKER, "RAYNU-V-M7-E5-PRIV-VMCS-OK");
    assert!(E5_PRIV_VMCS_RESIDUAL_NOTE.contains("UnsupportedOnFirmware"));
    assert!(E5_PRIV_VMCS_RESIDUAL_NOTE.contains("not a shipped OVMF.fd"));
    assert!(E5_PRIV_VMCS_RESIDUAL_NOTE.contains("VMLAUNCH insn not issued"));
    assert!(E5_PRIV_VMCS_RESIDUAL_NOTE.contains("live EPT is not written"));
    assert!(prop_priv_vmcs_after_require());
    assert!(prop_priv_vmcs_rejects());
    assert!(prop_rest_priv_vmcs());
    assert!(priv_vmcs_surface_present());
    assert!(
        run_m7_e5_priv_vmcs_gate(),
        "E5 Stage 21 private guest-UEFI VMCS arm must hold"
    );
}
