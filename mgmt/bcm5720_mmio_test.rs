use super::{
    bmsr_an_complete, bmsr_link_up, decode_phy_link, inherit_snp_phy, mac_mode_from_link,
    parse_mocked_rx_bd_bytes, pci_id_is_bcm5720, phy_addr_5717_plus, pick_bcm5720_pci,
    rx_bd_packet_len, station_mac, Bcm5720PickReason, BCM5720_DEVICE, BCM5720_VENDOR, FRAME_MAX,
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
fn pick_r640_peek_39_vs_snp_3a_falls_back_func1() {
    let peek_func1 = [0xb0, 0x26, 0x28, 0x5c, 0x5a, 0x39];
    let snp = MAC_FUNC1;
    let cands = [(1, 0, 0, MAC_FUNC0), (1, 0, 1, peek_func1)];
    let p = pick_bcm5720_pci(&cands, snp).unwrap();
    assert_eq!((p.bus, p.dev, p.func), (1, 0, 1));
    assert_eq!(p.reason, Bcm5720PickReason::PreferFunc1);
    assert_eq!(station_mac(peek_func1, snp), snp);
}

#[test]
fn station_mac_prefers_non_zero_lease() {
    let peeked = [0xb0, 0x26, 0x28, 0x5c, 0x5a, 0x39];
    assert_eq!(station_mac(peeked, MAC_FUNC1), MAC_FUNC1);
}

#[test]
fn station_mac_falls_back_to_peek_when_lease_zero() {
    let peeked = [0xb0, 0x26, 0x28, 0x5c, 0x5a, 0x39];
    assert_eq!(station_mac(peeked, [0; 6]), peeked);
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

#[test]
fn bringup_follows_linux_tg3_not_bnxt() {
    let src = include_str!("bcm5720_mmio.rs");
    assert!(src.contains("Linux `tg3`"));
    assert!(src.contains("**not** `bnxt`"));
    assert!(src.contains("GRC_MISC_CFG_PRESERVE_PCIE"));
    assert!(src.contains("RDMAC_MODE"));
    assert!(src.contains("WDMAC_MODE"));
    assert!(src.contains("5717_PLUS"));
    assert!(src.contains("do not program RCVDBDI_STD_BD+NIC_ADDR"));
    assert!(src.contains("RCVDBDI_MODE_INV_RING_SZ"));
    assert!(src.contains("RCVBDI_STD_THRESH"));
    assert!(src.contains("DEFAULT_MB_MACRX_LOW_WATER_57765"));
    assert!(src.contains("phy_addr = u32::from(func) + 1"));
    assert!(src.contains("tg3_write_sig_pre_reset"));
    assert!(src.contains("fn station_mac("));
    assert!(src.contains("BMSR_LSTATUS"));
    assert!(src.contains("RX_MODE_PROMISC"));
    assert!(src.contains("link=up"));
    assert!(src.contains("fn decode_phy_link("));
    assert!(src.contains("fn mac_mode_from_link("));
    assert!(src.contains("tg3_adjust_link"));
    assert!(src.contains("BMCR_RESET"));
    assert!(src.contains("MII_TG3_AUXCTL_SHDWSEL_PWRCTL"));
    assert!(src.contains("MII_TG3_AUXCTL_MISC_FORCE_AMDIX"));
    assert!(src.contains("CPMU_CTRL_LINK_IDLE_MODE"));
    assert!(src.contains("fn phy_addr_5717_plus("));
    assert!(src.contains("tg3_bmcr_reset"));
    assert!(src.contains("inherit SNP PHY"));
    assert!(src.contains("pre-reset bmsr"));
    assert!(src.contains("MII_TG3_MISC_SHDW"));
    assert!(src.contains("skip CORECLK_RESET"));
    assert!(src.contains("MII_TG3_MISC_SHDW_APD_ENABLE"));
    assert!(src.contains("fn inherit_snp_phy("));
    assert!(!src.contains("drivers/net/ethernet/broadcom/bnxt"));
}

#[test]
fn inherit_snp_phy_follows_bmsr_lstatus() {
    assert!(!inherit_snp_phy(0x7949));
    assert!(inherit_snp_phy(0x0004));
    assert!(inherit_snp_phy(0x794d));
}

#[test]
fn iron_bmsr_7949_is_link_down_an_incomplete() {
    const IRON: u16 = 0x7949;
    assert!(!bmsr_link_up(IRON));
    assert!(!bmsr_an_complete(IRON));
    assert!(bmsr_an_complete(0x0020));
}

#[test]
fn phy_addr_5717_plus_copper_func1_is_2() {
    assert_eq!(phy_addr_5717_plus(1, 0), 2);
    assert_eq!(phy_addr_5717_plus(0, 0), 1);
    assert_eq!(phy_addr_5717_plus(1, 0x100), 9);
}

#[test]
fn bmsr_link_up_is_bit2() {
    assert!(!bmsr_link_up(0));
    assert!(bmsr_link_up(0x0004));
    assert!(!bmsr_link_up(0x0002));
}

#[test]
fn decode_phy_link_prefers_1000_full() {
    assert_eq!(decode_phy_link(0x01E1, 0x0C00), (1000, true));
    assert_eq!(decode_phy_link(0x01E1, 0x0400), (1000, false));
    assert_eq!(decode_phy_link(0x0100, 0), (100, true));
    assert_eq!(decode_phy_link(0x0080, 0), (100, false));
    assert_eq!(decode_phy_link(0x0040, 0), (10, true));
    assert_eq!(decode_phy_link(0, 0), (10, false));
}

#[test]
fn mac_mode_from_link_gmii_vs_mii() {
    const GMII: u32 = 0x08;
    const MII: u32 = 0x04;
    const HALF: u32 = 0x02;
    let g = mac_mode_from_link(1000, true);
    let m = mac_mode_from_link(100, true);
    let h = mac_mode_from_link(100, false);
    assert_eq!(g & 0x0c, GMII);
    assert_eq!(m & 0x0c, MII);
    assert_eq!(h & HALF, HALF);
    assert_eq!(g & HALF, 0);
}
