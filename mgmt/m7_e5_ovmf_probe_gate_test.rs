use super::{
    ovmf_probe_surface_present, prop_ovmf_probe_after_load, prop_ovmf_probe_rejects,
    prop_rest_ovmf_probe, run_m7_e5_ovmf_probe_gate, E5_OVMF_PROBE_RESIDUAL_NOTE,
    M7_E5_OVMF_PROBE_OK_MARKER,
};

#[test]
fn m7_e5_ovmf_probe_gate_passes() {
    assert_eq!(M7_E5_OVMF_PROBE_OK_MARKER, "RAYNU-V-M7-E5-OVMF-PROBE-OK");
    assert!(E5_OVMF_PROBE_RESIDUAL_NOTE.contains("UnsupportedOnFirmware"));
    assert!(prop_ovmf_probe_after_load());
    assert!(prop_ovmf_probe_rejects());
    assert!(prop_rest_ovmf_probe());
    assert!(ovmf_probe_surface_present());
    assert!(
        run_m7_e5_ovmf_probe_gate(),
        "E5 Stage 5 OVMF FV probe must hold"
    );
}
