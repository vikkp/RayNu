//! M3.16 host verification gate (Verus-linkable EptMap ghost model).
//!
//! Pillar: [V]
//! Proven Core: companion to `ept_model` (not a boot path).
//!
//! Checks in-tree that `ept_model` opts into Verus and carries the linked
//! exclusivity lemmas. Runtime `cargo verus verify -p ept_model` is exercised
//! by `tools/verus-link-smoke.sh`.

/// Host / CI marker when the M3.16 L3-link gate passes.
pub const M3_L3_LINK_OK_MARKER: &str = "RAYNU-V-M3-L3-LINK-OK";

/// True when `ept_model` opts into Verus verification.
pub fn ept_model_opts_into_verus() -> bool {
    let cargo = include_str!("../ept_model/Cargo.toml");
    cargo.contains("[package.metadata.verus]")
        && cargo.contains("verify = true")
        && cargo.contains("vstd")
}

/// True when the Verus-linked ghost model + lemma names are present.
pub fn ept_model_is_linked() -> bool {
    let s = include_str!("../ept_model/src/lib.rs");
    s.contains("verus!")
        && s.contains("struct GhostEptMap")
        && s.contains("exclusive_ownership")
        && s.contains("lemma_empty_exclusive")
        && s.contains("lemma_map_ok_exclusive")
        && s.contains("lemma_unmap_ok_exclusive")
        && s.contains("theorem_single_guest_4k_map_unmap_exclusive")
        && s.contains("admit()")
        && s.contains(M3_L3_LINK_OK_MARKER)
        && s.contains("GAP(M3.17)")
}

/// True when the link smoke script is present.
pub fn l3_link_scripts_present() -> bool {
    let smoke = include_str!("../tools/verus-link-smoke.sh");
    smoke.contains("cargo verus verify -p ept_model")
        && smoke.contains(M3_L3_LINK_OK_MARKER)
        && smoke.contains("install-verus.sh")
}

/// Full M3.16 artifact gate (does not run Verus).
pub fn run_l3_link_gate() -> bool {
    ept_model_opts_into_verus() && ept_model_is_linked() && l3_link_scripts_present()
}

#[cfg(test)]
#[path = "l3_link_gate_test.rs"]
mod l3_link_gate_test;
