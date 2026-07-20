use super::*;

#[test]
fn serial_listed() {
    assert!(supported_kinds().contains(&DeviceKind::Serial));
}

#[test]
fn virtio_blk_listed() {
    assert!(supported_kinds().contains(&DeviceKind::VirtioBlk));
}
