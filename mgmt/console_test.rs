//! M8.3 console: SPA POST /console/keys → guest COM1. Not iron. Not VNC.

use super::{
    console_ok_clear_printed, firmware_console_is_serial_log_only, firmware_console_serves_spa_keys,
    host_never_prints_iron_console_ok, inject_operator_keys, maybe_print_iron_console_ok,
    prop_console_host_package, take_spa_keys_injected, ConsoleMode, M8_CONSOLE_HOST_OK_MARKER,
    M8_CONSOLE_OK_MARKER, CONSOLE_FIRMWARE_SPA_KEYS_NOTE, CONSOLE_HOST_RESIDUAL_NOTE,
};
use crate::devices::guest_uart::{pio, reset as uart_reset};
use crate::mgmt::datastore::ImageTable;
use crate::mgmt::http::{handle_http_request, HTTP_RESPONSE_CAP};
use crate::mgmt::iso::IsoDeployPlan;
use crate::mgmt::iso_install::InstallToDiskPlan;
use crate::mgmt::VmTable;

#[test]
fn firmware_console_serves_spa_keys_in_tree() {
    assert_eq!(super::FIRMWARE_CONSOLE_MODE, ConsoleMode::FirmwareSpaKeys);
    assert!(firmware_console_serves_spa_keys());
    assert!(!firmware_console_is_serial_log_only());
    assert!(CONSOLE_FIRMWARE_SPA_KEYS_NOTE.contains("/console/keys"));
    assert!(CONSOLE_FIRMWARE_SPA_KEYS_NOTE.contains("not VNC"));
    assert_eq!(M8_CONSOLE_OK_MARKER, "RAYNU-V-M8-CONSOLE-OK");
    assert_eq!(M8_CONSOLE_HOST_OK_MARKER, "RAYNU-V-M8-CONSOLE-HOST-OK");
    assert_ne!(M8_CONSOLE_OK_MARKER, M8_CONSOLE_HOST_OK_MARKER);
    assert!(host_never_prints_iron_console_ok());
    assert_eq!(inject_operator_keys(ConsoleMode::SerialLogOnly, b"x"), 0);
    assert!(prop_console_host_package());
    console_ok_clear_printed();
    assert!(maybe_print_iron_console_ok(true, true));
    assert!(!maybe_print_iron_console_ok(true, true));
    console_ok_clear_printed();
    assert!(!maybe_print_iron_console_ok(true, false));
}

#[test]
fn spa_post_console_keys_reach_guest_com1() {
    assert!(prop_console_host_package());
    uart_reset();
    crate::devices::guest_irq::reset();
    let _ = take_spa_keys_injected();

    let mut table = VmTable::new();
    let mut images = ImageTable::new();
    let mut iso_plan = IsoDeployPlan::empty();
    let mut iso_install = InstallToDiskPlan::empty();
    let mut out = [0u8; HTTP_RESPONSE_CAP];
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
    assert!(spa.contains("/console/keys"), "{spa}");
    assert!(spa.contains("g-keys"), "{spa}");
    assert!(spa.contains("not VNC"), "{spa}");
    assert!(spa.contains("Host serial log"), "{spa}");
    assert!(CONSOLE_HOST_RESIDUAL_NOTE.contains("not iron"));
    assert!(firmware_console_serves_spa_keys());

    let n = handle_http_request(
        &mut table,
        &mut images,
        &mut iso_plan,
        &mut iso_install,
        "POST /console/keys HTTP/1.1\r\nAuthorization: Bearer raynu-v-bringup\r\nContent-Length: 2\r\n\r\nhi",
        &mut out,
    )
    .unwrap_or(0);
    let resp = core::str::from_utf8(&out[..n]).unwrap_or("");
    assert!(resp.contains("HTTP/1.1 200"), "{resp}");
    assert!(resp.contains("{\"ok\":true}"), "{resp}");
    assert!(take_spa_keys_injected());
    let (h, _, _) = pio(0x03F8, true, 0);
    let (i, _, _) = pio(0x03F8, true, 0);
    assert_eq!(h, b'h');
    assert_eq!(i, b'i');

    let n = handle_http_request(
        &mut table,
        &mut images,
        &mut iso_plan,
        &mut iso_install,
        "POST /console/keys HTTP/1.1\r\n\r\nx",
        &mut out,
    )
    .unwrap_or(0);
    let unauth = core::str::from_utf8(&out[..n]).unwrap_or("");
    assert!(unauth.contains("HTTP/1.1 401"), "{unauth}");

    crate::boot::serial::spa_guest_log_clear();
    crate::boot::serial::spa_guest_log_push_for_test(b'l');
    crate::boot::serial::spa_guest_log_push_for_test(b'o');
    let n = handle_http_request(
        &mut table,
        &mut images,
        &mut iso_plan,
        &mut iso_install,
        "GET /logs/guest HTTP/1.1\r\nAuthorization: Bearer raynu-v-bringup\r\n\r\n",
        &mut out,
    )
    .unwrap_or(0);
    let guest_log = core::str::from_utf8(&out[..n]).unwrap_or("");
    assert!(guest_log.contains("HTTP/1.1 200"), "{guest_log}");
    assert!(guest_log.contains("lo"), "{guest_log}");
    crate::boot::serial::spa_guest_log_clear();

    uart_reset();
    crate::devices::guest_irq::reset();
    println!("{M8_CONSOLE_HOST_OK_MARKER}");
}
