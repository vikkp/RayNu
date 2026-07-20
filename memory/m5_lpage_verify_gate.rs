//! M5.7 host verification gate (large-page L3 discharge).
//!
//! Pillar: [V]
//! Proven Core: companion to `ept_model` (not a boot path).
//!
//! Checks that `ept_model` discharges large-page span map/unmap exclusivity
//! (no `admit`), embeds `RAYNU-V-M5-LPAGE-VERIFY-OK`, and that `ept_proof` /
//! `ept_spec` close the Large-page L3 GAP. Runtime verify is exercised by
//! `tools/verus-lpage-verify-smoke.sh`.

/// Host / CI marker when the M5.7 large-page L3-verify gate passes.
pub const M5_LPAGE_VERIFY_OK_MARKER: &str = "RAYNU-V-M5-LPAGE-VERIFY-OK";

/// True when `ept_model` opts into Verus verification.
pub fn ept_model_opts_into_verus() -> bool {
    let cargo = include_str!("../ept_model/Cargo.toml");
    cargo.contains("[package.metadata.verus]")
        && cargo.contains("verify = true")
        && cargo.contains("vstd")
}

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

/// True when large-page L3 theorems are present (no admit; marker embedded).
pub fn ept_model_is_verified_lpage_l3() -> bool {
    let s = include_str!("../ept_model/src/lib.rs");
    s.contains("verus!")
        && s.contains("theorem_large_page_map_unmap_exclusive")
        && s.contains("lemma_large_map_ok_exclusive")
        && s.contains("lemma_large_unmap_ok_exclusive")
        && s.contains("lemma_2m_map_unmap_exclusive")
        && s.contains("lemma_1g_map_unmap_exclusive")
        && s.contains("lemma_two_guests_large_map_distinct_spans_exclusive")
        && s.contains("ghost_large_map")
        && s.contains("ghost_large_unmap")
        && s.contains(M5_LPAGE_VERIFY_OK_MARKER)
        && !source_has_admit_call(s)
}

/// True when ept_proof / ept_spec close the Large-page L3 GAP.
pub fn ept_proof_closes_lpage_l3() -> bool {
    let proof = include_str!("ept_proof.rs");
    let spec = include_str!("ept_spec.rs");
    proof.contains("GAP(CLOSED M5.7): Large-page L3 discharge")
        && proof.contains("theorem_large_page_map_unmap_exclusive")
        && spec.contains("TODO(M5.7 CLOSED): large-page L3")
        && !proof.contains("GAP: Large-page L3 discharge (M5)")
}

/// True when the large-page-verify smoke script enforces no-admit green verify.
pub fn lpage_verify_scripts_present() -> bool {
    let smoke = include_str!("../tools/verus-lpage-verify-smoke.sh");
    smoke.contains("cargo verus verify -p ept_model")
        && smoke.contains(M5_LPAGE_VERIFY_OK_MARKER)
        && smoke.contains("theorem_large_page_map_unmap_exclusive")
        && smoke.contains("install-verus.sh")
        && smoke.contains(r"^\s*admit\s*\(")
        && smoke.contains("0 errors")
}

/// Full M5.7 artifact gate (does not run Verus).
pub fn run_m5_lpage_verify_gate() -> bool {
    ept_model_opts_into_verus()
        && ept_model_is_verified_lpage_l3()
        && ept_proof_closes_lpage_l3()
        && lpage_verify_scripts_present()
}

#[cfg(test)]
#[path = "m5_lpage_verify_gate_test.rs"]
mod m5_lpage_verify_gate_test;
