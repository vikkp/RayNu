//! M4.3 host verification gate (virtio-blk MMIO + write/readback).
//!
//! Pillar: [V]
//! Runtime gate: `tools/qemu-boot-test.sh` → `RAYNU-V-M4-BLK-OK`.

use super::virtio_blk::M4_BLK_OK_MARKER;

pub fn virtio_blk_module_present() -> bool {
    let s = include_str!("virtio_blk.rs");
    s.contains(M4_BLK_OK_MARKER)
        && s.contains("fn mmio_access")
        && s.contains("fn init")
        && s.contains("run_write_readback")
        && s.contains("VIRTIO_ID_BLOCK")
}

pub fn blk_launch_path_present() -> bool {
    let launch = include_str!("../vmx/launch.rs");
    let main = include_str!("../src/main.rs");
    let ept = include_str!("../memory/ept_hw.rs");
    launch.contains(M4_BLK_OK_MARKER)
        && launch.contains("try_launch_blk_probe")
        && launch.contains("virtio_blk")
        && main.contains("virtio_blk::init")
        && main.contains("write_guest_blk_probe_page")
        && ept.contains("fn write_guest_blk_probe_page")
}

pub fn m4_blk_boot_scripts_present() -> bool {
    let smoke = include_str!("../tools/qemu-boot-test.sh");
    smoke.contains(M4_BLK_OK_MARKER)
        && smoke.contains("MARKER_BLK")
        && smoke.contains("M4.3")
}

pub fn run_m4_blk_gate() -> bool {
    virtio_blk_module_present() && blk_launch_path_present() && m4_blk_boot_scripts_present()
}

#[cfg(test)]
#[path = "m4_blk_gate_test.rs"]
mod m4_blk_gate_test;
