use super::*;

#[test]
fn rsdp_signature_and_rev0() {
    let mut sig = [0u8; 8];
    for i in 0..8 {
        sig[i] = acpi_rsdp_byte(i as u16);
    }
    assert_eq!(&sig, b"RSD PTR ");
    assert_eq!(acpi_rsdp_byte(8), 0, "checksum filled by table-loader");
    assert_eq!(acpi_rsdp_byte(15), 0, "rev 0 20-byte RSDP");
    assert_eq!(ACPI_RSDP_LEN, 20);
}

#[test]
fn tables_contain_madt_with_pcat_compat() {
    assert_eq!(acpi_tables_byte(0), b'R');
    assert_eq!(acpi_tables_byte(1), b'S');
    assert_eq!(acpi_tables_byte(2), b'D');
    assert_eq!(acpi_tables_byte(3), b'T');
    let madt = 0xC0u16;
    assert_eq!(acpi_tables_byte(madt), b'A');
    assert_eq!(acpi_tables_byte(madt + 1), b'P');
    assert_eq!(acpi_tables_byte(madt + 2), b'I');
    assert_eq!(acpi_tables_byte(madt + 3), b'C');
    assert_eq!(acpi_tables_byte(madt + 40), 1, "PCAT_COMPAT");
    let lapic = acpi_madt_lapic_addr().to_le_bytes();
    for i in 0..4 {
        assert_eq!(acpi_tables_byte(madt + 36 + i as u16), lapic[i]);
    }
    let ioapic = acpi_madt_ioapic_addr().to_le_bytes();
    for i in 0..4 {
        assert_eq!(acpi_tables_byte(madt + 56 + i as u16), ioapic[i]);
    }
    assert_eq!(acpi_tables_byte(madt + 64), 2, "MADT IRQ0 ISO GSI 2 type");
    assert_eq!(acpi_tables_byte(madt + 65), 10);
    assert_eq!(acpi_tables_byte(madt + 67), 0, "ISA IRQ 0");
    assert_eq!(acpi_tables_byte(madt + 68), 2, "GSI 2");
    assert_eq!(ACPI_TABLES_LEN, 0x1F1);
}

#[test]
fn facp_points_at_dsdt_offset_until_linker() {
    let facp = 0x40u16;
    assert_eq!(acpi_tables_byte(facp), b'F');
    assert_eq!(acpi_tables_byte(facp + 1), b'A');
    assert_eq!(acpi_tables_byte(facp + 2), b'C');
    assert_eq!(acpi_tables_byte(facp + 3), b'P');
    let dsdt = u32::from(0x110u16).to_le_bytes();
    for i in 0..4 {
        assert_eq!(acpi_tables_byte(facp + 40 + i as u16), dsdt[i]);
    }
    assert_eq!(acpi_tables_byte(0x110), b'D');
    assert_eq!(acpi_tables_byte(0x111), b'S');
    assert_eq!(acpi_tables_byte(0x112), b'D');
    assert_eq!(acpi_tables_byte(0x113), b'T');
}

#[test]
fn dsdt_pci0_prt_virtio_gsi() {
    let mut body = [0u8; 225];
    for i in 0..225u16 {
        body[i as usize] = acpi_tables_byte(0x110 + i);
    }
    assert_eq!(&body[0..4], b"DSDT");
    assert_eq!(u32::from_le_bytes(body[4..8].try_into().unwrap()), 225);
    assert!(body.windows(4).any(|w| w == b"PCI0"));
    assert!(body.windows(4).any(|w| w == b"_PRT"));
    assert!(body.windows(4).any(|w| w == b"_CRS"), "DSDT PCI0 _CRS");
    assert!(
        body.windows(4).any(|w| w == [0xFF, 0xFF, 0x02, 0x00]),
        "slot 2 INTA"
    );
    assert!(
        body.windows(4).any(|w| w == [0xFF, 0xFF, 0x03, 0x00]),
        "slot 3 INTA"
    );
    let s2 = body.windows(4).position(|w| w == [0xFF, 0xFF, 0x02, 0x00]).unwrap();
    assert_eq!(body[s2 + 7], 17, "GSI 17");
    let s3 = body.windows(4).position(|w| w == [0xFF, 0xFF, 0x03, 0x00]).unwrap();
    assert_eq!(body[s3 + 7], 18, "GSI 18");
    assert!(
        body.windows(4).any(|w| w == [0x00, 0x00, 0x00, 0xC0]),
        "PCI MMIO 0xC0000000"
    );
    assert!(
        body.windows(4).any(|w| w == [0xFF, 0xFF, 0xBF, 0xFE]),
        "PCI MMIO 0xFEBFFFFF"
    );
}

#[test]
fn loader_allocate_then_add_pointer_qemu_layout() {
    assert_eq!(ACPI_LOADER_ENTRIES, 11);
    assert_eq!(ACPI_LOADER_LEN, 11 * 128);
    assert_eq!(acpi_loader_byte(0), 1, "ALLOCATE");
    let mut name = [0u8; 16];
    for i in 0..16 {
        name[i] = acpi_loader_byte(4 + i as u16);
    }
    assert!(name.starts_with(b"etc/acpi/tables\0"));
    assert_eq!(acpi_loader_byte(64), 2, "ACPI tables ZONE_FSEG");
    let e2 = 2 * 128;
    assert_eq!(acpi_loader_byte(e2 as u16), 2, "ADD_POINTER");
    let mut dest = [0u8; 16];
    let mut src = [0u8; 16];
    for i in 0..16 {
        dest[i] = acpi_loader_byte((e2 + 4 + i) as u16);
        src[i] = acpi_loader_byte((e2 + 60 + i) as u16);
    }
    assert!(dest.starts_with(b"etc/acpi/rsdp\0"));
    assert!(src.starts_with(b"etc/acpi/tables\0"));
    let off = u32::from(acpi_loader_byte((e2 + 116) as u16))
        | u32::from(acpi_loader_byte((e2 + 117) as u16)) << 8
        | u32::from(acpi_loader_byte((e2 + 118) as u16)) << 16
        | u32::from(acpi_loader_byte((e2 + 119) as u16)) << 24;
    assert_eq!(off, 16, "RSDP RsdtAddress");
    assert_eq!(acpi_loader_byte((e2 + 120) as u16), 4);
}

#[test]
fn selectors_follow_boot_wait() {
    assert_eq!(FW_CFG_ACPI_LOADER_SEL, 0x23);
    assert_eq!(FW_CFG_ACPI_TABLES_SEL, 0x24);
    assert_eq!(FW_CFG_ACPI_RSDP_SEL, 0x25);
    assert_eq!(FW_CFG_NAMED_FILE_COUNT_ACPI, 3);
}

#[test]
fn acpi_blobs_readable_without_panic() {
    for off in 0..ACPI_TABLES_LEN {
        let _ = acpi_tables_byte(off);
    }
    for off in 0..ACPI_LOADER_LEN {
        let _ = acpi_loader_byte(off);
    }
    for off in 0..ACPI_RSDP_LEN {
        let _ = acpi_rsdp_byte(off);
    }
}
