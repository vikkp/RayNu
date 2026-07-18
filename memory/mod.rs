//! Physical allocators, page tables, EPT, address translation.
//!
//! Pillar: [V]
//! Proven Core: **inside** (ADR-002, ADR-004)
//! VERIFICATION: L2 — EPT ownership + frame allocator specs (M2.6); L1 runtime

pub mod boot_alloc;
pub mod ept;
pub mod ept_hw;
pub mod frame_allocator;
pub mod l2_gate;

pub use ept::{
    ownership_selftest_ok, run_ownership_selftest, EptError, EptMap, EptPermissions,
    M2_BRINGUP_GUEST_ID, M2_OWN_OK_MARKER,
};
pub use ept_hw::{
    EptHwError, EptPageSize, M2_EPT_OK_MARKER, M2_GUEST_OK_MARKER, SECONDARY_ENABLE_EPT,
};
pub use frame_allocator::{
    allocator_selftest_ok, run_allocator_selftest, AllocError, FrameAllocator, PhysFrame,
    M2_ALLOC_OK_MARKER,
};
pub use l2_gate::{run_l2_gate, M2_L2_OK_MARKER};
