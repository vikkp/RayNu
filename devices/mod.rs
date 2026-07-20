//! Emulated and passthrough device logic.
//!
//! Pillar: [Z]
//! Proven Core: **outside** (ADR-002)
//! VERIFICATION: N/A

pub mod lapic_virt;
pub mod m4_blk_gate;
pub mod serial_pio;
pub mod virtio_blk;

pub use lapic_virt::M3_GTIMER3_OK_MARKER;
pub use serial_pio::{
    guest_early_ok, guest_io_ok, guest_shell_ok, M3_EARLY_OK_MARKER, M3_IO_OK_MARKER,
    M3_SHELL_OK_MARKER,
};
pub use virtio_blk::M4_BLK_OK_MARKER;

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

/// Registry — serial PIO is live (M3.0); virtio-blk MMIO is live (M4.3).
pub fn supported_kinds() -> &'static [DeviceKind] {
    &[DeviceKind::Serial, DeviceKind::VirtioBlk]
}

#[cfg(test)]
#[path = "devices_test.rs"]
mod devices_test;
