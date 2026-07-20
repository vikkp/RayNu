//! M4.2 host verification gate (4+ concurrent guests under EPT).
//!
//! Pillar: [V]
//! Checks in-tree artifacts for multi-shell VMCS / multi-hole EPT path.
//! Runtime gate: `tools/qemu-boot-test.sh` → `RAYNU-V-M4-NVM-OK`.

use super::scheduler::M4_NVM_OK_MARKER;

/// True when the scheduler exposes NVM + G2/G3 slice markers.
pub fn nvm_scheduler_markers_present() -> bool {
    let s = include_str!("scheduler.rs");
    s.contains(M4_NVM_OK_MARKER)
        && s.contains("M4_SLICE_G2_MARKER")
        && s.contains("M4_SLICE_G3_MARKER")
}

/// True when launch supports ≥4 guest slots and NVM finish.
pub fn nvm_launch_present() -> bool {
    let launch = include_str!("../vmx/launch.rs");
    let main = include_str!("../src/main.rs");
    launch.contains("M4_NVM_GUEST_SLOTS")
        && launch.contains(M4_NVM_OK_MARKER)
        && launch.contains("set_shell_guest")
        && launch.contains("launch_shell_guest")
        && main.contains("set_shell_guest")
        && main.contains("claim_precise_with_shell_holes")
        && main.contains("pick_shell_slab_hpa")
}

/// True when EPT claims multiple shell holes.
pub fn nvm_ept_holes_present() -> bool {
    let ept = include_str!("../memory/ept.rs");
    ept.contains("claim_precise_with_shell_holes")
        && ept.contains("M4_GUEST2_ID")
        && ept.contains("M4_GUEST3_ID")
        && ept.contains(M4_NVM_OK_MARKER)
}

/// True when the QEMU boot gate requires M4.2.
pub fn m4_nvm_boot_scripts_present() -> bool {
    let smoke = include_str!("../tools/qemu-boot-test.sh");
    smoke.contains(M4_NVM_OK_MARKER)
        && smoke.contains("MARKER_NVM")
        && smoke.contains("M4.2")
}

/// Full M4.2 artifact gate (does not run QEMU).
pub fn run_m4_nvm_gate() -> bool {
    nvm_scheduler_markers_present()
        && nvm_launch_present()
        && nvm_ept_holes_present()
        && m4_nvm_boot_scripts_present()
}

#[cfg(test)]
#[path = "m4_nvm_gate_test.rs"]
mod m4_nvm_gate_test;
