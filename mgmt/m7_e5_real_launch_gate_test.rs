use super::{
    prop_real_launch_after_qualify, prop_real_launch_rejects, prop_rest_real_launch,
    real_launch_surface_present, run_m7_e5_real_launch_gate, E5_REAL_LAUNCH_RESIDUAL_NOTE,
    M7_E5_REAL_LAUNCH_OK_MARKER,
};

#[test]
fn m7_e5_real_launch_gate_passes() {
    assert_eq!(M7_E5_REAL_LAUNCH_OK_MARKER, "RAYNU-V-M7-E5-REAL-LAUNCH-OK");
    assert!(E5_REAL_LAUNCH_RESIDUAL_NOTE.contains("UnsupportedOnFirmware"));
    assert!(E5_REAL_LAUNCH_RESIDUAL_NOTE.contains("not a shipped OVMF.fd"));
    assert!(E5_REAL_LAUNCH_RESIDUAL_NOTE.contains("VMLAUNCH insn not issued"));
    assert!(E5_REAL_LAUNCH_RESIDUAL_NOTE.contains("live EPT is not written"));
    assert!(prop_real_launch_after_qualify());
    assert!(prop_real_launch_rejects());
    assert!(prop_rest_real_launch());
    assert!(real_launch_surface_present());
    assert!(
        run_m7_e5_real_launch_gate(),
        "E5 Stage 19 guest-UEFI VMLAUNCH insn-path arm must hold"
    );
}
