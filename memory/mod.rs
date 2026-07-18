//! Physical allocators, page tables, EPT, address translation.
//!
//! Pillar: [V]
//! Proven Core: **inside** (ADR-002, ADR-004)
//! VERIFICATION: L0

pub mod ept;
pub mod frame_allocator;

pub use ept::{EptError, EptMap, EptPermissions};
pub use frame_allocator::{FrameAllocator, PhysFrame};
