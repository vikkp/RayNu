use super::*;

#[test]
fn markers_and_magic_stable() {
    assert_eq!(M4_BLK_OK_MARKER, "RAYNU-V-M4-BLK-OK");
    assert_eq!(VIRTIO_MMIO_MAGIC, 0x7472_6976);
    assert_eq!(VIRTIO_ID_BLOCK, 2);
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
    assert_eq!(u32::from_le_bytes(disk[0..4].try_into().unwrap()), DISK_PATTERN);
}
