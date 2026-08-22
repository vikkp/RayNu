use super::{
    prop_reset_vec_after_map, prop_reset_vec_rejects, prop_rest_reset_vec, reset_vec_surface_present,
    run_m7_e5_reset_vec_gate, E5_RESET_VEC_RESIDUAL_NOTE, M7_E5_RESET_VEC_OK_MARKER,
};

#[test]
fn m7_e5_reset_vec_gate_passes() {
    assert_eq!(M7_E5_RESET_VEC_OK_MARKER, "RAYNU-V-M7-E5-RESET-VEC-OK");
    assert!(E5_RESET_VEC_RESIDUAL_NOTE.contains("UnsupportedOnFirmware"));
    assert!(E5_RESET_VEC_RESIDUAL_NOTE.contains("not a shipped OVMF.fd"));
    assert!(E5_RESET_VEC_RESIDUAL_NOTE.contains("VMLAUNCH insn not issued"));
    assert!(prop_reset_vec_after_map());
    assert!(prop_reset_vec_rejects());
    assert!(prop_rest_reset_vec());
    assert!(reset_vec_surface_present());
    assert!(
        run_m7_e5_reset_vec_gate(),
        "E5 Stage 14 reset-vector VMCS contract must hold"
    );
}
