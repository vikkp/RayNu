//! Host tests for M8.0 persist: GPT+ESP+ext4 round-trip after HV reboot.
//!
//! Never prints `RAYNU-V-M7-ISO-INSTALL-OK`. Nested QEMU is not this file.

use super::*;
use crate::raynu_f::crc32;
use crate::raynu_f::gpt::{
    parse_gpt_header, ESP_TYPE_GUID, GPT_HEADER_SIZE_MIN, GPT_REVISION_1_0, GPT_SIGNATURE,
    MBR_TYPE_GPT,
};

const ENTRY_LBA: u64 = 2;
const N_ENTRIES: u32 = 128;
const ENTRY_SIZE: u32 = 128;
const ESP_UNIQUE: [u8; 16] = [
    0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF, 0x00,
];
const DATA_UNIQUE: [u8; 16] = [
    0xAB, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02,
];

/// In-process HV view of one virtio disk plus its persist backend.
struct PersistStore {
    kind: PersistKind,
    durable: Option<Vec<u8>>,
    leftover: Option<Vec<u8>>,
}

impl PersistStore {
    fn new(kind: PersistKind) -> Self {
        Self {
            kind,
            durable: None,
            leftover: None,
        }
    }

    fn write_disk(&mut self, image: &[u8]) {
        match self.kind {
            PersistKind::File | PersistKind::DurableLun => {
                self.durable = Some(image.to_vec());
                self.leftover = None;
            }
            PersistKind::LeftoverDram => {
                self.leftover = Some(image.to_vec());
                self.durable = None;
            }
        }
    }

    /// Allocator reset / process restart of RayNu-V: leftover DRAM is gone.
    fn hv_reboot(&mut self) {
        self.leftover = None;
    }

    fn restore(&self) -> Option<&[u8]> {
        match self.kind {
            PersistKind::File | PersistKind::DurableLun => self.durable.as_deref(),
            PersistKind::LeftoverDram => self.leftover.as_deref(),
        }
    }
}

fn put_u32(buf: &mut [u8], off: usize, v: u32) {
    buf[off..off + 4].copy_from_slice(&v.to_le_bytes());
}
fn put_u64(buf: &mut [u8], off: usize, v: u64) {
    buf[off..off + 8].copy_from_slice(&v.to_le_bytes());
}

fn write_entry(
    disk: &mut [u8],
    idx: u32,
    type_guid: &[u8; 16],
    unique: &[u8; 16],
    start: u64,
    end: u64,
) {
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
    put_u32(hdr, 16, 0);
    put_u64(hdr, 24, 1);
    put_u64(hdr, 32, (HOST_PERSIST_DISK_BYTES as u64 / 512) - 1);
    put_u64(hdr, 40, 34);
    put_u64(hdr, 48, (HOST_PERSIST_DISK_BYTES as u64 / 512) - 34);
    put_u64(hdr, 72, ENTRY_LBA);
    put_u32(hdr, 80, N_ENTRIES);
    put_u32(hdr, 84, ENTRY_SIZE);
    put_u32(hdr, 88, array_crc);
    let c = crc32(&hdr[..GPT_HEADER_SIZE_MIN as usize]);
    put_u32(hdr, 16, c);
}

