//! CLI, REST API, Web UI, VM lifecycle.
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** (ADR-002)
//! VERIFICATION: N/A

/// VM lifecycle state for management plane (not Proven Core vCPU state).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmLifecycle {
    Defined,
    Running,
    Stopped,
    Deleted,
}

pub fn initial_lifecycle() -> VmLifecycle {
    VmLifecycle::Defined
}

#[cfg(test)]
#[path = "mgmt_test.rs"]
mod mgmt_test;
