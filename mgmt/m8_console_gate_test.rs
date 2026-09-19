use super::{console_surface_present, run_m8_console_host_gate, M8_CONSOLE_GATE_MARKER};
use crate::mgmt::console::{
    firmware_console_is_serial_log_only, firmware_console_serves_spa_keys,
    host_never_prints_iron_console_ok, M8_CONSOLE_HOST_OK_MARKER, M8_CONSOLE_OK_MARKER,
};

#[test]
fn m8_console_host_gate_passes() {
    assert_eq!(M8_CONSOLE_GATE_MARKER, "RAYNU-V-M8-CONSOLE-HOST-OK");
    assert_eq!(M8_CONSOLE_OK_MARKER, "RAYNU-V-M8-CONSOLE-OK");
    assert_ne!(M8_CONSOLE_HOST_OK_MARKER, M8_CONSOLE_OK_MARKER);
    assert!(firmware_console_serves_spa_keys());
    assert!(!firmware_console_is_serial_log_only());
    assert!(host_never_prints_iron_console_ok());
    assert!(console_surface_present());
    assert!(
        run_m8_console_host_gate(),
        "M8.3 SPA keys package must hold (not iron CONSOLE-OK)"
    );
    println!("{M8_CONSOLE_GATE_MARKER}");
}
