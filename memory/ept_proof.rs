//! Verus L3 proof sketch for EPT isolation (ADR-004), M3.14 → M3.17 / M4.6.
//!
//! VERIFICATION: **L3** (scoped ghost model + refine) — `ept_model` exclusivity
//! lemmas for 4K map/unmap are discharged (M3.17) and refined against a concrete
//! ownership view of `EptMap` (M3.18) with **no `admit()`**
//! → `RAYNU-V-M3-L3-VERIFY-OK` / `RAYNU-V-M3-L3-REFINE-OK`.
//! M4.6 extends posts to N guests (`theorem_n_guest_4k_map_unmap_exclusive`) →
//! `RAYNU-V-M4-NGUEST-SPEC-OK`; M4.7 claims ADR-006 L3 for N guests →
//! `RAYNU-V-M4-NGUEST-VERIFY-OK`; M4.9 extends concrete refine to N guests →
//! `RAYNU-V-M4-REFINE-OK`.
//! Live `EptMap` runtime maturity stays **L2** (asserts + Kani); remaining GAPs below.
//! Runtime asserts and Kani harnesses remain defense-in-depth.
//!
//! Historical M3.14 tag retained for the host L3-attempt gate:
//! VERIFICATION: **L3-attempt**
//!
//! Linked + verified model: `ept_model/` → `RAYNU-V-M3-L3-LINK-OK` /
//! `RAYNU-V-M3-L3-VERIFY-OK`.
//!
//! # Scope (M3.14–M3.17)
//!
//! - Single guest (`guest_id = BRINGUP_GUEST`)
//! - 4K page granularity (GPA/HPA page-aligned)
//! - Operations: `map`, `unmap`
//! - Property: exclusive HPA ownership + unique `(guest, gpa)` after each step
//!
//! # Out of scope / documented gaps
//!
//! ```text
//! GAP(CLOSED M3.17): Linked `ept_model` lemmas discharged without `admit()`
//! GAP(CLOSED M3.18): Ghost model refined against concrete ownership view of `EptMap`
//! GAP(CLOSED M4.6): N concurrent guests in ghost model (spec OK; marker RAYNU-V-M4-NGUEST-SPEC-OK)
//! GAP(CLOSED M4.7): N-guest L3 discharge / ADR-006 claim (marker RAYNU-V-M4-NGUEST-VERIFY-OK)
//! GAP(CLOSED M4.8): Large pages (2M/1G) in ghost model (spec OK; marker RAYNU-V-M4-LPAGE-OK)
//! GAP(CLOSED M4.9): N-guest ghost↔exec refine (marker RAYNU-V-M4-REFINE-OK)
//! GAP: Large-page L3 discharge (M5)
//! GAP: Frame-allocator ↔ EPT L3 coupling beyond ConcreteEptMap (M5)
//! GAP: EPT violation handler preserves exclusivity
//! GAP: Live migration page transfer (M6)
//! GAP: Hardware EPT PTE correspondence (`ept_hw` identity builder)
//! GAP: Precise range registry (`EptRangeMap`) vs per-page lemmas
//! ```
//!
//! # Ghost model (from `ept_spec.rs`)
//!
//! ```text
//! ghost Owned: Map<PhysFrame /* HPA */, GuestId>
//! ghost ByGpa: Map<(GuestId, Gpa), PhysFrame>
//! ```
//!
//! # Lemmas (Verus-shaped sketch)
//!
//! The blocks below are the M3.14 proof attempt. Discharged bodies live in
//! `ept_model/src/lib.rs`. [`l3_gate`](crate::memory::l3_gate) checks that this
//! file still contains the lemma names and GAP list, and re-runs the concrete
//! 4K single-guest properties those lemmas claim.
//!
//! ```text
//! // ---- lemma: empty map is exclusive ----
//! proof fn lemma_empty_exclusive(m: GhostEptMap)
//!     requires m.Owned.dom().is_empty() && m.ByGpa.dom().is_empty()
//!     ensures exclusive_ownership(m)
//! { /* trivial */ }
//!
//! // ---- lemma: map Ok strengthens exclusive ownership (single guest, 4K) ----
//! proof fn lemma_map_ok_exclusive(
//!     m: GhostEptMap,
//!     m2: GhostEptMap,
//!     guest: GuestId,
//!     gpa: Gpa,
//!     frame: PhysFrame,
//! )
//!     requires
//!         guest != 0,   // M4.6: any non-zero guest (was BRINGUP_GUEST-only)
//!         page_aligned_4k(gpa),
//!         exclusive_ownership(m),
//!         !m.Owned.contains(frame),
//!         !m.ByGpa.contains((guest, gpa)),
//!         m2 == ghost_map(m, guest, gpa, frame),
//!     ensures
//!         exclusive_ownership(m2),
//!         m2.Owned[frame] == guest,
//!         m2.ByGpa[(guest, gpa)] == frame,
//! { /* discharged in ept_model (M3.17; N-guest posts M4.6) */ }
//!
//! // ---- lemma: map AlreadyOwned leaves state unchanged ----
//! proof fn lemma_map_already_owned_unchanged(
//!     m: GhostEptMap,
//!     m2: GhostEptMap,
//!     guest: GuestId,
//!     gpa: Gpa,
//!     frame: PhysFrame,
//! )
//!     requires
//!         exclusive_ownership(m),
//!         m.Owned.contains(frame) || m.ByGpa.contains((guest, gpa)),
//!         m2 == m,   // Err path
//!     ensures
//!         m2 == m,
//!         exclusive_ownership(m2),
//! { /* reframes Err posts from ept_spec */ }
//!
//! // ---- lemma: unmap Ok restores exclusive ownership ----
//! proof fn lemma_unmap_ok_exclusive(
//!     m: GhostEptMap,
//!     m2: GhostEptMap,
//!     guest: GuestId,
//!     gpa: Gpa,
//!     frame: PhysFrame,
//! )
//!     requires
//!         guest != 0,   // M4.6: any non-zero guest
//!         page_aligned_4k(gpa),
//!         exclusive_ownership(m),
//!         m.ByGpa[(guest, gpa)] == frame,
//!         m2 == ghost_unmap(m, guest, gpa),
//!     ensures
//!         exclusive_ownership(m2),
//!         !m2.Owned.contains(frame),
//!         !m2.ByGpa.contains((guest, gpa)),
//! { /* discharged in ept_model (M3.17; N-guest posts M4.6) */ }
//!
//! // ---- theorem: single-guest 4K map/unmap preserve exclusivity (M3.17 name) ----
//! proof fn theorem_single_guest_4k_map_unmap_exclusive(
//!     m: GhostEptMap,
//!     steps: Seq<MapUnmapStep>,
//! )
//!     requires
//!         exclusive_ownership(m),
//!         steps_ok(m, steps),   // each step.guest != 0
//!     ensures
//!         exclusive_ownership(fold_steps(m, steps)),
//! {
//!     // Discharged in ept_model (M3.17): induct on steps; case-split map/unmap
//!     // via lemma_map_ok_exclusive / lemma_unmap_ok_exclusive.
//! }
//!
//! // ---- theorem: N-guest 4K map/unmap exclusivity (M4.6 / M4.7 ADR-006) ----
//! proof fn theorem_n_guest_4k_map_unmap_exclusive(
//!     m: GhostEptMap,
//!     steps: Seq<MapUnmapStep>,
//! )
//!     requires
//!         exclusive_ownership(m),
//!         steps_ok(m, steps),
//!     ensures
//!         exclusive_ownership(fold_steps(m, steps)),
//! {
//!     // Discharged in ept_model; ADR-006 L3 claim → RAYNU-V-M4-NGUEST-VERIFY-OK (M4.7).
//! }
//!
//! // ---- lemma: two distinct guests map distinct frames (M4.7 ≥2-guest post) ----
//! proof fn lemma_two_guests_map_distinct_frames_exclusive(...)
//! { /* discharged in ept_model (M4.7) */ }
//!
//! // ---- M4.8: large-page ghost *spec* (L3 → M5) ----
//! // GhostPageSize::{FourK,TwoM,OneG}, frames_covered, large_map_enabled,
//! // large_map_post_owned, lemma_2m_covers_512_frames / lemma_1g_covers_262144_frames
//! // live in ept_model. Exclusivity preservation across large map/unmap is M5.
//!
//! // ---- M4.9: N-guest concrete refine ----
//! proof fn theorem_concrete_n_guest_4k_refine(c, steps)
//!     requires refines(c), concrete_steps_ok(c, steps)
//!     ensures refines(fold_concrete_steps(c, steps))
//! { /* discharged in ept_model (M4.9); no admit */ }
//! proof fn lemma_concrete_two_guests_map_refines(...)
//! { /* discharged in ept_model (M4.9) */ }
//! ```
//!
//! # Predicate glossary
//!
//! ```text
//! exclusive_ownership(m) ⇔
//!     (forall f. m.Owned.contains(f) <==> exists g,a. m.ByGpa[(g,a)] == f)
//!     && (forall g,a,a'. m.ByGpa[(g,a)] == m.ByGpa[(g,a')] ==> a == a')
//!     && |m.Owned| == |m.ByGpa|
//! ```

#![allow(dead_code)]

/// Marker string embedded for the M3.14 host L3 gate (`include_str!` check).
pub const M3_L3_ATTEMPT_TAG: &str = "VERIFICATION: **L3-attempt**";

/// Bring-up guest id used by the single-guest lemmas (matches `M2_BRINGUP_GUEST_ID`).
pub const BRINGUP_GUEST: u64 = 1;

/// 4K page size assumed by the M3.14 lemmas.
pub const PAGE_4K: u64 = 4096;
