//! B4 ACPI tables for the power button.
//!
//! Pillar: [Z]
//! Proven Core: **outside** (ADR-016 / ADR-021)
//! VERIFICATION: L1 (host tests)
//!
//! The installed Ubuntu guest looks for an RSDP in the EFI configuration
//! table. This page is that RSDP plus an RSDT, an XSDT, a FADT, a FACS, and
//! a DSDT whose only device is the power button (`PNP0C0C`) and `_S5`.
//! There is no MADT: a MADT would move Linux onto the I/O APIC, and the
//! virtio disk that reaches `login:` is on the 8259.

/// Page inside the firmware image (`tables::IMAGE_ACPI_OFF`).
pub const ACPI_PAGE: usize = 0x1000;

pub const RSDP_OFF: usize = 0x000;
pub const RSDT_OFF: usize = 0x040;
pub const XSDT_OFF: usize = 0x080;
pub const FACP_OFF: usize = 0x0C0;
pub const DSDT_OFF: usize = 0x160;
pub const FACS_OFF: usize = 0x200;

pub const FACP_LEN: usize = 129;
pub const FACS_LEN: usize = 64;

/// `EFI_ACPI_TABLE_GUID` (`eb9d2d30-2d88-11d3-9a16-0090273fc14d`).
pub const ACPI10_GUID: [u8; 16] = [
    0x30, 0x2d, 0x9d, 0xeb, 0x88, 0x2d, 0xd3, 0x11, 0x9a, 0x16, 0x00, 0x90, 0x27, 0x3f, 0xc1, 0x4d,
];
/// `EFI_ACPI_20_TABLE_GUID` (`8868e871-e4f1-11d3-bc22-0080c73c8881`).
pub const ACPI20_GUID: [u8; 16] = [
    0x71, 0xe8, 0x68, 0x88, 0xf1, 0xe4, 0xd3, 0x11, 0xbc, 0x22, 0x00, 0x80, 0xc7, 0x3c, 0x88, 0x81,
];

/// FADT `SLP_TYP` value named by `_S5`. Matches [`crate::mgmt::guest_power::SLP_TYP_S5`].
pub const S5_SLP_TYP: u8 = 5;

/// Fill `page` (one ACPI page) with tables whose pointers use `gpa` as the
/// page base. Returns false when the buffer is short.
pub fn write_acpi_page(page: &mut [u8], gpa: u64) -> bool {
    if page.len() < ACPI_PAGE || gpa & 0xfff != 0 {
        return false;
    }
    for b in page.iter_mut() {
        *b = 0;
    }
    let facs = gpa + FACS_OFF as u64;
    let dsdt = gpa + DSDT_OFF as u64;
    let facp = gpa + FACP_OFF as u64;
    let rsdt = gpa + RSDT_OFF as u64;
    let xsdt = gpa + XSDT_OFF as u64;

    write_facs(&mut page[FACS_OFF..FACS_OFF + FACS_LEN]);
    let dsdt_len = write_dsdt(&mut page[DSDT_OFF..]);
    if dsdt_len == 0 || DSDT_OFF + dsdt_len > FACS_OFF {
        return false;
    }
    write_facp(&mut page[FACP_OFF..FACP_OFF + FACP_LEN], facs, dsdt);
    write_rsdt(&mut page[RSDT_OFF..RSDT_OFF + 40], facp);
    write_xsdt(&mut page[XSDT_OFF..XSDT_OFF + 44], facp);
    write_rsdp(&mut page[RSDP_OFF..RSDP_OFF + 36], rsdt, xsdt);
    true
}

fn sum(bytes: &[u8]) -> u8 {
    bytes.iter().fold(0u8, |a, b| a.wrapping_add(*b))
}

fn fix_checksum(buf: &mut [u8], at: usize) {
    buf[at] = 0;
    buf[at] = 0u8.wrapping_sub(sum(buf));
}

fn put_u16(buf: &mut [u8], off: usize, v: u16) {
    buf[off..off + 2].copy_from_slice(&v.to_le_bytes());
}

