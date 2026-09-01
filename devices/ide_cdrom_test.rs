    use super::{
    ata_io, ata_io_accesses, bmide_io, cdrom_visible_evidence, eltorito_boot_image_read,
    eltorito_catalog_read, eltorito_validation_checksum_ok, host_identify_word0, host_read10,
    is_ata_data_port, is_ata_primary_port, is_bmide_port, is_pci_data_port, last_ata_cmd, last_read_lba, last_scsi,
    pci_addr_selects_cd, pci_bdf, pci_bar0, pci_bar4, pci_command, pci_cmd_writes, last_pci_cmd_write,
    pci_idetim,
    pci_config_addr, pci_read_data, pci_write_addr, pci_write_data,
    take_ide_pci_cmd_wr_exit,
    take_ide_pci_cmd_ata_hlt,
    ide_pci_cmd_ata_hlt_pending,
    linux_hides_duplicate_slot0_ide, linux_hides_piix_ide, linux_ata_floating_bus,
    product_iso_hides_ide,
    present, present_placeholder, product_iso_window_armed, is_lab_eltorito_media,
    is_lab_eltorito_stub_len, reset, retained_len, sectors_read, take_marker, write_eltorito_efi_pe,
    write_eltorito_fat12, edk2_eltorito_partition_blocks, edk2_fat12_bootx64_ok,
    edk2_iso9660_bootx64_ok, edk2_pe_loadimage_ok, write_placeholder_iso,
    ELTORITO_BOOTX64_OFF, ELTORITO_PAYLOAD_MAGIC, ELTORITO_SECTOR_COUNT, GUEST_CD_ISO_CAP,
    GUEST_CD_PCI_DEVICE,
    GUEST_CD_PCI_VENDOR, ISO_SECTOR, M7_E5_OVMF_CDROM_OK_MARKER, MOCK_EFI_ISO_BYTES,
    GUEST_CD_PCI_CLASS, GUEST_CD_PCI_PROG_IF, GUEST_CD_PCI_IDETIM,
    GUEST_CD_PCI_CMD_WMASK, GUEST_CD_PCI_STATUS,
    GUEST_CD_PCI_INT_LINE_RESET, GUEST_CD_PCI_INT_PIN,
    GUEST_CD_PCI_BAR4_PROBE, GUEST_CD_BMIDE_WIDE, GUEST_CD_BMIDE_UNUSED,
    pci_int_line, pci_latency, pci_cache_line,
    bmide_cmd, bmide_status, bmide_ins,
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
    assert!(is_ata_data_port(0x1F0));
    assert!(is_ata_primary_port(0x1F7));
    assert!(!is_ata_data_port(0x1F7));
    assert!(is_ata_primary_port(0x3F6));
    assert!(!is_ata_data_port(0x3F6));
    assert!(is_ata_primary_port(0x170));
    assert!(is_ata_primary_port(0x177));
    assert!(is_ata_primary_port(0x376));
    assert_eq!(
        ata_io(0x170, true, 1, 0) as u8,
        0xFF,
        "nested iso=0 firmware IdeBus secondary empty: 0x170 floating"
    );
    assert_eq!(
        ata_io(0x376, true, 1, 0) as u8,
        0xFF,
        "nested iso=0 firmware IdeBus secondary empty: 0x376 floating"
    );
    assert!(!is_ata_primary_port(0x02), "unimplemented BAR1 does not steal port 2");
    assert!(!is_ata_primary_port(0x3F8));
    assert!(!is_bmide_port(0xC400));
    assert!(is_pci_data_port(0xCFC));
    assert!(is_pci_data_port(0xCFE));
    assert!(!is_pci_data_port(0xCF8));
}

#[test]
fn secondary_channel_is_empty_not_atapi_alias() {
    reset();
    assert!(present_placeholder());
    assert_eq!(
        ata_io(0x1F7, true, 1, 0) as u8 & 0x40,
        0x40,
        "primary DRDY"
    );
    assert_eq!(
        ata_io(0x170, true, 1, 0) as u8,
        0xFF,
        "nested iso=0 firmware IdeBus secondary empty: not ATAPI alias"
    );
    assert_eq!(ata_io(0x376, true, 1, 0) as u8, 0xFF);
    assert!(!is_ata_data_port(0x170));
    assert!(!is_ata_primary_port(0x02));
    let before = ata_io_accesses();
    let _ = ata_io(0x376, false, 1, 0x02);
    assert!(
        ata_io_accesses() > before,
        "nested iso=0 firmware IdeBus secondary empty: Start PIO counted"
    );
    reset();
}

