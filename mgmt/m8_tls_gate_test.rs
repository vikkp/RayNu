use super::{run_m8_tls_host_gate, tls_surface_present, M8_TLS_GATE_MARKER};
use crate::mgmt::tls::{
    firmware_listen_is_plaintext, host_never_prints_iron_tls_ok, M8_TLS_HOST_OK_MARKER,
    M8_TLS_OK_MARKER,
};

#[test]
fn m8_tls_host_gate_passes() {
    assert_eq!(M8_TLS_GATE_MARKER, "RAYNU-V-M8-TLS-HOST-OK");
    assert_eq!(M8_TLS_OK_MARKER, "RAYNU-V-M8-TLS-OK");
    assert_ne!(M8_TLS_HOST_OK_MARKER, M8_TLS_OK_MARKER);
    assert!(firmware_listen_is_plaintext());
    assert!(host_never_prints_iron_tls_ok());
    assert!(tls_surface_present());
    assert!(
        run_m8_tls_host_gate(),
        "M8.1 host TLS package must hold (not iron TLS-OK)"
    );
    println!("{M8_TLS_GATE_MARKER}");
}
