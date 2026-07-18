//! Emulated and passthrough device logic.
//!
//! Pillar: [Z]
//! Proven Core: **outside** (ADR-002)
//! VERIFICATION: N/A

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

/// Registry placeholder — real MMIO/PIO handlers in M3+.
pub fn supported_kinds() -> &'static [DeviceKind] {
    &[DeviceKind::Serial]
}

#[cfg(test)]
#[path = "devices_test.rs"]
mod devices_test;
