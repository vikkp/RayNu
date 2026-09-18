use super::{auth_surface_present, run_m8_auth_host_gate, M8_AUTH_GATE_MARKER};
use crate::mgmt::auth::{
    firmware_auth_is_lab_bringup, host_never_prints_iron_auth_ok, M8_AUTH_HOST_OK_MARKER,
    M8_AUTH_OK_MARKER,
};

#[test]
fn m8_auth_host_gate_passes() {
    assert_eq!(M8_AUTH_GATE_MARKER, "RAYNU-V-M8-AUTH-HOST-OK");
    assert_eq!(M8_AUTH_OK_MARKER, "RAYNU-V-M8-AUTH-OK");
    assert_ne!(M8_AUTH_HOST_OK_MARKER, M8_AUTH_OK_MARKER);
    assert!(firmware_auth_is_lab_bringup());
    assert!(host_never_prints_iron_auth_ok());
    assert!(auth_surface_present());
    assert!(
        run_m8_auth_host_gate(),
        "M8.2 host auth package must hold (not iron AUTH-OK)"
    );
    println!("{M8_AUTH_GATE_MARKER}");
}
