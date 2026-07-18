//! VMXON / VMXOFF lifecycle state machine.
//!
//! Pillar: [V] · Proven Core · VERIFICATION: L1 (runtime asserts on transitions)
//! See `lifecycle_spec.rs` / `lifecycle_proof.rs`.

#[cfg(target_os = "uefi")]
use crate::vmx::hardware::{self, VmxHwError};

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
    /// CPUID reports no VMX.
    NotSupported,
    /// IA32_FEATURE_CONTROL rejected VMX.
    FeatureControl,
    /// VMXON instruction failed.
    VmxonFailed,
    /// VMXOFF instruction failed.
    VmxoffFailed,
}

#[cfg(target_os = "uefi")]
impl From<VmxHwError> for VmxError {
    fn from(e: VmxHwError) -> Self {
        match e {
            VmxHwError::NotSupported => Self::NotSupported,
            VmxHwError::FeatureControl => Self::FeatureControl,
            VmxHwError::VmxonFailed => Self::VmxonFailed,
            VmxHwError::VmxoffFailed => Self::VmxoffFailed,
        }
    }
}

/// Owns per-CPU VMX lifecycle.
///
/// INVARIANTS:
///   - `state == Root` only after a successful `enable` (VMXON)
///   - `state == Off` after `disable` (VMXOFF) or before first enable
///   - Root implies a valid VMXON region was used (uefi builds)
///
/// VERIFICATION: L1 — see lifecycle_spec.rs
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
    /// `vmxon_region_phys` — 4K-aligned physical address of the VMXON region.
    /// On host unit tests, hardware is not touched (software state only).
    ///
    /// INVARIANTS:
    ///   - Pre: state == Off
    ///   - Post on Ok: state == Root
    ///   - Post on Err: state unchanged
    ///
    /// VERIFICATION: L1
    pub fn enable(&mut self, vmxon_region_phys: u64) -> Result<(), VmxError> {
        if self.state != VmxState::Off {
            return Err(VmxError::InvalidState);
        }

        #[cfg(target_os = "uefi")]
        {
            // SAFETY: caller owns the frame; single-CPU bring-up; identity map.
            unsafe {
                hardware::vmxon(vmxon_region_phys)?;
            }
        }
        #[cfg(not(target_os = "uefi"))]
        {
            let _ = vmxon_region_phys;
        }

        self.state = VmxState::Root;
        Ok(())
    }

    /// Leave VMX root operation (VMXOFF).
    ///
    /// INVARIANTS:
    ///   - Pre: state == Root
    ///   - Post on Ok: state == Off
    ///
    /// VERIFICATION: L1
    pub fn disable(&mut self) -> Result<(), VmxError> {
        if self.state != VmxState::Root {
            return Err(VmxError::InvalidState);
        }

        #[cfg(target_os = "uefi")]
        {
            // SAFETY: state Root means VMXON succeeded on this CPU.
            unsafe {
                hardware::vmxoff()?;
            }
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
