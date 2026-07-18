//! Verus-shaped specifications for the physical frame allocator.
//!
//! VERIFICATION: **L2** (spec written, M2.6) — ghost allocated-set + posts.
//! Machine-checked Verus proofs remain L0/L3 (`frame_allocator_proof.rs`);
//! Kani covers bitmap allocate/free in `frame_allocator_test.rs`.
//!
//! # Ghost model
//!
//! ```text
//! ghost Allocated: Set<PhysFrame>
//! ghost Pool: { base_frame .. base_frame + capacity }
//!
//! invariant Allocated ⊆ Pool
//! invariant |Allocated| == allocated_count
//! invariant bitmap bit i set  ⇔  (base_frame + i) ∈ Allocated
//! ```
//!
//! Double-alloc and UAF are critical isolation failures (ADR-002).
//!
//! # `FrameAllocator::allocate_frame`
//! requires
//!   - (none; may return `None` when exhausted)
//! ensures
//!   - on `Some(f)`:
//!       `f ∈ Pool`
//!       `f ∉ Allocated` (pre-state)
//!       after: `f ∈ Allocated` ∧ `|Allocated|` increased by 1
//!       no other frame's membership changed
//!   - on `None`: `Allocated` unchanged ∧ pool exhausted
//!
//! # `FrameAllocator::free_frame(f)`
//! requires
//!   - (none; errors encode precondition failures)
//! ensures
//!   - on `Ok`:
//!       `f ∈ Allocated` (pre-state)
//!       after: `f ∉ Allocated` ∧ `|Allocated|` decreased by 1
//!   - on `Err(DoubleFree)`: `f ∈ Pool ∧ f ∉ Allocated`; set unchanged
//!   - on `Err(InvalidFrame)`: `f ∉ Pool`; set unchanged
//!
//! # `run_allocator_selftest`
//! ensures
//!   - two successive allocates yield distinct frames
//!   - double-free of the same frame is rejected
//!   - a freed frame is reused on the next allocate
//!   - `allocated_count` restored to the pre-selftest value
//!
//! TODO(M3/M4): Verus L3 proof over the bitmap (`frame_allocator_proof.rs`).

#![allow(dead_code)]
