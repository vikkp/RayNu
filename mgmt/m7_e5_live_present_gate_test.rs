use super::{
    live_present_surface_present, prop_live_present_after_live_fd, prop_live_present_rejects,
    prop_rest_live_present, run_m7_e5_live_present_gate, E5_LIVE_PRESENT_RESIDUAL_NOTE,
    M7_E5_LIVE_PRESENT_OK_MARKER,
};

#[test]
fn m7_e5_live_present_gate_passes() {
    assert_eq!(
        M7_E5_LIVE_PRESENT_OK_MARKER,
        "RAYNU-V-M7-E5-LIVE-PRESENT-OK"
    );
    assert!(E5_LIVE_PRESENT_RESIDUAL_NOTE.contains("UnsupportedOnFirmware"));
    assert!(E5_LIVE_PRESENT_RESIDUAL_NOTE.contains("not a shipped OVMF.fd"));
    assert!(E5_LIVE_PRESENT_RESIDUAL_NOTE.contains("VMLAUNCH insn not issued"));
    assert!(E5_LIVE_PRESENT_RESIDUAL_NOTE.contains("live EPT is not written"));
    assert!(prop_live_present_after_live_fd());
    assert!(prop_live_present_rejects());
    assert!(prop_rest_live_present());
    assert!(live_present_surface_present());
    assert!(
        run_m7_e5_live_present_gate(),
        "E5 Stage 25 live-ESP present must hold"
    );
}
