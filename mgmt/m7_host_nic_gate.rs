//! M7.8 host-owned NIC scaffold / package gate (ADR-013 Phase C).
//!
//! Pillar: [Z]
//! Proven Core: **outside**
//!
//! Host/CI proves wiring. Does **not** print
//! `RAYNU-V-M7-HOST-NIC-HTTP-OK` (iron Phase D) or claim QEMU GET / from this
//! path (`RAYNU-V-M7-HOST-NIC-QEMU-OK` is firmware runtime only).

use super::e1000_mmio::{pci_id_is_qemu_e1000, E1000_DEVICE, E1000_VENDOR};
use super::host_nic::{
    M7_HOST_NIC_HTTP_OK_MARKER, M7_HOST_NIC_QEMU_MARKER, M7_HOST_NIC_SCAFFOLD_MARKER,
};
use super::host_nic_poll::prop_bounded_poll_respects_budget;

/// Host / CI marker when the M7.8 scaffold package passes.
pub const M7_HOST_NIC_GATE_MARKER: &str = M7_HOST_NIC_SCAFFOLD_MARKER;

pub fn host_nic_surface_present() -> bool {
    let mmio = include_str!("e1000_mmio.rs");
    let phy = include_str!("e1000.rs");
    let listen = include_str!("host_nic_listen.rs");
    let http = include_str!("http_listen.rs");
    let main = include_str!("../src/main.rs");
    let idle = include_str!("snp_listen_uefi.rs");
    mmio.contains("All PCI config")
        && mmio.contains("8086:100e")
        && mmio.contains("fn pci_id_is_qemu_e1000(")
        && mmio.contains("// SAFETY:")
        && mmio.contains("KANI-TARGET")
        && phy.contains("impl Device for E1000Device")
        && phy.contains("e1000_mmio")
        && listen.contains("fn run_post_ebs_host_nic_listen(")
        && listen.contains("post-EBS e1000")
        && listen.contains("M7_HOST_NIC_QEMU_MARKER")
        && !listen.contains("CURL NOW (post-EBS)")
        && !listen.contains("SnpDevice")
        && listen.contains("iface.poll")
        && listen.contains("bounded_poll")
        && listen.contains("handle_http_request")
        && http.contains("run_post_ebs_host_nic_listen")
        && main.contains("probe_host_nic_lab_flag")
        && main.contains("run_post_ebs_mgmt_listen")
        && idle.contains("firmware SNP dead after EBS")
        && pci_id_is_qemu_e1000(E1000_VENDOR, E1000_DEVICE)
}

/// Native listen must not print the iron Phase D marker.
pub fn host_nic_listen_does_not_claim_iron() -> bool {
    let listen = include_str!("host_nic_listen.rs");
    !listen.contains(M7_HOST_NIC_HTTP_OK_MARKER)
}

pub fn host_nic_scripts_present() -> bool {
    let smoke = include_str!("../tools/m7-host-nic-smoke.sh");
    let qemu = include_str!("../tools/m7-host-nic-qemu-smoke.sh");
    let runbook = include_str!("../docs/runbooks/mgmt_http.md");
    smoke.contains(M7_HOST_NIC_SCAFFOLD_MARKER)
        && smoke.contains("m7_8_host_nic_scaffold_passes")
        && smoke.contains(M7_HOST_NIC_HTTP_OK_MARKER)
        && smoke.contains("never print iron")
        && qemu.contains(M7_HOST_NIC_QEMU_MARKER)
        && qemu.contains("hostnic.txt")
        && qemu.contains("e1000")
        && qemu.contains("never print iron")
        && runbook.contains("ADR-013")
        && runbook.contains("Phase C")
        && runbook.contains(M7_HOST_NIC_QEMU_MARKER)
        && runbook.contains(M7_HOST_NIC_SCAFFOLD_MARKER)
        && runbook.contains("8086:100e")
}

pub fn prop_host_nic_scaffold_package() -> bool {
    host_nic_surface_present()
        && host_nic_listen_does_not_claim_iron()
        && host_nic_scripts_present()
        && prop_bounded_poll_respects_budget()
}

pub fn run_m7_host_nic_scaffold_gate() -> bool {
    prop_host_nic_scaffold_package()
}

#[cfg(test)]
#[path = "m7_host_nic_gate_test.rs"]
mod m7_host_nic_gate_test;
