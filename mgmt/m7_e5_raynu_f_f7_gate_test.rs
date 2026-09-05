use super::{
    raynu_f_f7_surface_present, run_m7_e5_raynu_f_f7_gate, E5_RAYNU_F_F7_RESIDUAL_NOTE,
    M7_E5_RAYNU_F_F7_OK_MARKER,
};

#[test]
fn m7_e5_raynu_f_f7_gate_passes() {
    assert_eq!(M7_E5_RAYNU_F_F7_OK_MARKER, "RAYNU-V-M7-E5-RAYNU-F-F7-OK");
    assert!(E5_RAYNU_F_F7_RESIDUAL_NOTE.contains("ISO-INSTALL-OK"));
    assert!(E5_RAYNU_F_F7_RESIDUAL_NOTE.contains("fe4785a"));
    assert!(E5_RAYNU_F_F7_RESIDUAL_NOTE.contains("not claimed"));
    assert!(raynu_f_f7_surface_present());
    assert!(
        run_m7_e5_raynu_f_f7_gate(),
        "F7 reset/disk-ESP surfaces must hold (not ISO-INSTALL-OK)"
    );
}
