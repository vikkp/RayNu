use super::{
    live_hold_surface_present, prop_live_hold_after_live_lock, prop_live_hold_rejects,
    prop_rest_live_hold, run_m7_e5_live_hold_gate, E5_LIVE_HOLD_RESIDUAL_NOTE,
    M7_E5_LIVE_HOLD_OK_MARKER,
};

#[test]
fn m7_e5_live_hold_gate_passes() {
    assert_eq!(M7_E5_LIVE_HOLD_OK_MARKER, "RAYNU-V-M7-E5-LIVE-HOLD-OK");
    assert!(E5_LIVE_HOLD_RESIDUAL_NOTE.contains("UnsupportedOnFirmware"));
    assert!(E5_LIVE_HOLD_RESIDUAL_NOTE.contains("not a shipped OVMF.fd"));
    assert!(E5_LIVE_HOLD_RESIDUAL_NOTE.contains("VMLAUNCH insn not issued"));
    assert!(E5_LIVE_HOLD_RESIDUAL_NOTE.contains("live EPT is not written"));
    assert!(prop_live_hold_after_live_lock());
    assert!(prop_live_hold_rejects());
    assert!(prop_rest_live_hold());
    assert!(live_hold_surface_present());
    assert!(
        run_m7_e5_live_hold_gate(),
        "E5 Stage 35 live-ESP hold must hold"
    );
}
