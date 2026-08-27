use super::{
    latch_dxe_virtio_did, pci_addr_selects_owned, pci_addr_selects_slot0, pci_addr_selects_virtio,
    pci_config_addr, pci_config_addr_slot0, pci_enumerated, pci_read_data, pci_write_addr,
    pei_host_bridge_did, present, reset, take_marker, virtio_disk_evidence, GUEST_VIRTIO_PCI_DEVICE,
    GUEST_VIRTIO_PCI_VENDOR, M7_E5_OVMF_VIRTIO_OK_MARKER,
};
use crate::devices::guest_platform::{
    boot_order_cd_then_disk, pci_bdf, HOST_BRIDGE_DEVICE, HOST_BRIDGE_VENDOR,
};
use crate::devices::ide_cdrom;

#[test]
fn pci_bdf_is_probe_slot_not_ide() {
    let addr = pci_config_addr();
    assert_eq!(pci_bdf(addr), (0, 2, 0, 0));
    assert!(pci_addr_selects_virtio(addr));
    assert_eq!(pci_config_addr_slot0(), 0x8000_0000);
    assert!(pci_addr_selects_slot0(0x8000_0000));
    assert!(pci_addr_selects_owned(0x8000_0000));
    assert!(!pci_addr_selects_virtio(0x8000_0000));
    assert!(!pci_addr_selects_virtio(0x8000_0900)); // 00:01.1 IDE
    assert!(!pci_addr_selects_virtio(0x8000_0100)); // 00:00.1 IDE
    assert!(!pci_addr_selects_virtio(0x8000_4000)); // 00:08.0 host
    assert!(!pci_addr_selects_virtio(0x8000_0800)); // 00:01.0 ISA
}

#[test]
fn present_enumerates_virtio_and_cd_then_disk() {
    reset();
    ide_cdrom::reset();
    assert!(boot_order_cd_then_disk());
    assert!(!virtio_disk_evidence(false, true, true));
    assert!(!virtio_disk_evidence(true, false, true));
    assert!(!virtio_disk_evidence(true, true, false));
    assert!(present());
    pci_write_addr(pci_config_addr_slot0());
    let pei_id = pci_read_data(0xCFC, 4);
    assert!(pei_host_bridge_did());
    assert_eq!(pei_id as u16, HOST_BRIDGE_VENDOR);
    assert_eq!((pei_id >> 16) as u16, HOST_BRIDGE_DEVICE);
    // PEI DID probe: CF8=0x80000002 + inw(CFC) at 00:00.0 — i440FX HostBridgeDevId.
    pci_write_addr(0x8000_0002);
    assert_eq!(
        pci_read_data(0xCFC, 2) & 0xffff,
        u32::from(HOST_BRIDGE_DEVICE)
    );
    assert!(!pci_enumerated(), "PEI i440FX DID is not virtio enum");
    pci_write_addr(pci_config_addr());
    assert_eq!(pci_read_data(0xCFC, 4), 0xFFFF_FFFF, "virtio hidden until latch");
    assert!(latch_dxe_virtio_did());
    assert!(!pei_host_bridge_did());
    // CpuDxe AcpiTimerLib still reads OVMF_HOSTBRIDGE_DID at 00:00.0.
    pci_write_addr(0x8000_0002);
    assert_eq!(
        pci_read_data(0xCFC, 2) & 0xffff,
        u32::from(HOST_BRIDGE_DEVICE)
    );
    pci_write_addr(pci_config_addr());
    let id = pci_read_data(0xCFC, 4);
    assert_eq!(id as u16, GUEST_VIRTIO_PCI_VENDOR);
    assert_eq!((id >> 16) as u16, GUEST_VIRTIO_PCI_DEVICE);
    assert!(virtio_disk_evidence(true, true, true));
    assert!(take_marker());
    assert!(!take_marker());
    assert_eq!(M7_E5_OVMF_VIRTIO_OK_MARKER, "RAYNU-V-M7-E5-OVMF-VIRTIO-OK");
    reset();
}

#[test]
fn virtio_fn0_is_multifunction() {
    use crate::devices::guest_platform::pci_header_is_multifunction;
    reset();
    assert!(present());
    assert!(latch_dxe_virtio_did());
    pci_write_addr(pci_config_addr() | 0x0C);
    let ht = pci_read_data(0xCFC, 4);
    assert!(pci_header_is_multifunction(ht));
    pci_write_addr(pci_config_addr_slot0() | 0x0C);
    let slot0_ht = pci_read_data(0xCFC, 4);
    assert!(pci_header_is_multifunction(slot0_ht));
    reset();
}

#[test]
fn unpresented_pci_is_empty() {
    reset();
    pci_write_addr(pci_config_addr());
    assert_eq!(pci_read_data(0xCFC, 4), 0xFFFF_FFFF);
    reset();
}
