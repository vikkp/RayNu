//! M3.18 host verification gate (ghost↔exec refinement for ADR-004).
//!
//! Pillar: [V]
//! Proven Core: companion to `ept_model` + live `EptMap` (not a boot path).
//!
//! Checks that `ept_model` carries `abs` / `refines` / the refine theorem, and
//! that live `EptMap` 4K bring-up map/unmap steps preserve exclusivity in the
//! same shape. Runtime `cargo verus verify -p ept_model` is exercised by
//! `tools/verus-refine-smoke.sh`.

use crate::memory::ept::{EptError, EptMap, EptPermissions, M2_BRINGUP_GUEST_ID};
use crate::memory::frame_allocator::PhysFrame;

/// Host / CI marker when the M3.18 L3-refine gate passes.
pub const M3_L3_REFINE_OK_MARKER: &str = "RAYNU-V-M3-L3-REFINE-OK";

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

/// True when refine artifacts are present and discharged (no admit).
pub fn ept_model_has_refine() -> bool {
    let s = include_str!("../ept_model/src/lib.rs");
    s.contains("struct ConcreteEptMap")
        && s.contains("pub open spec fn abs(")
        && s.contains("pub open spec fn refines(")
        && s.contains("lemma_abs_map_commutes")
        && s.contains("lemma_concrete_map_ok_refines")
        && s.contains("theorem_concrete_single_guest_4k_refine")
        && s.contains(M3_L3_REFINE_OK_MARKER)
        && !source_has_admit_call(s)
}

/// True when the refine smoke script is present.
pub fn l3_refine_scripts_present() -> bool {
    let smoke = include_str!("../tools/verus-refine-smoke.sh");
    smoke.contains("cargo verus verify -p ept_model")
        && smoke.contains(M3_L3_REFINE_OK_MARKER)
        && smoke.contains("theorem_concrete_single_guest_4k_refine")
        && smoke.contains("install-verus.sh")
        && smoke.contains("0 errors")
}

/// Live `EptMap` 4K bring-up map/unmap matches the refine correspondence.
///
/// Each Ok step preserves ADR-004 invariants (the exec side of `refines`);
/// AlreadyOwned / NotMapped leave state unchanged.
pub fn prop_live_map_unmap_refines() -> bool {
    let mut map = EptMap::new();
    let guest = M2_BRINGUP_GUEST_ID;
    let gpa0 = 0x1000u64;
    let gpa1 = 0x2000u64;
    let f0 = PhysFrame(20);
    let f1 = PhysFrame(21);

    if !map.is_empty() || !map.check_invariants() {
        return false;
    }

    if map
        .map(guest, gpa0, f0, EptPermissions::READ_WRITE)
        .is_err()
    {
        return false;
    }
    if map.owner_of(f0) != Some(guest)
        || map.owner_of_gpa(guest, gpa0) != Some(f0)
        || !map.check_invariants()
    {
        return false;
    }

    // Concrete/ghost "enabled" failure: frame already owned.
    if !matches!(
        map.map(guest, gpa1, f0, EptPermissions::READ_WRITE),
        Err(EptError::AlreadyOwned)
    ) {
        return false;
    }
    if map.owner_of_gpa(guest, gpa1).is_some() || !map.check_invariants() {
        return false;
    }

    if map
        .map(guest, gpa1, f1, EptPermissions::READ_WRITE)
        .is_err()
    {
        return false;
    }
    if map.len() != 2 || !map.check_invariants() {
        return false;
    }

    match map.unmap(guest, gpa0) {
        Ok(f) if f == f0 => {}
        _ => return false,
    }
    if map.owner_of(f0).is_some() || map.len() != 1 || !map.check_invariants() {
        return false;
    }

    // Remap freed frame — enabled again after unmap.
    if map
        .map(guest, gpa0, f0, EptPermissions::READ_WRITE)
        .is_err()
    {
        return false;
    }
    map.check_invariants() && map.owner_of(f0) == Some(guest) && map.len() == 2
}

/// Full M3.18 artifact + correspondence gate (does not run Verus).
pub fn run_l3_refine_gate() -> bool {
    ept_model_opts_into_verus()
        && ept_model_has_refine()
        && l3_refine_scripts_present()
        && prop_live_map_unmap_refines()
}

#[cfg(test)]
#[path = "l3_refine_gate_test.rs"]
mod l3_refine_gate_test;
