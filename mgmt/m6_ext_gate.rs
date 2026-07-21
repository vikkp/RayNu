//! M6.9 host verification gate (external audit + spec review).
//!
//! Pillar: [V] [A]
//! Proven Core: outside (process companion — ADR-008 pin + review artifacts).
//!
//! Checks frozen pin readiness, R09 spec-review note, findings register (no open
//! CRITICAL/HIGH), proof-maintenance dry-run, runbook, and smoke/CI wiring.

use super::ext::{
    prop_external_audit_package, EXT_GAP_NOTE, M6_EXT_OK_MARKER,
};

/// Host / CI marker when the M6.9 external audit gate passes.
pub const M6_EXT_GATE_MARKER: &str = M6_EXT_OK_MARKER;

/// True when ext module exposes package props, closed GAP, marker.
pub fn ext_surface_present() -> bool {
    let s = include_str!("ext.rs");
    s.contains("fn prop_auditor_pin_ready(")
        && s.contains("fn prop_spec_review_filed(")
        && s.contains("fn prop_findings_no_open_critical(")
        && s.contains("fn prop_proof_maintenance_dry_run(")
        && s.contains("fn prop_external_audit_package(")
        && s.contains(M6_EXT_OK_MARKER)
        && s.contains(EXT_GAP_NOTE)
        && EXT_GAP_NOTE.contains("CLOSED M6.9")
}

/// True when review / findings / runbook artifacts exist with required phrases.
pub fn ext_docs_present() -> bool {
    let review = include_str!("../docs/reviews/m6_spec_review.md");
    let findings = include_str!("../docs/findings/m6_external.md");
    let maint = include_str!("../docs/reviews/m6_proof_maintenance.md");
    let runbook = include_str!("../docs/runbooks/external_audit.md");
    review.contains("R09")
        && findings.contains("Open critical findings: **0**")
        && maint.contains("Breakage measured")
        && runbook.contains("RAYNU-V-M6-EXT-OK")
        && runbook.contains("verus-version.toml")
}

/// True when the M6.9 smoke script is present.
pub fn ext_scripts_present() -> bool {
    let smoke = include_str!("../tools/m6-ext-smoke.sh");
    smoke.contains(M6_EXT_OK_MARKER)
        && smoke.contains("m6_9_ext_gate_passes")
        && smoke.contains("prop_external_audit_package")
        && smoke.contains("install-verus.sh")
        && smoke.contains("ept_model")
}

/// Full M6.9 artifact + package gate.
pub fn run_m6_ext_gate() -> bool {
    ext_surface_present()
        && ext_docs_present()
        && ext_scripts_present()
        && prop_external_audit_package()
}

#[cfg(test)]
#[path = "m6_ext_gate_test.rs"]
mod m6_ext_gate_test;