/// FAT12 ESP with `\EFI\BOOT\BOOTX64.EFI`. 128 sectors = 64 KiB.
fn build_fat12_esp(payload: &[u8]) -> Vec<u8> {
    const SEC: usize = 512;
    const TOTAL: usize = 128;
    const RESERVED: usize = 1;
    const FAT_SECS: usize = 2;
    const ROOT_ENTS: usize = 64;
    let root_secs = ROOT_ENTS * 32 / SEC;
    let mut v = vec![0u8; TOTAL * SEC];
    v[0] = 0xEB;
    v[1] = 0x3C;
    v[2] = 0x90;
    v[3..11].copy_from_slice(b"RAYNUM8 ");
    v[11..13].copy_from_slice(&(SEC as u16).to_le_bytes());
    v[13] = 1;
    v[14..16].copy_from_slice(&(RESERVED as u16).to_le_bytes());
    v[16] = 1;
    v[17..19].copy_from_slice(&(ROOT_ENTS as u16).to_le_bytes());
    v[19..21].copy_from_slice(&(TOTAL as u16).to_le_bytes());
    v[21] = 0xF8;
    v[22..24].copy_from_slice(&(FAT_SECS as u16).to_le_bytes());
    v[510] = 0x55;
    v[511] = 0xAA;

    let fat_start = RESERVED;
    let root_start = fat_start + FAT_SECS;
    let data_start = root_start + root_secs;

    let set_fat = |v: &mut Vec<u8>, cl: usize, val: u16| {
        let off = fat_start * SEC + cl * 3 / 2;
        let cur = u16::from_le_bytes([v[off], v[off + 1]]);
        let new = if cl & 1 == 0 {
            (cur & 0xF000) | (val & 0x0FFF)
        } else {
            (cur & 0x000F) | (val << 4)
        };
        v[off..off + 2].copy_from_slice(&new.to_le_bytes());
    };
    set_fat(&mut v, 0, 0xFF8);
    set_fat(&mut v, 1, 0xFFF);

    let mk_ent = |name: &[u8; 11], attr: u8, cluster: u16, size: u32| {
        let mut e = [0u8; 32];
        e[..11].copy_from_slice(name);
        e[11] = attr;
        e[26..28].copy_from_slice(&cluster.to_le_bytes());
        e[28..32].copy_from_slice(&size.to_le_bytes());
        e
    };

    let cl_efi = 2usize;
    let cl_boot = 3usize;
    let cl_file = 4usize;
    let root_off = root_start * SEC;
    v[root_off..root_off + 32].copy_from_slice(&mk_ent(b"EFI        ", 0x10, cl_efi as u16, 0));
    set_fat(&mut v, cl_efi, 0xFFF);
    let efi_off = (data_start + cl_efi - 2) * SEC;
    v[efi_off..efi_off + 32].copy_from_slice(&mk_ent(b".          ", 0x10, cl_efi as u16, 0));
    v[efi_off + 32..efi_off + 64].copy_from_slice(&mk_ent(b"..         ", 0x10, 0, 0));
    v[efi_off + 64..efi_off + 96].copy_from_slice(&mk_ent(b"BOOT       ", 0x10, cl_boot as u16, 0));
    set_fat(&mut v, cl_boot, 0xFFF);
    let boot_off = (data_start + cl_boot - 2) * SEC;
    v[boot_off..boot_off + 32].copy_from_slice(&mk_ent(b".          ", 0x10, cl_boot as u16, 0));
    v[boot_off + 32..boot_off + 64].copy_from_slice(&mk_ent(
        b"..         ",
        0x10,
        cl_efi as u16,
        0,
    ));
    v[boot_off + 64..boot_off + 96].copy_from_slice(&mk_ent(
        b"BOOTX64 EFI",
        0x20,
        cl_file as u16,
        payload.len() as u32,
    ));
    let nclusters = (payload.len() + SEC - 1) / SEC;
    for i in 0..nclusters {
        let cl = cl_file + i;
        let off = (data_start + cl - 2) * SEC;
        let start = i * SEC;
        let end = (start + SEC).min(payload.len());
        v[off..off + (end - start)].copy_from_slice(&payload[start..end]);
        let next = if i + 1 == nclusters {
            0xFFF
        } else {
            (cl + 1) as u16
        };
        set_fat(&mut v, cl, next);
    }
    v
}

fn build_gpt_esp_ext4_image() -> Vec<u8> {
    let mut disk = vec![0u8; HOST_PERSIST_DISK_BYTES];
    disk[510] = 0x55;
    disk[511] = 0xAA;
    disk[0x1BE + 4] = MBR_TYPE_GPT;
    put_u32(&mut disk, 0x1BE + 8, 1);
    put_u32(
        &mut disk,
        0x1BE + 12,
        (HOST_PERSIST_DISK_BYTES as u32 / 512) - 1,
    );
    write_entry(
        &mut disk,
        0,
        &ESP_TYPE_GUID,
        &ESP_UNIQUE,
        HOST_PERSIST_ESP_START_LBA,
        HOST_PERSIST_ESP_END_LBA,
    );
    write_entry(
        &mut disk,
        1,
        &LINUX_FS_GUID,
        &DATA_UNIQUE,
        HOST_PERSIST_DATA_START_LBA,
        HOST_PERSIST_DATA_END_LBA,
    );
    let array_bytes = (N_ENTRIES * ENTRY_SIZE) as usize;
    let array_off = (ENTRY_LBA * 512) as usize;
    let array_crc = crc32(&disk[array_off..array_off + array_bytes]);
    finish_header(&mut disk, array_crc);

    let payload = b"MZ\0RAYNU-M8-PERSIST\\EFI\\BOOT\\BOOTX64.EFI";
    let fat = build_fat12_esp(payload);
    let esp_off = (HOST_PERSIST_ESP_START_LBA * 512) as usize;
    disk[esp_off..esp_off + fat.len()].copy_from_slice(&fat);

    let data_off = (HOST_PERSIST_DATA_START_LBA * 512) as usize;
    disk[data_off + EXT4_MAGIC_OFF..data_off + EXT4_MAGIC_OFF + 2]
        .copy_from_slice(&EXT4_SUPER_MAGIC.to_le_bytes());
    let cmd = HOST_PERSIST_ROOT_CMDLINE.as_bytes();
    let cmd_off = data_off + 0x600;
    disk[cmd_off..cmd_off + cmd.len()].copy_from_slice(cmd);
    disk
}

