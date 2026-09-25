//! M8.3 guest console (outside Proven Core).
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** (ADR-002 / ADR-018). Do not touch VMX/EPT.
//! VERIFICATION: L1 host tests (SPA POST `/console/keys` → guest COM1 RBR;
//! guest THR echo).
//!
//! Firmware SPA [`POST /console/keys`](crate::mgmt::http) injects operator
//! keystrokes through the same COM1 RX path iDRAC SOL / serial auto-answer
//! use. [`GET /logs/serial`](crate::mgmt::http) remains HV UART (not a guest
//! console). This is **not VNC**.
//!
//! Iron close marker [`M8_CONSOLE_OK_MARKER`] is operator typing in Alpine
//! from the SPA after `BOOT-OK` on BCM5720, then COM2. The print uses
//! `write_line_nowait` because Linux earlycon share hushes `write_line`.
//! Host/CI print [`M8_CONSOLE_HOST_OK_MARKER`]. Nested QEMU ≠ R640. Not VNC.

use core::sync::atomic::{AtomicBool, Ordering};

use crate::devices::guest_uart::{pio, push_host_rx, reset as uart_reset};

/// Iron COM2 / operator LAN close: type in the guest from the SPA.
/// Host/CI/nested must **never** print this.
pub const M8_CONSOLE_OK_MARKER: &str = "RAYNU-V-M8-CONSOLE-OK";

/// Host/CI: operator keys reach guest COM1; guest echo is captured. Not iron.
pub const M8_CONSOLE_HOST_OK_MARKER: &str = "RAYNU-V-M8-CONSOLE-HOST-OK";

/// Honesty: firmware SPA keys ≠ iron CONSOLE-OK until COM2 on BCM5720.
pub const CONSOLE_HOST_RESIDUAL_NOTE: &str =
    "residual: firmware SPA POST /console/keys is not iron RAYNU-V-M8-CONSOLE-OK until COM2 on BCM5720; GET /logs/serial is still HV UART (not a guest console); not VNC; nested QEMU ≠ R640; do not print iron CONSOLE-OK from host/CI";

/// Firmware listen: SPA keyboard injects guest COM1. Host serial log stays HV UART.
pub const CONSOLE_FIRMWARE_SPA_KEYS_NOTE: &str =
    "firmware SPA POST /console/keys injects guest COM1; GET /logs/serial is HV UART (not a guest console); not VNC; iron CONSOLE-OK is COM2 after SPA keys on BCM5720";

/// How the operator talks to the guest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsoleMode {
    /// No SPA keyboard (GET /logs/serial only).
    SerialLogOnly,
    /// Firmware coexist: POST /console/keys → guest COM1 RBR.
    FirmwareSpaKeys,
    /// Host/`cfg(test)`: keystrokes into guest COM1 RX; THR echo captured.
    HostReady,
}

/// Product firmware console today. Host tests also use [`ConsoleMode::HostReady`].
pub const FIRMWARE_CONSOLE_MODE: ConsoleMode = ConsoleMode::FirmwareSpaKeys;

const CONSOLE_TX_CAP: usize = 256;
static mut CONSOLE_TX: [u8; CONSOLE_TX_CAP] = [0; CONSOLE_TX_CAP];
static mut CONSOLE_TX_LEN: usize = 0;
static CONSOLE_OK_PRINTED: AtomicBool = AtomicBool::new(false);
static SPA_KEYS_INJECTED: AtomicBool = AtomicBool::new(false);

/// True when firmware still has no SPA keyboard (lab residual).
pub fn firmware_console_is_serial_log_only() -> bool {
    matches!(FIRMWARE_CONSOLE_MODE, ConsoleMode::SerialLogOnly)
}

/// True when firmware SPA POST /console/keys injects guest COM1.
pub fn firmware_console_serves_spa_keys() -> bool {
    matches!(FIRMWARE_CONSOLE_MODE, ConsoleMode::FirmwareSpaKeys)
        && CONSOLE_FIRMWARE_SPA_KEYS_NOTE.contains("/console/keys")
        && CONSOLE_FIRMWARE_SPA_KEYS_NOTE.contains("not VNC")
        && CONSOLE_FIRMWARE_SPA_KEYS_NOTE.contains("HV UART")
}

/// Host/CI must never print the iron console marker.
pub fn host_never_prints_iron_console_ok() -> bool {
    M8_CONSOLE_OK_MARKER == "RAYNU-V-M8-CONSOLE-OK"
        && M8_CONSOLE_HOST_OK_MARKER == "RAYNU-V-M8-CONSOLE-HOST-OK"
        && M8_CONSOLE_HOST_OK_MARKER != M8_CONSOLE_OK_MARKER
        && M8_CONSOLE_OK_MARKER != crate::mgmt::tls::M8_TLS_OK_MARKER
        && M8_CONSOLE_OK_MARKER != crate::mgmt::auth::M8_AUTH_OK_MARKER
}

