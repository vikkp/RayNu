//! Physical allocators, page tables, EPT, address translation.
//!
//! Pillar: [V]
//! Proven Core: **inside** (ADR-002, ADR-004)
//! VERIFICATION: L2 live EptMap (M2.6); ghost L3 + refine (M3.17–M3.18) in `ept_model`

pub mod boot_alloc;
pub mod ept;
pub mod ept_hw;
pub mod ept3_gate;
pub mod frame_allocator;
pub mod l2_gate;
pub mod l3_gate;
pub mod l3_link_gate;
pub mod l3_refine_gate;
pub mod l3_verify_gate;
pub mod verus_gate;

pub use ept::{
    claim_precise_identity_ranges, ownership_selftest_ok, precise_ranges_ok,
    run_ownership_selftest, EptError, EptMap, EptPermissions, M2_BRINGUP_GUEST_ID,
    M2_OWN_OK_MARKER,
};
pub use ept_hw::{
    EptHwError, EptPageSize, M2_EPT_OK_MARKER, M2_GUEST_OK_MARKER, M3_EPT2_OK_MARKER,
    M3_EPT3_OK_MARKER, PRECISE_BYTES, PRECISE_GIB, PRECISE_MIB, SECONDARY_ENABLE_EPT,
};
pub use ept3_gate::run_ept3_gate;
pub use frame_allocator::{
    allocator_selftest_ok, run_allocator_selftest, AllocError, FrameAllocator, PhysFrame,
    M2_ALLOC_OK_MARKER,
};
pub use l2_gate::{run_l2_gate, M2_L2_OK_MARKER};
pub use l3_gate::{run_l3_gate, M3_L3_OK_MARKER};
pub use l3_link_gate::{run_l3_link_gate, M3_L3_LINK_OK_MARKER};
pub use l3_refine_gate::{run_l3_refine_gate, M3_L3_REFINE_OK_MARKER};
pub use l3_verify_gate::{run_l3_verify_gate, M3_L3_VERIFY_OK_MARKER};
pub use verus_gate::{run_verus_pin_gate, M3_VERUS_OK_MARKER};
