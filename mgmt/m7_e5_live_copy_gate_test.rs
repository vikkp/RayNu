use super::{
    live_copy_surface_present, prop_live_copy_after_live_read, prop_live_copy_rejects,
    prop_rest_live_copy, run_m7_e5_live_copy_gate, E5_LIVE_COPY_RESIDUAL_NOTE,
    M7_E5_LIVE_COPY_OK_MARKER,
};

#[test]
fn m7_e5_live_copy_gate_passes() {
    assert_eq!(M7_E5_LIVE_COPY_OK_MARKER, "RAYNU-V-M7-E5-LIVE-COPY-OK");
    assert!(E5_LIVE_COPY_RESIDUAL_NOTE.contains("UnsupportedOnFirmware"));
    assert!(E5_LIVE_COPY_RESIDUAL_NOTE.contains("not a shipped OVMF.fd"));
    assert!(E5_LIVE_COPY_RESIDUAL_NOTE.contains("VMLAUNCH insn not issued"));
    assert!(E5_LIVE_COPY_RESIDUAL_NOTE.contains("live EPT is not written"));
    assert!(prop_live_copy_after_live_read());
    assert!(prop_live_copy_rejects());
    assert!(prop_rest_live_copy());
    assert!(live_copy_surface_present());
    assert!(
        run_m7_e5_live_copy_gate(),
        "E5 Stage 28 live-ESP copy must hold"
    );
}