/// Iron COM2: print [`M8_CONSOLE_OK_MARKER`] once after SPA keys reach guest
/// COM1 on BCM5720 coexist. Host/CI never `println!` the iron string.
pub fn maybe_print_iron_console_ok(keys_ok: bool, bcm5720: bool) -> bool {
    if !(keys_ok && bcm5720) {
        return false;
    }
    if CONSOLE_OK_PRINTED.swap(true, Ordering::AcqRel) {
        return false;
    }
    #[cfg(feature = "uefi-bin")]
    {
        // linux earlycon share drops write_line after Alpine boot. COM2
        // after login only shows write_line_nowait (same path as HTTP keep-alive).
        crate::boot::serial::write_line_nowait(M8_CONSOLE_OK_MARKER);
    }
    true
}

/// Host tests / handoff reset of the one-shot iron print latch.
pub fn console_ok_clear_printed() {
    CONSOLE_OK_PRINTED.store(false, Ordering::Release);
}

/// Latch that a SPA `/console/keys` POST injected at least one byte.
pub fn note_spa_keys_injected(n: usize) {
    if n > 0 {
        SPA_KEYS_INJECTED.store(true, Ordering::Release);
    }
}

/// Consume the SPA-keys latch (native coexist print path).
pub fn take_spa_keys_injected() -> bool {
    SPA_KEYS_INJECTED.swap(false, Ordering::AcqRel)
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

fn map_key(prev: Option<u8>, b: u8) -> Option<u8> {
    if b == b'\n' {
        if prev == Some(b'\r') {
            None
        } else {
            Some(b'\r')
        }
    } else {
        Some(b)
    }
}

/// Inject operator keystrokes into guest COM1 RBR. SerialLogOnly is a no-op.
pub fn inject_operator_keys(mode: ConsoleMode, keys: &[u8]) -> usize {
    match mode {
        ConsoleMode::SerialLogOnly => 0,
        ConsoleMode::FirmwareSpaKeys | ConsoleMode::HostReady => {
            let mut n = 0;
            let mut prev = None;
            for &b in keys {
                let Some(k) = map_key(prev, b) else {
                    prev = Some(b);
                    continue;
                };
                prev = Some(b);
                if !push_host_rx(k) {
                    break;
                }
                n += 1;
            }
            n
        }
    }
}

/// Host package: HostReady + firmware SPA keys reach RBR; GET /logs/serial stays HV UART.
pub fn prop_console_host_package() -> bool {
    crate::devices::guest_irq::reset();
    uart_reset();
    console_tx_clear();
    console_ok_clear_printed();
    let _ = take_spa_keys_injected();
    let plan = include_str!("../docs/m8_plan.md");
    let html = include_str!("../assets/webui.html");
    let http = include_str!("http.rs");
    let listen = include_str!("host_nic_listen.rs");
    let serial_log_only_refuses = inject_operator_keys(ConsoleMode::SerialLogOnly, b"x") == 0;
    let injected = inject_operator_keys(ConsoleMode::HostReady, b"hi") == 2;
    let fw_injected = inject_operator_keys(ConsoleMode::FirmwareSpaKeys, b"ab") == 2;
    let (h, _, _) = pio(0x03F8, true, 0);
    let (i, _, _) = pio(0x03F8, true, 0);
    let (a, _, _) = pio(0x03F8, true, 0);
    let (b, _, _) = pio(0x03F8, true, 0);
    let rbr_ok = h == b'h' && i == b'i' && a == b'a' && b == b'b';
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
    let nl = inject_operator_keys(ConsoleMode::FirmwareSpaKeys, b"z\n") == 2;
    let (z, _, _) = pio(0x03F8, true, 0);
    let (cr, _, _) = pio(0x03F8, true, 0);
    uart_reset();
    crate::devices::guest_irq::reset();
    note_spa_keys_injected(1);
    let latched = take_spa_keys_injected() && !take_spa_keys_injected();
    firmware_console_serves_spa_keys()
        && !firmware_console_is_serial_log_only()
        && host_never_prints_iron_console_ok()
        && serial_log_only_refuses
        && injected
        && fw_injected
        && rbr_ok
        && echo_ok
        && nl
        && z == b'z'
        && cr == b'\r'
        && latched
        && maybe_print_iron_console_ok(true, true)
        && !maybe_print_iron_console_ok(true, true)
        && CONSOLE_HOST_RESIDUAL_NOTE.contains("not iron")
        && CONSOLE_HOST_RESIDUAL_NOTE.contains("not VNC")
        && CONSOLE_FIRMWARE_SPA_KEYS_NOTE.contains("/console/keys")
        && plan.contains("M8.3")
        && plan.contains("not VNC")
        && html.contains("/console/keys")
        && html.contains("g-keys")
        && html.contains("/logs/guest")
        && html.contains("not VNC")
        && http.contains("/console/keys")
        && http.contains("Not a guest console")
        && http.contains("/logs/guest")
        && listen.contains("maybe_print_iron_console_ok")
}

#[cfg(test)]
#[path = "console_test.rs"]
mod console_test;
