use super::*;

#[test]
fn markers_and_magic_stable() {
    assert_eq!(M4_BLK_OK_MARKER, "RAYNU-V-M4-BLK-OK");
    assert_eq!(VIRTIO_MMIO_MAGIC, 0x7472_6976);
    assert_eq!(VIRTIO_ID_BLOCK, 2);
    assert_eq!(DEFAULT_INSTALL_DISK_BYTES, 64 * 1024 * 1024);
    assert_eq!(
        capacity_sectors_for(DEFAULT_INSTALL_DISK_BYTES),
        (DEFAULT_INSTALL_DISK_BYTES / 512) as u64
    );
}

#[test]
fn install_sized_disk_writes_lba1_marker() {
    let mut disk = vec![0u8; 4096];
    // SAFETY: heap buffer as fake disk HPA.
    unsafe {
        init(0x1000_0000, disk.as_mut_ptr() as u64, disk.len());
    }
    assert!(mmio_access(0x1000_0000 + OFF_STATUS, true, STATUS_DRIVER_OK).is_some());
    assert!(blk_ok());
    assert!(install_disk_written());
    let lba1 = &disk[512..516];
    assert_eq!(
        u32::from_le_bytes(lba1.try_into().unwrap()),
        INSTALL_DISK_PATTERN
    );
}

/// Iron E5: ESP persist is 1 KiB; virtio disk is 64 MiB (use 8 KiB stand-in).
/// Also covers equal-length persist images (lab 1 KiB disk).
#[test]
fn reboot_detect_accepts_persist_prefix_on_larger_disk() {
    let persist = 1024usize;
    let disk_len = 8192usize;
    let mut stamps = vec![0u8; persist];
    let sector = 512;
    for i in 0..(sector / 4) {
        let v = (DISK_PATTERN ^ (i as u32)).to_le_bytes();
        stamps[i * 4..i * 4 + 4].copy_from_slice(&v);
    }
    for i in 0..(sector / 4) {
        let v = (INSTALL_DISK_PATTERN ^ (i as u32)).to_le_bytes();
        let off = sector + i * 4;
        stamps[off..off + 4].copy_from_slice(&v);
    }
    let mut disk = vec![0u8; disk_len];
    unsafe {
        init_with_image(
            0x1000_0000,
            disk.as_mut_ptr() as u64,
            disk.len(),
            Some(&stamps),
        );
        set_reboot_detect(true);
    }
    assert_eq!(&disk[..persist], stamps.as_slice());
    assert!(disk[persist..].iter().all(|&b| b == 0));
    assert!(mmio_access(0x1000_0000 + OFF_STATUS, true, STATUS_DRIVER_OK).is_some());
    assert!(blk_ok());
    assert!(booted_from_disk());
    // Do not assert `!install_disk_written()`: that flag is process-global and
    // `install_sized_disk_writes_lba1_marker` may set it under `--test-threads>1`.
    // Detection (`booted_from_disk`) is the contract under test.
    set_reboot_detect(false);
}

#[test]
fn mmio_magic_and_status_handshake() {
    let mut disk = [0u8; 512];
    // SAFETY: stack buffer as fake disk HPA for unit test.
    unsafe {
        init(0x1000_0000, disk.as_mut_ptr() as u64, disk.len());
    }
    assert_eq!(
        mmio_access(0x1000_0000, false, 0).unwrap().unwrap(),
        VIRTIO_MMIO_MAGIC
    );
    assert_eq!(
        mmio_access(0x1000_0000 + OFF_DEVICE_ID, false, 0)
            .unwrap()
            .unwrap(),
        VIRTIO_ID_BLOCK
    );
    assert!(mmio_access(0x1000_0000 + OFF_STATUS, true, STATUS_DRIVER_OK).is_some());
    assert!(blk_ok());
    assert_eq!(
        u32::from_le_bytes(disk[0..4].try_into().unwrap()),
        DISK_PATTERN
    );
}
