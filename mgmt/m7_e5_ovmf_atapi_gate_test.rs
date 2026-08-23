use super::{ovmf_atapi_surface_present, run_m7_e5_ovmf_atapi_gate, M7_E5_OVMF_ATAPI_GATE_MARKER};

#[test]
fn m7_e5_ovmf_atapi_gate_passes() {
    assert_eq!(M7_E5_OVMF_ATAPI_GATE_MARKER, "RAYNU-V-M7-E5-OVMF-ATAPI-OK");
    assert!(ovmf_atapi_surface_present());
    assert!(
        run_m7_e5_ovmf_atapi_gate(),
        "E5 Stage 44 OVMF-ATAPI gate must hold"
    );
}
