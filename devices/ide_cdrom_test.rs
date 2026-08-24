use super::{
    ata_io, ata_io_accesses, bmide_io, cdrom_visible_evidence, host_identify_word0, host_read10,
    is_ata_primary_port, is_bmide_port, is_pci_data_port, last_scsi, pci_addr_selects_cd, pci_bdf,
    pci_config_addr, pci_read_data, pci_write_addr, pci_write_data, present, present_placeholder,
    reset, sectors_read, take_marker, GUEST_CD_PCI_DEVICE, GUEST_CD_PCI_VENDOR, ISO_SECTOR,
    M7_E5_OVMF_CDROM_OK_MARKER,
};

#[test]
fn pci_bdf_and_ports() {
    reset();
    let addr = pci_config_addr();
    assert_eq!(pci_bdf(addr), (0, 0, 1, 0));
    assert!(pci_addr_selects_cd(addr));
    assert!(pci_addr_selects_cd(0x8000_0100)); // 00:00.1 IDE (virtio fn1)
    assert!(pci_addr_selects_cd(0x8000_0900)); // 00:01.1 same CD (PIIX fn1)
    assert!(!pci_addr_selects_cd(0x8000_0000)); // 00:00.0 virtio
    assert!(!pci_addr_selects_cd(0x8000_0800)); // 00:01.0 ISA
    assert!(!pci_addr_selects_cd(0x8000_4000)); // 00:08.0 host
    assert!(is_ata_primary_port(0x1F0));
    assert!(is_ata_primary_port(0x1F7));
    assert!(is_ata_primary_port(0x3F6));
    assert!(is_ata_primary_port(0x170));
    assert!(is_ata_primary_port(0x177));
    assert!(is_ata_primary_port(0x376));
    assert!(!is_ata_primary_port(0x3F8));
    assert!(!is_bmide_port(0xC400));
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
    // IDE is 00:00.1 — virtio fn1; PEI DID probe is 00:00.0.
    pci_write_addr(0x8000_0102);
    assert_eq!(
        pci_read_data(0xCFC, 2) & 0xffff,
        u32::from(GUEST_CD_PCI_DEVICE)
    );
    pci_write_addr(0x8000_0902);
    assert_eq!(
        pci_read_data(0xCFC, 2) & 0xffff,
        u32::from(GUEST_CD_PCI_DEVICE)
    );
    pci_write_addr(pci_config_addr() | 0x0C);
    let ide_ht = pci_read_data(0xCFC, 4);
    assert_eq!((ide_ht >> 16) & 0xff, 0x00);
    assert_eq!(host_identify_word0(), Some(0x85C0));
    let pvd = host_read10(16).expect("READ(10) LBA 16");
    assert_eq!(&pvd[1..6], b"CD001");
    let br = host_read10(17).expect("READ(10) LBA 17");
    assert_eq!(&br[7..30], b"EL TORITO SPECIFICATION");
    assert!(cdrom_visible_evidence(true, true, 1));
    assert!(take_marker());
    assert!(!take_marker());
    assert_eq!(M7_E5_OVMF_CDROM_OK_MARKER, "RAYNU-V-M7-E5-OVMF-CDROM-OK");
    reset();
}

#[test]
fn atapi_signature_packet_reason_and_ata_identify_abort() {
    reset();
    assert!(present_placeholder());
    assert_eq!(ata_io_accesses(), 0);
    assert_eq!(ata_io(0x01F4, true, 1, 0) as u8, 0x14);
    assert_eq!(ata_io(0x01F5, true, 1, 0) as u8, 0xEB);
    assert_eq!(ata_io_accesses(), 2);
    let _ = ata_io(0x01F7, false, 1, 0xEC);
    assert_eq!(ata_io(0x01F7, true, 1, 0) as u8 & 0x01, 0x01);
    assert_eq!(ata_io(0x01F4, true, 1, 0) as u8, 0x14);
    let _ = ata_io(0x01F7, false, 1, 0xA0);
    assert_eq!(ata_io(0x01F2, true, 1, 0) as u8, 0x01);
    let _ = ata_io(0x01F7, false, 1, 0x90);
    assert_eq!(ata_io(0x01F4, true, 1, 0) as u8, 0x14);
    assert_eq!(ata_io(0x01F5, true, 1, 0) as u8, 0xEB);
    reset();
}