fn put_u32(buf: &mut [u8], off: usize, v: u32) {
    buf[off..off + 4].copy_from_slice(&v.to_le_bytes());
}

fn put_u64(buf: &mut [u8], off: usize, v: u64) {
    buf[off..off + 8].copy_from_slice(&v.to_le_bytes());
}

fn sdt_header(buf: &mut [u8], sig: &[u8; 4], len: u32, rev: u8) {
    buf[..4].copy_from_slice(sig);
    put_u32(buf, 4, len);
    buf[8] = rev;
    buf[9] = 0;
    buf[10..16].copy_from_slice(b"RAYNUV");
    buf[16..24].copy_from_slice(b"RAYNUV  ");
    put_u32(buf, 24, 1);
    buf[28..32].copy_from_slice(b"RNUV");
    put_u32(buf, 32, 1);
}

fn write_facs(buf: &mut [u8]) {
    buf[..4].copy_from_slice(b"FACS");
    put_u32(buf, 4, FACS_LEN as u32);
}

fn write_dsdt(buf: &mut [u8]) -> usize {
    // AML after the 36-byte header.
    // Name(_S5, Package(4){5,5,0,0})
    // Device(PWRB) { Name(_HID, EisaId("PNP0C0C")) Name(_UID, Zero) }
    const AML: &[u8] = &[
        0x08, 0x5F, 0x53, 0x35, 0x5F, 0x12, 0x08, 0x04, 0x0A, S5_SLP_TYP, 0x0A, S5_SLP_TYP, 0x00,
        0x00, 0x5B, 0x82, 0x15, b'P', b'W', b'R', b'B', 0x08, 0x5F, 0x48, 0x49, 0x44, 0x0C, 0x41,
        0xD0, 0x0C, 0x0C, 0x08, 0x5F, 0x55, 0x49, 0x44, 0x00,
    ];
    let len = 36 + AML.len();
    if buf.len() < len {
        return 0;
    }
    sdt_header(buf, b"DSDT", len as u32, 2);
    buf[36..len].copy_from_slice(AML);
    fix_checksum(&mut buf[..len], 9);
    len
}

fn write_facp(buf: &mut [u8], facs: u64, dsdt: u64) {
    sdt_header(buf, b"FACP", FACP_LEN as u32, 3);
    put_u32(buf, 36, facs as u32);
    put_u32(buf, 40, dsdt as u32);
    buf[44] = 1;
    put_u16(buf, 46, 9);
    put_u32(buf, 56, 0xB000);
    put_u32(buf, 64, 0xB004);
    put_u32(buf, 76, 0xB008);
    buf[88] = 4;
    buf[89] = 2;
    buf[91] = 4;
    // WBINVD and RESET_REG_SUP.
    put_u32(buf, 112, (1 << 0) | (1 << 10));
    buf[116] = 1;
    buf[117] = 8;
    buf[119] = 1;
    put_u64(buf, 120, 0xCF9);
    buf[128] = 0x06;
    fix_checksum(buf, 9);
}

fn write_rsdt(buf: &mut [u8], facp: u64) {
    sdt_header(buf, b"RSDT", 40, 1);
    put_u32(buf, 36, facp as u32);
    fix_checksum(buf, 9);
}

fn write_xsdt(buf: &mut [u8], facp: u64) {
    sdt_header(buf, b"XSDT", 44, 1);
    put_u64(buf, 36, facp);
    fix_checksum(buf, 9);
}

fn write_rsdp(buf: &mut [u8], rsdt: u64, xsdt: u64) {
    buf[..8].copy_from_slice(b"RSD PTR ");
    buf[9..15].copy_from_slice(b"RAYNUV");
    buf[15] = 2;
    put_u32(buf, 16, rsdt as u32);
    put_u32(buf, 20, 36);
    put_u64(buf, 24, xsdt);
    buf[8] = 0;
    buf[8] = 0u8.wrapping_sub(sum(&buf[..20]));
    buf[32] = 0;
    buf[32] = 0u8.wrapping_sub(sum(&buf[..36]));
}

