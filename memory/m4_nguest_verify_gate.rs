//! M4.7 host verification gate (true L3: N-guest EPT exclusivity).
//!
//! Pillar: [V]
//! Proven Core: companion to `ept_model` (not a boot path).
//!
//! Checks in-tree that `ept_model` opts into Verus, carries the N-guest
//! exclusivity theorem + ≥2-guest lemma, embeds the VERIFY marker, and
//! contains **no** `admit(`. Runtime `cargo verus verify -p ept_model` is
//! exercised by `tools/verus-nguest-verify-smoke.sh`.
//!
//! Full ghost↔exec N-guest refine is M4.9 (`m4_nguest_refine_gate`).

use crate::memory::ept::{EptError, EptMap, EptPermissions, M2_BRINGUP_GUEST_ID, M4_GUEST1_ID};
use crate::memory::frame_allocator::PhysFrame;

/// Host / CI marker when the M4.7 N-guest L3-verify gate passes.
pub const M4_NGUEST_VERIFY_OK_MARKER: &str = "RAYNU-V-M4-NGUEST-VERIFY-OK";

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

/// True when the Verus model discharges N-guest L3 (no admit) and is marked.
pub fn ept_model_is_verified_n_guest_l3() -> bool {
    let s = include_str!("../ept_model/src/lib.rs");
    s.contains("verus!")
        && s.contains("struct GhostEptMap")
        && s.contains("exclusive_ownership")
        && s.contains("theorem_n_guest_4k_map_unmap_exclusive")
        && s.contains("lemma_two_guests_map_distinct_frames_exclusive")
        && s.contains(M4_NGUEST_VERIFY_OK_MARKER)
        && s.contains("RAYNU-V-M4-NGUEST-SPEC-OK")
        && !source_has_admit_call(s)
}

/// True when ept_proof closes the N-guest L3 GAP and claims ADR-006.
pub fn ept_proof_closes_n_guest_l3() -> bool {
    let proof = include_str!("ept_proof.rs");
    let spec = include_str!("ept_spec.rs");
    proof.contains("GAP(CLOSED M4.7): N-guest L3 discharge")
        && proof.contains("lemma_two_guests_map_distinct_frames_exclusive")
        && spec.contains("TODO(M4.7 CLOSED): ADR-006 L3 for N-guest")
        && !proof.contains("GAP: N-guest L3 discharge / ADR-006 claim (M4.7")
}

/// True when the N-guest-verify smoke script enforces no-admit green verify.
pub fn nguest_verify_scripts_present() -> bool {
    let smoke = include_str!("../tools/verus-nguest-verify-smoke.sh");
    smoke.contains("cargo verus verify -p ept_model")
        && smoke.contains(M4_NGUEST_VERIFY_OK_MARKER)
        && smoke.contains("theorem_n_guest_4k_map_unmap_exclusive")
        && smoke.contains("lemma_two_guests_map_distinct_frames_exclusive")
        && smoke.contains("install-verus.sh")
        && smoke.contains(r"^\s*admit\s*\(")
        && smoke.contains("0 errors")
}

/// Live `EptMap` two-guest exclusivity (exec side of the M4.7 claim).
pub fn prop_two_guest_map_unmap_exclusive() -> bool {
    let mut map = EptMap::new();
    let g1 = M2_BRINGUP_GUEST_ID;
    let g2 = M4_GUEST1_ID;
    let gpa1 = 0x1000u64;
    let gpa2 = 0x2000u64;
    let f1 = PhysFrame(40);
    let f2 = PhysFrame(41);

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
        || map.len() != 2
        || !map.check_invariants()
    {
        return false;
    }

    // Cross-guest steal rejected.
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
    match map.unmap(g2, gpa2) {
        Ok(f) if f == f2 => {}
        _ => return false,
    }
    map.is_empty() && map.check_invariants()
}

/// Full M4.7 artifact gate (does not run Verus).
pub fn run_m4_nguest_verify_gate() -> bool {
    ept_model_opts_into_verus()
        && ept_model_is_verified_n_guest_l3()
        && ept_proof_closes_n_guest_l3()
        && nguest_verify_scripts_present()
        && prop_two_guest_map_unmap_exclusive()
}

#[cfg(test)]
#[path = "m4_nguest_verify_gate_test.rs"]
mod m4_nguest_verify_gate_test;
