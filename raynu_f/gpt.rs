//! GPT + EFI System Partition lookup for RayNu-F disk boot (F7).
//!
//! Pillar: [Z] · Proven Core: **outside** (ADR-016 / ADR-017)
//!
//! Pure functions over a [`crate::raynu_f::fat::VolumeRead`] (byte offsets
//! from LBA 0 of the install disk). Host tests feed a synthetic 1 MiB image
//! with a real header CRC. Not `ISO-INSTALL-OK`.

use super::fat::VolumeRead;
use super::tables::crc32;

/// Protective-MBR partition type for a GPT disk.
pub const MBR_TYPE_GPT: u8 = 0xEE;
/// GPT header signature `"EFI PART"`.
pub const GPT_SIGNATURE: &[u8; 8] = b"EFI PART";
/// GPT revision 1.0.
pub const GPT_REVISION_1_0: u32 = 0x0001_0000;
/// Minimum valid GPT header size (UEFI / EFI spec).
pub const GPT_HEADER_SIZE_MIN: u32 = 92;
/// LBA size we speak (512-byte logical blocks).
pub const GPT_LBA_SIZE: u32 = 512;
/// Default partition-entry size.
pub const GPT_ENTRY_SIZE_DEFAULT: u32 = 128;
/// Cap so a corrupt NumberOfPartitionEntries cannot spin.
pub const GPT_MAX_ENTRIES: u32 = 128;
/// EFI System Partition type GUID `C12A7328-F81F-11D2-BA4B-00A0C93EC93B`
/// in mixed-endian on-disk order.
pub const ESP_TYPE_GUID: [u8; 16] = [
    0x28, 0x73, 0x2A, 0xC1, 0x1F, 0xF8, 0xD2, 0x11, 0xBA, 0x4B, 0x00, 0xA0, 0xC9, 0x3E, 0xC9, 0x3B,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GptError {
    ShortRead,
    NoProtectiveMbr,
    BadSignature,
    BadRevision,
    BadHeaderSize,
    BadHeaderCrc,
    BadEntryArrayCrc,
    BadEntrySize,
    NoEsp,
}

/// Parsed GPT header fields we need to walk the partition array.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GptHeader {
    pub revision: u32,
    pub header_size: u32,
    pub my_lba: u64,
    pub partition_entry_lba: u64,
    pub number_of_entries: u32,
    pub size_of_entry: u32,
}

/// First ESP on the disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EspPartition {
    /// 1-based partition number (UEFI HardDrive device-path field).
    pub partition_number: u32,
    pub start_lba: u64,
    pub end_lba: u64,
    pub unique_guid: [u8; 16],
}

impl EspPartition {
    pub const fn size_lba(&self) -> u64 {
        self.end_lba.saturating_sub(self.start_lba).saturating_add(1)
    }
}

/// Disk-before-ISO boot order (F7). First boot has a zeroed disk, so ISO wins.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootSource {
    Disk,
    Iso,
}

/// Prefer the install disk when it already has a GPT ESP.
pub fn raynu_f_boot_source(disk_has_gpt_esp: bool) -> BootSource {
    if disk_has_gpt_esp {
        BootSource::Disk
    } else {
        BootSource::Iso
    }
}

fn u16_at(b: &[u8], off: usize) -> u16 {
    u16::from_le_bytes([b[off], b[off + 1]])
}
fn u32_at(b: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]])
}
fn u64_at(b: &[u8], off: usize) -> u64 {
    u64::from_le_bytes([
        b[off],
        b[off + 1],
        b[off + 2],
        b[off + 3],
        b[off + 4],
        b[off + 5],
        b[off + 6],
        b[off + 7],
    ])
}

/// Protective MBR: 0x55AA and a type-0xEE partition.
pub fn parse_protective_mbr(lba0: &[u8]) -> Result<(), GptError> {
    if lba0.len() < GPT_LBA_SIZE as usize {
        return Err(GptError::ShortRead);
    }
    if lba0[510] != 0x55 || lba0[511] != 0xAA {
        return Err(GptError::NoProtectiveMbr);
    }
    for i in 0..4 {
        let off = 0x1BE + i * 16;
        if lba0[off + 4] == MBR_TYPE_GPT {
            return Ok(());
        }
    }
    Err(GptError::NoProtectiveMbr)
}