#[test]
fn linux_hides_duplicate_slot0_ide_not_piix() {
    reset();
    crate::boot::serial::set_linux_earlycon_share(false);
    assert!(!linux_hides_duplicate_slot0_ide(false, 0x8000_0100));
    assert!(linux_hides_duplicate_slot0_ide(true, 0x8000_0100));
    assert!(!linux_hides_duplicate_slot0_ide(true, 0x8000_0900));
    assert!(present_placeholder());
    pci_write_addr(0x8000_0100);
    assert_ne!(pci_read_data(0xCFC, 4), 0xFFFF_FFFF);
    crate::boot::serial::set_linux_earlycon_share(true);
    pci_write_addr(0x8000_0100);
    assert_eq!(pci_read_data(0xCFC, 4), 0xFFFF_FFFF);
    crate::boot::serial::set_linux_earlycon_share(false);
    reset();
}

#[test]
fn linux_hides_piix_ide_after_high_half() {
    reset();
    crate::boot::serial::set_linux_earlycon_share(false);
    crate::boot::serial::set_linux_high_half(false);
    assert!(!linux_hides_piix_ide(false, 0x8000_0900));
    assert!(linux_hides_piix_ide(true, 0x8000_0900));
    assert!(!linux_hides_piix_ide(true, 0x8000_0100));
    assert!(!linux_hides_piix_ide(true, 0x8000_0800));
    assert!(present_placeholder());
    pci_write_addr(0x8000_0900);
    crate::boot::serial::set_linux_earlycon_share(true);
    assert_ne!(
        pci_read_data(0xCFC, 4),
        0xFFFF_FFFF,
        "bootimg share must not hide PIIX (GRUB ATAPI)"
    );
    crate::boot::serial::set_linux_high_half(true);
    pci_write_addr(0x8000_0900);
    assert_eq!(pci_read_data(0xCFC, 4), 0xFFFF_FFFF);
    crate::boot::serial::set_linux_earlycon_share(false);
    crate::boot::serial::set_linux_high_half(false);
    reset();
}

#[test]
fn product_iso_hides_ide_on_window_iso0_keeps_ide() {
    reset();
    crate::boot::serial::set_linux_earlycon_share(false);
    crate::boot::serial::set_linux_high_half(false);
    assert!(!product_iso_hides_ide(0x8000_0900));
    assert!(!product_iso_hides_ide(0x8000_0100));
    assert!(present_placeholder());
    pci_write_addr(0x8000_0900);
    assert_ne!(
        pci_read_data(0xCFC, 4),
        0xFFFF_FFFF,
        "iso=0 firmware still enumerates PIIX IDE"
    );
    pci_write_addr(0x8000_0100);
    assert_ne!(
        pci_read_data(0xCFC, 4),
        0xFFFF_FFFF,
        "iso=0 firmware still enumerates slot0 IDE"
    );
    reset();
    let extra = MOCK_EFI_ISO_BYTES + ISO_SECTOR;
    let mut iso = vec![0u8; extra];
    write_placeholder_iso(&mut iso[..MOCK_EFI_ISO_BYTES]);
    assert!(present(&iso, 9));
    assert!(product_iso_window_armed());
    assert!(
        !product_iso_hides_ide(0x8000_0900),
        "firmware HLT skip without inject: OVMF El Torito needs PIIX ATAPI"
    );
    assert!(!product_iso_hides_ide(0x8000_0100));
    assert!(!product_iso_hides_ide(0x8000_0800), "do not hide PIIX ISA");
    assert!(!product_iso_hides_ide(0x8000_1000), "do not hide virtio-blk");
    pci_write_addr(0x8000_0900);
    assert_ne!(
        pci_read_data(0xCFC, 4),
        0xFFFF_FFFF,
        "product ISO still enumerates PIIX IDE for El Torito"
    );
    pci_write_addr(0x8000_0100);
    assert_ne!(pci_read_data(0xCFC, 4), 0xFFFF_FFFF);
    reset();
}

#[test]
fn linux_ata_floating_bus_after_high_half() {
    reset();
    crate::boot::serial::set_linux_high_half(false);
    assert!(!linux_ata_floating_bus(false));
    assert!(linux_ata_floating_bus(true));
    assert!(present_placeholder());
    let st = ata_io(0x1F7, true, 1, 0) as u8;
    assert_ne!(st, 0xFF, "GRUB/OVMF still need live ATA status");
    crate::boot::serial::set_linux_high_half(true);
    assert_eq!(ata_io(0x1F7, true, 1, 0) as u8, 0xFF);
    assert_eq!(ata_io(0x3F6, true, 1, 0) as u8, 0xFF);
    let _ = ata_io(0x1F7, false, 1, 0xA1);
    assert_eq!(ata_io(0x1F7, true, 1, 0) as u8, 0xFF);
    crate::boot::serial::set_linux_high_half(false);
    reset();
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
        0,
        "nested iso=0 firmware IdeBus ISA BAR: unimplemented BAR0 probe is 0"
    );
    pci_write_addr(pci_config_addr() | 0x20);
    pci_write_data(0xCFC, 4, 0xFFFF_FFFF);
    assert_eq!(
        pci_read_data(0xCFC, 4),
        0xFFFF_FFF1,
        "16-byte I/O BMIDE: mask 0xFFFFFFF0 | 1"
    );
    assert_eq!(
        pci_bar0(),
        0,
        "nested iso=0 firmware IdeBus ISA BAR: BAR0 stays unimplemented"
    );
    reset();
}

