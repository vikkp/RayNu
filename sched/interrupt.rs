//! Interrupt injection firewall (M2.4).
//!
//! Pillar: [V] · Proven Core · VERIFICATION: L1
//!
//! Packs VM-entry interruption-information values for software-injected
//! external interrupts. Real host IRQ → VMEXIT (APIC timer) is deferred;
//! this gate proves vector validation + guest ISR delivery via VMRESUME.

/// COM1 marker when the injected guest ISR runs and HLTs (M2.4 gate).
pub const M2_IRQ_OK_MARKER: &str = "RAYNU-V-M2-IRQ-OK";

/// Bring-up vector injected on the first HLT VMEXIT.
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
