use super::*;

#[test]
fn markers_and_magic_stable() {
    assert_eq!(M4_NET_OK_MARKER, "RAYNU-V-M4-NET-OK");
    assert_eq!(VIRTIO_MMIO_MAGIC, 0x7472_6976);
    assert_eq!(VIRTIO_ID_NET, 1);
}

#[test]
fn dual_port_handshake_exchanges_frame() {
    let mut b0 = [0u8; 4096];
    let mut b1 = [0u8; 4096];
    // SAFETY: stack buffers as fake host packet HPAs.
    unsafe {
        init(
            0x2000_0000,
            0x2000_1000,
            b0.as_mut_ptr() as u64,
            b1.as_mut_ptr() as u64,
        );
    }
    assert_eq!(
        mmio_access(0x2000_0000, false, 0).unwrap().unwrap(),
        VIRTIO_MMIO_MAGIC
    );
    assert_eq!(
        mmio_access(0x2000_0000 + OFF_DEVICE_ID, false, 0)
            .unwrap()
            .unwrap(),
        VIRTIO_ID_NET
    );
    assert!(mmio_access(0x2000_0000 + OFF_STATUS, true, 0x0F).is_some());
    assert!(!net_ok()); // need both ports
    assert!(mmio_access(0x2000_1000 + OFF_STATUS, true, 0x0F).is_some());
    assert!(net_ok());
    assert_eq!(
        &b1[crate::net::ETH_HDR_LEN..crate::net::ETH_HDR_LEN + PROBE_PAYLOAD.len()],
        PROBE_PAYLOAD
    );
}
