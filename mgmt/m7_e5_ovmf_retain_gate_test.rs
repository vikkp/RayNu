use super::{
    ovmf_retain_surface_present, prop_retain_refuses_vmlaunch, prop_retain_rejects_alias_fixture,
    prop_retain_sets_presence, run_m7_e5_ovmf_retain_gate, M7_E5_OVMF_RETAIN_OK_MARKER,
};
use crate::boot::ovmf_esp::E5_OVMF_RETAIN_RESIDUAL_NOTE;

#[test]
fn m7_e5_ovmf_retain_gate_passes() {
    assert_eq!(
        M7_E5_OVMF_RETAIN_OK_MARKER,
        "RAYNU-V-M7-E5-LIVE-BYTES-PRESENT-OK"
    );
    assert!(E5_OVMF_RETAIN_RESIDUAL_NOTE.contains("UnsupportedOnFirmware"));
    assert!(E5_OVMF_RETAIN_RESIDUAL_NOTE.contains("VMLAUNCH insn not issued"));
    assert!(E5_OVMF_RETAIN_RESIDUAL_NOTE.contains("not allocated"));
    assert!(prop_retain_rejects_alias_fixture());
    assert!(prop_retain_sets_presence());
    assert!(prop_retain_refuses_vmlaunch());
    assert!(ovmf_retain_surface_present());
    assert!(
        run_m7_e5_ovmf_retain_gate(),
        "E5 Stage 36 real ESP OVMF retain must hold"
    );
}
