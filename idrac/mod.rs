//! iDRAC Redfish client, hardware health integration.
//!
//! Pillar: [D]
//! Proven Core: **outside** (ADR-002, ADR-005)
//! VERIFICATION: N/A
//!
//! Tier 1 ships first; Tier 2 never blocks a milestone (ADR-005).

/// Redfish integration tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdracTier {
    /// Self-sufficient: serial, thermal/fan/PSU, SMBIOS, ACPI, X710, NVMe.
    Tier1,
    /// Partnership / RE: PERC OEM, SPD detail, predictive failure, auto-throttle.
    Tier2,
}

pub fn default_tier() -> IdracTier {
    IdracTier::Tier1
}

/// Placeholder thermal read — real Redfish in M0+/M5.
pub fn thermal_ok_stub() -> bool {
    true
}

#[cfg(test)]
#[path = "idrac_test.rs"]
mod idrac_test;
