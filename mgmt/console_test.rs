//! M8.3 host console: operator keys reach guest COM1. Not iron. Not VNC.

use super::{
    firmware_console_is_serial_log_only, host_never_prints_iron_console_ok,
    inject_operator_keys, prop_console_host_package, ConsoleMode, M8_CONSOLE_HOST_OK_MARKER,
    M8_CONSOLE_OK_MARKER, CONSOLE_FIRMWARE_SERIAL_LOG_NOTE, CONSOLE_HOST_RESIDUAL_NOTE,
};
use crate::mgmt::datastore::ImageTable;
use crate::mgmt::http::handle_http_request;
use crate::mgmt::iso::IsoDeployPlan;
use crate::mgmt::iso_install::InstallToDiskPlan;
use crate::mgmt::VmTable;

#[test]
fn firmware_console_stays_serial_log_only() {
    assert_eq!(super::FIRMWARE_CONSOLE_MODE, ConsoleMode::SerialLogOnly);
    assert!(firmware_console_is_serial_log_only());
    assert!(CONSOLE_FIRMWARE_SERIAL_LOG_NOTE.contains("HV UART"));
    assert!(CONSOLE_FIRMWARE_SERIAL_LOG_NOTE.contains("not VNC"));
    assert_eq!(M8_CONSOLE_OK_MARKER, "RAYNU-V-M8-CONSOLE-OK");
    assert_eq!(M8_CONSOLE_HOST_OK_MARKER, "RAYNU-V-M8-CONSOLE-HOST-OK");
    assert_ne!(M8_CONSOLE_OK_MARKER, M8_CONSOLE_HOST_OK_MARKER);
    assert!(host_never_prints_iron_console_ok());
    assert_eq!(inject_operator_keys(ConsoleMode::SerialLogOnly, b"x"), 0);
    assert!(prop_console_host_package());
}

#[test]
fn host_ready_keys_reach_guest_com1_and_capture_echo() {
    assert!(prop_console_host_package());

    let mut table = VmTable::new();
    let mut images = ImageTable::new();
    let mut iso_plan = IsoDeployPlan::empty();
    let mut iso_install = InstallToDiskPlan::empty();
    let mut out = [0u8; 16384];
    let n = handle_http_request(
        &mut table,
        &mut images,
        &mut iso_plan,
        &mut iso_install,
        "GET / HTTP/1.1\r\nHost: localhost\r\n\r\n",
        &mut out,
    )
    .unwrap_or(0);
    let spa = core::str::from_utf8(&out[..n]).unwrap_or("");
    assert!(spa.contains("HTTP/1.1 200"), "{spa}");
    assert!(spa.contains("data-go=\"overview\""), "{spa}");
    assert!(spa.contains("not guest console"), "{spa}");
    assert!(!spa.contains("/console/keys"), "{spa}");
    assert!(CONSOLE_HOST_RESIDUAL_NOTE.contains("not iron"));
    assert!(firmware_console_is_serial_log_only());
    println!("{M8_CONSOLE_HOST_OK_MARKER}");
}
