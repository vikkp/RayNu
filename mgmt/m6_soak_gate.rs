//! M6.8 host verification gate (72-hr soak thresholds).
//!
//! Pillar: [Z] [A]
//! Proven Core: outside (companion to `mgmt/soak` — not a boot path).
//!
//! Checks simulated 72-hour soak metrics (leak / fairness / exit-rate), closed
//! GAP, audit events, runbook, and smoke/CI wiring.

use super::soak::{prop_soak_72h_thresholds, SOAK_GAP_NOTE, SOAK_TARGET_HOURS, M6_SOAK_OK_MARKER};

/// Host / CI marker when the M6.8 soak gate passes.
pub const M6_SOAK_GATE_MARKER: &str = M6_SOAK_OK_MARKER;

/// True when soak module exposes simulation, thresholds, closed GAP, marker.
pub fn soak_surface_present() -> bool {
    let s = include_str!("soak.rs");
    s.contains("fn run_soak_simulation(")
        && s.contains("fn prop_soak_72h_thresholds(")
        && s.contains("fn thresholds_met(")
        && s.contains("struct SoakMetrics")
        && s.contains("SOAK_TARGET_HOURS")
        && s.contains(M6_SOAK_OK_MARKER)
        && s.contains(SOAK_GAP_NOTE)
        && SOAK_GAP_NOTE.contains("CLOSED M6.8")
        && SOAK_TARGET_HOURS == 72
}

/// True when audit soak events exist.
pub fn audit_soak_events_present() -> bool {
    let s = include_str!("../audit/integrity.rs");
    s.contains("SoakStarted") && s.contains("SoakCompleted") && s.contains("SoakFailed")
}

/// True when the M6.8 smoke script is present.
pub fn soak_scripts_present() -> bool {
    let smoke = include_str!("../tools/m6-soak-smoke.sh");
    smoke.contains(M6_SOAK_OK_MARKER)
        && smoke.contains("m6_8_soak_gate_passes")
        && smoke.contains("prop_soak_72h_thresholds")
        && smoke.contains("SOAK_TARGET_HOURS")
}

/// True when the soak runbook documents thresholds.
pub fn soak_runbook_present() -> bool {
    let rb = include_str!("../docs/runbooks/soak.md");
    rb.contains("RAYNU-V-M6-SOAK-OK")
        && rb.contains("72")
        && rb.contains("fairness")
        && rb.contains("exit-rate")
        && rb.contains("leak")
}

/// Full M6.8 artifact + soak threshold gate.
pub fn run_m6_soak_gate() -> bool {
    soak_surface_present()
        && audit_soak_events_present()
        && soak_scripts_present()
        && soak_runbook_present()
        && prop_soak_72h_thresholds()
}

#[cfg(test)]
#[path = "m6_soak_gate_test.rs"]
mod m6_soak_gate_test;
