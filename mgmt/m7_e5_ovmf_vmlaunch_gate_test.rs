use super::{
    ovmf_vmlaunch_surface_present, prop_host_retain_does_not_vmlaunch,
    run_m7_e5_ovmf_vmlaunch_gate, M7_E5_OVMF_VMLAUNCH_GATE_MARKER,
};
use crate::vmx::guest_uefi::E5_OVMF_VMLAUNCH_RESIDUAL_NOTE;

#[test]
fn m7_e5_ovmf_vmlaunch_gate_passes() {
    assert_eq!(
        M7_E5_OVMF_VMLAUNCH_GATE_MARKER,
        "RAYNU-V-M7-E5-OVMF-VMLAUNCH-OK"
    );
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("not ISO-INSTALL-OK"));
    assert!(prop_host_retain_does_not_vmlaunch());
    assert!(ovmf_vmlaunch_surface_present());
    assert!(
        run_m7_e5_ovmf_vmlaunch_gate(),
        "E5 Stage 37 private guest-UEFI VMLAUNCH gate must hold"
    );
}