#[test]
fn pci_bar0_probe_does_not_clobber_live_bar() {
    reset();
    assert!(present_placeholder());
    pci_write_addr(pci_config_addr() | 0x10);
    pci_write_data(0xCFC, 4, 0xFFFF_FFFF);
    assert_eq!(pci_read_data(0xCFC, 4), 0);
    assert_eq!(
        pci_bar0(),
        0,
        "nested iso=0 firmware IdeBus ISA BAR: live BAR0 stays 0"
    );
    assert!(
        is_ata_primary_port(0x1F0),
        "legacy 0x1F0 stays decoded after size probe"
    );
    pci_write_data(0xCFC, 4, 0x1F1);
    assert_eq!(pci_bar0(), 0);
    reset();
}

#[test]
fn pci_bar0_probe_oneshot_second_read_is_live() {
    reset();
    assert!(present_placeholder());
    pci_write_addr(pci_config_addr() | 0x10);
    pci_write_data(0xCFC, 4, 0xFFFF_FFFF);
    assert_eq!(pci_read_data(0xCFC, 4), 0);
    assert_eq!(
        pci_read_data(0xCFC, 4),
        0,
        "nested iso=0 firmware IdeBus ISA BAR: second dword stays unimplemented"
    );
    assert_eq!(pci_bar0(), 0);
    reset();
}

#[test]
fn pci_bar4_bmide_unprogrammed_until_assigned() {
    reset();
    assert!(present_placeholder());
    assert_eq!(
        pci_bar4(),
        1,
        "nested iso=0 firmware IdeBus BM unprogrammed: BAR4 I/O address 0"
    );
    assert!(!is_bmide_port(0xCC00));
    assert!(!is_bmide_port(0xC400));
    pci_write_addr(pci_config_addr() | 0x20);
    pci_write_data(0xCFC, 4, 0xCC01);
    assert_eq!(pci_bar4(), 0xCC01);
    assert!(
        !is_bmide_port(0xCC00),
        "nested iso=0 firmware IdeBus BMIDE IO: BAR4 assigned, COMMAND.IO off"
    );
    pci_write_addr(pci_config_addr() | 0x04);
    pci_write_data(0xCFC, 4, 0x0001);
    assert!(
        is_bmide_port(0xCC00),
        "nested iso=0 firmware IdeBus BMIDE IO: COMMAND.IO decodes BAR4"
    );
    pci_write_addr(pci_config_addr() | 0x20);
    pci_write_data(0xCFC, 4, 0);
    assert_eq!(pci_bar4(), 1);
    reset();
}

#[test]
fn pci_bar4_probe_stays_until_restore() {
    reset();
    assert!(present_placeholder());
    pci_write_addr(pci_config_addr() | 0x20);
    pci_write_data(0xCFC, 4, 0xFFFF_FFFF);
    assert_eq!(
        pci_read_data(0xCFC, 4),
        GUEST_CD_PCI_BAR4_PROBE,
        "nested iso=0 firmware IdeBus BM sticky: first dword is mask"
    );
    assert_eq!(
        pci_read_data(0xCFC, 4),
        GUEST_CD_PCI_BAR4_PROBE,
        "nested iso=0 firmware IdeBus BM sticky: second dword stays mask"
    );
    assert_eq!(pci_bar4(), 1);
    pci_write_data(0xCFC, 4, 0xCC01);
    assert_eq!(pci_bar4(), 0xCC01);
    assert_eq!(pci_read_data(0xCFC, 4), 0xCC01);
    reset();
}

#[test]
fn pci_command_write_latches_ide_cmd_wake() {
    reset();
    assert!(present_placeholder());
    pci_write_addr(pci_config_addr() | 0x04);
    assert_eq!(
        pci_read_data(0xCFC, 4) & 0xffff,
        0,
        "product ISO firmware IDE cmd reset 0"
    );
    assert!(
        !take_ide_pci_cmd_wr_exit(),
        "product ISO firmware wake IDE cmd: idle"
    );
    pci_write_data(0xCFC, 4, 0x0005);
    assert_eq!(
        pci_command(),
        0x0005,
        "nested iso=0 firmware IdeBus PCI"
    );
    assert!(
        take_ide_pci_cmd_wr_exit(),
        "product ISO firmware wake IDE cmd"
    );
    assert!(
        ide_pci_cmd_ata_hlt_pending(),
        "product ISO firmware IDE cmd ATA on HLT: survives I/O take"
    );
    assert!(
        take_ide_pci_cmd_ata_hlt(),
        "product ISO firmware IDE cmd ATA on HLT: survives I/O take"
    );
    assert!(!take_ide_pci_cmd_wr_exit());
    assert!(!take_ide_pci_cmd_ata_hlt());
    assert!(!ide_pci_cmd_ata_hlt_pending());
    pci_write_addr(pci_config_addr() | 0x10);
    pci_write_data(0xCFC, 4, 0x1F1);
    assert!(
        !take_ide_pci_cmd_wr_exit(),
        "product ISO firmware wake IDE cmd: BAR write is not Start"
    );
    assert!(
        !take_ide_pci_cmd_ata_hlt(),
        "product ISO firmware IDE cmd ATA on HLT: BAR write is not Start"
    );
    reset();
}

