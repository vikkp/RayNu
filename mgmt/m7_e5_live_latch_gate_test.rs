use super::{
    live_latch_surface_present, prop_live_latch_after_live_commit, prop_live_latch_rejects,
    prop_rest_live_latch, run_m7_e5_live_latch_gate, E5_LIVE_LATCH_RESIDUAL_NOTE,
    M7_E5_LIVE_LATCH_OK_MARKER,
};

#[test]
fn m7_e5_live_latch_gate_passes() {
    assert_eq!(M7_E5_LIVE_LATCH_OK_MARKER, "RAYNU-V-M7-E5-LIVE-LATCH-OK");
    assert!(E5_LIVE_LATCH_RESIDUAL_NOTE.contains("UnsupportedOnFirmware"));
    assert!(E5_LIVE_LATCH_RESIDUAL_NOTE.contains("not a shipped OVMF.fd"));
    assert!(E5_LIVE_LATCH_RESIDUAL_NOTE.contains("VMLAUNCH insn not issued"));
    assert!(E5_LIVE_LATCH_RESIDUAL_NOTE.contains("live EPT is not written"));
    assert!(prop_live_latch_after_live_commit());
    assert!(prop_live_latch_rejects());
    assert!(prop_rest_live_latch());
    assert!(live_latch_surface_present());
    assert!(
        run_m7_e5_live_latch_gate(),
        "E5 Stage 32 live-ESP latch must hold"
    );
}
