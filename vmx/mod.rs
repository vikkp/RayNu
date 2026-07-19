//! VT-x / VMX setup, VMCS management, entry/exit.
//!
//! Pillar: [V]
//! Proven Core: **inside** (ADR-002)
//! VERIFICATION: L0/L1 — see `*_spec.rs` placeholders

pub mod fields;
pub mod hardware;
pub mod launch;
pub mod lifecycle;
pub mod mmio_decode;
pub mod ops;
pub mod vmcs;

pub use hardware::{M1_VMXON_OK_MARKER, M1_VMXON_SKIP_MARKER};
pub use crate::memory::{M2_EPT_OK_MARKER, M2_GUEST_OK_MARKER, M2_OWN_OK_MARKER};
pub use launch::{LaunchError, LaunchFrames, M1_VMEXIT_OK_MARKER};
pub use lifecycle::{VmxError, VmxLifecycle, VmxState};
pub use vmcs::{VmcsHandle, VmcsRegion};