fn file_round_trip(image: &[u8]) -> Vec<u8> {
    let path =
        std::env::temp_dir().join(format!("raynu-m8-persist-{}-{}.img", std::process::id(), 1));
    std::fs::write(&path, image).expect("write persist file");
    // HV reboot: in-memory image is gone; the QEMU file / LUN is not.
    let restored = std::fs::read(&path).expect("re-open persist file after HV reboot");
    let _ = std::fs::remove_file(&path);
    restored
}

#[test]
fn select_kind_prefers_durable_then_leftover_fallback() {
    assert_eq!(
        select_persist_kind(true, true),
        PersistKind::File,
        "nested + QEMU file"
    );
    assert_eq!(
        select_persist_kind(false, true),
        PersistKind::DurableLun,
        "iron + USB/NVMe LUN"
    );
    assert_eq!(select_persist_kind(true, false), PersistKind::LeftoverDram);
    assert_eq!(select_persist_kind(false, false), PersistKind::LeftoverDram);
    assert!(kind_survives_hv_reboot(PersistKind::File));
    assert!(kind_survives_hv_reboot(PersistKind::DurableLun));
    assert!(!kind_survives_hv_reboot(PersistKind::LeftoverDram));
    assert!(esp_copy_is_rejected());
    assert!(PERSIST_EXCLUSIVE_OWNERSHIP_NOTE.contains("ADR-004"));
    assert!(PERC_UBUNTU_UNTOUCHED_NOTE.contains("PERC"));
}

#[test]
fn leftover_dram_dies_on_hv_reboot() {
    let image = build_gpt_esp_ext4_image();
    assert!(restored_disk_is_installed(&image));
    let mut store = PersistStore::new(PersistKind::LeftoverDram);
    store.write_disk(&image);
    assert!(restored_disk_is_installed(store.restore().unwrap()));
    store.hv_reboot();
    assert!(
        store.restore().is_none(),
        "leftover DRAM must not claim persist across HV reboot"
    );
}

#[test]
fn durable_lun_survives_in_process_hv_reboot() {
    let image = build_gpt_esp_ext4_image();
    let mut store = PersistStore::new(PersistKind::DurableLun);
    store.write_disk(&image);
    store.hv_reboot();
    let restored = store.restore().expect("LUN bytes");
    assert!(restored_disk_is_installed(restored));
}

#[test]
fn file_backend_round_trips_gpt_esp_ext4_after_hv_reboot() {
    let image = build_gpt_esp_ext4_image();
    parse_gpt_header(&image[512..1024]).expect("header CRC");
    assert!(restored_disk_is_installed(&image));

    let mut store = PersistStore::new(PersistKind::File);
    store.write_disk(&image);
    let owned = image; // drop the original binding after write
    store.hv_reboot();
    let from_store = store.restore().expect("file bytes in HV view").to_vec();
    assert!(restored_disk_is_installed(&from_store));

    // Nested analogue: QEMU file survives process restart (`std::fs` re-open).
    let from_fs = file_round_trip(&owned);
    assert_eq!(from_fs.len(), HOST_PERSIST_DISK_BYTES);
    assert!(
        restored_disk_is_installed(&from_fs),
        "restored file must still have GPT ESP, BOOTX64.EFI, ext4, root=UUID="
    );
    assert!(from_fs
        .windows(HOST_PERSIST_ROOT_CMDLINE.len())
        .any(|w| w == HOST_PERSIST_ROOT_CMDLINE.as_bytes()));
    assert_ne!(M8_DISK_PERSIST_HOST_OK_MARKER, "RAYNU-V-M7-ISO-INSTALL-OK");
    println!("{M8_DISK_PERSIST_HOST_OK_MARKER}");
}

#[test]
fn host_never_prints_everest_iso_install_ok() {
    assert!(host_never_prints_iso_install_ok());
    assert_eq!(M8_DISK_PERSIST_OK_MARKER, "RAYNU-V-M8-DISK-PERSIST-OK");
    assert_eq!(
        M8_DISK_PERSIST_HOST_OK_MARKER,
        "RAYNU-V-M8-DISK-PERSIST-HOST-OK"
    );
}
