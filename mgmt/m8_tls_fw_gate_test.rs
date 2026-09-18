use super::{run_m8_tls_fw_host_gate, tls_fw_surface_present, M8_TLS_FW_GATE_MARKER};
use crate::mgmt::tls_coexist::M8_TLS_FW_HOST_OK_MARKER;
use crate::mgmt::tls::{firmware_listen_is_plaintext, M8_TLS_OK_MARKER};

#[test]
fn m8_tls_fw_host_gate_passes() {
    assert_eq!(M8_TLS_FW_GATE_MARKER, "RAYNU-V-M8-TLS-FW-HOST-OK");
    assert_eq!(M8_TLS_OK_MARKER, "RAYNU-V-M8-TLS-OK");
    assert_ne!(M8_TLS_FW_HOST_OK_MARKER, M8_TLS_OK_MARKER);
    assert!(firmware_listen_is_plaintext());
    assert!(tls_fw_surface_present());
    assert!(
        run_m8_tls_fw_host_gate(),
        "M8.1 firmware TLS wrap must hold (plaintext session; not iron TLS-OK)"
    );
    println!("{M8_TLS_FW_GATE_MARKER}");
}
