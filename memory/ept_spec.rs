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
//! TODO(M4): N guests + large pages in ghost model.

#![allow(dead_code)]