#[test]
fn pci_bar0_probe_reports_eight_byte_io() {
    reset();
    assert!(present_placeholder());
    pci_write_addr(pci_config_addr() | 0x10);
    pci_write_data(0xCFC, 4, 0xFFFF_FFFF);
    assert_eq!(
        pci_read_data(0xCFC, 4),
        0xFFFF_FFF9,
        "8-byte I/O BAR: mask 0xFFFFFFF8 | 1"
    );
    pci_write_addr(pci_config_addr() | 0x20);
    pci_write_data(0xCFC, 4, 0xFFFF_FFFF);
    assert_eq!(
        pci_read_data(0xCFC, 4),
        0xFFFF_FFF1,
        "16-byte I/O BMIDE: mask 0xFFFFFFF0 | 1"
    );
    reset();
}

#[test]
fn pci_bar0_relocated_packet_read10_counts_sector() {
    reset();
    assert!(present_placeholder());
    pci_write_addr(pci_config_addr() | 0x10);
    pci_write_data(0xCFC, 4, 0xC000);
    assert_eq!(pci_read_data(0xCFC, 4) & 0xFFF8, 0xC000);
    assert!(is_ata_primary_port(0xC000));
    assert!(is_ata_primary_port(0x1F0), "legacy 1F0 stays decoded");
    let _ = ata_io(0xC006, false, 1, 0xA0);
    let _ = ata_io(0xC007, false, 1, 0xA0);
    assert_eq!(ata_io(0xC002, true, 1, 0) as u8, 0x01);
    let cdb = [0x28u8, 0, 0, 0, 0, 16, 0, 0, 1, 0, 0, 0];
    for chunk in cdb.chunks(2) {
        let w = u64::from(chunk[0]) | (u64::from(chunk[1]) << 8);
        let _ = ata_io(0xC000, false, 2, w);
    }
    assert_eq!(last_scsi(), 0x28);
    assert_eq!(ata_io(0xC000, true, 1, 0) as u8, 1);
    assert!(sectors_read() >= 1);
    pci_write_addr(pci_config_addr() | 0x20);
    pci_write_data(0xCFC, 4, 0xC400);
    assert!(is_bmide_port(0xC400));
    assert_eq!(bmide_io(0xC400, true, 1, 0xFF) as u8, 0);
    assert!(!is_bmide_port(0x1F0));
    reset();
}

#[test]
fn slave_absent_status_zero_identify_aborts() {
    reset();
    assert!(present_placeholder());
    // DEV bit 4: slave. Nested Intel f93caee identified four CDs (0xA1 x4).
    let _ = ata_io(0x01F6, false, 1, 0xB0);
    assert_eq!(ata_io(0x01F7, true, 1, 0) as u8, 0);
    let _ = ata_io(0x01F7, false, 1, 0xA1);
    assert_eq!(ata_io(0x01F7, true, 1, 0) as u8, 0);
    let _ = ata_io(0x01F6, false, 1, 0xA0);
    assert_eq!(host_identify_word0(), Some(0x85C0));
    reset();
}

#[test]
fn secondary_channel_packet_read10() {
    reset();
    assert!(present_placeholder());
    let _ = ata_io(0x0177, false, 1, 0xA0);
    let cdb = [0x28u8, 0, 0, 0, 0, 16, 0, 0, 1, 0, 0, 0];
    for chunk in cdb.chunks(2) {
        let w = u64::from(chunk[0]) | (u64::from(chunk[1]) << 8);
        let _ = ata_io(0x0170, false, 2, w);
    }
    assert_eq!(last_scsi(), 0x28);
    assert!(sectors_read() >= 1);
    reset();
}

#[test]
fn unpresented_pci_is_empty() {
    reset();
    pci_write_addr(pci_config_addr());
    assert_eq!(pci_read_data(0xCFC, 4), 0xFFFF_FFFF);
    assert!(present(&[0u8; ISO_SECTOR], 2));
    pci_write_addr(0x8000_0800);
    assert_eq!(pci_read_data(0xCFC, 4), 0xFFFF_FFFF);
    reset();
}
