//! VT-x / VMX setup, VMCS management, entry/exit.
//!
//! Pillar: [V]
//! Proven Core: **inside** (ADR-002)
//! VERIFICATION: L0/L1 — see `*_spec.rs` placeholders

pub mod hardware;
pub mod lifecycle;
pub mod vmcs;

pub use hardware::{M1_VMXON_OK_MARKER, M1_VMXON_SKIP_MARKER};
pub use lifecycle::{VmxError, VmxLifecycle, VmxState};
pub use vmcs::{VmcsHandle, VmcsRegion};