#[test]
fn pci_command_disable_is_not_start() {
    reset();
    assert!(present_placeholder());
    pci_write_addr(pci_config_addr() | 0x04);
    pci_write_data(0xCFC, 4, 0);
    assert_eq!(
        pci_command(),
        0,
        "nested iso=0 firmware IdeBus PCI cmd: write 0 stays 0"
    );
    assert_eq!(
        pci_cmd_writes(),
        1,
        "nested iso=0 firmware IdeBus prog-if: disable still counts cmdn"
    );
    assert_eq!(last_pci_cmd_write(), 0);
    assert!(
        !take_ide_pci_cmd_wr_exit(),
        "nested iso=0 firmware IdeBus PCI cmd: disable is not Start"
    );
    reset();
}

#[test]
fn pci_command_wmask_drops_mse() {
    reset();
    assert!(present_placeholder());
    pci_write_addr(pci_config_addr() | 0x04);
    pci_write_data(0xCFC, 4, 0x0007);
    assert_eq!(
        pci_command(),
        GUEST_CD_PCI_CMD_WMASK,
        "nested iso=0 firmware IdeBus PCI cmd mask: EnableAttributes 0x7 stores 0x5"
    );
    assert_eq!(last_pci_cmd_write(), 0x0005);
    assert!(take_ide_pci_cmd_wr_exit());
    reset();
}

#[test]
fn pci_status_fast_back_with_devsel() {
    reset();
    assert!(present_placeholder());
    pci_write_addr(pci_config_addr() | 0x04);
    assert_eq!(
        pci_read_data(0xCFC, 4),
        GUEST_CD_PCI_STATUS,
        "nested iso=0 firmware IdeBus PCI status: reset FAST_BACK+DEVSEL"
    );
    pci_write_data(0xCFC, 4, 0x0007);
    assert_eq!(
        pci_read_data(0xCFC, 4),
        u32::from(GUEST_CD_PCI_CMD_WMASK) | GUEST_CD_PCI_STATUS,
        "nested iso=0 firmware IdeBus PCI status: command write keeps FAST_BACK"
    );
    reset();
}

#[test]
fn pci_int_line_reset_zero_persists() {
    reset();
    assert!(present_placeholder());
    pci_write_addr(pci_config_addr() | 0x3C);
    assert_eq!(
        pci_read_data(0xCFC, 4),
        u32::from(GUEST_CD_PCI_INT_LINE_RESET) | (u32::from(GUEST_CD_PCI_INT_PIN) << 8),
        "nested iso=0 firmware IdeBus INTLINE: reset line 0 pin 0"
    );
    assert_eq!(pci_int_line(), 0);
    pci_write_data(0xCFC, 4, 0x0000_010E);
    assert_eq!(
        pci_int_line(),
        0x0E,
        "nested iso=0 firmware IdeBus INTLINE: PciBus write persists"
    );
    assert_eq!(
        pci_read_data(0xCFC, 4) & 0xff,
        0x0E
    );
    assert_eq!(
        (pci_read_data(0xCFC, 4) >> 8) & 0xff,
        u32::from(GUEST_CD_PCI_INT_PIN),
        "nested iso=0 firmware IdeBus INTPIN: pin stays 0"
    );
    reset();
}

#[test]
fn pci_cache_line_and_latency_persist() {
    reset();
    assert!(present_placeholder());
    pci_write_addr(pci_config_addr() | 0x0C);
    assert_eq!(
        pci_read_data(0xCFC, 4),
        0,
        "nested iso=0 firmware IdeBus LAT: reset CLS and latency 0"
    );
    pci_write_data(0xCFC, 4, 0x0000_2010);
    assert_eq!(
        pci_cache_line(),
        0x10,
        "nested iso=0 firmware IdeBus LAT: cache line persists"
    );
    assert_eq!(
        pci_latency(),
        0x20,
        "nested iso=0 firmware IdeBus LAT: latency persists"
    );
    assert_eq!(
        (pci_read_data(0xCFC, 4) >> 16) & 0xff,
        0,
        "nested iso=0 firmware IdeBus LAT: header type stays 0"
    );
    reset();
}

