use super::{ovmf_cdrom_surface_present, run_m7_e5_ovmf_cdrom_gate, M7_E5_OVMF_CDROM_GATE_MARKER};

#[test]
fn m7_e5_ovmf_cdrom_gate_passes() {
    assert_eq!(M7_E5_OVMF_CDROM_GATE_MARKER, "RAYNU-V-M7-E5-OVMF-CDROM-OK");
    assert!(ovmf_cdrom_surface_present());
    assert!(
        run_m7_e5_ovmf_cdrom_gate(),
        "E5 Stage 40 OVMF-CDROM gate must hold"
    );
}
