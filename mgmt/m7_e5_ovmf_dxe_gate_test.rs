use super::{ovmf_dxe_surface_present, run_m7_e5_ovmf_dxe_gate, M7_E5_OVMF_DXE_GATE_MARKER};

#[test]
fn m7_e5_ovmf_dxe_gate_passes() {
    assert_eq!(M7_E5_OVMF_DXE_GATE_MARKER, "RAYNU-V-M7-E5-OVMF-DXE-OK");
    assert!(ovmf_dxe_surface_present());
    assert!(
        run_m7_e5_ovmf_dxe_gate(),
        "E5 Stage 41 OVMF-DXE gate must hold"
    );
}
