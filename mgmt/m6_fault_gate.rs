//! M6.7 host verification gate (fault injection suite).
//!
//! Pillar: [Z] [A]
//! Proven Core: outside (companion to `mgmt/fault` — not a boot path).
//!
//! Checks kill-vCPU / corrupt-page / drop-IRQ / net-partition props, closed GAP,
//! audit events, runbook, and smoke/CI wiring.

use super::fault::{prop_fault_suite, FAULT_GAP_NOTE, M6_FAULT_OK_MARKER};

/// Host / CI marker when the M6.7 fault gate passes.
pub const M6_FAULT_GATE_MARKER: &str = M6_FAULT_OK_MARKER;

/// True when fault module exposes suite, closed GAP, and marker.
pub fn fault_surface_present() -> bool {
    let s = include_str!("fault.rs");
    s.contains("fn prop_kill_vcpu_recover(")
        && s.contains("fn prop_corrupt_page_fail_closed(")
        && s.contains("fn prop_drop_irq_fail_closed(")
        && s.contains("fn prop_net_partition_recover(")
        && s.contains("fn prop_fault_suite(")
        && s.contains("struct IrqDropLatch")
        && s.contains(M6_FAULT_OK_MARKER)
        && s.contains(FAULT_GAP_NOTE)
        && FAULT_GAP_NOTE.contains("CLOSED M6.7")
}

/// True when audit fault events exist.
pub fn audit_fault_events_present() -> bool {
    let s = include_str!("../audit/integrity.rs");
    s.contains("FaultInjected")
        && s.contains("FaultRecovered")
        && s.contains("FaultFailClosed")
}

/// True when vCPU tear_down and VSwitch partition hooks exist.
pub fn fault_hooks_present() -> bool {
    let vcpu = include_str!("../sched/vcpu.rs");
    let net = include_str!("../net/mod.rs");
    vcpu.contains("fn tear_down(")
        && vcpu.contains("TornDown")
        && net.contains("fn set_partitioned(")
        && net.contains("partitioned")
}

/// True when the M6.7 smoke script is present.
pub fn fault_scripts_present() -> bool {
    let smoke = include_str!("../tools/m6-fault-smoke.sh");
    smoke.contains(M6_FAULT_OK_MARKER)
        && smoke.contains("m6_7_fault_gate_passes")
        && smoke.contains("prop_fault_suite")
}

/// True when the fault runbook is present.
pub fn fault_runbook_present() -> bool {
    let rb = include_str!("../docs/runbooks/fault.md");
    rb.contains("RAYNU-V-M6-FAULT-OK")
        && rb.contains("KillVcpu")
        && rb.contains("CorruptPage")
        && rb.contains("DropIrq")
        && rb.contains("NetPartition")
}

/// Full M6.7 artifact + fault suite gate.
pub fn run_m6_fault_gate() -> bool {
    fault_surface_present()
        && audit_fault_events_present()
        && fault_hooks_present()
        && fault_scripts_present()
        && fault_runbook_present()
        && prop_fault_suite()
}

#[cfg(test)]
#[path = "m6_fault_gate_test.rs"]
mod m6_fault_gate_test;
