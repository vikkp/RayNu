//! El Torito boot-catalog probe (ADR-014 / E5). Outside Proven Core.
//!
//! Parses a boot record + catalog from ISO 9660 bytes. Does **not** attach a
//! CD-ROM or VMLAUNCH guest UEFI firmware. Live `attach_cdrom_uefi` stays
//! `UnsupportedOnFirmware` until a firmware blob + virtio CD path exist.

/// ISO 9660 logical sector size (El Torito / ECMA-119).
pub const ISO_SECTOR: usize = 2048;

/// First volume-descriptor sector (Primary / Boot Record live here).
const VD_START_SECTOR: usize = 16;
/// Scan a bounded window so a garbage ISO cannot walk the whole buffer.
const VD_SCAN_SECTORS: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElToritoError {
    Truncated,
    NoBootRecord,
    BadCatalog,
    NotBootable,
}

/// One El Torito boot image (no-emulation, typically EFI).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ElToritoImage {
    pub catalog_lba: u32,
    pub load_lba: u32,
    pub sector_count: u16,
    /// True when the validation/section platform is EFI (`0xEF`).
    pub efi: bool,
}

/// Parse El Torito from a whole ISO (or a prefix long enough to hold the catalog).
///
/// INVARIANTS:
/// - Does not allocate
/// - Does not mutate `iso`
/// - Rejects a catalog that is not 0x55AA-keyed
pub fn parse_el_torito(iso: &[u8]) -> Result<ElToritoImage, ElToritoError> {
    let boot_lba = find_boot_record_lba(iso)?;
    let br = sector(iso, boot_lba)?;
    if br[0] != 0 || &br[1..6] != b"CD001" {
        return Err(ElToritoError::NoBootRecord);
    }
    if !id_starts_with(&br[7..39], b"EL TORITO SPECIFICATION") {
        return Err(ElToritoError::NoBootRecord);
    }
    let catalog_lba = u32::from_le_bytes([br[71], br[72], br[73], br[74]]);
    if catalog_lba == 0 {
        return Err(ElToritoError::BadCatalog);
    }
    let cat = sector(iso, catalog_lba)?;
    parse_catalog(cat, catalog_lba)
}

fn find_boot_record_lba(iso: &[u8]) -> Result<u32, ElToritoError> {
    for s in VD_START_SECTOR..VD_START_SECTOR + VD_SCAN_SECTORS {
        let Some(sec) = sector(iso, s as u32).ok() else {
            break;
        };
        if sec[0] == 0xFF {
            break;
        }
        if sec[0] == 0 && &sec[1..6] == b"CD001" {
            return Ok(s as u32);
        }
    }
    Err(ElToritoError::NoBootRecord)
}

fn parse_catalog(cat: &[u8], catalog_lba: u32) -> Result<ElToritoImage, ElToritoError> {
    if cat.len() < 64 {
        return Err(ElToritoError::Truncated);
    }
    // Validation entry (32 bytes).
    if cat[0] != 0x01 || cat[30] != 0x55 || cat[31] != 0xAA {
        return Err(ElToritoError::BadCatalog);
    }
    let mut off = 32;
    let mut efi = cat[1] == 0xEF;
    // Optional EFI section header 0x90/0x91.
    if cat.len() >= off + 32 && (cat[off] == 0x90 || cat[off] == 0x91) {
        efi = efi || cat[off + 1] == 0xEF;
        off += 32;
    }
    if cat.len() < off + 32 {
        return Err(ElToritoError::Truncated);
    }
    let ent = &cat[off..off + 32];
    if ent[0] != 0x88 {
        return Err(ElToritoError::NotBootable);
    }
    let sector_count = u16::from_le_bytes([ent[6], ent[7]]);
    let load_lba = u32::from_le_bytes([ent[8], ent[9], ent[10], ent[11]]);
    if load_lba == 0 {
        return Err(ElToritoError::NotBootable);
    }
    Ok(ElToritoImage {
        catalog_lba,
        load_lba,
        sector_count,
        efi,
    })
}

fn sector(iso: &[u8], lba: u32) -> Result<&[u8], ElToritoError> {
    let start = (lba as usize).saturating_mul(ISO_SECTOR);
    let end = start.saturating_add(ISO_SECTOR);
    if end > iso.len() {
        return Err(ElToritoError::Truncated);
    }
    Ok(&iso[start..end])
}

fn id_starts_with(field: &[u8], prefix: &[u8]) -> bool {
    field.len() >= prefix.len() && &field[..prefix.len()] == prefix
}

/// Bytes needed for [`write_mock_efi_iso`] (boot record + catalog + load LBA).
pub const MOCK_EFI_ISO_BYTES: usize = 24 * ISO_SECTOR;

/// Write a minimal EFI El Torito prefix into `iso`. No allocation.
pub fn write_mock_efi_iso(iso: &mut [u8]) -> Result<usize, ElToritoError> {
    if iso.len() < MOCK_EFI_ISO_BYTES {
        return Err(ElToritoError::Truncated);
    }
    iso[..MOCK_EFI_ISO_BYTES].fill(0);
    let br = 17 * ISO_SECTOR;
    iso[br] = 0;
    iso[br + 1..br + 6].copy_from_slice(b"CD001");
    iso[br + 6] = 1;
    iso[br + 7..br + 7 + 23].copy_from_slice(b"EL TORITO SPECIFICATION");
    iso[br + 71..br + 75].copy_from_slice(&20u32.to_le_bytes());
    let cat = 20 * ISO_SECTOR;
    iso[cat] = 0x01;
    iso[cat + 1] = 0xEF;
    iso[cat + 30] = 0x55;
    iso[cat + 31] = 0xAA;
    iso[cat + 32] = 0x91;
    iso[cat + 33] = 0xEF;
    iso[cat + 64] = 0x88;
    iso[cat + 70..cat + 72].copy_from_slice(&4u16.to_le_bytes());
    iso[cat + 72..cat + 76].copy_from_slice(&22u32.to_le_bytes());
    Ok(MOCK_EFI_ISO_BYTES)
}

/// Minimal ISO prefix with a Boot Record at sector 17 and a catalog at 20.
#[cfg(test)]
pub fn mock_efi_iso() -> [u8; MOCK_EFI_ISO_BYTES] {
    let mut iso = [0u8; MOCK_EFI_ISO_BYTES];
    let _ = write_mock_efi_iso(&mut iso);
    iso
}

#[cfg(test)]
#[path = "el_torito_test.rs"]
mod el_torito_test;
