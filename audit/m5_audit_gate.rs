//! M5.3 host verification gate (audit ring + hash chain).
//!
//! Pillar: [A] [V]
//! Proven Core: companion to `audit/integrity` (integrity path is inside ADR-002).
//!
//! Checks append-only ring + verify/tamper path, mandatory VMCS/EPT/lifecycle
//! (and MSR-block) categories, and wiring of those events into call sites.

use super::{
    prop_audit_integrity_gate, prop_mandatory_events_chain, prop_tamper_detected, M5_AUDIT_OK_MARKER,
};

/// True when integrity module exposes ring, verify, tamper, and marker.
pub fn audit_integrity_surface_present() -> bool {
    let s = include_str!("integrity.rs");
    s.contains("struct AuditRing")
        && s.contains("fn append(")
        && s.contains("fn verify_chain(")
        && s.contains("fn tamper_hash_at(")
        && s.contains("fn boot_ring_verify(")
        && s.contains("fn prop_mandatory_events_chain(")
        && s.contains("fn prop_tamper_detected(")
        && s.contains("VmcsCreated")
        && s.contains("EptMapped")
        && s.contains("EptUnmapped")
        && s.contains("MsrBlocked")
        && s.contains("VmCreated")
        && s.contains(M5_AUDIT_OK_MARKER)
}

/// True when mandatory categories are wired at call sites (not only enum stubs).
pub fn mandatory_events_wired() -> bool {
    let mgmt = include_str!("../mgmt/mod.rs");
    let main = include_str!("../src/main.rs");
    let vmcs = include_str!("../vmx/vmcs.rs");
    let launch = include_str!("../vmx/launch.rs");
    let ept = include_str!("../memory/ept.rs");
    mgmt.contains("AuditEvent::VmCreated")
        && mgmt.contains("AuditEvent::VmStarted")
        && mgmt.contains("AuditEvent::VmStopped")
        && mgmt.contains("AuditEvent::VmDestroyed")
        && main.contains("AuditEvent::EptMapped")
        && vmcs.contains("AuditEvent::VmcsCreated")
        && launch.contains("AuditEvent::MsrBlocked")
        && ept.contains("AuditEvent::EptUnmapped")
}

/// True when the M5.3 smoke script is present.
pub fn audit_scripts_present() -> bool {
    let smoke = include_str!("../tools/m5-audit-smoke.sh");
    smoke.contains(M5_AUDIT_OK_MARKER)
        && smoke.contains("m5_3_audit_gate_passes")
        && smoke.contains("prop_tamper_detected")
}

/// Full M5.3 artifact + integrity gate.
pub fn run_m5_audit_gate() -> bool {
    audit_integrity_surface_present()
        && mandatory_events_wired()
        && audit_scripts_present()
        && prop_mandatory_events_chain()
        && prop_tamper_detected()
        && prop_audit_integrity_gate()
}

#[cfg(test)]
#[path = "m5_audit_gate_test.rs"]
mod m5_audit_gate_test;
