//! Interrupt injection firewall (M2.4 / M2.5 / M3.4).
//!
//! Pillar: [V] · Proven Core · VERIFICATION: L1
//!
//! Packs VM-entry interruption-information values for software-injected
//! external interrupts. M2.5 / M3.4 use host LAPIC-timer → external-interrupt
//! VMEXIT → EOI → re-inject through this same path (M3.4 is post-proto).

/// COM1 marker when the injected guest ISR runs and HLTs (M2.4 gate).
pub const M2_IRQ_OK_MARKER: &str = "RAYNU-V-M2-IRQ-OK";

/// COM1 marker when LAPIC timer → VMEXIT → EOI → re-inject ISR HLTs (M2.5).
pub const M2_TIMER_OK_MARKER: &str = "RAYNU-V-M2-TIMER-OK";

/// COM1 marker when post-proto guest timer → EOI → inject ISR HLTs (M3.4).
pub const M3_GTIMER_OK_MARKER: &str = "RAYNU-V-M3-GTIMER-OK";

/// COM1 marker when post-earlyprintk host LAPIC → ext-IRQ VMEXIT (M3.9).
pub const M3_GTIMER2_OK_MARKER: &str = "RAYNU-V-M3-GTIMER2-OK";

/// Bring-up vector for inject and LAPIC timer LVT.
pub const M2_IRQ_VECTOR: u32 = 0x21;

/// VM-entry interruption type: external interrupt (SDM Vol. 3C).
pub const INTR_TYPE_EXTERNAL: u32 = 0;

/// Valid bit in VM-entry interruption-information field.
pub const INTR_INFO_VALID: u32 = 1 << 31;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InjectError {
    InvalidVector,
    NotRunning,
}

/// INVARIANTS:
///   - Only vectors 0..=255 accepted
///   - Injection requires a running vCPU context (checked by caller state)
///
/// VERIFICATION: L1 — see interrupt_spec.rs
pub fn validate_vector(vector: u32) -> Result<u8, InjectError> {
    if vector > 255 {
        return Err(InjectError::InvalidVector);
    }
    Ok(vector as u8)
}

/// Pack VM-entry interruption-information (vector | type<<8 | valid).
pub fn pack_entry_intr_info(vector: u8, type_: u32) -> u32 {
    (vector as u32) | ((type_ & 7) << 8) | INTR_INFO_VALID
}

/// Validate `vector` and pack an external-interrupt injection word.
pub fn prepare_external_inject(vector: u32) -> Result<u32, InjectError> {
    let v = validate_vector(vector)?;
    Ok(pack_entry_intr_info(v, INTR_TYPE_EXTERNAL))
}

#[cfg(test)]
#[path = "interrupt_test.rs"]
mod interrupt_test;
