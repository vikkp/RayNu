//! Early boot, firmware handoff, CPU bring-up.
//!
//! Pillar: [Z] [D]
//! Proven Core: **outside** (ADR-002)
//! VERIFICATION: N/A (outside core) — unit tests only

/// Perform minimal post-UEFI early init (M0 stub).
///
/// Real bring-up (GDT/IDT, AP wake, memory map ownership) lands in M0–M1.
pub fn early_init() {
    // Placeholder: serial is owned by UEFI helpers for now.
}

#[cfg(test)]
#[path = "boot_test.rs"]
mod boot_test;
