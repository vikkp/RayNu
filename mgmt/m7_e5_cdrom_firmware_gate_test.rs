use super::{
    firmware_cd_surface_present, prop_firmware_arm_from_host, prop_firmware_arm_rejects,
    prop_rest_firmware_arm, run_m7_e5_cdrom_firmware_gate, E5_CDROM_FIRMWARE_RESIDUAL_NOTE,
    M7_E5_CDROM_FIRMWARE_OK_MARKER,
};

#[test]
fn m7_e5_cdrom_firmware_gate_passes() {
    assert_eq!(
        M7_E5_CDROM_FIRMWARE_OK_MARKER,
        "RAYNU-V-M7-E5-CDROM-FIRMWARE-OK"
    );
    assert!(E5_CDROM_FIRMWARE_RESIDUAL_NOTE.contains("UnsupportedOnFirmware"));
    assert!(prop_firmware_arm_from_host());
    assert!(prop_firmware_arm_rejects());
    assert!(prop_rest_firmware_arm());
    assert!(firmware_cd_surface_present());
    assert!(
        run_m7_e5_cdrom_firmware_gate(),
        "E5 Stage 2 firmware CD attach must hold"
    );
}
