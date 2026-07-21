use super::*;

#[test]
fn vswitch_ports() {
    assert_eq!(VSwitch::new(8).port_count(), 8);
}

#[test]
fn learn_and_unicast_forward() {
    let mut sw = VSwitch::new(2);
    let mac0 = [0x52, 0x54, 0x00, 0x12, 0x34, 0x56];
    let mac1 = [0x52, 0x54, 0x00, 0x12, 0x34, 0x57];
    sw.attach(0, mac0).unwrap();
    sw.attach(1, mac1).unwrap();

    let mut frame = [0u8; 64];
    let payload = b"RAYNU-NET";
    let n = build_eth_frame(&mut frame, &mac1, &mac0, 0x88B5, payload).unwrap();
    let dst = sw.forward(0, &frame[..n]).unwrap();
    assert_eq!(dst, Some(1));
}

#[test]
fn unknown_dst_floods() {
    let mut sw = VSwitch::new(2);
    let mac0 = [0x52, 0x54, 0x00, 0x00, 0x00, 0x01];
    sw.attach(0, mac0).unwrap();
    let mut frame = [0u8; 32];
    let unknown = [0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFE]; // unicast unknown
    let n = build_eth_frame(&mut frame, &unknown, &mac0, 0x0800, b"x").unwrap();
    assert_eq!(sw.forward(0, &frame[..n]).unwrap(), None);
}

#[test]
fn partition_drops_unicast() {
    let mut sw = VSwitch::new(2);
    let mac0 = [0x52, 0x54, 0x00, 0x12, 0x34, 0x56];
    let mac1 = [0x52, 0x54, 0x00, 0x12, 0x34, 0x57];
    sw.attach(0, mac0).unwrap();
    sw.attach(1, mac1).unwrap();
    let mut frame = [0u8; 64];
    let n = build_eth_frame(&mut frame, &mac1, &mac0, 0x88B5, b"p").unwrap();
    sw.set_partitioned(true);
    assert_eq!(sw.forward(0, &frame[..n]).unwrap(), None);
    sw.set_partitioned(false);
    assert_eq!(sw.forward(0, &frame[..n]).unwrap(), Some(1));
}
