//! Emulated and passthrough device logic.
//!
//! Pillar: [Z]
//! Proven Core: **outside** (ADR-002)
//! VERIFICATION: N/A

pub mod serial_pio;

pub use serial_pio::{guest_early_ok, guest_io_ok, M3_EARLY_OK_MARKER, M3_IO_OK_MARKER};

/// Device class stub for future virtio / serial / RTC.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceKind {
    Serial,
    Rtc,
    VirtioBlk,
    VirtioNet,
    VirtioConsole,
    Passthrough,
}

/// Registry — serial PIO is live (M3.0); others still stubs.
pub fn supported_kinds() -> &'static [DeviceKind] {
    &[DeviceKind::Serial]
}

#[cfg(test)]
#[path = "devices_test.rs"]
mod devices_test;
