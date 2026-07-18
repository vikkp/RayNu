//! Verus specifications for the physical frame allocator.
//!
//! VERIFICATION: L1 posts (M2.3) — L2/L3 ghost model still TODO.
//!
//! # `FrameAllocator::allocate_frame`
//! requires
//!   - `allocated_count < capacity`
//! ensures
//!   - on `Some(f)`: `f` was free; after return `is_allocated(f)`
//!   - on `None`: pool exhausted; map unchanged
//!
//! # `FrameAllocator::free_frame`
//! requires
//!   - `f` in pool
//! ensures
//!   - on `Ok`: was allocated; after return not allocated
//!   - on `Err(DoubleFree)`: was already free; map unchanged
//!
//! # `run_allocator_selftest`
//! ensures
//!   - allocate ≠ allocate
//!   - double-free rejected
//!   - freed frame is reused on next allocate
//!   - pool `allocated_count` restored
//!
//! TODO(M2 L2/L3): ghost allocated-set; Kani harness.

#![allow(dead_code)]
