use super::{iso_upload_surface_present, run_m8_iso_upload_host_gate, M8_ISO_UPLOAD_GATE_MARKER};
use crate::mgmt::iso_upload::{
    firmware_upload_is_esp_staged, host_never_prints_iron_iso_upload_ok,
    M8_ISO_UPLOAD_HOST_OK_MARKER, M8_ISO_UPLOAD_OK_MARKER,
};

#[test]
fn m8_iso_upload_host_gate_passes() {
    assert_eq!(M8_ISO_UPLOAD_GATE_MARKER, "RAYNU-V-M8-ISO-UPLOAD-HOST-OK");
    assert_eq!(M8_ISO_UPLOAD_OK_MARKER, "RAYNU-V-M8-ISO-UPLOAD-OK");
    assert_ne!(M8_ISO_UPLOAD_HOST_OK_MARKER, M8_ISO_UPLOAD_OK_MARKER);
    assert!(firmware_upload_is_esp_staged());
    assert!(host_never_prints_iron_iso_upload_ok());
    assert!(iso_upload_surface_present());
    assert!(
        run_m8_iso_upload_host_gate(),
        "M8.4 host ISO upload package must hold (not iron ISO-UPLOAD-OK)"
    );
    println!("{M8_ISO_UPLOAD_GATE_MARKER}");
}
