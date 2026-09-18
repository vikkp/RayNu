//! M8.3 host console gate (outside Proven Core).
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** (ADR-018)
//!
//! Proves HostReady keystrokes reach guest COM1, firmware SPA is still the
//! host serial log, and the iron marker is never printed from host/CI.
//! Nested QEMU is not this gate. Iron SPA keyboard / VNC after `BOOT-OK`
//! is not this gate.

use crate::mgmt::console::{
    firmware_console_is_serial_log_only, host_never_prints_iron_console_ok, prop_console_host_package,
    CONSOLE_FIRMWARE_SERIAL_LOG_NOTE, CONSOLE_HOST_RESIDUAL_NOTE, M8_CONSOLE_HOST_OK_MARKER,
    M8_CONSOLE_OK_MARKER,
};

/// Host / CI marker when the M8.3 console package passes.
pub const M8_CONSOLE_GATE_MARKER: &str = M8_CONSOLE_HOST_OK_MARKER;

/// True when plan, markers, serial-log firmware, and HostReady UART hold.
pub fn console_surface_present() -> bool {
    let console = include_str!("console.rs");
    let plan = include_str!("../docs/m8_plan.md");
    let smoke = include_str!("../tools/m8-console-smoke.sh");
    let runbook = include_str!("../docs/runbooks/ops_ui.md");
    let forbidden = concat!("println!(", "\"RAYNU-V-M8-CONSOLE-OK\")");
    console.contains("enum ConsoleMode")
        && console.contains("SerialLogOnly")
        && console.contains("HostReady")
        && console.contains("fn firmware_console_is_serial_log_only(")
        && console.contains("fn host_never_prints_iron_console_ok(")
        && console.contains("fn prop_console_host_package(")
        && console.contains("fn inject_operator_keys(")
        && console.contains(M8_CONSOLE_OK_MARKER)
        && console.contains(M8_CONSOLE_HOST_OK_MARKER)
        && !console.contains(forbidden)
        && !include_str!("http.rs").contains(forbidden)
        && include_str!("http.rs").contains("Not a guest console")
        && !include_str!("http.rs").contains("/console/keys")
        && include_str!("../assets/webui.html").contains("not guest console")
        && CONSOLE_HOST_RESIDUAL_NOTE.contains("not iron")
        && CONSOLE_HOST_RESIDUAL_NOTE.contains("not VNC")
        && CONSOLE_FIRMWARE_SERIAL_LOG_NOTE.contains("HV UART")
        && plan.contains("M8.3")
        && plan.contains("not VNC")
        && smoke.contains(M8_CONSOLE_HOST_OK_MARKER)
        && smoke.contains("m8_console_host_gate_passes")
        && runbook.contains("M8.3")
        && runbook.contains("not VNC")
}

/// Full M8.3 host artifact gate (not iron SPA keyboard).
pub fn run_m8_console_host_gate() -> bool {
    console_surface_present()
        && prop_console_host_package()
        && firmware_console_is_serial_log_only()
        && host_never_prints_iron_console_ok()
        && M8_CONSOLE_OK_MARKER == "RAYNU-V-M8-CONSOLE-OK"
        && M8_CONSOLE_GATE_MARKER == "RAYNU-V-M8-CONSOLE-HOST-OK"
}

#[cfg(test)]
#[path = "m8_console_gate_test.rs"]
mod m8_console_gate_test;
