//! Audit ring buffer, hash chain, report generation.
//!
//! Pillar: [A] [V]
//! Proven Core: **integrity path inside** (ADR-002); report templates outside
//! VERIFICATION: L0 for integrity

pub mod integrity;
pub mod m5_audit_gate;
pub mod report;

pub use integrity::{
    boot_ring_verify, prop_audit_integrity_gate, prop_mandatory_events_chain, prop_tamper_detected,
    record_event, AuditEvent, AuditRing, Milestone, AUDIT_GENESIS_HASH, AUDIT_RING_CAP,
    M5_AUDIT_OK_MARKER,
};
pub use m5_audit_gate::run_m5_audit_gate;

/// Emit a security-relevant audit event into the process-local stub ring.
///
/// M0: records via `integrity::record_event` until a spinlock global exists.
#[macro_export]
macro_rules! audit_log {
    ($event:expr) => {{
        $crate::audit::integrity::record_event($event);
    }};
}