#[test]
fn pci_class_prog_if_is_native_capable() {
    reset();
    assert!(present_placeholder());
    pci_write_addr(pci_config_addr() | 0x08);
    assert_eq!(
        pci_read_data(0xCFC, 4),
        GUEST_CD_PCI_CLASS,
        "nested iso=0 firmware IdeBus ISA BAR: class dword 0x01018000"
    );
    pci_write_addr(pci_config_addr() | 0x09);
    assert_eq!(
        pci_read_data(0xCFC, 1),
        u32::from(GUEST_CD_PCI_PROG_IF),
        "nested iso=0 firmware IdeBus ISA BAR: byte 0x80 not 0x8F"
    );
    assert_eq!(pci_cmd_writes(), 0);
    reset();
}

#[test]
fn pci_idetim_decode_enable_persists() {
    reset();
    assert!(present_placeholder());
    pci_write_addr(pci_config_addr() | 0x40);
    assert_eq!(
        pci_read_data(0xCFC, 4),
        GUEST_CD_PCI_IDETIM,
        "nested iso=0 firmware IdeBus IDETIM: reset decode-enable"
    );
    assert_eq!(pci_idetim(), 0x8000_8000);
    pci_write_data(0xCFC, 2, 0);
    assert_eq!(
        pci_read_data(0xCFC, 2),
        0,
        "nested iso=0 firmware IdeBus IDETIM: primary write persists"
    );
    assert_eq!(pci_idetim() & 0xFFFF, 0);
    assert_eq!(pci_idetim() >> 16, 0x8000);
    reset();
}

#[test]
fn pci_bar0_relocated_packet_read10_counts_sector() {
    reset();
    assert!(present_placeholder());
    pci_write_addr(pci_config_addr() | 0x10);
    pci_write_data(0xCFC, 4, 0xC000);
    assert_eq!(
        pci_read_data(0xCFC, 4),
        0,
        "nested iso=0 firmware IdeBus ISA BAR: BAR0 write ignored"
    );
    assert!(!is_ata_primary_port(0xC000));
    assert!(is_ata_data_port(0x1F0), "legacy 1F0 stays a data FIFO");
    assert!(!is_ata_data_port(0x1F7));
    assert!(is_ata_primary_port(0x1F0), "legacy 1F0 stays decoded");
    let _ = ata_io(0x1F6, false, 1, 0xA0);
    let _ = ata_io(0x1F7, false, 1, 0xA0);
    assert_eq!(ata_io(0x1F2, true, 1, 0) as u8, 0x01);
    let cdb = [0x28u8, 0, 0, 0, 0, 16, 0, 0, 1, 0, 0, 0];
    for chunk in cdb.chunks(2) {
        let w = u64::from(chunk[0]) | (u64::from(chunk[1]) << 8);
        let _ = ata_io(0x1F0, false, 2, w);
    }
    assert_eq!(last_scsi(), 0x28);
    assert_eq!(ata_io(0x1F0, true, 1, 0) as u8, 1);
    assert!(sectors_read() >= 1);
    pci_write_addr(pci_config_addr() | 0x20);
    pci_write_data(0xCFC, 4, 0xC400);
    pci_write_addr(pci_config_addr() | 0x04);
    pci_write_data(0xCFC, 4, 0x0001);
    assert!(is_bmide_port(0xC400));
    assert_eq!(bmide_io(0xC400, true, 1, 0xFF) as u8, 0);
    assert!(!is_bmide_port(0x1F0));
    reset();
}

#[test]
fn bmide_qemu_byte_ops() {
    reset();
    assert!(present_placeholder());
    pci_write_addr(pci_config_addr() | 0x20);
    pci_write_data(0xCFC, 4, 0xC400);
    pci_write_addr(pci_config_addr() | 0x04);
    pci_write_data(0xCFC, 4, 0x0001);
    assert!(is_bmide_port(0xC400));
    assert_eq!(
        bmide_io(0xC400, true, 4, 0) as u32,
        GUEST_CD_BMIDE_WIDE,
        "nested iso=0 firmware IdeBus BMIDE: dword IN is all-ones"
    );
    assert_eq!(
        bmide_io(0xC401, true, 1, 0) as u8,
        GUEST_CD_BMIDE_UNUSED,
        "nested iso=0 firmware IdeBus BMIDE: unused byte is 0xff"
    );
    assert_eq!(bmide_io(0xC400, true, 1, 0) as u8, 0);
    assert_eq!(bmide_io(0xC402, true, 1, 0) as u8, 0);
    let _ = bmide_io(0xC400, false, 1, 0x09);
    assert_eq!(bmide_cmd(), 0x09);
    let _ = bmide_io(0xC402, false, 1, 0x60);
    assert_eq!(bmide_status(), 0x60);
    assert!(bmide_ins() >= 4);
    reset();
}

