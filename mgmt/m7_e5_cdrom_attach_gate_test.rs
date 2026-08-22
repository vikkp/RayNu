use super::{
    cdrom_attach_surface_present, e4_shell_launch_no_cdrom, prop_host_attach_mock_efi,
    prop_host_attach_rejects, prop_rest_cdrom_attach, run_m7_e5_cdrom_attach_gate,
    E5_CDROM_ATTACH_RESIDUAL_NOTE, M7_E5_CDROM_ATTACH_OK_MARKER,
};

#[test]
fn m7_e5_cdrom_attach_gate_passes() {
    assert_eq!(
        M7_E5_CDROM_ATTACH_OK_MARKER,
        "RAYNU-V-M7-E5-CDROM-ATTACH-OK"
    );
    assert!(E5_CDROM_ATTACH_RESIDUAL_NOTE.contains("UnsupportedOnFirmware"));
    assert!(prop_host_attach_mock_efi());
    assert!(prop_host_attach_rejects());
    assert!(prop_rest_cdrom_attach());
    assert!(cdrom_attach_surface_present());
    assert!(e4_shell_launch_no_cdrom());
    assert!(
        run_m7_e5_cdrom_attach_gate(),
        "E5 Stage 1 host CD-ROM attach must hold"
    );
}
