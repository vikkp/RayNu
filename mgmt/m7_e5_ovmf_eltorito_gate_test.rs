use super::{
    ovmf_eltorito_surface_present, run_m7_e5_ovmf_eltorito_gate, M7_E5_OVMF_ELTORITO_GATE_MARKER,
};

#[test]
fn m7_e5_ovmf_eltorito_gate_passes() {
    assert_eq!(
        M7_E5_OVMF_ELTORITO_GATE_MARKER,
        "RAYNU-V-M7-E5-OVMF-ELTORITO-OK"
    );
    assert!(ovmf_eltorito_surface_present());
    assert!(
        run_m7_e5_ovmf_eltorito_gate(),
        "Stage 45 El Torito host package"
    );
}
