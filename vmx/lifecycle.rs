//! VMXON / VMXOFF lifecycle state machine.
//!
//! Pillar: [V] · Proven Core · VERIFICATION: L0
//! See `lifecycle_spec.rs` / `lifecycle_proof.rs`.

/// Explicit VMX root-mode lifecycle (no boolean flags — CLAUDE.md coding standards).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmxState {
    /// CR4.VMXE not yet set / VMXON not executed.
    Off,
    /// VMXON succeeded; CPU is in VMX root operation.
    Root,
}

/// Errors from VMX lifecycle transitions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmxError {
    /// Transition illegal for the current state.
    InvalidState,
    /// Hardware rejected VMXON (feature / BIOS lock) — stubbed until M1.
    HardwareReject,
}

/// Owns per-CPU VMX lifecycle.
///
/// INVARIANTS:
///   - `state == Root` only after a successful `enable` (VMXON)
///   - `state == Off` after `disable` (VMXOFF) or before first enable
///   - Never claims Root without a valid VMXON region (enforced in M1)
///
/// VERIFICATION: L0 — see lifecycle_spec.rs
/// FALLBACK: L1 runtime asserts planned for M1
pub struct VmxLifecycle {
    state: VmxState,
}

impl VmxLifecycle {
    pub const fn new() -> Self {
        Self {
            state: VmxState::Off,
        }
    }

    pub fn state(&self) -> VmxState {
        self.state
    }

    /// Enter VMX root operation (VMXON).
    ///
    /// INVARIANTS:
    ///   - Pre: state == Off
    ///   - Post on Ok: state == Root
    ///   - Post on Err: state unchanged
    ///
    /// VERIFICATION: L0
    pub fn enable(&mut self) -> Result<(), VmxError> {
        if self.state != VmxState::Off {
            return Err(VmxError::InvalidState);
        }
        // M1: execute VMXON (Intel SDM Vol. 3C). Stub keeps state Off→Root for unit tests only.
        self.state = VmxState::Root;
        Ok(())
    }

    /// Leave VMX root operation (VMXOFF).
    ///
    /// INVARIANTS:
    ///   - Pre: state == Root
    ///   - Post on Ok: state == Off
    ///
    /// VERIFICATION: L0
    pub fn disable(&mut self) -> Result<(), VmxError> {
        if self.state != VmxState::Root {
            return Err(VmxError::InvalidState);
        }
        self.state = VmxState::Off;
        Ok(())
    }
}

impl Default for VmxLifecycle {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "lifecycle_test.rs"]
mod lifecycle_test;
