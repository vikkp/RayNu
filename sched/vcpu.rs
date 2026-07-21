//! vCPU state save/restore.
//!
//! Pillar: [V] · Proven Core · VERIFICATION: L0
//! Incomplete save/restore leaks host pointers to guest.

/// Explicit vCPU runstate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VcpuState {
    Created,
    Runnable,
    Running,
    Halted,
    TornDown,
}

/// Software vCPU control block (registers omitted until M1/M2).
///
/// INVARIANTS:
///   - state transitions follow Created→Runnable→Running↔Halted→TornDown
///   - Host-only fields must never be guest-readable (enforced when regs land)
///
/// VERIFICATION: L0 — see vcpu_spec.rs
pub struct Vcpu {
    pub id: u32,
    state: VcpuState,
}

impl Vcpu {
    pub const fn new(id: u32) -> Self {
        Self {
            id,
            state: VcpuState::Created,
        }
    }

    pub fn state(&self) -> VcpuState {
        self.state
    }

    /// INVARIANTS: Pre Created; Post Runnable
    pub fn make_runnable(&mut self) {
        debug_assert_eq!(self.state, VcpuState::Created);
        self.state = VcpuState::Runnable;
    }

    /// INVARIANTS: Pre Runnable|Halted; Post Running
    pub fn enter_running(&mut self) {
        debug_assert!(matches!(
            self.state,
            VcpuState::Runnable | VcpuState::Halted
        ));
        self.state = VcpuState::Running;
    }

    /// INVARIANTS: Pre Running; Post Halted
    pub fn halt(&mut self) {
        debug_assert_eq!(self.state, VcpuState::Running);
        self.state = VcpuState::Halted;
    }

    /// Kill / tear down a live vCPU (M6.7 fault injection).
    ///
    /// INVARIANTS: Pre Runnable|Running|Halted; Post TornDown.
    /// Created or already TornDown → Err (fail-closed).
    pub fn tear_down(&mut self) -> Result<(), ()> {
        match self.state {
            VcpuState::Runnable | VcpuState::Running | VcpuState::Halted => {
                self.state = VcpuState::TornDown;
                Ok(())
            }
            VcpuState::Created | VcpuState::TornDown => Err(()),
        }
    }
}

#[cfg(test)]
#[path = "vcpu_test.rs"]
mod vcpu_test;
