use super::{
    host_nic_listen_does_not_claim_iron, host_nic_scripts_present, host_nic_surface_present,
    prop_host_nic_scaffold_package, run_m7_host_nic_scaffold_gate, M7_HOST_NIC_GATE_MARKER,
};
use crate::mgmt::host_nic::{
    M7_HOST_NIC_HTTP_OK_MARKER, M7_HOST_NIC_QEMU_MARKER, M7_HOST_NIC_SCAFFOLD_MARKER,
};

#[test]
fn m7_8_host_nic_scaffold_passes() {
    assert_eq!(M7_HOST_NIC_GATE_MARKER, M7_HOST_NIC_SCAFFOLD_MARKER);
    assert_eq!(
        M7_HOST_NIC_SCAFFOLD_MARKER,
        "RAYNU-V-M7-HOST-NIC-SCAFFOLD-OK"
    );
    assert_eq!(M7_HOST_NIC_QEMU_MARKER, "RAYNU-V-M7-HOST-NIC-QEMU-OK");
    assert_eq!(M7_HOST_NIC_HTTP_OK_MARKER, "RAYNU-V-M7-HOST-NIC-HTTP-OK");
    assert!(host_nic_surface_present(), "e1000 + listen + main wiring");
    assert!(host_nic_listen_does_not_claim_iron());
    assert!(host_nic_scripts_present(), "smoke + runbook must name M7.8");
    assert!(prop_host_nic_scaffold_package());
    assert!(run_m7_host_nic_scaffold_gate());
    println!("{M7_HOST_NIC_SCAFFOLD_MARKER}");
}