#[cfg(test)]
mod acpi_publish_test {
    use super::*;

    #[test]
    fn the_page_is_a_power_button_and_has_no_madt() {
        let gpa = 0x804000u64;
        let mut page = [0u8; ACPI_PAGE];
        assert!(write_acpi_page(&mut page, gpa));
        assert_eq!(&page[0..8], b"RSD PTR ");
        assert_eq!(page[15], 2);
        assert_eq!(sum(&page[..20]), 0);
        assert_eq!(sum(&page[..36]), 0);
        assert_eq!(&page[RSDT_OFF..RSDT_OFF + 4], b"RSDT");
        assert_eq!(&page[XSDT_OFF..XSDT_OFF + 4], b"XSDT");
        assert_eq!(&page[FACP_OFF..FACP_OFF + 4], b"FACP");
        assert_eq!(u16::from_le_bytes(page[FACP_OFF + 46..FACP_OFF + 48].try_into().unwrap()), 9);
        assert_eq!(&page[DSDT_OFF..DSDT_OFF + 4], b"DSDT");
        assert_eq!(&page[FACS_OFF..FACS_OFF + 4], b"FACS");
        let dsdt_len = u32::from_le_bytes(page[DSDT_OFF + 4..DSDT_OFF + 8].try_into().unwrap()) as usize;
        assert!(DSDT_OFF + dsdt_len <= FACS_OFF);
        let dsdt = &page[DSDT_OFF..DSDT_OFF + dsdt_len];
        assert!(dsdt.windows(4).any(|w| w == [0x41, 0xD0, 0x0C, 0x0C]));
        assert_eq!(sum(dsdt), 0);
        assert_eq!(sum(&page[FACP_OFF..FACP_OFF + FACP_LEN]), 0);
        assert_eq!(u32::from_le_bytes(page[XSDT_OFF + 4..XSDT_OFF + 8].try_into().unwrap()), 44);
    }

    #[test]
    fn publish_appends_behind_smbios() {
        use crate::raynu_f::tables::{
            build_firmware_image, get_u64, header_crc_valid, publish_acpi, publish_smbios,
            IMAGE_ACPI_OFF, IMAGE_BYTES, IMAGE_CONFIG_TABLE_OFF, IMAGE_SYSTEM_TABLE_OFF,
            SYSTEM_TABLE_NUM_TABLE_ENTRIES_OFF, SYSTEM_TABLE_SIZE,
        };
        let base = 0x80_0000u64;
        let mut img = vec![0u8; IMAGE_BYTES];
        let layout = build_firmware_image(base, &mut img).unwrap();
        assert_eq!(publish_acpi(&mut img, &layout).unwrap(), layout.acpi);
        assert_eq!(
            get_u64(&img, IMAGE_SYSTEM_TABLE_OFF + SYSTEM_TABLE_NUM_TABLE_ENTRIES_OFF),
            2
        );
        assert_eq!(&img[IMAGE_CONFIG_TABLE_OFF..IMAGE_CONFIG_TABLE_OFF + 16], &ACPI10_GUID);
        assert_eq!(&img[IMAGE_ACPI_OFF..IMAGE_ACPI_OFF + 8], b"RSD PTR ");
        assert!(header_crc_valid(&img, IMAGE_SYSTEM_TABLE_OFF));
        let _ = SYSTEM_TABLE_SIZE;

        let mut img = vec![0u8; IMAGE_BYTES];
        let layout = build_firmware_image(base, &mut img).unwrap();
        publish_smbios(&mut img, &layout, &[]).unwrap();
        publish_acpi(&mut img, &layout).unwrap();
        assert_eq!(
            get_u64(&img, IMAGE_SYSTEM_TABLE_OFF + SYSTEM_TABLE_NUM_TABLE_ENTRIES_OFF),
            3
        );
        assert!(header_crc_valid(&img, IMAGE_SYSTEM_TABLE_OFF));
    }
}
