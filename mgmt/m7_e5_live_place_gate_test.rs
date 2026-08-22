use super::{
    live_place_surface_present, prop_live_place_after_live_copy, prop_live_place_rejects,
    prop_rest_live_place, run_m7_e5_live_place_gate, E5_LIVE_PLACE_RESIDUAL_NOTE,
    M7_E5_LIVE_PLACE_OK_MARKER,
};

#[test]
fn m7_e5_live_place_gate_passes() {
    assert_eq!(M7_E5_LIVE_PLACE_OK_MARKER, "RAYNU-V-M7-E5-LIVE-PLACE-OK");
    assert!(E5_LIVE_PLACE_RESIDUAL_NOTE.contains("UnsupportedOnFirmware"));
    assert!(E5_LIVE_PLACE_RESIDUAL_NOTE.contains("not a shipped OVMF.fd"));
    assert!(E5_LIVE_PLACE_RESIDUAL_NOTE.contains("VMLAUNCH insn not issued"));
    assert!(E5_LIVE_PLACE_RESIDUAL_NOTE.contains("live EPT is not written"));
    assert!(prop_live_place_after_live_copy());
    assert!(prop_live_place_rejects());
    assert!(prop_rest_live_place());
    assert!(live_place_surface_present());
    assert!(
        run_m7_e5_live_place_gate(),
        "E5 Stage 29 live-ESP place must hold"
    );
}
