//! E4 SPA VMLAUNCH wiring gate (outside Proven Core).
//!
//! Host/CI: REST start queues a launch; launch.rs relocates VMCS into the
//! G1 slab with a private 2 MiB EPT. Iron marker is not claimed from host.

use super::spa_launch::{prop_spa_start_queues, M7_E4_SPA_LAUNCH_OK_MARKER};

/// True when REST start/stop queue the coexist VMLAUNCH path.
pub fn spa_rest_queues_launch() -> bool {
    let api = include_str!("api.rs");
    api.contains("note_spa_start") && api.contains("note_spa_stop") && prop_spa_start_queues()
}

/// True when launch.rs consumes the queue with a slab-local VMCS + 2 MiB EPT.
pub fn spa_launch_relocates_vmcs() -> bool {
    let launch = include_str!("../vmx/launch.rs");
    launch.contains("fn try_spa_vmlaunch(")
        && launch.contains("G1_SLAB_OFF_VMCS")
        && launch.contains("build_single_2m_identity")
        && launch.contains("M7_E4_SPA_LAUNCH_OK_MARKER")
        && launch.contains("private 2M EPT")
        && launch.contains("if !M4_LADDER_DONE")
        && launch.contains("M4_LADDER_DONE && next == 1 && !SPA_RUNNABLE")
        && launch.contains("save_live_gprs_to_slot(SCHED_SLOT_CUR)")
        && launch.contains("fn relocate_g0_vmcs_to_host_slab(")
        && launch.contains("fn clone_current_vmcs_to(")
        && launch.contains("G0_NEEDS_VMLAUNCH")
        && launch.contains("G0_VMPTRLD_FAILED")
        && launch.contains("failsoft_sched_or_finish")
        && !launch.contains("copy_nonoverlapping(g0.vmcs_phys")
        && include_str!("spa_launch.rs").contains(M7_E4_SPA_LAUNCH_OK_MARKER)
}

/// True when the slab layout reserves host VMCS away from G0 identity.
pub fn spa_slab_holds_vmcs() -> bool {
    let ept = include_str!("../memory/ept_hw.rs");
    ept.contains("G1_SLAB_OFF_VMCS")
        && ept.contains("G1_SLAB_OFF_EPT_PML4")
        && ept.contains("G1_SLAB_OFF_HOST_STACK")
        && ept.contains("G0_HOST_SLAB_OFF_VMCS")
        && ept.contains("fn host_only_slab_after_shells")
}

pub fn run_m7_e4_spa_gate() -> bool {
    let _ = M7_E4_SPA_LAUNCH_OK_MARKER;
    spa_rest_queues_launch() && spa_launch_relocates_vmcs() && spa_slab_holds_vmcs()
}

#[cfg(test)]
#[path = "m7_e4_spa_gate_test.rs"]
mod m7_e4_spa_gate_test;
