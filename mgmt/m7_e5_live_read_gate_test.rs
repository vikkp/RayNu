use super::{
    live_read_surface_present, prop_live_read_after_live_admit, prop_live_read_rejects,
    prop_rest_live_read, run_m7_e5_live_read_gate, E5_LIVE_READ_RESIDUAL_NOTE,
    M7_E5_LIVE_READ_OK_MARKER,
};

#[test]
fn m7_e5_live_read_gate_passes() {
    assert_eq!(M7_E5_LIVE_READ_OK_MARKER, "RAYNU-V-M7-E5-LIVE-READ-OK");
    assert!(E5_LIVE_READ_RESIDUAL_NOTE.contains("UnsupportedOnFirmware"));
    assert!(E5_LIVE_READ_RESIDUAL_NOTE.contains("not a shipped OVMF.fd"));
    assert!(E5_LIVE_READ_RESIDUAL_NOTE.contains("VMLAUNCH insn not issued"));
    assert!(E5_LIVE_READ_RESIDUAL_NOTE.contains("live EPT is not written"));
    assert!(prop_live_read_after_live_admit());
    assert!(prop_live_read_rejects());
    assert!(prop_rest_live_read());
    assert!(live_read_surface_present());
    assert!(
        run_m7_e5_live_read_gate(),
        "E5 Stage 27 live-ESP read must hold"
    );
}
