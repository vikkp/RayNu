use super::{
    ovmf_virtio_surface_present, run_m7_e5_ovmf_virtio_gate, M7_E5_OVMF_VIRTIO_GATE_MARKER,
};

#[test]
fn m7_e5_ovmf_virtio_gate_passes() {
    assert_eq!(
        M7_E5_OVMF_VIRTIO_GATE_MARKER,
        "RAYNU-V-M7-E5-OVMF-VIRTIO-OK"
    );
    assert!(ovmf_virtio_surface_present());
    assert!(
        run_m7_e5_ovmf_virtio_gate(),
        "E5 Stage 42 OVMF-VIRTIO gate must hold"
    );
}
