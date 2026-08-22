use super::{
    live_bytes_surface_present, prop_live_bytes_after_live_issue, prop_live_bytes_rejects,
    prop_rest_live_bytes, run_m7_e5_live_bytes_gate, E5_LIVE_BYTES_RESIDUAL_NOTE,
    M7_E5_LIVE_BYTES_OK_MARKER,
};

#[test]
fn m7_e5_live_bytes_gate_passes() {
    assert_eq!(M7_E5_LIVE_BYTES_OK_MARKER, "RAYNU-V-M7-E5-LIVE-BYTES-OK");
    assert!(E5_LIVE_BYTES_RESIDUAL_NOTE.contains("UnsupportedOnFirmware"));
    assert!(E5_LIVE_BYTES_RESIDUAL_NOTE.contains("not a shipped OVMF.fd"));
    assert!(E5_LIVE_BYTES_RESIDUAL_NOTE.contains("VMLAUNCH insn not issued"));
    assert!(E5_LIVE_BYTES_RESIDUAL_NOTE.contains("live EPT is not written"));
    assert!(prop_live_bytes_after_live_issue());
    assert!(prop_live_bytes_rejects());
    assert!(prop_rest_live_bytes());
    assert!(live_bytes_surface_present());
    assert!(
        run_m7_e5_live_bytes_gate(),
        "E5 Stage 23 live-ESP bytes probe must hold"
    );
}
