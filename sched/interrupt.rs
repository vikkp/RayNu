//! Interrupt injection firewall.
//!
//! Pillar: [V] · Proven Core · VERIFICATION: L0

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InjectError {
    InvalidVector,
    NotRunning,
}

/// INVARIANTS:
///   - Only vectors 0..=255 accepted
///   - Injection requires a running vCPU context (checked by caller state)
///
/// VERIFICATION: L0 — see interrupt_spec.rs
pub fn validate_vector(vector: u32) -> Result<u8, InjectError> {
    if vector > 255 {
        return Err(InjectError::InvalidVector);
    }
    Ok(vector as u8)
}

#[cfg(test)]
#[path = "interrupt_test.rs"]
mod interrupt_test;
