//! M3.14 host verification gate (Verus L3 *attempt* for ADR-004).
//!
//! Pillar: [V]
//! Proven Core: companion to `ept` / `ept_proof` (not a boot path).
//!
//! Closes M3.14 without a serial / Latitude marker: host `cargo test` must
//! pass [`run_l3_gate`], which checks that the L3 proof attempt artifacts are
//! present and that the concrete 4K single-guest exclusivity properties those
//! lemmas claim still hold.
//!
//! Ghost-model true L3 is M3.17 (`l3_verify_gate` / `RAYNU-V-M3-L3-VERIFY-OK`).
//! Live `EptMap` remains L2 until ghost↔exec refinement.

use crate::memory::ept::{self, EptError, EptMap, EptPermissions, M2_BRINGUP_GUEST_ID};
use crate::memory::frame_allocator::PhysFrame;
use crate::memory::l2_gate::{self, ept_spec_is_l2};

/// Host / CI marker when the M3.14 L3-attempt gate passes.
pub const M3_L3_OK_MARKER: &str = "RAYNU-V-M3-L3-OK";

/// True when `ept_proof.rs` documents the M3.14 L3 attempt + remaining gaps.
pub fn ept_proof_is_l3_attempt() -> bool {
    let s = include_str!("ept_proof.rs");
    s.contains("VERIFICATION: **L3-attempt**")
        && s.contains("lemma_map_ok_exclusive")
        && s.contains("lemma_unmap_ok_exclusive")
        && s.contains("theorem_single_guest_4k_map_unmap_exclusive")
        && s.contains("GAP(CLOSED M3.17): Linked `ept_model` lemmas discharged without `admit()`")
        && s.contains("GAP(CLOSED M4.6): N concurrent guests")
        && s.contains("GAP(CLOSED M4.7): N-guest L3 discharge")
        && s.contains("GAP: Live migration page transfer")
        && s.contains("GAP: Hardware EPT PTE correspondence")
}

/// True when the proof file does not claim live `EptMap` is fully L3.
pub fn ept_proof_does_not_claim_l3_complete() -> bool {
    let s = include_str!("ept_proof.rs");
    s.contains("Live `EptMap` runtime maturity stays **L2**")
        && s.contains(
            "GAP(CLOSED M3.18): Ghost model refined against concrete ownership view of `EptMap`",
        )
}

/// Concrete 4K single-guest map/unmap exclusivity (lemma target of M3.14).
pub fn prop_single_guest_4k_map_unmap_exclusive() -> bool {
    let mut map = EptMap::new();
    let guest = M2_BRINGUP_GUEST_ID;
    let gpa0 = 0x1000u64;
    let gpa1 = 0x2000u64;
    let f0 = PhysFrame(10);
    let f1 = PhysFrame(11);

    if map
        .map(guest, gpa0, f0, EptPermissions::READ_WRITE)
        .is_err()
    {
        return false;
    }
    if !map.check_invariants() || map.owner_of(f0) != Some(guest) {
        return false;
    }

    // Same HPA, different GPA → AlreadyOwned (exclusive ownership).
    if !matches!(
        map.map(guest, gpa1, f0, EptPermissions::READ_WRITE),
        Err(EptError::AlreadyOwned)
    ) {
        return false;
    }

    // Distinct HPA succeeds; still exclusive.
    if map
        .map(guest, gpa1, f1, EptPermissions::READ_WRITE)
        .is_err()
    {
        return false;
    }
    if !map.check_invariants() {
        return false;
    }

    // Unmap restores availability of HPA.
    match map.unmap(guest, gpa0) {
        Ok(f) if f == f0 => {}
        _ => return false,
    }
    if map.owner_of(f0).is_some() || !map.check_invariants() {
        return false;
    }
    if map
        .map(guest, gpa0, f0, EptPermissions::READ_WRITE)
        .is_err()
    {
        return false;
    }

    // Second guest still cannot steal an owned HPA (lemma scope is single-guest
    // steps, but exclusivity is global — keep the ADR-004 check).
    matches!(
        map.map(2, 0x3000, f1, EptPermissions::READ_WRITE),
        Err(EptError::AlreadyOwned)
    ) && map.check_invariants()
        && ept::run_ownership_selftest(0x4000, 0x5000, 0x6000).is_ok()
}

/// Full M3.14 gate: L3-attempt artifacts + L2 specs retained + exclusivity props.
pub fn run_l3_gate() -> bool {
    if !ept_proof_is_l3_attempt() || !ept_proof_does_not_claim_l3_complete() {
        return false;
    }
    // L3 attempt sits on top of L2 specs; do not regress the M2.6 floor.
    if !ept_spec_is_l2() || !l2_gate::run_l2_gate() {
        return false;
    }
    if !prop_single_guest_4k_map_unmap_exclusive() {
        return false;
    }
    true
}

#[cfg(test)]
#[path = "l3_gate_test.rs"]
mod l3_gate_test;
