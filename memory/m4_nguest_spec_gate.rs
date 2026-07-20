//! M4.6 host verification gate (N-guest exclusivity in ghost model).
//!
//! Pillar: [V]
//! Proven Core: companion to `ept_model` + live `EptMap` (not a boot path).
//!
//! Checks that `ept_model` carries `theorem_n_guest_4k_map_unmap_exclusive`,
//! that `ept_spec` / `ept_proof` close the N-guest TODO/GAP (spec side), and
//! that live `EptMap` rejects HPA sharing across two guests. Does **not** claim
//! ADR-006 L3 for N guests (`RAYNU-V-M4-NGUEST-VERIFY-OK` is M4.7).
//! Runtime `cargo verus verify -p ept_model` is exercised by
//! `tools/verus-nguest-spec-smoke.sh`.

use crate::memory::ept::{EptError, EptMap, EptPermissions, M2_BRINGUP_GUEST_ID, M4_GUEST1_ID};
use crate::memory::frame_allocator::PhysFrame;

/// Host / CI marker when the M4.6 N-guest-spec gate passes.
pub const M4_NGUEST_SPEC_OK_MARKER: &str = "RAYNU-V-M4-NGUEST-SPEC-OK";

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

/// True when N-guest ghost artifacts are present (no admit; marker embedded).
pub fn ept_model_has_n_guest_spec() -> bool {
    let s = include_str!("../ept_model/src/lib.rs");
    s.contains("enum MapUnmapStep")
        && s.contains("theorem_n_guest_4k_map_unmap_exclusive")
        && s.contains("theorem_single_guest_4k_map_unmap_exclusive")
        && s.contains("step_guest_ok")
        && s.contains(M4_NGUEST_SPEC_OK_MARKER)
        && !source_has_admit_call(s)
        && !s.contains("RAYNU-V-M4-NGUEST-VERIFY-OK")
}

/// True when ept_spec / ept_proof close N-guest TODO/GAP on the spec side.
pub fn ept_spec_closes_n_guest_todo() -> bool {
    let spec = include_str!("ept_spec.rs");
    let proof = include_str!("ept_proof.rs");
    spec.contains("TODO(M4.6 CLOSED): N guests in ghost model")
        && spec.contains("TODO(M4.8): large pages")
        && !spec.contains("TODO(M4): N guests + large pages")
        && proof.contains("GAP(CLOSED M4.6): N concurrent guests")
        && proof.contains("GAP: N-guest L3 discharge")
        && proof.contains("theorem_n_guest_4k_map_unmap_exclusive")
}

/// True when the N-guest-spec smoke script is present.
pub fn nguest_spec_scripts_present() -> bool {
    let smoke = include_str!("../tools/verus-nguest-spec-smoke.sh");
    smoke.contains("cargo verus verify -p ept_model")
        && smoke.contains(M4_NGUEST_SPEC_OK_MARKER)
        && smoke.contains("theorem_n_guest_4k_map_unmap_exclusive")
        && smoke.contains("install-verus.sh")
        && smoke.contains("0 errors")
}

/// Live `EptMap`: two distinct guests cannot share an HPA (N-guest exclusivity).
pub fn prop_n_guest_hpa_exclusive() -> bool {
    let mut map = EptMap::new();
    let g0 = M2_BRINGUP_GUEST_ID;
    let g1 = M4_GUEST1_ID;
    let gpa0 = 0x1000u64;
    let gpa1 = 0x2000u64;
    let f0 = PhysFrame(30);
    let f1 = PhysFrame(31);

    if map
        .map(g0, gpa0, f0, EptPermissions::READ_WRITE)
        .is_err()
    {
        return false;
    }
    if map.owner_of(f0) != Some(g0) || !map.check_invariants() {
        return false;
    }

    // Second guest cannot steal g0's HPA.
    if !matches!(
        map.map(g1, gpa1, f0, EptPermissions::READ_WRITE),
        Err(EptError::AlreadyOwned)
    ) {
        return false;
    }
    if map.owner_of_gpa(g1, gpa1).is_some() || !map.check_invariants() {
        return false;
    }

    // Distinct HPA for g1 succeeds; both owners present.
    if map
        .map(g1, gpa1, f1, EptPermissions::READ_WRITE)
        .is_err()
    {
        return false;
    }
    if map.owner_of(f1) != Some(g1) || map.len() != 2 || !map.check_invariants() {
        return false;
    }

    // Unmap g0 frees f0 for remapping by either guest.
    match map.unmap(g0, gpa0) {
        Ok(f) if f == f0 => {}
        _ => return false,
    }
    if map.owner_of(f0).is_some() || !map.check_invariants() {
        return false;
    }
    if map
        .map(g1, gpa0, f0, EptPermissions::READ_WRITE)
        .is_err()
    {
        return false;
    }
    map.check_invariants()
        && map.owner_of(f0) == Some(g1)
        && map.owner_of(f1) == Some(g1)
        && map.len() == 2
}

/// Full M4.6 artifact + exclusivity gate (does not run Verus).
pub fn run_m4_nguest_spec_gate() -> bool {
    ept_model_opts_into_verus()
        && ept_model_has_n_guest_spec()
        && ept_spec_closes_n_guest_todo()
        && nguest_spec_scripts_present()
        && prop_n_guest_hpa_exclusive()
}

#[cfg(test)]
#[path = "m4_nguest_spec_gate_test.rs"]
mod m4_nguest_spec_gate_test;
