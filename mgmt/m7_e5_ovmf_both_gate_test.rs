use super::{ovmf_both_surface_present, run_m7_e5_ovmf_both_gate, M7_E5_OVMF_BOTH_GATE_MARKER};

#[test]
fn m7_e5_ovmf_both_gate_passes() {
    assert_eq!(M7_E5_OVMF_BOTH_GATE_MARKER, "RAYNU-V-M7-E5-OVMF-BOTH-OK");
    assert!(ovmf_both_surface_present());
    assert!(
        run_m7_e5_ovmf_both_gate(),
        "E5 Stage 43 OVMF-BOTH gate must hold"
    );
}
