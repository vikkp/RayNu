//! El Torito boot-catalog probe (ADR-014 / E5). Outside Proven Core.
//!
//! Parses a boot record + catalog from ISO 9660 bytes. Host attach lives in
//! [`crate::mgmt::iso::attach_cdrom_host`]. That is not guest UEFI firmware.
//! Live `attach_cdrom_uefi` stays `UnsupportedOnFirmware` until a live
//! guest UEFI firmware payload + virtio CD path exist. Stage 3 boxes the
//! ADR-003 envelope only.

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
    // Walk the catalog: the default (initial) entry follows validation and
    // inherits the validation platform; later 0x90/0x91 section headers
    // switch platform for the entries after them. A hybrid BIOS+UEFI ISO
    // (Alpine, Debian, Ubuntu…) puts isolinux first (platform 0) and the FAT
    // ESP under an EFI (0xEF) section header — so **prefer the EFI entry**
    // and fall back to the default only when no EFI section exists.
    let validation_efi = cat[1] == 0xEF;
    let mut platform_efi = validation_efi;
    let mut default: Option<(u32, u16)> = None;
    let mut efi_entry: Option<(u32, u16)> = None;
    let mut off = 32;
    let mut seen = 0;
    while off + 32 <= cat.len() && seen < 64 {
        let e = &cat[off..off + 32];
        match e[0] {
            0x90 | 0x91 => {
                platform_efi = e[1] == 0xEF;
            }
            0x88 => {
                let count = u16::from_le_bytes([e[6], e[7]]);
                let lba = u32::from_le_bytes([e[8], e[9], e[10], e[11]]);
                if lba != 0 {
                    if platform_efi && efi_entry.is_none() {
                        efi_entry = Some((lba, count));
                    }
                    if default.is_none() {
                        default = Some((lba, count));
                    }
                }
            }
            // 0x00 right after validation is a non-bootable default entry;
            // anywhere else it is the end of the catalog.
            0x00 if off == 32 => {}
            _ => break,
        }
        off += 32;
        seen += 1;
    }
    let (load_lba, sector_count, efi) = match (efi_entry, default) {
        (Some((l, c)), _) => (l, c, true),
        (None, Some((l, c))) => (l, c, validation_efi),
        (None, None) => return Err(ElToritoError::NotBootable),
    };
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

/// Bytes needed for [`write_mock_efi_iso`] (boot record + catalog + FAT + ISO9660).
/// Boot record at 17, catalog at 20, load LBA 22 with 8 FAT ISO sectors, ISO9660
/// `\EFI\BOOT` at LBA 30–33 → 36.
pub const MOCK_EFI_ISO_BYTES: usize = crate::devices::ide_cdrom::MOCK_EFI_ISO_BYTES;

/// Write a minimal EFI El Torito prefix into `iso`. No allocation.
/// Same bytes as [`crate::devices::ide_cdrom::write_placeholder_iso`].
pub fn write_mock_efi_iso(iso: &mut [u8]) -> Result<usize, ElToritoError> {
    if iso.len() < MOCK_EFI_ISO_BYTES {
        return Err(ElToritoError::Truncated);
    }
    iso[..MOCK_EFI_ISO_BYTES].fill(0);
    crate::devices::ide_cdrom::write_placeholder_iso(iso);
    let load = 22 * ISO_SECTOR;
    if iso.len() < load + crate::devices::ide_cdrom::ELTORITO_BOOTX64_OFF + 2 {
        return Err(ElToritoError::Truncated);
    }
    if &iso[load + crate::devices::ide_cdrom::ELTORITO_BOOTX64_OFF
        ..load + crate::devices::ide_cdrom::ELTORITO_BOOTX64_OFF + 2]
        != b"MZ"
    {
        return Err(ElToritoError::Truncated);
    }
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
