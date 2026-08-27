use super::{mock_efi_iso, parse_el_torito, ElToritoError, ISO_SECTOR};

#[test]
fn parses_mock_efi_catalog() {
    let iso = mock_efi_iso();
    let img = parse_el_torito(&iso).expect("catalog");
    assert_eq!(img.catalog_lba, 20);
    assert_eq!(img.load_lba, 22);
    assert_eq!(img.sector_count, 4);
    assert!(img.efi);
    assert_eq!(&iso[22 * ISO_SECTOR..22 * ISO_SECTOR + 2], b"MZ");
    assert_eq!(&iso[22 * ISO_SECTOR + 0x80..22 * ISO_SECTOR + 0x84], b"PE\0\0");
}

#[test]
fn rejects_truncated() {
    assert_eq!(parse_el_torito(&[0u8; 32]), Err(ElToritoError::NoBootRecord));
    let short = [0u8; 17 * ISO_SECTOR];
    assert_eq!(parse_el_torito(&short), Err(ElToritoError::NoBootRecord));
}

#[test]
fn rejects_missing_55aa() {
    let mut iso = mock_efi_iso();
    iso[20 * ISO_SECTOR + 30] = 0x00;
    assert_eq!(parse_el_torito(&iso), Err(ElToritoError::BadCatalog));
}

#[test]
fn rejects_not_bootable() {
    let mut iso = mock_efi_iso();
    iso[20 * ISO_SECTOR + 64] = 0x00;
    assert_eq!(parse_el_torito(&iso), Err(ElToritoError::NotBootable));
}