#[test]
fn set_features_succeeds_then_packet_read10() {
    reset();
    assert!(present_placeholder());
    assert_eq!(host_identify_word0(), Some(0x85C0));
    // Nested Intel 48c598a: OUT 0xEF then IN EAX,DX; ABRT never PACKET.
    let _ = ata_io(0x01F1, false, 1, 0x03);
    let _ = ata_io(0x01F7, false, 1, 0xEF);
    assert_eq!(last_ata_cmd(), 0xEF);
    let st = ata_io(0x01F7, true, 1, 0) as u8;
    assert_eq!(st & 0x01, 0, "SET FEATURES must not ABRT");
    assert_ne!(st & 0x40, 0, "DRDY");
    let pvd = host_read10(16).expect("READ(10) after SET FEATURES");
    assert_eq!(&pvd[1..6], b"CD001");
    assert_eq!(last_scsi(), 0x28);
    assert!(sectors_read() >= 1);
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

#[test]
fn placeholder_eltorito_pe_and_catalog_load_reads() {
    reset();
    let mut pe = [0u8; 0x800];
    assert!(write_eltorito_efi_pe(&mut pe) > 0);
    assert_eq!(&pe[0..2], b"MZ");
    assert_eq!(&pe[0x80..0x84], b"PE\0\0");
    assert_eq!(pe[0x98 + 0x44], 10, "EFI_APPLICATION subsystem");
    assert_eq!(
        u16::from_le_bytes([pe[0x96], pe[0x97]]),
        0x2022,
        "GenFw EFI app characteristics"
    );
    assert_eq!(
        &pe[0x200..0x200 + 8],
        &[0xBA, 0xFB, 0x03, 0x00, 0x00, 0xB0, 0x03, 0xEE],
        "entry clears COM1 LCR.DLAB before THR"
    );
    assert_eq!(u16::from_le_bytes([pe[0x86], pe[0x87]]), 2, ".text + .reloc");
    assert_eq!(
        u32::from_le_bytes(pe[0x98 + 0x20..0x98 + 0x24].try_into().unwrap()),
        0x1000,
        "SectionAlignment 0x1000 for ProtectUefiImage"
    );
    assert_eq!(
        u16::from_le_bytes(pe[0x98 + 0x46..0x98 + 0x48].try_into().unwrap()),
        0x0160,
        "NX_COMPAT | DYNAMIC_BASE | HIGH_ENTROPY_VA"
    );
    let dd5 = 0x98 + 0x70 + 5 * 8;
    assert_eq!(u32::from_le_bytes(pe[dd5..dd5 + 4].try_into().unwrap()), 0x2000);
    assert_eq!(u32::from_le_bytes(pe[dd5 + 4..dd5 + 8].try_into().unwrap()), 8);
    let mut fat = [0u8; 16384];
    assert_eq!(write_eltorito_fat12(&mut fat), 16384);
    assert_eq!(fat[510], 0x55);
    assert_eq!(fat[511], 0xAA);
    assert_eq!(&fat[ELTORITO_BOOTX64_OFF..ELTORITO_BOOTX64_OFF + 2], b"MZ");
    assert!(edk2_pe_loadimage_ok(&pe), "DxeCore LoadImage headers");
    assert!(edk2_fat12_bootx64_ok(&fat), "FatDxe OpenDevice + BOOTX64 walk");
    assert_eq!(edk2_eltorito_partition_blocks(ELTORITO_SECTOR_COUNT), 8);
    let mut iso_buf = [0u8; crate::devices::ide_cdrom::MOCK_EFI_ISO_BYTES];
    write_placeholder_iso(&mut iso_buf);
    assert!(edk2_iso9660_bootx64_ok(&iso_buf), "ISO9660 EFI/BOOT/BOOTX64");
    assert!(present_placeholder());
    assert!(!eltorito_catalog_read());
    assert!(!eltorito_boot_image_read());
    let pvd = host_read10(16).expect("PVD");
    assert_eq!(&pvd[1..6], b"CD001");
    assert!(!eltorito_catalog_read());
    assert!(!eltorito_boot_image_read());
    let cat = host_read10(20).expect("catalog");
    assert_eq!(cat[0], 0x01);
    assert_eq!(cat[30], 0x55);
    assert_eq!(cat[31], 0xAA);
    assert!(eltorito_validation_checksum_ok(&cat[..32]));
    assert_eq!(cat[32], 0x88);
    assert!(eltorito_catalog_read());
    assert!(!eltorito_boot_image_read());
    assert_eq!(last_read_lba(), 20);
    let img = host_read10(22).expect("load LBA FAT");
    assert_eq!(img[510], 0x55);
    assert_eq!(img[511], 0xAA);
    assert!(eltorito_boot_image_read());
    assert_eq!(last_read_lba(), 22);
    let pe_lba = 22 + (ELTORITO_BOOTX64_OFF / ISO_SECTOR) as u32;
    let file_sec = host_read10(pe_lba).expect("BOOTX64 ISO sector");
    assert_eq!(&file_sec[0..2], b"MZ");
    assert_eq!(&file_sec[0x80..0x84], b"PE\0\0");
    let mut found = false;
    for w in file_sec.windows(ELTORITO_PAYLOAD_MAGIC.len()) {
        if w == ELTORITO_PAYLOAD_MAGIC {
            found = true;
            break;
        }
    }
    assert!(found, "PE .text must embed RN-ELT immediates");
    let iso_pe = host_read10(33).expect("ISO9660 BOOTX64.EFI");
    assert_eq!(&iso_pe[0..2], b"MZ");
    reset();
}

#[test]
fn product_iso_window_does_not_truncate_and_is_not_lab_stub() {
    reset();
    assert!(is_lab_eltorito_stub_len(0));
    assert!(is_lab_eltorito_stub_len(MOCK_EFI_ISO_BYTES));
    assert!(!is_lab_eltorito_stub_len(MOCK_EFI_ISO_BYTES + ISO_SECTOR));
    assert!(present_placeholder());
    assert_eq!(retained_len(), MOCK_EFI_ISO_BYTES);
    assert_eq!(GUEST_CD_ISO_CAP, MOCK_EFI_ISO_BYTES);
    assert!(!product_iso_window_armed());
    assert!(is_lab_eltorito_media());
    reset();
    let extra = MOCK_EFI_ISO_BYTES + ISO_SECTOR;
    let mut iso = vec![0u8; extra];
    write_placeholder_iso(&mut iso[..MOCK_EFI_ISO_BYTES]);
    iso[MOCK_EFI_ISO_BYTES] = 0x5A;
    iso[extra - 1] = 0xA5;
    assert!(present(&iso, 9));
    assert_eq!(retained_len(), extra);
    assert!(product_iso_window_armed());
    assert!(!is_lab_eltorito_media());
    let last_lba = (MOCK_EFI_ISO_BYTES / ISO_SECTOR) as u32;
    let last = host_read10(last_lba).expect("product window last sector");
    assert_eq!(last[0], 0x5A);
    assert_eq!(last[ISO_SECTOR - 1], 0xA5);
    reset();
    assert!(!product_iso_window_armed());
    assert!(is_lab_eltorito_media());
}

fn host_read10_n(lba: u32, nsec: u8) -> Vec<u8> {
    host_read10_count(lba, u16::from(nsec), 0)
}

/// PACKET READ(10) of `nsec` CD sectors. `cyl` is the ATAPI byte-count
/// written to LBA mid/high before PACKET (0 / 0xFFFF = full 31-sector DRQ).
fn host_read10_count(lba: u32, nsec: u16, cyl: u16) -> Vec<u8> {
    let _ = ata_io(0x01F4, false, 1, u64::from(cyl as u8));
    let _ = ata_io(0x01F5, false, 1, u64::from((cyl >> 8) as u8));
    let _ = ata_io(0x01F7, false, 1, 0xA0);
    let cdb = [
        0x28u8,
        0,
        (lba >> 24) as u8,
        (lba >> 16) as u8,
        (lba >> 8) as u8,
        lba as u8,
        0,
        (nsec >> 8) as u8,
        nsec as u8,
        0,
        0,
        0,
    ];
    for chunk in cdb.chunks(2) {
        let w = u64::from(chunk[0]) | (u64::from(chunk.get(1).copied().unwrap_or(0)) << 8);
        let _ = ata_io(0x01F0, false, 2, w);
    }
    let bytes = nsec as usize * ISO_SECTOR;
    let mut out = vec![0u8; bytes];
    for i in 0..bytes / 2 {
        let w = ata_io(0x01F0, true, 2, 0);
        out[i * 2] = w as u8;
        out[i * 2 + 1] = (w >> 8) as u8;
    }
    out
}

#[test]
fn product_iso_read10_eight_sectors_is_not_short() {
    reset();
    let extra = MOCK_EFI_ISO_BYTES + 8 * ISO_SECTOR;
    let mut iso = vec![0u8; extra];
    write_placeholder_iso(&mut iso[..MOCK_EFI_ISO_BYTES]);
    for i in 0..8 {
        iso[MOCK_EFI_ISO_BYTES + i * ISO_SECTOR] = 0xA0 + i as u8;
    }
    assert!(present(&iso, 9));
    assert!(product_iso_window_armed());
    let lba = (MOCK_EFI_ISO_BYTES / ISO_SECTOR) as u32;
    let buf = host_read10_n(lba, 8);
    assert_eq!(buf.len(), 8 * ISO_SECTOR);
    for i in 0..8 {
        assert_eq!(buf[i * ISO_SECTOR], 0xA0 + i as u8, "sector {i}");
    }
    reset();
}

#[test]
fn product_iso_read10_thirty_two_sectors_is_not_short() {
    reset();
    let extra = MOCK_EFI_ISO_BYTES + 32 * ISO_SECTOR;
    let mut iso = vec![0u8; extra];
    write_placeholder_iso(&mut iso[..MOCK_EFI_ISO_BYTES]);
    for i in 0..32 {
        iso[MOCK_EFI_ISO_BYTES + i * ISO_SECTOR] = 0xB0 + i as u8;
        iso[MOCK_EFI_ISO_BYTES + i * ISO_SECTOR + ISO_SECTOR - 1] = 0x40 + i as u8;
    }
    assert!(present(&iso, 9));
    assert!(product_iso_window_armed());
    let lba = (MOCK_EFI_ISO_BYTES / ISO_SECTOR) as u32;
    // OVMF IdeMode MaxBlock = 0xFFFF/2048 = 31; cylinder 0xFFFF.
    let buf = host_read10_count(lba, 32, 0xFFFF);
    assert_eq!(buf.len(), 32 * ISO_SECTOR);
    for i in 0..32 {
        assert_eq!(buf[i * ISO_SECTOR], 0xB0 + i as u8, "sector {i} first");
        assert_eq!(
            buf[i * ISO_SECTOR + ISO_SECTOR - 1],
            0x40 + i as u8,
            "sector {i} last"
        );
    }
    reset();
}

#[test]
fn product_iso_read10_forty_sectors_two_drqs() {
    reset();
    let extra = MOCK_EFI_ISO_BYTES + 40 * ISO_SECTOR;
    let mut iso = vec![0u8; extra];
    write_placeholder_iso(&mut iso[..MOCK_EFI_ISO_BYTES]);
    for i in 0..40 {
        iso[MOCK_EFI_ISO_BYTES + i * ISO_SECTOR] = 0xC0 + i as u8;
    }
    assert!(present(&iso, 9));
    let lba = (MOCK_EFI_ISO_BYTES / ISO_SECTOR) as u32;
    let buf = host_read10_count(lba, 40, 0xFFFF);
    assert_eq!(buf.len(), 40 * ISO_SECTOR);
    for i in 0..40 {
        assert_eq!(buf[i * ISO_SECTOR], 0xC0 + i as u8, "sector {i}");
    }
    reset();
}

#[test]
fn product_iso_read10_eight_sectors_four_sector_drq() {
    reset();
    let extra = MOCK_EFI_ISO_BYTES + 8 * ISO_SECTOR;
    let mut iso = vec![0u8; extra];
    write_placeholder_iso(&mut iso[..MOCK_EFI_ISO_BYTES]);
    for i in 0..8 {
        iso[MOCK_EFI_ISO_BYTES + i * ISO_SECTOR] = 0xD0 + i as u8;
    }
    assert!(present(&iso, 9));
    let lba = (MOCK_EFI_ISO_BYTES / ISO_SECTOR) as u32;
    // Old 4-sector DRQ completed the CDB short; continue until count=8.
    let buf = host_read10_count(lba, 8, (4 * ISO_SECTOR) as u16);
    assert_eq!(buf.len(), 8 * ISO_SECTOR);
    for i in 0..8 {
        assert_eq!(buf[i * ISO_SECTOR], 0xD0 + i as u8, "sector {i}");
    }
    reset();
}

#[test]
fn product_iso_identify_is_pio_only_and_nien_masks_irq() {
    use crate::devices::guest_irq::{self, ioapic_write, take_inject_vector, ATA_GSI};
    use crate::devices::guest_platform;
    reset();
    guest_irq::reset();
    guest_platform::reset();
    let extra = MOCK_EFI_ISO_BYTES + ISO_SECTOR;
    let mut iso = vec![0u8; extra];
    write_placeholder_iso(&mut iso[..MOCK_EFI_ISO_BYTES]);
    assert!(present(&iso, 9));
    let w0 = host_identify_word0().expect("IDENTIFY");
    assert_eq!(w0, 0x85C0);
    let mut w49 = 0u16;
    for n in 1..50 {
        let w = ata_io(0x01F0, true, 2, 0) as u16;
        if n == 49 {
            w49 = w;
        }
    }
    assert_eq!(w49, 0x0200, "IDENTIFY word 49 LBA, no DMA");
    guest_irq::reset();
    ioapic_write(0, 0x10 + 2 * u32::from(ATA_GSI));
    ioapic_write(0x10, 0x40);
    let _ = ata_io(0x03F6, false, 1, 0x02);
    assert!(
        crate::devices::ide_cdrom::ata_nien(),
        "nIEN latched on device control"
    );
    let _ = ata_io(0x01F7, false, 1, 0xA0);
    assert!(
        take_inject_vector().is_none(),
        "nIEN must suppress ATA IRQ 14"
    );
    let _ = ata_io(0x03F6, false, 1, 0x00);
    let _ = ata_io(0x01F7, false, 1, 0xA0);
    assert_eq!(take_inject_vector(), Some(0x40));
    reset();
    guest_irq::reset();
    guest_platform::reset();
}
