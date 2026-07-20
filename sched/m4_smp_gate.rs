//! M4.5 host verification gate (dual-vCPU BSP+AP probe).
//!
//! Pillar: [V]
//! Runtime gate: `tools/qemu-boot-test.sh` → `RAYNU-V-M4-SMP-OK`.

use crate::sched::smp_probe::M4_SMP_OK_MARKER;

pub fn smp_probe_module_present() -> bool {
    let s = include_str!("../sched/smp_probe.rs");
    s.contains(M4_SMP_OK_MARKER)
        && s.contains("fn note_bsp_ready")
        && s.contains("fn note_ap_ready")
        && s.contains("fn init")
        && s.contains("READY_MAGIC")
}

pub fn smp_launch_path_present() -> bool {
    let launch = include_str!("../vmx/launch.rs");
    let main = include_str!("../src/main.rs");
    let ept = include_str!("../memory/ept_hw.rs");
    launch.contains(M4_SMP_OK_MARKER)
        && launch.contains("try_launch_smp_probe")
        && launch.contains("set_smp_probe")
        && launch.contains("smp_probe")
        && main.contains("smp_probe::init")
        && main.contains("write_guest_smp_bsp_page")
        && main.contains("write_guest_smp_ap_page")
        && ept.contains("fn write_guest_smp_bsp_page")
        && ept.contains("fn write_guest_smp_ap_page")
}

pub fn m4_smp_boot_scripts_present() -> bool {
    let smoke = include_str!("../tools/qemu-boot-test.sh");
    smoke.contains(M4_SMP_OK_MARKER)
        && smoke.contains("MARKER_SMP")
        && smoke.contains("M4.5")
}

pub fn run_m4_smp_gate() -> bool {
    smp_probe_module_present() && smp_launch_path_present() && m4_smp_boot_scripts_present()
}

#[cfg(test)]
#[path = "m4_smp_gate_test.rs"]
mod m4_smp_gate_test;
