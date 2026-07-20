//! Verus-shaped specifications for the EPT ownership registry (ADR-004).
//!
//! VERIFICATION: **L2** (spec written, M2.6) — ghost model + precise posts.
//! M3.14 drafted a Verus L3 *attempt* in `ept_proof.rs` (4K single-guest
//! map/unmap exclusivity + documented gaps). Not machine-checked until Verus
//! is pinned; maturity stays L2 (ADR-006). Kani covers bounded map/unmap in
//! `ept_test.rs`.
//!
//! # Ghost model
//!
//! ```text
//! ghost Owned: Map<PhysFrame /* HPA */, GuestId>
//! ghost ByGpa: Map<(GuestId, Gpa), PhysFrame>
//!
//! invariant forall f. Owned.contains(f) <==> exists g,a. ByGpa[(g,a)] == f
//! invariant forall g,a,a'. ByGpa[(g,a)] == ByGpa[(g,a')] ==> a == a'  // unique GPA/guest
//! invariant |Owned| == |ByGpa| == EptMap.len
//! ```
//!
//! ADR-004: every mapped HPA is exclusively owned by exactly one guest.
//!
//! # `EptMap::map(guest_id, gpa, frame, perms)`
//! requires
//!   - `guest_id != 0`
//! ensures
//!   - on `Ok`:
//!       `Owned[frame] == guest_id`
//!       `ByGpa[(guest_id, gpa)] == frame`
//!       `check_invariants()`
//!   - on `Err(AlreadyOwned)`: `Owned` / `ByGpa` unchanged
//!   - on `Err(AlreadyMapped)`: unchanged
//!   - on `Err(InvalidGuest)`: unchanged (`guest_id == 0`)
//!
//! # `EptMap::unmap(guest_id, gpa)`
//! requires
//!   - `ByGpa` contains `(guest_id, gpa)`
//! ensures
//!   - on `Ok(frame)`:
//!       `!Owned.contains(frame)`
//!       `!ByGpa.contains((guest_id, gpa))`
//!       `check_invariants()`
//!   - on `Err(NotMapped)`: unchanged
//!
//! # `EptMap::owner_of` / `check_invariants`
//! ensures
//!   - `owner_of(f) == Owned.get(f)`
//!   - `check_invariants()` ⇔ ghost invariants hold over the concrete table
//!
//! # `run_ownership_selftest`
//! ensures
//!   - bring-up guest uniquely owns code + stack + IDT frames
//!   - a second guest cannot map the code HPA (`AlreadyOwned`)
//!   - unmap then remap of stack succeeds
//!
//! TODO(M3.14 done as attempt): discharge `theorem_single_guest_4k_map_unmap_exclusive`
//!   under a pinned Verus (`verus-version.toml`) — see GAP list in `ept_proof.rs`.
//! TODO(M4.6 CLOSED): N guests in ghost model — `MapUnmapStep.guest` +
//!   `theorem_n_guest_4k_map_unmap_exclusive` in `ept_model` → `RAYNU-V-M4-NGUEST-SPEC-OK`.
//! TODO(M4.7 CLOSED): ADR-006 L3 for N-guest 4K map/unmap — no `admit` →
//!   `RAYNU-V-M4-NGUEST-VERIFY-OK` (`lemma_two_guests_map_distinct_frames_exclusive`).
//! TODO(M4.8 CLOSED): large pages (2M/1G) in ghost *spec* — `GhostPageSize` /
//!   `large_map_enabled` / `frames_covered` in `ept_model` → `RAYNU-V-M4-LPAGE-OK`.
//! TODO(M5.7 CLOSED): large-page L3 — `theorem_large_page_map_unmap_exclusive` /
//!   `lemma_2m_map_unmap_exclusive` / `lemma_1g_map_unmap_exclusive` (no `admit`) →
//!   `RAYNU-V-M5-LPAGE-VERIFY-OK`.
//! TODO(M5.8 CLOSED): NUMA in ghost spec — `GhostNumaTopology` / `numa_map_enabled` /
//!   `mock_bringup_numa` (SRAT/SLIT bring-up) → `RAYNU-V-M5-NUMA-OK`.
//! TODO(M6.2 CLOSED): NUMA affinity / exclusivity L3 —
//!   `theorem_numa_map_unmap_affinity` / `guest_frames_on_node` →
//!   `RAYNU-V-M6-NUMA-L3-OK`.
//! TODO(M4.9 CLOSED): N-guest ghost↔exec refine — `theorem_concrete_n_guest_4k_refine`
//!   + `lemma_concrete_two_guests_map_refines` → `RAYNU-V-M4-REFINE-OK`.
//! TODO(M5.9 CLOSED): allocator↔EPT refine — `GhostFramePool` / `alloc_ept_refines` /
//!   `theorem_alloc_map_unmap_refines` + scoped `identity_leaf_ok` →
//!   `RAYNU-V-M5-ALLOC-REFINE-OK`. Full HW PTE bit-decode → M6.1.
//! TODO(M6.0 CLOSED): EPT-violation exclusivity —
//!   `theorem_ept_violation_preserves_exclusive` / `EptViolationDisposition`
//!   (EmulateNoMap | Reject | ClaimMap) → `RAYNU-V-M6-EPTVIO-OK`.
//! TODO(M6.1 CLOSED): HW PTE bit-decode — `ept_leaf_large_enc` /
//!   `hw_2m_identity_leaf_ok` / `theorem_hw_2m_leaf_refines_identity` →
//!   `RAYNU-V-M6-HWPTE-OK`. Full multi-level walk → polish.
//! TODO(M6.3 CLOSED): Live migration page transfer —
//!   `theorem_page_transfer_preserves_exclusive` / `PageTransferStep` →
//!   `RAYNU-V-M6-MIGRATE-XFER-OK`.
//!
//! # Large-page map (M4.8 — L2 spec)
//!
//! A 2M/1G leaf at aligned `(guest, gpa)` with base `FrameId` `base` covers
//! frames `[base, base + frames_covered(ps))`. Enabled when the span is free
//! (`large_span_free`) and GPA/base are leaf-aligned. On Ok, every frame in the
//! span is exclusively owned by `guest` (`large_map_post_owned`). Live `EptMap`
//! remains per-4K; HW large leaves are built by `ept_hw` (M3.20). L3 for these
//! posts is closed in M5.7 (`theorem_large_page_map_unmap_exclusive`).
//!
//! # NUMA domains (M5.8 — L2/L3-spec)
//!
//! `GhostNumaTopology` carries SRAT-style nodes, frame→node affinity, and SLIT
//! distances. `numa_map_enabled` requires ordinary 4K map enablement plus frame
//! affinity on the preferred node. Bring-up mock `mock_bringup_numa` matches
//! `assets/idrac/mock_topology.txt` (2 nodes, local 10 / remote 21). Host runtime
//! hook: `memory/numa.rs` ← `idrac::TopologySnapshot`. Affinity L3 closed in M6.2
//! (`theorem_numa_map_unmap_affinity`).
//!
//! # Allocator ↔ EPT refine (M5.9)
//!
//! `GhostFramePool` tracks the allocated-set. `alloc_ept_refines` requires
//! `refines(c)` and every owned frame ∈ allocated. `theorem_alloc_map_unmap_refines`
//! discharges allocate→map→unmap under that coupling. Scoped precise-identity
//! correspondence: `identity_leaf_ok` (GPA==HPA frame inside 512 MiB window).
//! HW PTE bit-decode closed in M6.1.
//!
//! # EPT-violation (M6.0)
//!
//! `EptViolationDisposition` models EmulateNoMap / Reject (ownership unchanged)
//! and ClaimMap (demand-fill via `ghost_map`). `theorem_ept_violation_preserves_exclusive`
//! discharges exclusivity for every enabled disposition. Live MMIO path uses
//! EmulateNoMap; unexpected GPA is Reject. Runtime hook:
//! `apply_violation_disposition` in `ept.rs`.
//!
//! # HW PTE bit-decode (M6.1)
//!
//! `ept_leaf_large_enc` matches `ept_hw::ept_leaf_large` (RWE + large + MT + HPA).
//! `hw_2m_identity_leaf_ok` / `theorem_hw_2m_leaf_refines_identity` discharge that
//! identity-builder 2 MiB leaves refine `identity_leaf_ok` at the leaf base (and
//! first covered 4 KiB frames). Runtime prop: `prop_hw_pte_identity_correspondence`.
//! Full multi-level walk → polish.
//!
//! # NUMA affinity L3 (M6.2)
//!
//! `guest_frames_on_node` is the affinity policy post. Under `numa_map_enabled`,
//! `theorem_numa_map_unmap_affinity` discharges that map→unmap preserves exclusivity
//! and affinity. Runtime prop: `prop_numa_affinity_l3` in `memory/numa.rs`.
//!
//! # Live migration page transfer (M6.3)
//!
//! `PageTransferStep` hands an HPA from `(src_guest, src_gpa)` to `(dst_guest, dst_gpa)`
//! via unmap→map. `theorem_page_transfer_preserves_exclusive` discharges exclusivity
//! and ownership handoff posts. Runtime hook: `transfer_page` / `prop_page_transfer_preserves_exclusive`
//! in `ept.rs`. Distinct from M5.5 inventory import. Full cross-host product → outside Proven Core.

#![allow(dead_code)]
