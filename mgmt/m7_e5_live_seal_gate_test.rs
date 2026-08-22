use super::{
    live_seal_surface_present, prop_live_seal_after_live_latch, prop_live_seal_rejects,
    prop_rest_live_seal, run_m7_e5_live_seal_gate, E5_LIVE_SEAL_RESIDUAL_NOTE,
    M7_E5_LIVE_SEAL_OK_MARKER,
};

#[test]
fn m7_e5_live_seal_gate_passes() {
    assert_eq!(M7_E5_LIVE_SEAL_OK_MARKER, "RAYNU-V-M7-E5-LIVE-SEAL-OK");
    assert!(E5_LIVE_SEAL_RESIDUAL_NOTE.contains("UnsupportedOnFirmware"));
    assert!(E5_LIVE_SEAL_RESIDUAL_NOTE.contains("not a shipped OVMF.fd"));
    assert!(E5_LIVE_SEAL_RESIDUAL_NOTE.contains("VMLAUNCH insn not issued"));
    assert!(E5_LIVE_SEAL_RESIDUAL_NOTE.contains("live EPT is not written"));
    assert!(prop_live_seal_after_live_latch());
    assert!(prop_live_seal_rejects());
    assert!(prop_rest_live_seal());
    assert!(live_seal_surface_present());
    assert!(
        run_m7_e5_live_seal_gate(),
        "E5 Stage 33 live-ESP seal must hold"
    );
}
