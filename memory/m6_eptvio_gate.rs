//! M6.0 host verification gate (EPT-violation exclusivity).
//!
//! Pillar: [V]
//! Proven Core: companion to `ept_model` + live `EptMap` (not a boot path).
//!
//! Checks that `ept_model` discharges `theorem_ept_violation_preserves_exclusive`
//! (no `admit`), embeds `RAYNU-V-M6-EPTVIO-OK`, that `ept_proof` / `ept_spec`
//! close the EPT-violation GAP, and that live violation dispositions preserve
//! exclusivity. Runtime verify is exercised by `tools/verus-eptvio-smoke.sh`.
//!
//! Live `handle_ept_violation_and_resume` remains EmulateNoMap / Reject for
//! MMIO; ClaimMap is the demand-fill path covered by the ghost theorem.
//! Full HW PTE bit-decode remains M6.1.

use crate::memory::ept::prop_violation_preserves_exclusive;

/// Host / CI marker when the M6.0 EPT-violation gate passes.
pub const M6_EPTVIO_OK_MARKER: &str = "RAYNU-V-M6-EPTVIO-OK";

/// True when a non-comment source line is an `admit(` statement.
fn source_has_admit_call(s: &str) -> bool {
    for line in s.lines() {
        let t = line.trim_start();
        if t.starts_with("//") {
            continue;
        }
        if t.starts_with("admit(") || t.starts_with("admit (") {
            return true;
        }
    }
    false
}

/// True when `ept_model` opts into Verus verification.
pub fn ept_model_opts_into_verus() -> bool {
    let cargo = include_str!("../ept_model/Cargo.toml");
    cargo.contains("[package.metadata.verus]")
        && cargo.contains("verify = true")
        && cargo.contains("vstd")
}

/// True when EPT-violation L3 artifacts are present (no admit; marker).
pub fn ept_model_has_eptvio() -> bool {
    let s = include_str!("../ept_model/src/lib.rs");
    s.contains("enum EptViolationDisposition")
        && s.contains("violation_enabled")
        && s.contains("apply_violation")
        && s.contains("theorem_ept_violation_preserves_exclusive")
        && s.contains("lemma_violation_claim_preserves_exclusive")
        && s.contains("lemma_violation_noop_preserves_exclusive")
        && s.contains("lemma_ept_violation_emulate_then_claim")
        && s.contains(M6_EPTVIO_OK_MARKER)
        && !source_has_admit_call(s)
}

/// True when ept_spec / ept_proof close the EPT-violation exclusivity GAP.
pub fn ept_spec_closes_eptvio() -> bool {
    let spec = include_str!("ept_spec.rs");
    let proof = include_str!("ept_proof.rs");
    spec.contains("TODO(M6.0 CLOSED): EPT-violation exclusivity")
        && spec.contains("theorem_ept_violation_preserves_exclusive")
        && proof.contains("GAP(CLOSED M6.0): EPT violation handler preserves exclusivity")
        && proof.contains("theorem_ept_violation_preserves_exclusive")
        && !proof.contains("GAP: EPT violation handler preserves exclusivity")
}

/// True when the EPT-violation smoke script is present.
pub fn eptvio_scripts_present() -> bool {
    let smoke = include_str!("../tools/verus-eptvio-smoke.sh");
    smoke.contains("cargo verus verify -p ept_model")
        && smoke.contains(M6_EPTVIO_OK_MARKER)
        && smoke.contains("theorem_ept_violation_preserves_exclusive")
        && smoke.contains("install-verus.sh")
        && smoke.contains("0 errors")
}

/// Full M6.0 artifact + live disposition gate (does not run Verus).
pub fn run_m6_eptvio_gate() -> bool {
    ept_model_opts_into_verus()
        && ept_model_has_eptvio()
        && ept_spec_closes_eptvio()
        && eptvio_scripts_present()
        && prop_violation_preserves_exclusive()
}

#[cfg(test)]
#[path = "m6_eptvio_gate_test.rs"]
mod m6_eptvio_gate_test;
