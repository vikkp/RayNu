use super::{
    cdrom_visible_evidence, host_identify_word0, host_read10, is_ata_primary_port,
    is_pci_data_port, pci_addr_selects_cd, pci_bdf, pci_config_addr, pci_read_data, pci_write_addr,
    present, present_placeholder, reset, take_marker, GUEST_CD_PCI_DEVICE, GUEST_CD_PCI_VENDOR,
    ISO_SECTOR, M7_E5_OVMF_CDROM_OK_MARKER,
};

#[test]
fn pci_bdf_and_ports() {
    let addr = pci_config_addr();
    assert_eq!(pci_bdf(addr), (0, 1, 1, 0));
    assert!(pci_addr_selects_cd(addr));
    assert!(!pci_addr_selects_cd(0x8000_0000)); // 00:00.0 host bridge
    assert!(!pci_addr_selects_cd(0x8000_0800)); // 00:01.0 ISA
    assert!(is_ata_primary_port(0x1F0));
    assert!(is_ata_primary_port(0x1F7));
    assert!(is_ata_primary_port(0x3F6));
    assert!(!is_ata_primary_port(0x3F8));
    assert!(is_pci_data_port(0xCFC));
    assert!(is_pci_data_port(0xCFE));
    assert!(!is_pci_data_port(0xCF8));
}

#[test]
fn present_placeholder_enumerates_and_reads_pvd() {
    reset();
    assert!(!cdrom_visible_evidence(false, true, 1));
    assert!(!cdrom_visible_evidence(true, false, 0));
    assert!(present_placeholder());
    pci_write_addr(pci_config_addr());
    let id = pci_read_data(0xCFC, 4);
    assert_eq!(id as u16, GUEST_CD_PCI_VENDOR);
    assert_eq!((id >> 16) as u16, GUEST_CD_PCI_DEVICE);
    pci_write_addr(pci_config_addr() | 0x0C);
    let ide_ht = pci_read_data(0xCFC, 4);
    assert_eq!((ide_ht >> 16) & 0xff, 0x80);
    pci_write_addr(pci_config_addr() | 0x0E);
    assert_eq!(pci_read_data(0xCFC, 1) & 0xff, 0x80);
    assert_eq!(host_identify_word0(), Some(0x8500));
    let pvd = host_read10(16).expect("READ(10) LBA 16");
    assert_eq!(&pvd[1..6], b"CD001");
    assert!(cdrom_visible_evidence(true, true, 1));
    assert!(take_marker());
    assert!(!take_marker());
    assert_eq!(M7_E5_OVMF_CDROM_OK_MARKER, "RAYNU-V-M7-E5-OVMF-CDROM-OK");
    reset();
}

#[test]
fn unpresented_pci_is_empty() {
    reset();
    pci_write_addr(pci_config_addr());
    assert_eq!(pci_read_data(0xCFC, 4), 0xFFFF_FFFF);
    assert!(present(&[0u8; ISO_SECTOR], 2));
    pci_write_addr(0x8000_0000);
    assert_eq!(pci_read_data(0xCFC, 4), 0xFFFF_FFFF);
    reset();
}
