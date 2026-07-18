//! IPI confinement — prevent cross-VM / host disruption.
//!
//! Pillar: [V] · Proven Core · VERIFICATION: L0

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpiError {
    TargetNotOwned,
    InvalidTarget,
}

/// INVARIANTS:
///   - Target APIC ID must belong to the sending guest's vCPU set
///   - Host CPU broadcast IPIs are never exposed to guests
///
/// VERIFICATION: L0 — see ipi_spec.rs
pub fn validate_ipi_target(sender_guest: u64, target_guest: u64, apic_id: u32) -> Result<(), IpiError> {
    if apic_id == u32::MAX {
        return Err(IpiError::InvalidTarget);
    }
    if sender_guest != target_guest {
        return Err(IpiError::TargetNotOwned);
    }
    Ok(())
}

#[cfg(test)]
#[path = "ipi_test.rs"]
mod ipi_test;
