use super::{ovmf_alive_surface_present, run_m7_e5_ovmf_alive_gate, M7_E5_OVMF_ALIVE_GATE_MARKER};

#[test]
fn m7_e5_ovmf_alive_gate_passes() {
    assert_eq!(M7_E5_OVMF_ALIVE_GATE_MARKER, "RAYNU-V-M7-E5-OVMF-ALIVE-OK");
    assert!(ovmf_alive_surface_present());
    assert!(
        run_m7_e5_ovmf_alive_gate(),
        "E5 Stage 38 OVMF-alive (past first triple-fault) gate must hold"
    );
}
