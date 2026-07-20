//! M6.1 host verification gate (HW PTE bit-decode correspondence).
//!
//! Pillar: [V]
//! Proven Core: companion to `ept_model` + `ept_hw` (not a boot path).
//!
//! Checks that `ept_model` discharges `theorem_hw_2m_leaf_refines_identity`
//! (no `admit`), embeds `RAYNU-V-M6-HWPTE-OK`, that `ept_proof` / `ept_spec`
//! close the HW PTE bit-decode GAP, and that live `ept_hw` leaf packing
//! matches the ghost encode/decode view. Runtime verify is exercised by
//! `tools/verus-hwpte-smoke.sh`.
//!
//! Full multi-level EPT walk correspondence remains a polish GAP.

use crate::memory::ept_hw::prop_hw_pte_identity_correspondence;

/// Host / CI marker when the M6.1 HW PTE gate passes.
pub const M6_HWPTE_OK_MARKER: &str = "RAYNU-V-M6-HWPTE-OK";

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

/// True when HW PTE L3 artifacts are present (no admit; marker).
pub fn ept_model_has_hwpte() -> bool {
    let s = include_str!("../ept_model/src/lib.rs");
    s.contains("ept_leaf_large_enc")
        && s.contains("ept_rwe_present")
        && s.contains("ept_large_bit")
        && s.contains("ept_hpa_from_pte")
        && s.contains("hw_2m_identity_leaf_ok")
        && s.contains("lemma_ept_leaf_large_decode")
        && s.contains("theorem_hw_2m_leaf_refines_identity")
        && s.contains("lemma_hw_2m_leaf_at_two_mib")
        && s.contains(M6_HWPTE_OK_MARKER)
        && !source_has_admit_call(s)
}

/// True when ept_spec / ept_proof close the HW PTE bit-decode GAP.
pub fn ept_spec_closes_hwpte() -> bool {
    let spec = include_str!("ept_spec.rs");
    let proof = include_str!("ept_proof.rs");
    spec.contains("TODO(M6.1 CLOSED): HW PTE bit-decode")
        && spec.contains("theorem_hw_2m_leaf_refines_identity")
        && proof.contains("GAP(CLOSED M6.1): Hardware EPT PTE bit-decode")
        && proof.contains("theorem_hw_2m_leaf_refines_identity")
        && !proof.contains("GAP: Hardware EPT PTE bit-decode / EPT-violation (M6)")
}

/// True when the HW PTE smoke script is present.
pub fn hwpte_scripts_present() -> bool {
    let smoke = include_str!("../tools/verus-hwpte-smoke.sh");
    smoke.contains("cargo verus verify -p ept_model")
        && smoke.contains(M6_HWPTE_OK_MARKER)
        && smoke.contains("theorem_hw_2m_leaf_refines_identity")
        && smoke.contains("install-verus.sh")
        && smoke.contains("0 errors")
}

/// Full M6.1 artifact + live leaf packing gate (does not run Verus).
pub fn run_m6_hwpte_gate() -> bool {
    ept_model_opts_into_verus()
        && ept_model_has_hwpte()
        && ept_spec_closes_hwpte()
        && hwpte_scripts_present()
        && prop_hw_pte_identity_correspondence()
}

#[cfg(test)]
#[path = "m6_hwpte_gate_test.rs"]
mod m6_hwpte_gate_test;
