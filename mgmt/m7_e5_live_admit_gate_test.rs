use super::{
    live_admit_surface_present, prop_live_admit_after_live_present, prop_live_admit_rejects,
    prop_rest_live_admit, run_m7_e5_live_admit_gate, E5_LIVE_ADMIT_RESIDUAL_NOTE,
    M7_E5_LIVE_ADMIT_OK_MARKER,
};

#[test]
fn m7_e5_live_admit_gate_passes() {
    assert_eq!(M7_E5_LIVE_ADMIT_OK_MARKER, "RAYNU-V-M7-E5-LIVE-ADMIT-OK");
    assert!(E5_LIVE_ADMIT_RESIDUAL_NOTE.contains("UnsupportedOnFirmware"));
    assert!(E5_LIVE_ADMIT_RESIDUAL_NOTE.contains("not a shipped OVMF.fd"));
    assert!(E5_LIVE_ADMIT_RESIDUAL_NOTE.contains("VMLAUNCH insn not issued"));
    assert!(E5_LIVE_ADMIT_RESIDUAL_NOTE.contains("live EPT is not written"));
    assert!(prop_live_admit_after_live_present());
    assert!(prop_live_admit_rejects());
    assert!(prop_rest_live_admit());
    assert!(live_admit_surface_present());
    assert!(
        run_m7_e5_live_admit_gate(),
        "E5 Stage 26 live-ESP admit must hold"
    );
}
