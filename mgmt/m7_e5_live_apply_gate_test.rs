use super::{
    live_apply_surface_present, prop_live_apply_after_live_place, prop_live_apply_rejects,
    prop_rest_live_apply, run_m7_e5_live_apply_gate, E5_LIVE_APPLY_RESIDUAL_NOTE,
    M7_E5_LIVE_APPLY_OK_MARKER,
};

#[test]
fn m7_e5_live_apply_gate_passes() {
    assert_eq!(M7_E5_LIVE_APPLY_OK_MARKER, "RAYNU-V-M7-E5-LIVE-APPLY-OK");
    assert!(E5_LIVE_APPLY_RESIDUAL_NOTE.contains("UnsupportedOnFirmware"));
    assert!(E5_LIVE_APPLY_RESIDUAL_NOTE.contains("not a shipped OVMF.fd"));
    assert!(E5_LIVE_APPLY_RESIDUAL_NOTE.contains("VMLAUNCH insn not issued"));
    assert!(E5_LIVE_APPLY_RESIDUAL_NOTE.contains("live EPT is not written"));
    assert!(prop_live_apply_after_live_place());
    assert!(prop_live_apply_rejects());
    assert!(prop_rest_live_apply());
    assert!(live_apply_surface_present());
    assert!(
        run_m7_e5_live_apply_gate(),
        "E5 Stage 30 live-ESP apply must hold"
    );
}
