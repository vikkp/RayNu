//! M6.3 host verification gate (live migration page transfer exclusivity).
//!
//! Pillar: [V]
//! Proven Core: companion to `ept_model` + live `EptMap` (not a boot path).
//!
//! Checks that `ept_model` discharges `theorem_page_transfer_preserves_exclusive`
//! (no `admit`), embeds `RAYNU-V-M6-MIGRATE-XFER-OK`, that `ept_proof` / `ept_spec`
//! close the live-migration page-transfer GAP, and that live `transfer_page`
//! preserves exclusivity. Runtime verify is exercised by
//! `tools/verus-migrate-xfer-smoke.sh`.
//!
//! Distinct from M5.5 inventory import (`RAYNU-V-M5-MIGRATE-OK`).

use crate::memory::ept::prop_page_transfer_preserves_exclusive;

/// Host / CI marker when the M6.3 migrate-xfer gate passes.
pub const M6_MIGRATE_XFER_OK_MARKER: &str = "RAYNU-V-M6-MIGRATE-XFER-OK";

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

/// True when migrate page-transfer L3 artifacts are present (no admit; marker).
pub fn ept_model_has_migrate_xfer() -> bool {
    let s = include_str!("../ept_model/src/lib.rs");
    s.contains("struct PageTransferStep")
        && s.contains("transfer_enabled")
        && s.contains("apply_transfer")
        && s.contains("transfer_posts")
        && s.contains("lemma_transfer_preserves_exclusive")
        && s.contains("theorem_page_transfer_preserves_exclusive")
        && s.contains("lemma_mock_page_transfer_exclusive")
        && s.contains(M6_MIGRATE_XFER_OK_MARKER)
        && !source_has_admit_call(s)
}

/// True when ept_spec / ept_proof close the live-migration page-transfer GAP.
pub fn ept_spec_closes_migrate_xfer() -> bool {
    let spec = include_str!("ept_spec.rs");
    let proof = include_str!("ept_proof.rs");
    spec.contains("TODO(M6.3 CLOSED): Live migration page transfer")
        && spec.contains("theorem_page_transfer_preserves_exclusive")
        && proof.contains("GAP(CLOSED M6.3): Live migration page transfer")
        && proof.contains("theorem_page_transfer_preserves_exclusive")
        && !proof.contains("GAP: Live migration page transfer (M6)")
}

/// True when the migrate-xfer smoke script is present.
pub fn migrate_xfer_scripts_present() -> bool {
    let smoke = include_str!("../tools/verus-migrate-xfer-smoke.sh");
    smoke.contains("cargo verus verify -p ept_model")
        && smoke.contains(M6_MIGRATE_XFER_OK_MARKER)
        && smoke.contains("theorem_page_transfer_preserves_exclusive")
        && smoke.contains("install-verus.sh")
        && smoke.contains("0 errors")
}

/// Full M6.3 artifact + live transfer gate (does not run Verus).
pub fn run_m6_migrate_gate() -> bool {
    ept_model_opts_into_verus()
        && ept_model_has_migrate_xfer()
        && ept_spec_closes_migrate_xfer()
        && migrate_xfer_scripts_present()
        && prop_page_transfer_preserves_exclusive()
}

#[cfg(test)]
#[path = "m6_migrate_gate_test.rs"]
mod m6_migrate_gate_test;
