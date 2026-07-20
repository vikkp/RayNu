//! M4.1 host verification gate (credit scheduler time-slices ≥2 VMs).
//!
//! Pillar: [V]
//! Checks in-tree artifacts for dual-VMCS scheduling path.
//! Runtime gate: `tools/qemu-boot-test.sh` → `RAYNU-V-M4-SCHED-OK`.

use super::scheduler::M4_SCHED_OK_MARKER;

/// True when the credit scheduler exposes quantum consume + fair pick.
pub fn credit_scheduler_ready() -> bool {
    let s = include_str!("scheduler.rs");
    s.contains("fn consume_quantum")
        && s.contains("fn pick_next_fair")
        && s.contains(M4_SCHED_OK_MARKER)
        && s.contains("M4_SLICE_G0_MARKER")
        && s.contains("M4_SLICE_G1_MARKER")
}

/// True when launch switches VMCSes under the scheduler.
pub fn dual_vmcs_switch_present() -> bool {
    let launch = include_str!("../vmx/launch.rs");
    launch.contains("SCHED_MODE")
        && launch.contains("switch_to_sched_slot")
        && launch.contains("schedule_preempt")
        && launch.contains(M4_SCHED_OK_MARKER)
        && launch.contains("FIRST_GUEST")
}

/// True when the QEMU boot gate requires M4.1.
pub fn m4_sched_boot_scripts_present() -> bool {
    let smoke = include_str!("../tools/qemu-boot-test.sh");
    smoke.contains(M4_SCHED_OK_MARKER)
        && smoke.contains("MARKER_SCHED")
        && smoke.contains("M4.1")
}

/// Full M4.1 artifact gate (does not run QEMU).
pub fn run_m4_sched_gate() -> bool {
    credit_scheduler_ready() && dual_vmcs_switch_present() && m4_sched_boot_scripts_present()
}

#[cfg(test)]
#[path = "m4_sched_gate_test.rs"]
mod m4_sched_gate_test;
