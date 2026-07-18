//! Early boot, firmware handoff, CPU bring-up.
//!
//! Pillar: [Z] [D]
//! Proven Core: **outside** (ADR-002)
//! VERIFICATION: N/A (outside core) — unit tests only

pub mod serial;

/// Perform minimal post-UEFI early init (M0).
///
/// Initializes COM1 so diagnostic output reaches QEMU `-serial stdio` and
/// iDRAC virtual console. Memory-map ownership / ExitBootServices land next.
///
/// On host unit-test builds this is a no-op (no port I/O in userspace).
pub fn early_init() {
    #[cfg(target_os = "uefi")]
    serial::init();
}

#[cfg(test)]
#[path = "boot_test.rs"]
mod boot_test;
