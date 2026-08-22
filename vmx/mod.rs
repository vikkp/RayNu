//! VT-x / VMX setup, VMCS management, entry/exit.
//!
//! Pillar: [V]
//! Proven Core: **inside** (ADR-002)
//! VERIFICATION: L0/L1 — see `*_spec.rs` placeholders

pub mod fields;
pub mod guest_pt;
pub mod hardware;
pub mod launch;
pub mod lifecycle;
pub mod mmio_decode;
pub mod noirq_gate;
pub mod ops;
pub mod vmcs;

pub use hardware::{M1_VMXON_OK_MARKER, M1_VMXON_SKIP_MARKER};
pub use crate::memory::{M2_EPT_OK_MARKER, M2_GUEST_OK_MARKER, M2_OWN_OK_MARKER};
pub use launch::{
    arm_live_esp_ovmf_mapping, live_esp_ovmf_is_mapped, reset_live_esp_ovmf_mapping,
    try_vmlaunch_guest_uefi_ovmf, GuestUefiLaunchError, LaunchError, LaunchFrames,
    GUEST_UEFI_OVMF_ESP_PATH, MIN_LIVE_ESP_OVMF_BYTES, M1_VMEXIT_OK_MARKER,
};
pub use lifecycle::{VmxError, VmxLifecycle, VmxState};
pub use noirq_gate::{run_noirq_gate, M3_NOIRQ_OK_MARKER};
pub use vmcs::{VmcsHandle, VmcsRegion};