/// Parse the GPT header at LBA 1. CRC32 is over `header_size` bytes with
/// the CRC field (offset 16) zeroed — the same IEEE CRC UEFI uses.
pub fn parse_gpt_header(lba1: &[u8]) -> Result<GptHeader, GptError> {
    if lba1.len() < GPT_LBA_SIZE as usize {
        return Err(GptError::ShortRead);
    }
    if &lba1[0..8] != GPT_SIGNATURE {
        return Err(GptError::BadSignature);
    }
    let revision = u32_at(lba1, 8);
    if revision != GPT_REVISION_1_0 {
        return Err(GptError::BadRevision);
    }
    let header_size = u32_at(lba1, 12);
    if header_size < GPT_HEADER_SIZE_MIN || header_size as usize > lba1.len() {
        return Err(GptError::BadHeaderSize);
    }
    let stored = u32_at(lba1, 16);
    let hs = header_size as usize;
    let mut tmp = [0u8; GPT_LBA_SIZE as usize];
    tmp[..hs].copy_from_slice(&lba1[..hs]);
    tmp[16..20].copy_from_slice(&[0; 4]);
    if crc32(&tmp[..hs]) != stored {
        return Err(GptError::BadHeaderCrc);
    }
    let size_of_entry = u32_at(lba1, 84);
    if size_of_entry < GPT_ENTRY_SIZE_DEFAULT || size_of_entry > 4096 || size_of_entry % 8 != 0 {
        return Err(GptError::BadEntrySize);
    }
    Ok(GptHeader {
        revision,
        header_size,
        my_lba: u64_at(lba1, 24),
        partition_entry_lba: u64_at(lba1, 72),
        number_of_entries: u32_at(lba1, 80),
        size_of_entry,
    })
}

/// Header field: CRC32 of the partition-entry array (offset 88).
pub fn gpt_entry_array_crc(lba1: &[u8]) -> u32 {
    u32_at(lba1, 88)
}

fn read_lba<R: VolumeRead>(r: &R, lba: u64, buf: &mut [u8; 512]) -> bool {
    r.read_at(lba.saturating_mul(u64::from(GPT_LBA_SIZE)), buf)
}

/// CRC32 of the partition-entry array (count × size bytes).
pub fn partition_array_crc<R: VolumeRead>(
    r: &R,
    hdr: &GptHeader,
) -> Result<u32, GptError> {
    let n = hdr.number_of_entries.min(GPT_MAX_ENTRIES);
    let es = hdr.size_of_entry;
    let total = n.saturating_mul(es);
    let mut crc_state = super::tables::crc32_start();
    let mut off = hdr.partition_entry_lba.saturating_mul(u64::from(GPT_LBA_SIZE));
    let mut left = total;
    let mut chunk = [0u8; 512];
    while left > 0 {
        let take = (left as usize).min(chunk.len());
        if !r.read_at(off, &mut chunk[..take]) {
            return Err(GptError::ShortRead);
        }
        crc_state = super::tables::crc32_feed(crc_state, &chunk[..take]);
        off = off.saturating_add(take as u64);
        left -= take as u32;
    }
    Ok(super::tables::crc32_finish(crc_state))
}

/// First ESP on `r`, or `GptError` if the disk is not a GPT with an ESP.
pub fn find_esp<R: VolumeRead>(r: &R) -> Result<EspPartition, GptError> {
    let mut lba0 = [0u8; 512];
    if !read_lba(r, 0, &mut lba0) {
        return Err(GptError::ShortRead);
    }
    parse_protective_mbr(&lba0)?;
    let mut lba1 = [0u8; 512];
    if !read_lba(r, 1, &mut lba1) {
        return Err(GptError::ShortRead);
    }
    let hdr = parse_gpt_header(&lba1)?;
    if partition_array_crc(r, &hdr)? != gpt_entry_array_crc(&lba1) {
        return Err(GptError::BadEntryArrayCrc);
    }
    let n = hdr.number_of_entries.min(GPT_MAX_ENTRIES);
    let es = hdr.size_of_entry as usize;
    let mut entry = [0u8; 256];
    if es > entry.len() {
        return Err(GptError::BadEntrySize);
    }
    for i in 0..n {
        let off = hdr
            .partition_entry_lba
            .saturating_mul(u64::from(GPT_LBA_SIZE))
            .saturating_add(u64::from(i) * hdr.size_of_entry as u64);
        if !r.read_at(off, &mut entry[..es]) {
            return Err(GptError::ShortRead);
        }
        if entry[..16] != ESP_TYPE_GUID {
            continue;
        }
        let start = u64_at(&entry, 32);
        let end = u64_at(&entry, 40);
        if start == 0 || end < start {
            continue;
        }
        let mut unique_guid = [0u8; 16];
        unique_guid.copy_from_slice(&entry[16..32]);
        return Ok(EspPartition {
            partition_number: i + 1,
            start_lba: start,
            end_lba: end,
            unique_guid,
        });
    }
    Err(GptError::NoEsp)
}

/// Convenience: whether `find_esp` succeeds.
pub fn disk_has_gpt_esp<R: VolumeRead>(r: &R) -> bool {
    find_esp(r).is_ok()
}

#[cfg(test)]
#[path = "gpt_test.rs"]
mod gpt_test;
