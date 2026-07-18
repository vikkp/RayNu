//! vCPU scheduling and Proven Core runstate helpers.
//!
//! Pillar: [V] (vcpu/ipi/…) · scheduler algorithms outside (ADR-002)
//! VERIFICATION: L0 for Proven Core stubs

pub mod hypercall;
pub mod interrupt;
pub mod ipi;
pub mod msr_firewall;
pub mod scheduler;
pub mod vcpu;

pub use scheduler::{CreditScheduler, SchedError};
pub use vcpu::{Vcpu, VcpuState};
