//! Physical allocators, page tables, EPT, address translation.
//!
//! Pillar: [V]
//! Proven Core: **inside** (ADR-002, ADR-004)
//! VERIFICATION: L2 live EptMap (M2.6); ghost L3 + refine (M3.17–M3.18) in `ept_model`;
//! N-guest L3 (M4.6–M4.7); large-page ghost *spec* (M4.8); N-guest refine (M4.9);
//! large-page L3 (M5.7); NUMA ghost *spec* (M5.8); allocator↔EPT refine (M5.9)

pub mod boot_alloc;
pub mod ept;
pub mod ept_hw;
pub mod ept3_gate;
pub mod frame_allocator;
pub mod kani_gate;
pub mod l2_gate;
pub mod l3_gate;
pub mod l3_link_gate;
pub mod l3_refine_gate;
pub mod l3_verify_gate;
pub mod m4_2vm_gate;
pub mod m4_lpage_gate;
pub mod m4_nguest_refine_gate;
pub mod m4_nguest_spec_gate;
pub mod m4_nguest_verify_gate;
pub mod m5_alloc_refine_gate;
pub mod m5_lpage_verify_gate;
pub mod m5_numa_gate;
pub mod numa;
pub mod verus_gate;

pub use ept::{
    claim_precise_identity_ranges, claim_precise_with_guest1_hole, claim_precise_with_shell_holes,
    ownership_selftest_ok, precise_ranges_ok, run_ownership_selftest, EptError, EptMap,
    EptPermissions, M2_BRINGUP_GUEST_ID, M2_OWN_OK_MARKER, M4_2VM_OK_MARKER, M4_GUEST1_ID,
    M4_GUEST2_ID, M4_GUEST3_ID, M4_NVM_OK_MARKER, M4_SHELL_G1_MARKER,
};
pub use m4_2vm_gate::run_m4_2vm_gate;
pub use m4_lpage_gate::{run_m4_lpage_gate, M4_LPAGE_OK_MARKER};
pub use m4_nguest_refine_gate::{run_m4_nguest_refine_gate, M4_REFINE_OK_MARKER};
pub use m4_nguest_spec_gate::{run_m4_nguest_spec_gate, M4_NGUEST_SPEC_OK_MARKER};
pub use m4_nguest_verify_gate::{run_m4_nguest_verify_gate, M4_NGUEST_VERIFY_OK_MARKER};
pub use m5_alloc_refine_gate::{run_m5_alloc_refine_gate, M5_ALLOC_REFINE_OK_MARKER};
pub use m5_lpage_verify_gate::{run_m5_lpage_verify_gate, M5_LPAGE_VERIFY_OK_MARKER};
pub use m5_numa_gate::{run_m5_numa_gate, M5_NUMA_GATE_MARKER};
pub use numa::{from_mock_topology, prop_mock_numa_runtime, HostNumaTopology, M5_NUMA_OK_MARKER};
pub use ept_hw::{
    EptHwError, EptPageSize, M2_EPT_OK_MARKER, M2_GUEST_OK_MARKER, M3_EPT2_OK_MARKER,
    M3_EPT3_OK_MARKER, PRECISE_BYTES, PRECISE_GIB, PRECISE_MIB, SECONDARY_ENABLE_EPT,
};
pub use ept3_gate::run_ept3_gate;
pub use kani_gate::{run_kani_gate, M3_KANI_OK_MARKER};
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
