//! VMCS region allocation and host-state management.
//!
//! Pillar: [V] · Proven Core · VERIFICATION: L0
//! Incorrect host-state = #1 escape vector (ADR-002).

/// Opaque handle for a VMCS region (physical address owned by HV).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VmcsHandle {
    /// Host-physical address of the 4K-aligned VMCS region (stub: opaque id).
    pub id: u64,
}

/// VMCS region metadata before hardware VMCS revision programming (M1).
///
/// INVARIANTS:
///   - Region is 4K-aligned when bound to hardware (enforced in M1)
///   - Host-state fields never expose HV stack/heap pointers to the guest
///
/// VERIFICATION: L0 — see vmcs_spec.rs
pub struct VmcsRegion {
    handle: VmcsHandle,
    launched: bool,
}

impl VmcsRegion {
    /// Create a software-only VMCS placeholder (no VMPTRLD yet).
    ///
    /// INVARIANTS:
    ///   - Returned region is not launched
    ///   - Handle id is uniquely associated with this region in software
    ///
    /// VERIFICATION: L0
    pub fn new(id: u64) -> Self {
        Self {
            handle: VmcsHandle { id },
            launched: false,
        }
    }

    pub fn handle(&self) -> VmcsHandle {
        self.handle
    }

    pub fn is_launched(&self) -> bool {
        self.launched
    }

    /// Mark region as launched after successful VMLAUNCH (M1 stub).
    ///
    /// INVARIANTS:
    ///   - Pre: !launched
    ///   - Post: launched
    ///
    /// VERIFICATION: L0
    pub fn mark_launched(&mut self) {
        debug_assert!(!self.launched);
        self.launched = true;
    }
}

#[cfg(test)]
#[path = "vmcs_test.rs"]
mod vmcs_test;
