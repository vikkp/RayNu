use super::{
    live_fd_surface_present, prop_live_fd_after_live_bytes, prop_live_fd_rejects,
    prop_rest_live_fd, run_m7_e5_live_fd_gate, E5_LIVE_FD_RESIDUAL_NOTE, M7_E5_LIVE_FD_OK_MARKER,
};

#[test]
fn m7_e5_live_fd_gate_passes() {
    assert_eq!(M7_E5_LIVE_FD_OK_MARKER, "RAYNU-V-M7-E5-LIVE-FD-OK");
    assert!(E5_LIVE_FD_RESIDUAL_NOTE.contains("UnsupportedOnFirmware"));
    assert!(E5_LIVE_FD_RESIDUAL_NOTE.contains("not a shipped OVMF.fd"));
    assert!(E5_LIVE_FD_RESIDUAL_NOTE.contains("VMLAUNCH insn not issued"));
    assert!(E5_LIVE_FD_RESIDUAL_NOTE.contains("live EPT is not written"));
    assert!(prop_live_fd_after_live_bytes());
    assert!(prop_live_fd_rejects());
    assert!(prop_rest_live_fd());
    assert!(live_fd_surface_present());
    assert!(
        run_m7_e5_live_fd_gate(),
        "E5 Stage 24 live-ESP FD require must hold"
    );
}
