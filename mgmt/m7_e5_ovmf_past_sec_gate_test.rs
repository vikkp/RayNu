use super::{
    ovmf_past_sec_surface_present, run_m7_e5_ovmf_past_sec_gate, M7_E5_OVMF_PAST_SEC_GATE_MARKER,
};

#[test]
fn m7_e5_ovmf_past_sec_gate_passes() {
    assert_eq!(
        M7_E5_OVMF_PAST_SEC_GATE_MARKER,
        "RAYNU-V-M7-E5-OVMF-PAST-SEC-OK"
    );
    assert!(ovmf_past_sec_surface_present());
    assert!(
        run_m7_e5_ovmf_past_sec_gate(),
        "E5 Stage 39 OVMF-past-SEC gate must hold"
    );
}
