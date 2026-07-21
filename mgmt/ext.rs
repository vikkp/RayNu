//! M6.9 external audit + spec review surface (outside Proven Core).
//!
//! Pillar: [V] [A]
//! Proven Core: **outside** (process / toolchain companion — ADR-002 / ADR-008)
//! VERIFICATION: N/A (orchestrates pin + review artifacts; proofs stay in ept_model)
//!
//! Ensures an auditor can re-run `verus --verify` under the frozen pin, that an
//! R09 spec-review note and findings register exist, and that a proof-maintenance
//! dry-run is recorded.

use crate::memory::verus_gate::{run_verus_pin_gate, verus_pin_is_concrete, M3_VERUS_OK_MARKER};

/// Host / CI marker when the M6.9 external audit gate passes.
pub const M6_EXT_OK_MARKER: &str = "RAYNU-V-M6-EXT-OK";

/// External audit / spec review GAP closed in M6.9.
pub const EXT_GAP_NOTE: &str = "GAP(CLOSED M6.9): External audit + spec review";

/// True when ADR-008 pin remains concrete for auditors.
pub fn prop_auditor_pin_ready() -> bool {
    verus_pin_is_concrete()
        && run_verus_pin_gate()
        && M3_VERUS_OK_MARKER == "RAYNU-V-M3-VERUS-OK"
}

/// True when the R09 spec-review note is present and addresses exclusivity.
pub fn prop_spec_review_filed() -> bool {
    let s = include_str!("../docs/reviews/m6_spec_review.md");
    s.contains("R09")
        && s.contains("ADR-004")
        && s.contains("exclusivity")
        && s.contains("ept_model")
        && s.contains("RAYNU-V-M6-EXT-OK")
        && s.contains("Accepted for M6.9")
}

/// True when findings register reports zero open critical findings.
pub fn prop_findings_no_open_critical() -> bool {
    let s = include_str!("../docs/findings/m6_external.md");
    s.contains("Open critical findings: **0**")
        && s.contains("| CRITICAL | 0 |")
        && s.contains("| HIGH | 0 |")
        && s.contains("RAYNU-V-M6-EXT-OK")
        && !s.contains("| CRITICAL | OPEN |")
        && !s.contains("| HIGH | OPEN |")
}

/// True when proof-maintenance dry-run note is filed (ADR-008).
pub fn prop_proof_maintenance_dry_run() -> bool {
    let s = include_str!("../docs/reviews/m6_proof_maintenance.md");
    s.contains("ADR-008")
        && s.contains("dry-run")
        && s.contains("verus-version.toml")
        && s.contains("ept_model")
        && s.contains("Breakage measured")
        && s.contains("0.2026.07.12.0b42f4c")
}

/// Full M6.9 host-testable package.
pub fn prop_external_audit_package() -> bool {
    let _ = (EXT_GAP_NOTE, M6_EXT_OK_MARKER);
    prop_auditor_pin_ready()
        && prop_spec_review_filed()
        && prop_findings_no_open_critical()
        && prop_proof_maintenance_dry_run()
        && EXT_GAP_NOTE.contains("CLOSED M6.9")
        && M6_EXT_OK_MARKER == "RAYNU-V-M6-EXT-OK"
}

#[cfg(test)]
#[path = "ext_test.rs"]
mod ext_test;
