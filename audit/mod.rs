//! Audit ring buffer, hash chain, report generation.
//!
//! Pillar: [A] [V]
//! Proven Core: **integrity path inside** (ADR-002); report templates outside
//! VERIFICATION: L0 for integrity

pub mod integrity;
pub mod report;

pub use integrity::{AuditEvent, AuditRing, Milestone, AUDIT_GENESIS_HASH};

/// Emit a security-relevant audit event into the process-local stub ring.
///
/// M0: records via `integrity::record_event` until a spinlock global exists.
#[macro_export]
macro_rules! audit_log {
    ($event:expr) => {{
        $crate::audit::integrity::record_event($event);
    }};
}
