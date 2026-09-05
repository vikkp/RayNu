//! Host tests for GPT / ESP lookup and disk-before-ISO boot order.

use super::*;
use super::super::tables::crc32;

struct SliceDisk<'a>(&'a [u8]);

impl VolumeRead for SliceDisk<'_> {
    fn read_at(&self, off: u64, buf: &mut [u8]) -> bool {
        let Ok(start) = usize::try_from(off) else {
            return false;
        };
        let Some(end) = start.checked_add(buf.len()) else {
            return false;
        };
        if end > self.0.len() {
            return false;
        }
        buf.copy_from_slice(&self.0[start..end]);
        true
    }
}

const DISK_BYTES: usize = 1024 * 1024;
const ESP_START: u64 = 34;
const ESP_END: u64 = 34 + 127; // 128 sectors
const DATA_START: u64 = 162;
const DATA_END: u64 = 200;
const ENTRY_LBA: u64 = 2;
const N_ENTRIES: u32 = 128;
const ENTRY_SIZE: u32 = 128;

/// Linux filesystem GUID `0FC63DAF-8483-4772-8E79-3D69D8477DE4`.
const LINUX_FS_GUID: [u8; 16] = [
    0xAF, 0x3D, 0xC6, 0x0F, 0x83, 0x84, 0x72, 0x47, 0x8E, 0x79, 0x3D, 0x69, 0xD8, 0x47, 0x7D, 0xE4,
];
const ESP_UNIQUE: [u8; 16] = [
    0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF, 0x00,
];

fn put_u32(buf: &mut [u8], off: usize, v: u32) {
    buf[off..off + 4].copy_from_slice(&v.to_le_bytes());
}
fn put_u64(buf: &mut [u8], off: usize, v: u64) {
    buf[off..off + 8].copy_from_slice(&v.to_le_bytes());
}

fn write_entry(disk: &mut [u8], idx: u32, type_guid: &[u8; 16], unique: &[u8; 16], start: u64, end: u64) {
    let off = (ENTRY_LBA * 512) as usize + (idx as usize) * ENTRY_SIZE as usize;
    disk[off..off + 16].copy_from_slice(type_guid);
    disk[off + 16..off + 32].copy_from_slice(unique);
    put_u64(disk, off + 32, start);
    put_u64(disk, off + 40, end);
}

fn finish_header(disk: &mut [u8], array_crc: u32) {
    let hdr = &mut disk[512..1024];
    hdr[0..8].copy_from_slice(GPT_SIGNATURE);
    put_u32(hdr, 8, GPT_REVISION_1_0);
    put_u32(hdr, 12, GPT_HEADER_SIZE_MIN);
    put_u32(hdr, 16, 0); // CRC zeroed for the computation
    put_u64(hdr, 24, 1); // MyLBA
    put_u64(hdr, 32, (DISK_BYTES as u64 / 512) - 1);
    put_u64(hdr, 40, 34);
    put_u64(hdr, 48, (DISK_BYTES as u64 / 512) - 34);
    put_u64(hdr, 72, ENTRY_LBA);
    put_u32(hdr, 80, N_ENTRIES);
    put_u32(hdr, 84, ENTRY_SIZE);
    put_u32(hdr, 88, array_crc);
    let c = crc32(&hdr[..GPT_HEADER_SIZE_MIN as usize]);
    put_u32(hdr, 16, c);
}

fn synthetic_gpt() -> Vec<u8> {
    let mut disk = vec![0u8; DISK_BYTES];
    disk[510] = 0x55;
    disk[511] = 0xAA;
    disk[0x1BE + 4] = MBR_TYPE_GPT;
    put_u32(&mut disk, 0x1BE + 8, 1);
    put_u32(&mut disk, 0x1BE + 12, (DISK_BYTES as u32 / 512) - 1);
    write_entry(&mut disk, 0, &ESP_TYPE_GUID, &ESP_UNIQUE, ESP_START, ESP_END);
    let mut data_unique = [0u8; 16];
    data_unique[0] = 0xAB;
    write_entry(&mut disk, 1, &LINUX_FS_GUID, &data_unique, DATA_START, DATA_END);
    let array_bytes = (N_ENTRIES * ENTRY_SIZE) as usize;
    let array_off = (ENTRY_LBA * 512) as usize;
    let array_crc = crc32(&disk[array_off..array_off + array_bytes]);
    finish_header(&mut disk, array_crc);
    disk
}

#[test]
fn boot_source_disk_before_iso() {
    assert_eq!(raynu_f_boot_source(true), BootSource::Disk);
    assert_eq!(raynu_f_boot_source(false), BootSource::Iso);
}

#[test]
fn zeros_are_not_gpt() {
    let z = vec![0u8; DISK_BYTES];
    assert_eq!(find_esp(&SliceDisk(&z)), Err(GptError::NoProtectiveMbr));
    assert!(!disk_has_gpt_esp(&SliceDisk(&z)));
}

#[test]
fn synthetic_gpt_finds_esp_with_real_crc() {
    let disk = synthetic_gpt();
    let hdr = parse_gpt_header(&disk[512..1024]).expect("header CRC");
    assert_eq!(hdr.revision, GPT_REVISION_1_0);
    assert_eq!(hdr.header_size, GPT_HEADER_SIZE_MIN);
    assert_eq!(hdr.partition_entry_lba, ENTRY_LBA);
    let esp = find_esp(&SliceDisk(&disk)).expect("ESP");
    assert_eq!(esp.partition_number, 1);
    assert_eq!(esp.start_lba, ESP_START);
    assert_eq!(esp.end_lba, ESP_END);
    assert_eq!(esp.size_lba(), 128);
    assert_eq!(esp.unique_guid, ESP_UNIQUE);
    assert!(disk_has_gpt_esp(&SliceDisk(&disk)));
}

#[test]
fn bad_header_crc_is_rejected() {
    let mut disk = synthetic_gpt();
    disk[512 + 24] ^= 0xFF; // MyLBA, inside the CRC coverage
    assert_eq!(
        parse_gpt_header(&disk[512..1024]),
        Err(GptError::BadHeaderCrc)
    );
    assert_eq!(find_esp(&SliceDisk(&disk)), Err(GptError::BadHeaderCrc));
}

#[test]
fn gpt_without_esp_is_no_esp() {
    let mut disk = synthetic_gpt();
    // Overwrite the ESP type GUID with the Linux FS GUID, then re-CRC.
    write_entry(&mut disk, 0, &LINUX_FS_GUID, &ESP_UNIQUE, ESP_START, ESP_END);
    let array_bytes = (N_ENTRIES * ENTRY_SIZE) as usize;
    let array_off = (ENTRY_LBA * 512) as usize;
    let array_crc = crc32(&disk[array_off..array_off + array_bytes]);
    finish_header(&mut disk, array_crc);
    assert_eq!(find_esp(&SliceDisk(&disk)), Err(GptError::NoEsp));
}

#[test]
fn bad_entry_array_crc_is_rejected() {
    let mut disk = synthetic_gpt();
    disk[(ENTRY_LBA * 512) as usize] ^= 0xFF;
    assert_eq!(find_esp(&SliceDisk(&disk)), Err(GptError::BadEntryArrayCrc));
}

#[test]
fn missing_mbr_signature_is_rejected() {
    let mut disk = synthetic_gpt();
    disk[510] = 0;
    assert_eq!(find_esp(&SliceDisk(&disk)), Err(GptError::NoProtectiveMbr));
}

#[test]
fn ieee_crc32_known_answer() {
    assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
}
