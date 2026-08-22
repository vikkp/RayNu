use super::{
    alias_ept_surface_present, prop_alias_ept_after_alias, prop_alias_ept_rejects,
    prop_rest_alias_ept, run_m7_e5_alias_ept_gate, E5_ALIAS_EPT_RESIDUAL_NOTE,
    M7_E5_ALIAS_EPT_OK_MARKER,
};

#[test]
fn m7_e5_alias_ept_gate_passes() {
    assert_eq!(M7_E5_ALIAS_EPT_OK_MARKER, "RAYNU-V-M7-E5-ALIAS-EPT-OK");
    assert!(E5_ALIAS_EPT_RESIDUAL_NOTE.contains("UnsupportedOnFirmware"));
    assert!(E5_ALIAS_EPT_RESIDUAL_NOTE.contains("not a shipped OVMF.fd"));
    assert!(E5_ALIAS_EPT_RESIDUAL_NOTE.contains("VMLAUNCH insn not issued"));
    assert!(E5_ALIAS_EPT_RESIDUAL_NOTE.contains("live EPT is not written"));
    assert!(prop_alias_ept_after_alias());
    assert!(prop_alias_ept_rejects());
    assert!(prop_rest_alias_ept());
    assert!(alias_ept_surface_present());
    assert!(
        run_m7_e5_alias_ept_gate(),
        "E5 Stage 16 alias-EPT program contract must hold"
    );
}
