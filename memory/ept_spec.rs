//! Verus specifications for the EPT engine (ADR-004).
//!
//! VERIFICATION: L1 posts (documented) — L2 ghost model still TODO.
//!
//! # ADR-004 exclusive ownership (M2.2)
//!
//! ## `EptMap::map`
//! requires
//!   - `guest_id != 0`
//!   - no existing mapping with the same `frame` (HPA)
//!   - no existing mapping with the same `(guest_id, gpa)`
//! ensures
//!   - on `Ok`: `owner_of(frame) == Some(guest_id)`
//!   - on `Ok`: `check_invariants()`
//!   - on `Err(AlreadyOwned)`: map unchanged
//!
//! ## `EptMap::unmap`
//! requires
//!   - a mapping exists for `(guest_id, gpa)`
//! ensures
//!   - on `Ok`: `owner_of(returned_frame) == None`
//!   - on `Ok`: `check_invariants()`
//!
//! ## `run_ownership_selftest`
//! ensures
//!   - bring-up guest uniquely owns code + stack + IDT frames
//!   - a second guest cannot map the code HPA
//!   - unmap then remap of stack succeeds
//!
//! TODO(M2 L2): exclusive-ownership ghost set; Kani harness for map/unmap.
//! TODO(M3): proof attempt for 4K single-guest.

#![allow(dead_code)]
