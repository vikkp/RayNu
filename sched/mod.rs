//! vCPU scheduling and Proven Core runstate helpers.
//!
//! Pillar: [V] (vcpu/ipi/…) · scheduler algorithms outside (ADR-002)
//! VERIFICATION: L0 for Proven Core stubs

pub mod hypercall;
pub mod interrupt;
pub mod ipi;
pub mod m4_sched_gate;
pub mod msr_firewall;
pub mod scheduler;
pub mod vcpu;

pub use interrupt::{
    prepare_external_inject, validate_vector, InjectError, M2_IRQ_OK_MARKER, M2_IRQ_VECTOR,
    M2_TIMER_OK_MARKER, M3_GTIMER2_OK_MARKER, M3_GTIMER_OK_MARKER,
};
pub use m4_sched_gate::run_m4_sched_gate;
pub use msr_firewall::{cpuid_filter_ok, filter_cpuid, msr_firewall_ok, M3_CPUID_OK_MARKER};
pub use scheduler::{
    CreditScheduler, SchedError, DEFAULT_CREDIT, M4_SCHED_OK_MARKER, M4_SLICE_G0_MARKER,
    M4_SLICE_G1_MARKER,
};
pub use vcpu::{Vcpu, VcpuState};
