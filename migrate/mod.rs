//! VMware migration engine (vCenter, VMDK, OVF).
//!
//! Pillar: [A] [Z]
//! Proven Core: **outside** (ADR-007)
//! VERIFICATION: N/A
//! Milestone: M5.5 dedicated workstream

/// Migration job status stub.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MigrateStatus {
    Idle,
    Running,
    Succeeded,
    Failed,
}

pub fn status_stub() -> MigrateStatus {
    MigrateStatus::Idle
}

#[cfg(test)]
#[path = "migrate_test.rs"]
mod migrate_test;
