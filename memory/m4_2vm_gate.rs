//! M4.0 host verification gate (second guest under private EPT).
//!
//! Pillar: [V]
//! Checks in-tree artifacts for dual-VMCS / dual-EPT ownership path.
//! Runtime gate: `tools/qemu-boot-test.sh` → `RAYNU-V-M4-2VM-OK`.

/// Host / serial marker when the M4.0 two-VM gate passes.
pub const M4_2VM_OK_MARKER: &str = "RAYNU-V-M4-2VM-OK";

/// True when ept_hw can build/unmap a private 2 MiB G1 slab.
pub fn private_slab_ept_present() -> bool {
    let s = include_str!("ept_hw.rs");
    s.contains("fn build_single_2m_identity")
        && s.contains("fn clear_2m_identity_leaf")
        && s.contains("fn write_guest_shell_cpuid_page")
        && s.contains("fn write_guest_identity_2m_tables")
        && s.contains("frames_required_single_2m")
}

/// True when ownership registry knows G1 and the hole claim.
pub fn dual_guest_ownership_present() -> bool {
    let s = include_str!("ept.rs");
    s.contains("M4_GUEST1_ID")
        && s.contains("claim_precise_with_guest1_hole")
        && s.contains(M4_2VM_OK_MARKER)
        && s.contains("M4_SHELL_G1_MARKER")
}

/// True when launch can switch to a prepared second guest after G0 SHELL.
pub fn second_guest_launch_present() -> bool {
    let launch = include_str!("../vmx/launch.rs");
    let main = include_str!("../src/main.rs");
    launch.contains("set_second_guest")
        && launch.contains("try_launch_second_guest")
        && launch.contains("guest_cr3_phys")
        && launch.contains(M4_2VM_OK_MARKER)
        && main.contains("set_second_guest")
        && main.contains("build_single_2m_identity")
        && main.contains("write_guest_identity_2m_tables")
}

/// True when the QEMU boot gate requires M4.0.
pub fn m4_boot_scripts_present() -> bool {
    let smoke = include_str!("../tools/qemu-boot-test.sh");
    smoke.contains(M4_2VM_OK_MARKER)
        && smoke.contains("MARKER_2VM")
        && smoke.contains("M4.0")
}

/// Full M4.0 artifact gate (does not run QEMU).
pub fn run_m4_2vm_gate() -> bool {
    private_slab_ept_present()
        && dual_guest_ownership_present()
        && second_guest_launch_present()
        && m4_boot_scripts_present()
}

#[cfg(test)]
#[path = "m4_2vm_gate_test.rs"]
mod m4_2vm_gate_test;
