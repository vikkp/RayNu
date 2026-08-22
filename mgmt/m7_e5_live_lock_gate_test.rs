use super::{
    live_lock_surface_present, prop_live_lock_after_live_seal, prop_live_lock_rejects,
    prop_rest_live_lock, run_m7_e5_live_lock_gate, E5_LIVE_LOCK_RESIDUAL_NOTE,
    M7_E5_LIVE_LOCK_OK_MARKER,
};

#[test]
fn m7_e5_live_lock_gate_passes() {
    assert_eq!(M7_E5_LIVE_LOCK_OK_MARKER, "RAYNU-V-M7-E5-LIVE-LOCK-OK");
    assert!(E5_LIVE_LOCK_RESIDUAL_NOTE.contains("UnsupportedOnFirmware"));
    assert!(E5_LIVE_LOCK_RESIDUAL_NOTE.contains("not a shipped OVMF.fd"));
    assert!(E5_LIVE_LOCK_RESIDUAL_NOTE.contains("VMLAUNCH insn not issued"));
    assert!(E5_LIVE_LOCK_RESIDUAL_NOTE.contains("live EPT is not written"));
    assert!(prop_live_lock_after_live_seal());
    assert!(prop_live_lock_rejects());
    assert!(prop_rest_live_lock());
    assert!(live_lock_surface_present());
    assert!(
        run_m7_e5_live_lock_gate(),
        "E5 Stage 34 live-ESP lock must hold"
    );
}
