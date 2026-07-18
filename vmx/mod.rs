//! VT-x / VMX setup, VMCS management, entry/exit.
//!
//! Pillar: [V]
//! Proven Core: **inside** (ADR-002)
//! VERIFICATION: L0 (documented invariants) — see `*_spec.rs` placeholders

pub mod lifecycle;
pub mod vmcs;

pub use lifecycle::{VmxError, VmxLifecycle, VmxState};
pub use vmcs::{VmcsHandle, VmcsRegion};
