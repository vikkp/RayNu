use super::{
    live_exec_surface_present, prop_live_exec_after_arm, prop_live_exec_rejects,
    prop_rest_live_exec, run_m7_e5_live_exec_gate, E5_LIVE_EXEC_RESIDUAL_NOTE,
    M7_E5_LIVE_EXEC_OK_MARKER,
};

#[test]
fn m7_e5_live_exec_gate_passes() {
    assert_eq!(M7_E5_LIVE_EXEC_OK_MARKER, "RAYNU-V-M7-E5-LIVE-EXEC-OK");
    assert!(E5_LIVE_EXEC_RESIDUAL_NOTE.contains("UnsupportedOnFirmware"));
    assert!(E5_LIVE_EXEC_RESIDUAL_NOTE.contains("not a shipped OVMF.fd"));
    assert!(E5_LIVE_EXEC_RESIDUAL_NOTE.contains("VMLAUNCH insn not issued"));
    assert!(E5_LIVE_EXEC_RESIDUAL_NOTE.contains("live EPT is not written"));
    assert!(prop_live_exec_after_arm());
    assert!(prop_live_exec_rejects());
    assert!(prop_rest_live_exec());
    assert!(live_exec_surface_present());
    assert!(
        run_m7_e5_live_exec_gate(),
        "E5 Stage 20 live-ESP VMLAUNCH execute gate must hold"
    );
}
