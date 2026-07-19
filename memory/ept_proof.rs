//! Verus L3 proof sketch for EPT isolation (ADR-004), M3.14 → M3.17.
//!
//! VERIFICATION: **L3** (scoped ghost model + refine) — `ept_model` exclusivity
//! lemmas for 4K single-guest map/unmap are discharged (M3.17) and refined
//! against a concrete ownership view of `EptMap` (M3.18) with **no `admit()`**
//! → `RAYNU-V-M3-L3-VERIFY-OK` / `RAYNU-V-M3-L3-REFINE-OK`.
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
//! GAP: N concurrent guests (ADR-004 M4 row)
//! GAP: Large pages (2M/1G) in ghost model and proof (M4/M5)
//! GAP: EPT violation handler preserves exclusivity
//! GAP: Live migration page transfer (M6)
//! GAP: Hardware EPT PTE correspondence (`ept_hw` identity builder)
//! GAP: Precise range registry (`EptRangeMap`) vs per-page lemmas
//! GAP: Frame-allocator coupling (alloc ⇒ exclusive claim) at L3
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
//!         guest == BRINGUP_GUEST,
//!         page_aligned_4k(gpa),
//!         exclusive_ownership(m),
//!         !m.Owned.contains(frame),
//!         !m.ByGpa.contains((guest, gpa)),
//!         m2 == ghost_map(m, guest, gpa, frame),
//!     ensures
//!         exclusive_ownership(m2),
//!         m2.Owned[frame] == guest,
//!         m2.ByGpa[(guest, gpa)] == frame,
//! { /* discharged in ept_model (M3.17) */ }
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
//!         guest == BRINGUP_GUEST,
//!         page_aligned_4k(gpa),
//!         exclusive_ownership(m),
//!         m.ByGpa[(guest, gpa)] == frame,
//!         m2 == ghost_unmap(m, guest, gpa),
//!     ensures
//!         exclusive_ownership(m2),
//!         !m2.Owned.contains(frame),
//!         !m2.ByGpa.contains((guest, gpa)),
//! { /* discharged in ept_model (M3.17) */ }
//!
//! // ---- theorem: single-guest 4K map/unmap preserve exclusivity ----
//! proof fn theorem_single_guest_4k_map_unmap_exclusive(
//!     m: GhostEptMap,
//!     steps: Seq<MapUnmapStep>,
//! )
//!     requires
//!         exclusive_ownership(m),
//!         forall|i| #![auto] steps[i].guest == BRINGUP_GUEST,
//!         forall|i| #![auto] page_aligned_4k(steps[i].gpa),
//!     ensures
//!         exclusive_ownership(fold_steps(m, steps)),
//! {
//!     // Discharged in ept_model (M3.17): induct on steps; case-split map/unmap
//!     // via lemma_map_ok_exclusive / lemma_unmap_ok_exclusive.
//! }
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
