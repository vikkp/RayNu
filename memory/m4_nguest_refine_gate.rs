//! M4.9 host verification gate (N-guest ghost↔exec refine for ADR-004).
//!
//! Pillar: [V]
//! Proven Core: companion to `ept_model` + live `EptMap` (not a boot path).
//!
//! Checks that `ept_model` carries `theorem_concrete_n_guest_4k_refine` and the
//! ≥2-guest concrete refine lemma, embeds the REFINE marker, and contains
//! **no** `admit(`. Live `EptMap` multi-guest map/unmap preserves exclusivity
//! in the same shape. Runtime `cargo verus verify -p ept_model` is exercised by
//! `tools/verus-nguest-refine-smoke.sh`.
//!
//! HW PTE bit-decode / EPT-violation remain M6; allocator↔EPT coupling closed in M5.9.

use crate::memory::ept::{EptError, EptMap, EptPermissions, M2_BRINGUP_GUEST_ID, M4_GUEST1_ID};
use crate::memory::frame_allocator::PhysFrame;

/// Host / CI marker when the M4.9 N-guest-refine gate passes.
pub const M4_REFINE_OK_MARKER: &str = "RAYNU-V-M4-REFINE-OK";

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

/// True when N-guest refine artifacts are present and discharged (no admit).
pub fn ept_model_has_n_guest_refine() -> bool {
    let s = include_str!("../ept_model/src/lib.rs");
    s.contains("struct ConcreteEptMap")
        && s.contains("pub open spec fn abs(")
        && s.contains("pub open spec fn refines(")
        && s.contains("theorem_concrete_n_guest_4k_refine")
        && s.contains("lemma_concrete_two_guests_map_refines")
        && s.contains("theorem_concrete_single_guest_4k_refine")
        && s.contains(M4_REFINE_OK_MARKER)
        && !source_has_admit_call(s)
}

/// True when ept_spec / ept_proof close N-guest refine TODO/GAP.
pub fn ept_spec_closes_n_guest_refine() -> bool {
    let spec = include_str!("ept_spec.rs");
    let proof = include_str!("ept_proof.rs");
    spec.contains("TODO(M4.9 CLOSED): N-guest ghost↔exec refine")
        && proof.contains("GAP(CLOSED M4.9): N-guest ghost↔exec refine")
        && proof.contains("theorem_concrete_n_guest_4k_refine")
        && (proof.contains("GAP: Frame-allocator ↔ EPT L3 coupling")
            || proof.contains("GAP(CLOSED M5.9): Frame-allocator ↔ EPT L3 coupling"))
}

/// True when the N-guest-refine smoke script is present.
pub fn nguest_refine_scripts_present() -> bool {
    let smoke = include_str!("../tools/verus-nguest-refine-smoke.sh");
    smoke.contains("cargo verus verify -p ept_model")
        && smoke.contains(M4_REFINE_OK_MARKER)
        && smoke.contains("theorem_concrete_n_guest_4k_refine")
        && smoke.contains("lemma_concrete_two_guests_map_refines")
        && smoke.contains("install-verus.sh")
        && smoke.contains("0 errors")
}

/// Live `EptMap`: two-guest map/unmap preserves exclusivity (exec side of refine).
pub fn prop_live_n_guest_map_unmap_refines() -> bool {
    let mut map = EptMap::new();
    let g1 = M2_BRINGUP_GUEST_ID;
    let g2 = M4_GUEST1_ID;
    let gpa1 = 0x1000u64;
    let gpa2 = 0x2000u64;
    let f1 = PhysFrame(50);
    let f2 = PhysFrame(51);

    if !map.is_empty() || !map.check_invariants() {
        return false;
    }

    if map
        .map(g1, gpa1, f1, EptPermissions::READ_WRITE)
        .is_err()
    {
        return false;
    }
    if map
        .map(g2, gpa2, f2, EptPermissions::READ_WRITE)
        .is_err()
    {
        return false;
    }
    if map.owner_of(f1) != Some(g1)
        || map.owner_of(f2) != Some(g2)
        || map.owner_of_gpa(g1, gpa1) != Some(f1)
        || map.owner_of_gpa(g2, gpa2) != Some(f2)
        || map.len() != 2
        || !map.check_invariants()
    {
        return false;
    }

    // Cross-guest steal rejected (concrete step not enabled).
    if !matches!(
        map.map(g2, 0x3000, f1, EptPermissions::READ_WRITE),
        Err(EptError::AlreadyOwned)
    ) {
        return false;
    }

    match map.unmap(g1, gpa1) {
        Ok(f) if f == f1 => {}
        _ => return false,
    }
    if map.owner_of(f1).is_some() || map.len() != 1 || !map.check_invariants() {
        return false;
    }

    // Remap freed frame to the other guest — enabled again.
    if map
        .map(g2, gpa1, f1, EptPermissions::READ_WRITE)
        .is_err()
    {
        return false;
    }
    map.check_invariants()
        && map.owner_of(f1) == Some(g2)
        && map.owner_of(f2) == Some(g2)
        && map.len() == 2
}

/// Full M4.9 artifact + correspondence gate (does not run Verus).
pub fn run_m4_nguest_refine_gate() -> bool {
    ept_model_opts_into_verus()
        && ept_model_has_n_guest_refine()
        && ept_spec_closes_n_guest_refine()
        && nguest_refine_scripts_present()
        && prop_live_n_guest_map_unmap_refines()
}

#[cfg(test)]
#[path = "m4_nguest_refine_gate_test.rs"]
mod m4_nguest_refine_gate_test;
