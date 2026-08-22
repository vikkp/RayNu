use super::{
    live_commit_surface_present, prop_live_commit_after_live_apply, prop_live_commit_rejects,
    prop_rest_live_commit, run_m7_e5_live_commit_gate, E5_LIVE_COMMIT_RESIDUAL_NOTE,
    M7_E5_LIVE_COMMIT_OK_MARKER,
};

#[test]
fn m7_e5_live_commit_gate_passes() {
    assert_eq!(M7_E5_LIVE_COMMIT_OK_MARKER, "RAYNU-V-M7-E5-LIVE-COMMIT-OK");
    assert!(E5_LIVE_COMMIT_RESIDUAL_NOTE.contains("UnsupportedOnFirmware"));
    assert!(E5_LIVE_COMMIT_RESIDUAL_NOTE.contains("not a shipped OVMF.fd"));
    assert!(E5_LIVE_COMMIT_RESIDUAL_NOTE.contains("VMLAUNCH insn not issued"));
    assert!(E5_LIVE_COMMIT_RESIDUAL_NOTE.contains("live EPT is not written"));
    assert!(prop_live_commit_after_live_apply());
    assert!(prop_live_commit_rejects());
    assert!(prop_rest_live_commit());
    assert!(live_commit_surface_present());
    assert!(
        run_m7_e5_live_commit_gate(),
        "E5 Stage 31 live-ESP commit must hold"
    );
}
