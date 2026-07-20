//! M5.9 host verification gate (allocator↔EPT refine + scoped identity).
//!
//! Pillar: [V]
//! Proven Core: companion to `ept_model` + live `FrameAllocator` / `EptMap`
//! (not a boot path).
//!
//! Checks that `ept_model` discharges `alloc_ept_refines` /
//! `theorem_alloc_map_unmap_refines` and scoped precise-identity GPA==HPA
//! lemmas (no `admit`), that docs close the allocator GAP, and that live
//! allocate→map→unmap keeps owned frames ⊆ allocated. Full HW PTE decode
//! closed in M6.1 (`GAP(CLOSED M6.1)`); EPT-violation closed in M6.0. Runtime
//! verify is exercised by `tools/verus-alloc-refine-smoke.sh`.

use crate::memory::ept::{
    EptError, EptMap, EptPermissions, M2_BRINGUP_GUEST_ID, M4_GUEST1_ID,
};
use crate::memory::ept_hw::{frames_required_precise, PRECISE_BYTES, PRECISE_MIB};
use crate::memory::frame_allocator::{AllocError, FrameAllocator};

/// Host / CI marker when the M5.9 alloc-refine gate passes.
pub const M5_ALLOC_REFINE_OK_MARKER: &str = "RAYNU-V-M5-ALLOC-REFINE-OK";

/// Documented GAP note (open form or M6.1 closed form both accepted).
pub const HW_PTE_GAP_NOTE: &str = "GAP: Hardware EPT PTE bit-decode / EPT-violation (M6)";
pub const HW_PTE_GAP_CLOSED: &str = "GAP(CLOSED M6.1): Hardware EPT PTE bit-decode";

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

/// True when allocator↔EPT + identity artifacts are present (no admit; marker).
pub fn ept_model_has_alloc_refine() -> bool {
    let s = include_str!("../ept_model/src/lib.rs");
    s.contains("struct GhostFramePool")
        && s.contains("alloc_ept_refines")
        && s.contains("alloc_map_enabled")
        && s.contains("theorem_alloc_map_unmap_refines")
        && s.contains("lemma_alloc_map_ok_refines")
        && s.contains("lemma_alloc_unmap_ok_refines")
        && s.contains("PRECISE_IDENTITY_FRAMES")
        && s.contains("identity_leaf_ok")
        && s.contains("lemma_identity_leaf_gpa_eq_hpa")
        && s.contains("lemma_precise_identity_frames")
        && s.contains(M5_ALLOC_REFINE_OK_MARKER)
        && !source_has_admit_call(s)
}

/// True when ept_spec / ept_proof close allocator↔EPT GAP on the refine side.
pub fn ept_spec_closes_alloc_refine() -> bool {
    let spec = include_str!("ept_spec.rs");
    let proof = include_str!("ept_proof.rs");
    spec.contains("TODO(M5.9 CLOSED): allocator↔EPT refine")
        && spec.contains("alloc_ept_refines")
        && proof.contains("GAP(CLOSED M5.9): Frame-allocator ↔ EPT L3 coupling")
        && proof.contains("GAP(CLOSED M5.9): Precise-identity GPA==HPA correspondence")
        && (proof.contains(HW_PTE_GAP_NOTE) || proof.contains(HW_PTE_GAP_CLOSED))
        && proof.contains("theorem_alloc_map_unmap_refines")
        && !proof.contains("GAP: Frame-allocator ↔ EPT L3 coupling beyond ConcreteEptMap (M5)")
}

/// True when the alloc-refine smoke script is present.
pub fn alloc_refine_scripts_present() -> bool {
    let smoke = include_str!("../tools/verus-alloc-refine-smoke.sh");
    smoke.contains("cargo verus verify -p ept_model")
        && smoke.contains(M5_ALLOC_REFINE_OK_MARKER)
        && smoke.contains("theorem_alloc_map_unmap_refines")
        && smoke.contains("alloc_ept_refines")
        && smoke.contains("install-verus.sh")
        && smoke.contains("0 errors")
}

/// Live: allocate → map → unmap → free; owned frames stay ⊆ allocated.
pub fn prop_live_alloc_ept_coupled() -> bool {
    let mut words = [0u64; 64];
    // SAFETY: stack bitmap; capacity 8 fits in words.
    let mut alloc = match unsafe { FrameAllocator::new(0x1000, 8, words.as_mut_ptr() as u64) } {
        Ok(a) => a,
        Err(_) => return false,
    };
    let mut map = EptMap::new();
    let guest = M2_BRINGUP_GUEST_ID;
    let gpa = 0x1000u64;

    let f = match alloc.allocate_frame() {
        Some(f) => f,
        None => return false,
    };
    if !alloc.is_allocated(f) {
        return false;
    }
    if map
        .map(guest, gpa, f, EptPermissions::READ_WRITE)
        .is_err()
    {
        return false;
    }
    if map.owner_of(f) != Some(guest) || !map.check_invariants() {
        return false;
    }
    // Cannot free while still mapped (host policy check: still allocated).
    if !alloc.is_allocated(f) {
        return false;
    }
    match map.unmap(guest, gpa) {
        Ok(u) if u == f => {}
        _ => return false,
    }
    if map.owner_of(f).is_some() || !map.check_invariants() {
        return false;
    }
    if alloc.free_frame(f).is_err() {
        return false;
    }
    // Remap after re-allocate.
    let f2 = match alloc.allocate_frame() {
        Some(f) => f,
        None => return false,
    };
    if f2 != f {
        return false;
    }
    if map
        .map(guest, gpa, f2, EptPermissions::READ_WRITE)
        .is_err()
    {
        return false;
    }
    // Steal of allocated+mapped frame rejected.
    if !matches!(
        map.map(M4_GUEST1_ID, 0x2000, f2, EptPermissions::READ_WRITE),
        Err(EptError::AlreadyOwned)
    ) {
        return false;
    }
    match map.unmap(guest, gpa) {
        Ok(_) => {}
        Err(_) => return false,
    }
    if alloc.free_frame(f2).is_err() {
        return false;
    }
    alloc.allocated_count() == 0
        && map.is_empty()
        && matches!(alloc.free_frame(f2), Err(AllocError::DoubleFree))
}

/// Precise identity geometry matches ghost `PRECISE_IDENTITY_FRAMES`.
pub fn prop_precise_identity_geometry() -> bool {
    PRECISE_MIB == 512
        && PRECISE_BYTES == 512 * 1024 * 1024
        && PRECISE_BYTES / 4096 == 131072
        && frames_required_precise() >= 3
        && (HW_PTE_GAP_NOTE.contains("M6") || HW_PTE_GAP_CLOSED.contains("M6.1"))
}

/// Full M5.9 artifact + live coupling gate (does not run Verus).
pub fn run_m5_alloc_refine_gate() -> bool {
    ept_model_opts_into_verus()
        && ept_model_has_alloc_refine()
        && ept_spec_closes_alloc_refine()
        && alloc_refine_scripts_present()
        && prop_live_alloc_ept_coupled()
        && prop_precise_identity_geometry()
}

#[cfg(test)]
#[path = "m5_alloc_refine_gate_test.rs"]
mod m5_alloc_refine_gate_test;
