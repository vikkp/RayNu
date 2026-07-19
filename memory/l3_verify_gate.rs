//! M3.17 host verification gate (true L3: discharged EptMap exclusivity).
//!
//! Pillar: [V]
//! Proven Core: companion to `ept_model` (not a boot path).
//!
//! Checks in-tree that `ept_model` opts into Verus, carries the exclusivity
//! theorem, embeds the VERIFY marker, and contains **no** `admit(`. Runtime
//! `cargo verus verify -p ept_model` is exercised by
//! `tools/verus-verify-smoke.sh`.

/// Host / CI marker when the M3.17 L3-verify gate passes.
pub const M3_L3_VERIFY_OK_MARKER: &str = "RAYNU-V-M3-L3-VERIFY-OK";

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

/// True when the Verus model is present, discharged (no admit), and marked.
pub fn ept_model_is_verified_l3() -> bool {
    let s = include_str!("../ept_model/src/lib.rs");
    s.contains("verus!")
        && s.contains("struct GhostEptMap")
        && s.contains("exclusive_ownership")
        && s.contains("lemma_empty_exclusive")
        && s.contains("lemma_map_ok_exclusive")
        && s.contains("lemma_unmap_ok_exclusive")
        && s.contains("theorem_single_guest_4k_map_unmap_exclusive")
        && s.contains(M3_L3_VERIFY_OK_MARKER)
        && !source_has_admit_call(s)
        && !s.contains("GAP(M3.17)")
}

/// True when the verify smoke script enforces no-admit green verify.
pub fn l3_verify_scripts_present() -> bool {
    let smoke = include_str!("../tools/verus-verify-smoke.sh");
    smoke.contains("cargo verus verify -p ept_model")
        && smoke.contains(M3_L3_VERIFY_OK_MARKER)
        && smoke.contains("install-verus.sh")
        && smoke.contains(r"^\s*admit\s*\(")
        && smoke.contains("0 errors")
}

/// Full M3.17 artifact gate (does not run Verus).
pub fn run_l3_verify_gate() -> bool {
    ept_model_opts_into_verus() && ept_model_is_verified_l3() && l3_verify_scripts_present()
}

#[cfg(test)]
#[path = "l3_verify_gate_test.rs"]
mod l3_verify_gate_test;
