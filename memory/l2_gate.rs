//! M2.6 host verification gate (L2 specs + property self-check).
//!
//! Pillar: [V]
//! Proven Core: companion to `ept` / `frame_allocator` (not a boot path).
//!
//! Closes M2.6 without a serial marker: host `cargo test` must pass
//! [`run_l2_gate`], which checks L2 spec artifacts and re-runs core
//! ADR-004 / allocator properties.

use crate::memory::ept::{self, EptError, EptMap, EptPermissions, M2_BRINGUP_GUEST_ID};
use crate::memory::frame_allocator::{self, AllocError, FrameAllocator, PhysFrame};

/// Host / CI marker when the M2.6 L2 gate passes.
pub const M2_L2_OK_MARKER: &str = "RAYNU-V-M2-L2-OK";

/// True when the EPT L2 spec text is present in-tree.
pub fn ept_spec_is_l2() -> bool {
    let s = include_str!("ept_spec.rs");
    s.contains("VERIFICATION: **L2**") && s.contains("ghost Owned:")
}

/// True when the allocator L2 spec text is present in-tree.
pub fn allocator_spec_is_l2() -> bool {
    let s = include_str!("frame_allocator_spec.rs");
    s.contains("VERIFICATION: **L2**") && s.contains("ghost Allocated:")
}

/// ADR-004 exclusive-ownership property (concrete, bounded).
pub fn prop_no_hpa_alias() -> bool {
    let mut map = EptMap::new();
    if map
        .map(
            M2_BRINGUP_GUEST_ID,
            0x1000,
            PhysFrame(10),
            EptPermissions::READ_WRITE,
        )
        .is_err()
    {
        return false;
    }
    matches!(
        map.map(2, 0x2000, PhysFrame(10), EptPermissions::READ_WRITE),
        Err(EptError::AlreadyOwned)
    ) && map.check_invariants()
}

/// Allocator: distinct alloc, double-free reject, reuse (concrete, bounded).
pub fn prop_allocator_integrity() -> bool {
    let mut words = [0u64; 64];
    // SAFETY: stack bitmap owned for this call; capacity fits in `words`.
    let mut a = match unsafe { FrameAllocator::new(0x1000, 4, words.as_mut_ptr() as u64) } {
        Ok(v) => v,
        Err(_) => return false,
    };
    let f0 = match a.allocate_frame() {
        Some(f) => f,
        None => return false,
    };
    let f1 = match a.allocate_frame() {
        Some(f) => f,
        None => return false,
    };
    if f0 == f1 {
        return false;
    }
    if a.free_frame(f0).is_err() {
        return false;
    }
    if a.free_frame(f0) != Err(AllocError::DoubleFree) {
        return false;
    }
    let f2 = match a.allocate_frame() {
        Some(f) => f,
        None => return false,
    };
    f2 == f0 && frame_allocator::run_allocator_selftest(&mut a).is_ok()
}

/// Full M2.6 gate: L2 specs present + ownership selftest + allocator props.
pub fn run_l2_gate() -> bool {
    if !ept_spec_is_l2() || !allocator_spec_is_l2() {
        return false;
    }
    if !prop_no_hpa_alias() {
        return false;
    }
    if ept::run_ownership_selftest(0x2000, 0x3000, 0x4000).is_err() {
        return false;
    }
    if !prop_allocator_integrity() {
        return false;
    }
    true
}

#[cfg(test)]
#[path = "l2_gate_test.rs"]
mod l2_gate_test;
