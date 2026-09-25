//! SPA host power-off (outside Proven Core).
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** (ADR-002). The audit event lives in `audit/`.
//!
//! `POST /host/poweroff` latches a shutdown and returns HTTP 200 on the
//! existing keep-alive socket. Firmware coexist drains that reply, then
//! `VMXOFF` and `EfiResetShutdown`. A failed `VMXOFF` leaves the page up.
//! Host/CI never call `ResetSystem`. This is the chassis, not a guest command.

use core::sync::atomic::{AtomicBool, Ordering};

/// REST path the Overview button posts.
pub const HOST_POWEROFF_PATH: &str = "/host/poweroff";

/// COM2 after the HTTP 200 has drained. A deliberate off, distinct from
/// `SYS1003` then `SYS1001` with no `RAC1195`.
pub const SPA_POWEROFF_COM2: &str =
    "boot: SPA host power-off — VMXOFF then ResetSystem SHUTDOWN";

/// COM2 when `VMXOFF` fails. The page stays up. No `ResetSystem`.
pub const SPA_POWEROFF_VMXOFF_FAIL: &str =
    "boot: WARN — SPA power-off VMXOFF failed; page stays up";

/// `AuditEvent::HostPowerOff.source` for an authenticated SPA POST.
pub const SPA_POWEROFF_SOURCE: u8 = 1;

/// JUSTIFICATION: one BSP latch. The coexist tick consumes it after the
/// HTTP reply is queued. Not the Proven Core allocator.
static SPA_POWEROFF: AtomicBool = AtomicBool::new(false);

/// Latch an authenticated SPA power-off. Does not reset the CPU.
pub fn note_spa_poweroff() {
    SPA_POWEROFF.store(true, Ordering::Release);
}

/// True after [`note_spa_poweroff`] until [`clear_spa_poweroff`].
pub fn spa_poweroff_latched() -> bool {
    SPA_POWEROFF.load(Ordering::Acquire)
}

/// Drop the latch. Firmware uses this when `VMXOFF` fails.
pub fn clear_spa_poweroff() {
    SPA_POWEROFF.store(false, Ordering::Release);
}

/// Host package: the button, the POST, and the firmware drain-before-reset
/// order are in tree. Host tests do not execute `ResetSystem`.
pub fn prop_spa_host_poweroff() -> bool {
    let http = include_str!("http.rs");
    let listen = include_str!("host_nic_listen.rs");
    let html = include_str!("../assets/webui.html");
    http.contains("HOST_POWEROFF_PATH")
        && http.contains("HostPowerOff")
        && !http.contains("ResetType::SHUTDOWN")
        && listen.contains("COEXIST_PENDING_POWEROFF")
        && listen.contains("ResetType::SHUTDOWN")
        && listen.contains("hardware::vmxoff()")
        && listen.contains("SPA_POWEROFF_COM2")
        && listen.contains("SPA_POWEROFF_VMXOFF_FAIL")
        && listen.contains("send_queue()")
        && html.contains("btn-poweroff")
        && html.contains(HOST_POWEROFF_PATH)
        && html.contains("listBusy")
        && html.contains("/console/keys")
        && html.contains("not VNC")
}

#[cfg(test)]
mod host_power_test {
    use super::*;
    use crate::audit::integrity::{AuditEvent, AuditRing};

    #[test]
    fn poweroff_event_chains() {
        let mut ring = AuditRing::new();
        assert!(ring
            .append(AuditEvent::HostPowerOff {
                source: SPA_POWEROFF_SOURCE,
            })
            .is_ok());
        assert!(ring.verify_chain());
        assert!(prop_spa_host_poweroff());
    }
}
