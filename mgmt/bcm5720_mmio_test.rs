use super::{
    parse_mocked_rx_bd_bytes, pci_id_is_bcm5720, rx_bd_packet_len, BCM5720_DEVICE, BCM5720_VENDOR,
    FRAME_MAX,
};

#[test]
fn iron_pci_id_is_14e4_165f_only() {
    assert!(pci_id_is_bcm5720(BCM5720_VENDOR, BCM5720_DEVICE));
    assert!(!pci_id_is_bcm5720(0x14e4, 0x1657)); // BCM5719 — not the census pick
    assert!(!pci_id_is_bcm5720(0x8086, 0x100E));
    assert!(!pci_id_is_bcm5720(0x8086, 0x1572));
}

#[test]
fn rx_bd_parse_requires_end_no_error() {
    assert_eq!(rx_bd_packet_len(64, 0, 0), None);
    assert_eq!(rx_bd_packet_len(64, 0x0004, 0x0001_0000), None);
    assert_eq!(rx_bd_packet_len(64, 0x0404, 0), None);
    assert_eq!(rx_bd_packet_len(0, 0x0004, 0), None);
    assert_eq!(rx_bd_packet_len(9000, 0x0004, 0), None);
    assert_eq!(rx_bd_packet_len(64, 0x0004, 0), Some(64));
    assert_eq!(
        rx_bd_packet_len(FRAME_MAX as u32, 0x0004, 0),
        Some(FRAME_MAX)
    );
}

#[test]
fn parse_mocked_rx_bd_bytes_good_frame() {
    let mut raw = [0u8; 32];
    raw[8] = 64;
    raw[12] = 0x04;
    assert_eq!(parse_mocked_rx_bd_bytes(&raw), Some(64));
    raw[20] = 0;
    raw[21] = 0;
    raw[22] = 0;
    raw[23] = 1;
    assert_eq!(parse_mocked_rx_bd_bytes(&raw), None);
}

#[test]
fn fuzz_parse_mocked_rx_bd_bytes_never_panics() {
    for flags in 0u8..=255 {
        for err_hi in [0u8, 1, 0xff] {
            for length in [0u16, 64, 1514, 65535] {
                let mut raw = [0u8; 32];
                let lb = length.to_le_bytes();
                raw[8] = lb[0];
                raw[9] = lb[1];
                raw[12] = flags;
                raw[23] = err_hi;
                let n = parse_mocked_rx_bd_bytes(&raw);
                if let Some(got) = n {
                    assert!(got > 0 && got <= FRAME_MAX);
                }
            }
        }
    }
}

#[cfg(miri)]
#[test]
fn miri_parse_mocked_rx_bd_bytes() {
    let mut raw = [0u8; 32];
    raw[8] = 128;
    raw[12] = 0x04;
    assert_eq!(parse_mocked_rx_bd_bytes(&raw), Some(128));
}

#[cfg(kani)]
#[kani::proof]
fn kani_rx_bd_packet_len_bounds() {
    let idx_len: u32 = kani::any();
    let type_flags: u32 = kani::any();
    let err_vlan: u32 = kani::any();
    if let Some(n) = rx_bd_packet_len(idx_len, type_flags, err_vlan) {
        assert!(n > 0 && n <= FRAME_MAX);
    }
}
