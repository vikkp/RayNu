//! M8.3 guest console (outside Proven Core).
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** (ADR-002 / ADR-018). Do not touch VMX/EPT.
//! VERIFICATION: L1 host tests (operator keys → guest COM1 RBR; guest THR echo).
//!
//! Firmware SPA still shows [`GET /logs/serial`](crate::mgmt::http) — HV UART,
//! not a guest keyboard. That is **not** the M8.3 product close. This module
//! is the host-first slice: [`ConsoleMode::HostReady`] injects keystrokes
//! through the same COM1 RX path iDRAC SOL / serial auto-answer use.
//!
//! Iron close marker [`M8_CONSOLE_OK_MARKER`] is operator typing in the
//! guest from the SPA (or equivalent) after `BOOT-OK`. Host/CI print
//! [`M8_CONSOLE_HOST_OK_MARKER`]. Nested QEMU ≠ R640. Do not flash. Not VNC.

use crate::devices::guest_uart::{pio, push_host_rx, reset as uart_reset};

/// Iron COM2 / operator LAN close: type in the guest from the SPA.
/// Host/CI/nested must **never** print this.
pub const M8_CONSOLE_OK_MARKER: &str = "RAYNU-V-M8-CONSOLE-OK";

/// Host/CI: operator keys reach guest COM1; guest echo is captured. Not iron.
pub const M8_CONSOLE_HOST_OK_MARKER: &str = "RAYNU-V-M8-CONSOLE-HOST-OK";

/// Honesty: host UART round-trip ≠ iron SPA keyboard / VNC.
pub const CONSOLE_HOST_RESIDUAL_NOTE: &str =
    "residual: HostReady UART keys are not iron RAYNU-V-M8-CONSOLE-OK; firmware SPA is still host serial log (not guest console); not VNC; nested QEMU ≠ R640; do not print iron CONSOLE-OK from host/CI";

/// Firmware listen today: GET /logs/serial is HV UART, not a guest keyboard.
pub const CONSOLE_FIRMWARE_SERIAL_LOG_NOTE: &str =
    "firmware SPA GET /logs/serial is HV UART (M8.3 host-ready; iron guest keyboard not claimed; not VNC)";

/// How the operator talks to the guest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsoleMode {
    /// Iron / firmware: host serial log only (no SPA keyboard).
    SerialLogOnly,
    /// Host/`cfg(test)`: keystrokes into guest COM1 RX; THR echo captured.
    HostReady,
}

/// Product firmware console today. Host tests use [`ConsoleMode::HostReady`].
pub const FIRMWARE_CONSOLE_MODE: ConsoleMode = ConsoleMode::SerialLogOnly;

const CONSOLE_TX_CAP: usize = 256;
static mut CONSOLE_TX: [u8; CONSOLE_TX_CAP] = [0; CONSOLE_TX_CAP];
static mut CONSOLE_TX_LEN: usize = 0;

/// True until coexist serves a guest keyboard after `BOOT-OK`.
pub fn firmware_console_is_serial_log_only() -> bool {
    matches!(FIRMWARE_CONSOLE_MODE, ConsoleMode::SerialLogOnly)
        && CONSOLE_FIRMWARE_SERIAL_LOG_NOTE.contains("HV UART")
        && CONSOLE_FIRMWARE_SERIAL_LOG_NOTE.contains("not VNC")
}

/// Host/CI must never print the iron console marker.
pub fn host_never_prints_iron_console_ok() -> bool {
    M8_CONSOLE_OK_MARKER == "RAYNU-V-M8-CONSOLE-OK"
        && M8_CONSOLE_HOST_OK_MARKER == "RAYNU-V-M8-CONSOLE-HOST-OK"
        && M8_CONSOLE_HOST_OK_MARKER != M8_CONSOLE_OK_MARKER
        && M8_CONSOLE_OK_MARKER != crate::mgmt::tls::M8_TLS_OK_MARKER
        && M8_CONSOLE_OK_MARKER != crate::mgmt::auth::M8_AUTH_OK_MARKER
}

/// Clear the HostReady guest-echo ring (tests).
pub fn console_tx_clear() {
    unsafe {
        CONSOLE_TX = [0; CONSOLE_TX_CAP];
        CONSOLE_TX_LEN = 0;
    }
}

/// Record one guest THR byte into the HostReady echo ring.
pub fn console_tx_note(b: u8) {
    unsafe {
        if CONSOLE_TX_LEN < CONSOLE_TX_CAP {
            CONSOLE_TX[CONSOLE_TX_LEN] = b;
            CONSOLE_TX_LEN += 1;
        }
    }
}

/// Copy the HostReady echo ring (oldest → newest).
pub fn console_tx_snapshot(out: &mut [u8]) -> usize {
    unsafe {
        let n = CONSOLE_TX_LEN.min(out.len());
        out[..n].copy_from_slice(&CONSOLE_TX[..n]);
        n
    }
}

/// Inject operator keystrokes into guest COM1 RBR. SerialLogOnly is a no-op.
pub fn inject_operator_keys(mode: ConsoleMode, keys: &[u8]) -> usize {
    match mode {
        ConsoleMode::SerialLogOnly => 0,
        ConsoleMode::HostReady => {
            let mut n = 0;
            for &b in keys {
                if !push_host_rx(b) {
                    break;
                }
                n += 1;
            }
            n
        }
    }
}

/// Host package: HostReady keys reach RBR; guest THR echo is captured; firmware log-only.
pub fn prop_console_host_package() -> bool {
    crate::devices::guest_irq::reset();
    uart_reset();
    console_tx_clear();
    let plan = include_str!("../docs/m8_plan.md");
    let html = include_str!("../assets/webui.html");
    let http = include_str!("http.rs");
    let serial_log_only_refuses = inject_operator_keys(ConsoleMode::SerialLogOnly, b"x") == 0;
    let injected = inject_operator_keys(ConsoleMode::HostReady, b"hi") == 2;
    let (h, _, _) = pio(0x03F8, true, 0);
    let (i, _, _) = pio(0x03F8, true, 0);
    let rbr_ok = h == b'h' && i == b'i';
    let (_, thr_o, _) = pio(0x03F8, false, b'o');
    if let Some(b) = thr_o {
        console_tx_note(b);
    }
    let (_, thr_k, _) = pio(0x03F8, false, b'k');
    if let Some(b) = thr_k {
        console_tx_note(b);
    }
    let mut echo = [0u8; 8];
    let n = console_tx_snapshot(&mut echo);
    let echo_ok = n == 2 && echo[0] == b'o' && echo[1] == b'k';
    uart_reset();
    crate::devices::guest_irq::reset();
    console_tx_clear();
    firmware_console_is_serial_log_only()
        && host_never_prints_iron_console_ok()
        && serial_log_only_refuses
        && injected
        && rbr_ok
        && echo_ok
        && CONSOLE_HOST_RESIDUAL_NOTE.contains("not iron")
        && CONSOLE_HOST_RESIDUAL_NOTE.contains("not VNC")
        && plan.contains("M8.3")
        && plan.contains("not VNC")
        && html.contains("not guest console")
        && http.contains("Not a guest console")
        && !http.contains("/console/keys")
}

#[cfg(test)]
#[path = "console_test.rs"]
mod console_test;
