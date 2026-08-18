use super::{
    parse_mocked_rx_bd_bytes, pci_id_is_bcm5720, pick_bcm5720_pci, rx_bd_packet_len,
    Bcm5720PickReason, BCM5720_DEVICE, BCM5720_VENDOR, FRAME_MAX,
};

/// R640 dual-port BCM5720 from COM2: func 0 = unused jack, func 1 = SNP / LAN.
const MAC_FUNC0: [u8; 6] = [0xb0, 0x26, 0x28, 0x5c, 0x5a, 0x38];
const MAC_FUNC1: [u8; 6] = [0xb0, 0x26, 0x28, 0x5c, 0x5a, 0x3a];

fn r640_cands() -> [(u8, u8, u8, [u8; 6]); 2] {
    [(1, 0, 0, MAC_FUNC0), (1, 0, 1, MAC_FUNC1)]
}

#[test]
fn pick_snp_lease_mac_binds_func1() {
    let p = pick_bcm5720_pci(&r640_cands(), MAC_FUNC1).unwrap();
    assert_eq!((p.bus, p.dev, p.func), (1, 0, 1));
    assert_eq!(p.reason, Bcm5720PickReason::MacMatch);
}

#[test]
fn pick_func0_mac_binds_func0() {
    let p = pick_bcm5720_pci(&r640_cands(), MAC_FUNC0).unwrap();
    assert_eq!((p.bus, p.dev, p.func), (1, 0, 0));
    assert_eq!(p.reason, Bcm5720PickReason::MacMatch);
}

#[test]
fn pick_no_match_prefers_func1() {
    let want = [0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff];
    let p = pick_bcm5720_pci(&r640_cands(), want).unwrap();
    assert_eq!((p.bus, p.dev, p.func), (1, 0, 1));
    assert_eq!(p.reason, Bcm5720PickReason::PreferFunc1);
}

#[test]
fn pick_only_func0_falls_back_func0() {
    let cands = [(1, 0, 0, MAC_FUNC0)];
    let p = pick_bcm5720_pci(&cands, MAC_FUNC1).unwrap();
    assert_eq!((p.bus, p.dev, p.func), (1, 0, 0));
    assert_eq!(p.reason, Bcm5720PickReason::PreferFunc0);
}

#[test]
fn pick_empty_is_none() {
    assert!(pick_bcm5720_pci(&[], MAC_FUNC1).is_none());
}

#[test]
fn pick_neither_func0_nor_1_uses_first() {
    let cands = [(2, 0, 2, MAC_FUNC0)];
    let p = pick_bcm5720_pci(&cands, MAC_FUNC1).unwrap();
    assert_eq!((p.bus, p.dev, p.func), (2, 0, 2));
    assert_eq!(p.reason, Bcm5720PickReason::FirstCand);
}

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
