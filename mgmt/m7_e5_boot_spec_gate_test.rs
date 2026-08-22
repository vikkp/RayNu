use super::{
    boot_spec_surface_present, e4_shell_launch_unchanged, prop_e4_shell_iso_zero,
    prop_el_torito_parse_not_attach, prop_product_iso_default, prop_rest_boot_spec,
    run_m7_e5_boot_spec_gate, E5_BOOT_SPEC_RESIDUAL_NOTE, M7_E5_BOOT_SPEC_OK_MARKER,
};

#[test]
fn m7_e5_boot_spec_gate_passes() {
    assert_eq!(M7_E5_BOOT_SPEC_OK_MARKER, "RAYNU-V-M7-E5-BOOT-SPEC-OK");
    assert!(E5_BOOT_SPEC_RESIDUAL_NOTE.contains("UnsupportedOnFirmware"));
    assert!(prop_rest_boot_spec());
    assert!(prop_e4_shell_iso_zero());
    assert!(prop_product_iso_default());
    assert!(prop_el_torito_parse_not_attach());
    assert!(boot_spec_surface_present());
    assert!(e4_shell_launch_unchanged());
    assert!(
        run_m7_e5_boot_spec_gate(),
        "E5 Stage 0 boot spec on the wire must hold"
    );
}
