//! M4.8 host verification gate (large-page ghost *spec* for ADR-004).
//!
//! Pillar: [V]
//! Proven Core: companion to `ept_model` + live `EptMap` / `EptRangeMap` (not a boot path).
//!
//! Checks that `ept_model` carries 2M/1G leaf sizes and span predicates, that
//! `ept_spec` / `ept_proof` close the large-page TODO/GAP on the *spec* side,
//! and that live ownership rejects HPA sharing across a multi-frame span
//! (4K stand-in for a large leaf). Large-page L3 discharge remains M5.
//! Runtime `cargo verus verify -p ept_model` is exercised by
//! `tools/verus-lpage-spec-smoke.sh`.

use crate::memory::ept::{
    EptError, EptMap, EptPermissions, EptRangeMap, M2_BRINGUP_GUEST_ID, M4_GUEST1_ID,
};
use crate::memory::frame_allocator::PhysFrame;

/// Host / CI marker when the M4.8 large-page-spec gate passes.
pub const M4_LPAGE_OK_MARKER: &str = "RAYNU-V-M4-LPAGE-OK";

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

/// True when large-page ghost *spec* artifacts are present (no admit; marker).
pub fn ept_model_has_lpage_spec() -> bool {
    let s = include_str!("../ept_model/src/lib.rs");
    s.contains("enum GhostPageSize")
        && s.contains("PAGE_2M")
        && s.contains("PAGE_1G")
        && s.contains("frames_covered")
        && s.contains("large_map_enabled")
        && s.contains("large_map_post_owned")
        && s.contains("lemma_2m_covers_512_frames")
        && s.contains("lemma_1g_covers_262144_frames")
        && s.contains(M4_LPAGE_OK_MARKER)
        && !source_has_admit_call(s)
}

/// True when ept_spec / ept_proof close large-page TODO/GAP on the spec side.
pub fn ept_spec_closes_lpage_todo() -> bool {
    let spec = include_str!("ept_spec.rs");
    let proof = include_str!("ept_proof.rs");
    spec.contains("TODO(M4.8 CLOSED): large pages")
        && spec.contains("large_map_enabled")
        && proof.contains("GAP(CLOSED M4.8): Large pages")
        && proof.contains("GAP: Large-page L3 discharge")
}

/// True when the large-page-spec smoke script is present.
pub fn lpage_spec_scripts_present() -> bool {
    let smoke = include_str!("../tools/verus-lpage-spec-smoke.sh");
    smoke.contains("cargo verus verify -p ept_model")
        && smoke.contains(M4_LPAGE_OK_MARKER)
        && smoke.contains("GhostPageSize")
        && smoke.contains("large_map_enabled")
        && smoke.contains("install-verus.sh")
        && smoke.contains("0 errors")
}

/// Live `EptMap`: a multi-frame span (2M stand-in) is exclusive across guests.
///
/// Maps four consecutive 4K frames for G0 (miniature large leaf); G1 cannot
/// claim any frame in that span.
pub fn prop_large_span_hpa_exclusive() -> bool {
    let mut map = EptMap::new();
    let g0 = M2_BRINGUP_GUEST_ID;
    let g1 = M4_GUEST1_ID;
    let base = 100u64;
    let span = 4u64; // stand-in for frames_covered(TwoM)=512

    for i in 0..span {
        let gpa = 0x10_0000 + i * 0x1000;
        let f = PhysFrame(base + i);
        if map
            .map(g0, gpa, f, EptPermissions::READ_WRITE)
            .is_err()
        {
            return false;
        }
    }
    if map.len() != span as usize || !map.check_invariants() {
        return false;
    }

    for i in 0..span {
        let f = PhysFrame(base + i);
        if map.owner_of(f) != Some(g0) {
            return false;
        }
        if !matches!(
            map.map(g1, 0x20_0000 + i * 0x1000, f, EptPermissions::READ_WRITE),
            Err(EptError::AlreadyOwned)
        ) {
            return false;
        }
    }
    map.check_invariants()
}

/// Live `EptRangeMap`: overlapping 2 MiB identity ranges cannot be dual-owned.
pub fn prop_range_2m_no_overlap() -> bool {
    let mut ranges = EptRangeMap::new();
    let g0 = M2_BRINGUP_GUEST_ID;
    let g1 = M4_GUEST1_ID;
    let len_2m = 0x20_0000u64;

    if ranges.claim_range(g0, 0, len_2m).is_err() {
        return false;
    }
    // Overlapping 2M claim by second guest must fail.
    if ranges.claim_range(g1, 0x10_0000, len_2m).is_ok() {
        return false;
    }
    // Adjacent non-overlapping 2M succeeds.
    if ranges.claim_range(g1, len_2m, len_2m).is_err() {
        return false;
    }
    ranges.contains_gpa(g0, 0)
        && ranges.contains_gpa(g0, len_2m - 0x1000)
        && ranges.contains_gpa(g1, len_2m)
        && !ranges.contains_gpa(g0, len_2m)
}

/// Full M4.8 artifact + span exclusivity gate (does not run Verus).
pub fn run_m4_lpage_gate() -> bool {
    ept_model_opts_into_verus()
        && ept_model_has_lpage_spec()
        && ept_spec_closes_lpage_todo()
        && lpage_spec_scripts_present()
        && prop_large_span_hpa_exclusive()
        && prop_range_2m_no_overlap()
}

#[cfg(test)]
#[path = "m4_lpage_gate_test.rs"]
mod m4_lpage_gate_test;
