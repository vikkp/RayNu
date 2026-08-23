use super::{
    pci_addr_selects_virtio, pci_config_addr, pci_read_data, pci_write_addr, present, reset,
    take_marker, virtio_disk_evidence, GUEST_VIRTIO_PCI_DEVICE, GUEST_VIRTIO_PCI_VENDOR,
    M7_E5_OVMF_VIRTIO_OK_MARKER,
};
use crate::devices::guest_platform::{
    boot_order_cd_then_disk, pci_bdf, pci_header_is_multifunction,
};
use crate::devices::ide_cdrom;

#[test]
fn pci_bdf_is_fn1_not_ide() {
    let addr = pci_config_addr();
    assert_eq!(pci_bdf(addr), (0, 0, 1, 0));
    assert!(pci_addr_selects_virtio(addr));
    assert!(pci_addr_selects_virtio(0x8000_0100));
    assert!(!pci_addr_selects_virtio(0x8000_0000)); // 00:00.0 IDE
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
    pci_write_addr(pci_config_addr());
    let id = pci_read_data(0xCFC, 4);
    assert_eq!(id as u16, GUEST_VIRTIO_PCI_VENDOR);
    assert_eq!((id >> 16) as u16, GUEST_VIRTIO_PCI_DEVICE);
    // raynuvsrv1-style DID probe: CF8 offset 2 + inw(CFC).
    pci_write_addr(0x8000_0102);
    assert_eq!(
        pci_read_data(0xCFC, 2) & 0xffff,
        u32::from(GUEST_VIRTIO_PCI_DEVICE)
    );
    assert!(virtio_disk_evidence(true, true, true));
    assert!(take_marker());
    assert!(!take_marker());
    assert_eq!(M7_E5_OVMF_VIRTIO_OK_MARKER, "RAYNU-V-M7-E5-OVMF-VIRTIO-OK");
    reset();
}

#[test]
fn ide_is_multifunction_so_fn1_is_scannable() {
    ide_cdrom::reset();
    assert!(ide_cdrom::present_placeholder());
    ide_cdrom::pci_write_addr(ide_cdrom::pci_config_addr() | 0x0C);
    let ht = ide_cdrom::pci_read_data(0xCFC, 4);
    assert!(pci_header_is_multifunction(ht));
    ide_cdrom::reset();
}

#[test]
fn unpresented_pci_is_empty() {
    reset();
    pci_write_addr(pci_config_addr());
    assert_eq!(pci_read_data(0xCFC, 4), 0xFFFF_FFFF);
    reset();
}
