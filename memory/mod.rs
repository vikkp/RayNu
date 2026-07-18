//! Physical allocators, page tables, EPT, address translation.
//!
//! Pillar: [V]
//! Proven Core: **inside** (ADR-002, ADR-004)
//! VERIFICATION: L0 (ownership registry) · L1 (hardware identity builder, M2.0/M2.1)

pub mod ept;
pub mod ept_hw;
pub mod frame_allocator;

pub use ept::{EptError, EptMap, EptPermissions};
pub use ept_hw::{
    EptHwError, EptPageSize, M2_EPT_OK_MARKER, M2_GUEST_OK_MARKER, SECONDARY_ENABLE_EPT,
};
pub use frame_allocator::{FrameAllocator, PhysFrame};
