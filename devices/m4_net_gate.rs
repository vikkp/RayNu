//! M4.4 host verification gate (virtio-net + vSwitch exchange).
//!
//! Pillar: [V]
//! Runtime gate: `tools/qemu-boot-test.sh` → `RAYNU-V-M4-NET-OK`.

use super::virtio_net::M4_NET_OK_MARKER;

pub fn virtio_net_module_present() -> bool {
    let s = include_str!("virtio_net.rs");
    let net = include_str!("../net/mod.rs");
    s.contains(M4_NET_OK_MARKER)
        && s.contains("fn mmio_access")
        && s.contains("fn init")
        && s.contains("run_port_exchange")
        && s.contains("VIRTIO_ID_NET")
        && net.contains("fn forward")
        && net.contains("struct VSwitch")
}

pub fn net_launch_path_present() -> bool {
    let launch = include_str!("../vmx/launch.rs");
    let main = include_str!("../src/main.rs");
    let ept = include_str!("../memory/ept_hw.rs");
    launch.contains(M4_NET_OK_MARKER)
        && launch.contains("try_launch_net_probe")
        && launch.contains("virtio_net")
        && main.contains("virtio_net::init")
        && main.contains("write_guest_net_probe_page")
        && ept.contains("fn write_guest_net_probe_page")
}

pub fn m4_net_boot_scripts_present() -> bool {
    let smoke = include_str!("../tools/qemu-boot-test.sh");
    smoke.contains(M4_NET_OK_MARKER)
        && smoke.contains("MARKER_NET")
        && smoke.contains("M4.4")
}

pub fn run_m4_net_gate() -> bool {
    virtio_net_module_present() && net_launch_path_present() && m4_net_boot_scripts_present()
}

#[cfg(test)]
#[path = "m4_net_gate_test.rs"]
mod m4_net_gate_test;
