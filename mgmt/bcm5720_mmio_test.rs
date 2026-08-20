use super::{
    ape_fw_ready, ape_host_nophylock, ape_lock_init_grant_bit, ape_lock_req_bit, ape_ncsi_enabled,
    ape_per_lock_grant, ape_per_lock_req, ape_phy_lock_num, bmsr_an_complete, bmsr_link_up,
    cpmu_is_link_speed_mode, decode_phy_link, e3b_path_idrac_dedicated, inherit_skips_chip_reset,
    inherit_snp_phy, keep_ape_phy_for_idrac, mac_mode_from_link, parse_mocked_rx_bd_bytes,
    pci_cfg_save_dword_count, pci_id_is_bcm5720, pci_mem_bar_addr, phy_addr_5717_plus,
    pick_bcm5720_pci, pick_bcm5720_try_order, rx_bd_packet_len, skip_bmcr_reset,
    skip_coreclk_reset, skip_http_listen_without_lstatus, station_mac, Bcm5720PickReason,
    BCM5720_DEVICE, BCM5720_VENDOR, ETH_FCS_LEN, FRAME_MAX, RX_LEN_MAX_HW,
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
fn pick_no_match_prefers_func0_ncsi_first() {
    let want = [0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff];
    let p = pick_bcm5720_pci(&r640_cands(), want).unwrap();
    assert_eq!((p.bus, p.dev, p.func), (1, 0, 0));
    assert_eq!(p.reason, Bcm5720PickReason::PreferFunc0);
    let (ord, n) = pick_bcm5720_try_order(&r640_cands(), &[], want);
    assert_eq!(n, 2);
    assert_eq!(ord[0].func, 0);
    assert_eq!(ord[0].reason, Bcm5720PickReason::PreferFunc0);
    assert_eq!(ord[1].func, 1);
    assert_eq!(ord[1].reason, Bcm5720PickReason::PreferFunc1);
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
fn pick_r640_peek_39_vs_snp_3a_tries_func0_then_func1() {
    let peek_func1 = [0xb0, 0x26, 0x28, 0x5c, 0x5a, 0x39];
    let snp = MAC_FUNC1;
    let cands = [(1, 0, 0, MAC_FUNC0), (1, 0, 1, peek_func1)];
    let p = pick_bcm5720_pci(&cands, snp).unwrap();
    assert_eq!((p.bus, p.dev, p.func), (1, 0, 0));
    assert_eq!(p.reason, Bcm5720PickReason::PreferFunc0);
    let (ord, n) = pick_bcm5720_try_order(&cands, &[], snp);
    assert_eq!(n, 2);
    assert_eq!(ord[0].func, 0);
    assert_eq!(ord[1].func, 1);
    assert_eq!(station_mac(peek_func1, snp, false), snp);
}

#[test]
fn pick_link_up_func1_beats_ncsi_func0() {
    let peek_func1 = [0xb0, 0x26, 0x28, 0x5c, 0x5a, 0x39];
    let snp = MAC_FUNC1;
    let cands = [(1, 0, 0, MAC_FUNC0), (1, 0, 1, peek_func1)];
    let link = [false, true];
    let (ord, n) = pick_bcm5720_try_order(&cands, &link, snp);
    assert_eq!(n, 2);
    assert_eq!(ord[0].func, 1);
    assert_eq!(ord[0].reason, Bcm5720PickReason::PreferLink);
    assert_eq!(ord[1].func, 0);
    assert_eq!(ord[1].reason, Bcm5720PickReason::PreferFunc0);
}

#[test]
fn pick_dedicated_lom_lstatus_beats_snp_ape_mac() {
    let link = [true, false];
    let (ord, n) = pick_bcm5720_try_order(&r640_cands(), &link, MAC_FUNC1);
    assert_eq!(n, 2);
    assert_eq!(ord[0].func, 0);
    assert_eq!(ord[0].reason, Bcm5720PickReason::PreferLink);
    assert_eq!(ord[1].func, 1);
    assert_eq!(ord[1].reason, Bcm5720PickReason::MacMatch);
}

#[test]
fn e3b_path_idrac_dedicated_is_locked() {
    assert!(e3b_path_idrac_dedicated());
}

#[test]
fn station_mac_prefers_non_zero_lease() {
    let peeked = [0xb0, 0x26, 0x28, 0x5c, 0x5a, 0x39];
    assert_eq!(station_mac(peeked, MAC_FUNC1, false), MAC_FUNC1);
}

#[test]
fn station_mac_falls_back_to_peek_when_lease_zero() {
    let peeked = [0xb0, 0x26, 0x28, 0x5c, 0x5a, 0x39];
    assert_eq!(station_mac(peeked, [0; 6], false), peeked);
}

#[test]
fn station_mac_live_gphy_keeps_eno3_not_ape_snp() {
    let peeked_eno3 = MAC_FUNC0;
    let ape_snp = MAC_FUNC1;
    assert_eq!(station_mac(peeked_eno3, ape_snp, true), peeked_eno3);
    assert_eq!(station_mac(peeked_eno3, ape_snp, false), ape_snp);
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
    assert_eq!(rx_bd_packet_len(ETH_FCS_LEN as u32, 0x0004, 0), None);
    assert_eq!(rx_bd_packet_len(9000, 0x0004, 0), None);
    assert_eq!(rx_bd_packet_len(64, 0x0004, 0), Some(64 - ETH_FCS_LEN));
    assert_eq!(
        rx_bd_packet_len(RX_LEN_MAX_HW as u32, 0x0004, 0),
        Some(FRAME_MAX)
    );
    assert_eq!(
        rx_bd_packet_len(FRAME_MAX as u32, 0x0004, 0),
        Some(FRAME_MAX - ETH_FCS_LEN)
    );
}

#[test]
fn parse_mocked_rx_bd_bytes_good_frame() {
    let mut raw = [0u8; 32];
    raw[8] = 64;
    raw[12] = 0x04;
    assert_eq!(parse_mocked_rx_bd_bytes(&raw), Some(64 - ETH_FCS_LEN));
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
            for length in [0u16, 64, 1514, 1518, 65535] {
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
    assert_eq!(parse_mocked_rx_bd_bytes(&raw), Some(128 - ETH_FCS_LEN));
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
    assert!(src.contains("station live LOM MAC"));
    assert!(src.contains("not APE SNP"));
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
    assert!(src.contains("inherit SNP analog"));
    assert!(src.contains("CORECLK_RESET for DMA"));
    assert!(src.contains("fn inherit_skips_chip_reset("));
    assert!(src.contains("HOSTCC_MODE_NOW"));
    assert!(src.contains("ETH_FCS_LEN"));
    assert!(src.contains("pre-reset bmsr"));
    assert!(src.contains("MII_TG3_MISC_SHDW"));
    assert!(src.contains("skip CORECLK_RESET"));
    assert!(src.contains("MII_TG3_MISC_SHDW_APD_ENABLE"));
    assert!(src.contains("fn inherit_snp_phy("));
    assert!(src.contains("reuse (armed post-EBS)"));
    assert!(src.contains("fn pick_bcm5720_try_order("));
    assert!(src.contains("fallback=func0 (NCSI/LOM1)"));
    assert!(src.contains("try next func"));
    assert!(src.contains("bmsr="));
    assert!(src.contains("fn skip_coreclk_reset("));
    assert!(src.contains("Follow Linux `tg3_chip_reset`"));
    assert!(src.contains("fn skip_bmcr_reset("));
    assert!(src.contains("fn ape_host_nophylock("));
    assert!(src.contains("fn keep_ape_phy_for_idrac("));
    assert!(src.contains("fn e3b_path_idrac_dedicated("));
    assert!(src.contains("Dedicated iDRAC NIC"));
    assert!(src.contains("keep-ape-phy=yes"));
    assert!(src.contains("keep APE PHY (iDRAC NCSI)"));
    assert!(src.contains("ape-nophylock="));
    assert!(src.contains("phy_reset=pre skip (ape-ncsi)"));
    assert!(src.contains("fn ape_ncsi_enabled("));
    assert!(src.contains("phy_reset=pre"));
    assert!(src.contains("an-restart="));
    assert!(src.contains("phy_setup=post"));
    assert!(src.contains("pci-restore="));
    assert!(src.contains("ape-grc="));
    assert!(src.contains("fn pci_cfg_save_dword_count("));
    assert!(src.contains("fn skip_http_listen_without_lstatus("));
    assert!(src.contains("fn peek_bcm5720_bmsr_pre_ebs("));
    assert!(src.contains("pre-EBS cand"));
    assert!(src.contains("tg3_reset_hw"));
    assert!(src.contains("tg3_setup_phy(false)"));
    assert!(src.contains("skip CORECLK_RESET (keep GPHY analog)"));
    assert!(src.contains("ape-ncsi="));
    assert!(src.contains("eee=off"));
    assert!(src.contains("TG3_CPMU_EEE_MODE"));
    assert!(src.contains("APE_FW_FEATURE_NCSI"));
    assert!(src.contains("CPMU_CTRL_LINK_SPEED_MODE"));
    assert!(src.contains("fn ape_phy_lock_num("));
    assert!(src.contains("fn ape_fw_ready("));
    assert!(src.contains("fn pci_mem_bar_addr("));
    assert!(src.contains("TG3_APE_PER_LOCK_REQ"));
    assert!(src.contains("TG3_APE_SEG_SIG"));
    assert!(src.contains("APE_HOST_BEHAV_NO_PHYLOCK"));
    assert!(src.contains("ape-bar="));
    assert!(src.contains("ape-fw="));
    assert!(src.contains("pci_ioremap_bar"));
    assert!(!src.contains("drivers/net/ethernet/broadcom/bnxt"));
}

#[test]
fn inherit_snp_phy_follows_bmsr_lstatus() {
    assert!(!inherit_snp_phy(0x7949));
    assert!(inherit_snp_phy(0x0004));
    assert!(inherit_snp_phy(0x794d));
    assert!(inherit_snp_phy(0x796d)); // iron COM2 live LOM :38
}

#[test]
fn inherit_skips_chip_reset_is_false() {
    assert!(!inherit_skips_chip_reset());
}

#[test]
fn skip_coreclk_reset_is_false_after_ncsi_skip_bmcr() {
    assert!(!skip_coreclk_reset());
}

#[test]
fn skip_bmcr_reset_when_ape_ncsi_keeps_idrac_phy() {
    assert!(keep_ape_phy_for_idrac());
    assert!(skip_bmcr_reset(true));
    assert!(!skip_bmcr_reset(false));
}

#[test]
fn ape_host_nophylock_is_true_to_keep_idrac() {
    assert!(ape_host_nophylock());
}

#[test]
fn pci_cfg_save_is_64_dwords() {
    assert_eq!(pci_cfg_save_dword_count(), 64);
}

#[test]
fn skip_listen_without_lstatus_is_true() {
    assert!(skip_http_listen_without_lstatus());
}

#[test]
fn ape_ncsi_enabled_reads_tg3_ape_fw_features_bit1() {
    const NCSI: u32 = 0x0000_0002;
    assert!(!ape_ncsi_enabled(0));
    assert!(ape_ncsi_enabled(NCSI));
    assert!(ape_ncsi_enabled(NCSI | 0x1));
    assert!(!ape_ncsi_enabled(0x1));
}

#[test]
fn iron_cpmu_4000_is_link_speed_not_idle() {
    assert!(cpmu_is_link_speed_mode(0x0000_4000));
    assert!(!cpmu_is_link_speed_mode(0x0000_0200));
    assert!(!cpmu_is_link_speed_mode(0));
}

#[test]
fn ape_phy_lock_num_matches_linux_func() {
    assert_eq!(ape_phy_lock_num(0), 0);
    assert_eq!(ape_phy_lock_num(1), 2);
    assert_eq!(ape_phy_lock_num(2), 3);
    assert_eq!(ape_phy_lock_num(3), 5);
}

#[test]
fn ape_per_lock_regs_are_5717_plus() {
    assert_eq!(ape_per_lock_req(2), 0x8408);
    assert_eq!(ape_per_lock_grant(2), 0x8428);
    assert_eq!(ape_lock_req_bit(2, 1), 0x1000);
    assert_eq!(ape_lock_req_bit(4, 1), 1 << 1);
    assert_eq!(ape_lock_req_bit(4, 0), 0x1000);
    assert_eq!(ape_lock_init_grant_bit(0, 1), 0x1000);
    assert_eq!(ape_lock_init_grant_bit(1, 1), 1 << 1);
    assert_eq!(ape_lock_init_grant_bit(7, 1), 1 << 1);
}

#[test]
fn ape_fw_ready_needs_magic_and_ready_bit() {
    assert!(ape_fw_ready(0x4150_4521, 0x100));
    assert!(!ape_fw_ready(0, 0x100));
    assert!(!ape_fw_ready(0x4150_4521, 0));
}

#[test]
fn pci_mem_bar_addr_32_and_64() {
    assert_eq!(pci_mem_bar_addr(1, 0), 0);
    assert_eq!(pci_mem_bar_addr(0x9290_0000, 0), 0x9290_0000);
    assert_eq!(pci_mem_bar_addr(0x9290_0004, 0), 0x9290_0000);
    assert_eq!(pci_mem_bar_addr(0x0000_0004, 0x0000_0001), 0x1_0000_0000);
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
