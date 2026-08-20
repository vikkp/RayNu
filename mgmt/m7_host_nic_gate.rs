//! M7.8 host-owned NIC scaffold / package gate (ADR-013 Phase 0 / C / D / E).
//!
//! Pillar: [Z]
//! Proven Core: **outside**
//!
//! Host/CI proves wiring. Does **not** print
//! `RAYNU-V-M7-HOST-NIC-HTTP-OK` (iron Phase D) or claim QEMU GET / from this
//! path (`RAYNU-V-M7-HOST-NIC-QEMU-OK` is firmware runtime only).

use super::e1000_mmio::{pci_id_is_qemu_e1000, E1000_DEVICE, E1000_VENDOR};
use super::host_nic::{
    M7_HOST_NIC_HTTP_OK_MARKER, M7_HOST_NIC_QEMU_MARKER, M7_HOST_NIC_SCAFFOLD_MARKER,
};
use super::host_nic_poll::prop_bounded_poll_respects_budget;
use super::mgmt_arena::prop_arena_reset_rewinds;
use super::pci_census::{
    census_nic_has_iron_driver, census_nic_has_lab_driver, pci_id_is_iron_census,
};

/// Host / CI marker when the M7.8 scaffold package passes.
pub const M7_HOST_NIC_GATE_MARKER: &str = M7_HOST_NIC_SCAFFOLD_MARKER;

pub fn host_nic_surface_present() -> bool {
    let mmio = include_str!("e1000_mmio.rs");
    let phy = include_str!("e1000.rs");
    let bcm = include_str!("bcm5720_mmio.rs");
    let bcm_phy = include_str!("bcm5720.rs");
    let listen = include_str!("host_nic_listen.rs");
    let http = include_str!("http_listen.rs");
    let census = include_str!("pci_census.rs");
    let arena = include_str!("mgmt_arena.rs");
    let main = include_str!("../src/main.rs");
    let idle = include_str!("snp_listen_uefi.rs");
    mmio.contains("All PCI config")
        && mmio.contains("8086:100e")
        && mmio.contains("fn pci_id_is_qemu_e1000(")
        && mmio.contains("fn parse_mocked_rx_desc_bytes(")
        && mmio.contains("// SAFETY:")
        && mmio.contains("KANI-TARGET")
        && phy.contains("impl Device for E1000Device")
        && phy.contains("e1000_mmio")
        && phy.contains("smoltcp::phy::Device")
        && census.contains("fn run_pre_ebs_pci_census(")
        && census.contains("vid:did=")
        && census.contains("IOMMU ACPI DMAR=")
        && census.contains("fn iron_marker_allowed(")
        && census.contains("fn pci_id_is_iron_census(")
        && census.contains("fn census_nic_has_iron_driver(")
        && census.contains("14e4:165f")
        && bcm.contains("fn parse_mocked_rx_bd_bytes(")
        && bcm.contains("reuse (armed post-EBS)")
        && bcm.contains("fn pick_bcm5720_try_order(")
        && bcm.contains("fallback=func0 (NCSI/LOM1)")
        && bcm.contains("try next func")
        && bcm.contains("fn pick_bcm5720_pci(")
        && bcm.contains("fn station_mac(")
        && bcm.contains("station live LOM MAC")
        && bcm.contains("BMSR_LSTATUS")
        && bcm.contains("RX_MODE_PROMISC")
        && bcm.contains("link=up")
        && bcm.contains("fn decode_phy_link(")
        && bcm.contains("fn mac_mode_from_link(")
        && bcm.contains("BMCR_RESET")
        && bcm.contains("MII_TG3_AUXCTL_SHDWSEL_PWRCTL")
        && bcm.contains("MII_TG3_AUXCTL_MISC_FORCE_AMDIX")
        && bcm.contains("CPMU_CTRL_LINK_IDLE_MODE")
        && bcm.contains("fn phy_addr_5717_plus(")
        && bcm.contains("fn inherit_snp_phy(")
        && bcm.contains("fn inherit_skips_chip_reset(")
        && bcm.contains("fn skip_coreclk_reset(")
        && bcm.contains("fn skip_bmcr_reset(")
        && bcm.contains("fn ape_host_nophylock(")
        && bcm.contains("fn keep_ape_phy_for_idrac(")
        && bcm.contains("fn e3b_path_idrac_dedicated(")
        && bcm.contains("Dedicated iDRAC NIC")
        && bcm.contains("keep APE PHY (iDRAC NCSI)")
        && bcm.contains("ape-nophylock=")
        && bcm.contains("fn ape_ncsi_enabled(")
        && bcm.contains("phy_reset=pre")
        && bcm.contains("phy_reset=pre skip (ape-ncsi)")
        && bcm.contains("an-restart=")
        && bcm.contains("phy_setup=post")
        && bcm.contains("pci-restore=")
        && bcm.contains("ape-grc=")
        && bcm.contains("fn pci_cfg_save_dword_count(")
        && bcm.contains("fn skip_http_listen_without_lstatus(")
        && bcm.contains("fn peek_bcm5720_bmsr_pre_ebs(")
        && bcm.contains("pre-EBS cand")
        && bcm.contains("tg3_reset_hw")
        && bcm.contains("tg3_setup_phy(false)")
        && bcm.contains("skip CORECLK_RESET (keep GPHY analog)")
        && bcm.contains("ape-ncsi=")
        && bcm.contains("eee=off")
        && bcm.contains("CPMU_CTRL_LINK_SPEED_MODE")
        && bcm.contains("fn ape_phy_lock_num(")
        && bcm.contains("fn ape_fw_ready(")
        && bcm.contains("fn pci_mem_bar_addr(")
        && bcm.contains("TG3_APE_PER_LOCK_REQ")
        && bcm.contains("TG3_APE_SEG_SIG")
        && bcm.contains("APE_HOST_BEHAV_NO_PHYLOCK")
        && bcm.contains("ape-bar=")
        && bcm.contains("ape-fw=")
        && bcm.contains("inherit SNP analog")
        && bcm.contains("CORECLK_RESET for DMA")
        && bcm.contains("HOSTCC_MODE_NOW")
        && bcm.contains("ETH_FCS_LEN")
        && bcm.contains("fn ring_idx(")
        && bcm.contains("fn rx_return_pending(")
        && bcm.contains("RING_MASK")
        && bcm.contains("fn grc_mode_le_host(")
        && bcm.contains("GRC_MODE_BSWAP_DATA")
        && bcm.contains("grc=bswap+wswap")
        && bcm.contains("fn eth_header_view(")
        && bcm.contains("fn dump_first_rx(")
        && bcm.contains("HOST-NIC BCM5720 rx to=")
        && bcm_phy.contains("Checksum::Tx")
        && bcm_phy.contains("impl Device for Bcm5720Device")
        && bcm.contains("inherit SNP PHY")
        && bcm.contains("pre-reset bmsr")
        && bcm.contains("MII_TG3_MISC_SHDW")
        && bcm.contains("skip CORECLK_RESET")
        && bcm.contains("fn bcm5720_present(")
        && bcm.contains("// SAFETY:")
        && bcm.contains("KANI-TARGET")
        && bcm.contains("matched SNP lease")
        && bcm.contains("Linux `tg3`")
        && bcm.contains("**not** `bnxt`")
        && bcm.contains("GRC_MISC_CFG_PRESERVE_PCIE")
        && bcm.contains("RDMAC_MODE")
        && bcm.contains("WDMAC_MODE")
        && bcm.contains("5717_PLUS")
        && bcm.contains("RCVDBDI_MODE_INV_RING_SZ")
        && bcm.contains("RCVBDI_STD_THRESH")
        && bcm.contains("phy_addr = u32::from(func) + 1")
        && bcm.contains("DEFAULT_MB_MACRX_LOW_WATER_57765")
        && bcm.contains("do not program RCVDBDI_STD_BD+NIC_ADDR")
        && arena.contains("enum MgmtFatal")
        && arena.contains("fn inject_mgmt_fatals(")
        && listen.contains("fn bringup_bcm5720_post_ebs(")
        && listen.contains("post-EBS bring-up (keep analog before guest path)")
        && listen.contains("fn run_post_ebs_host_nic_listen(")
        && listen.contains("fn run_post_boot_ok_native_idle(")
        && listen.contains("fn listen_bcm5720(")
        && listen.contains("skip listen (no LSTATUS; do not curl)")
        && listen.contains("Bcm5720Device")
        && listen.contains("lease.mac")
        && listen.contains("post-EBS ")
        && listen.contains("M7_HOST_NIC_QEMU_MARKER")
        && listen.contains("MgmtFatal")
        && listen.contains("MgmtArena")
        && !listen.contains("CURL NOW (post-EBS)")
        && !listen.contains("SnpDevice")
        && listen.contains("iface.poll")
        && listen.contains("bounded_poll")
        && listen.contains("handle_http_request")
        && listen.contains("rx_prod=")
        && listen.contains("tx_prod=")
        && listen.contains("fn print_bcm5720_poll_diag(")
        && listen.contains("COM2 idle after")
        && listen.contains("rx_drop rose")
        && http.contains("run_post_ebs_host_nic_listen")
        && http.contains("run_pre_ebs_pci_census")
        && http.contains("run_post_boot_ok_native_idle")
        && main.contains("probe_host_nic_lab_flag")
        && main.contains("run_post_ebs_mgmt_listen")
        && idle.contains("firmware SNP dead after EBS")
        && idle.contains("peek_bcm5720_bmsr_pre_ebs")
        && pci_id_is_qemu_e1000(E1000_VENDOR, E1000_DEVICE)
        && census_nic_has_lab_driver(E1000_VENDOR, E1000_DEVICE)
        && !census_nic_has_lab_driver(0x14e4, 0x165f)
        && pci_id_is_iron_census(0x14e4, 0x165f)
        && !pci_id_is_iron_census(E1000_VENDOR, E1000_DEVICE)
        && census_nic_has_iron_driver(0x14e4, 0x165f)
        && !census_nic_has_iron_driver(E1000_VENDOR, E1000_DEVICE)
}

/// Native listen must not print the iron Phase D marker.
pub fn host_nic_listen_does_not_claim_iron() -> bool {
    let listen = include_str!("host_nic_listen.rs");
    !listen.contains(M7_HOST_NIC_HTTP_OK_MARKER)
}

pub fn host_nic_scripts_present() -> bool {
    let smoke = include_str!("../tools/m7-host-nic-smoke.sh");
    let qemu = include_str!("../tools/m7-host-nic-qemu-smoke.sh");
    let miri = include_str!("../tools/host-nic-miri-smoke.sh");
    let runbook = include_str!("../docs/runbooks/mgmt_http.md");
    smoke.contains(M7_HOST_NIC_SCAFFOLD_MARKER)
        && smoke.contains("m7_8_host_nic_scaffold_passes")
        && smoke.contains(M7_HOST_NIC_HTTP_OK_MARKER)
        && smoke.contains("never print iron")
        && smoke.contains("pci_census.rs")
        && smoke.contains("mgmt_arena.rs")
        && smoke.contains("bcm5720_mmio.rs")
        && qemu.contains(M7_HOST_NIC_QEMU_MARKER)
        && qemu.contains("hostnic.txt")
        && qemu.contains("e1000")
        && qemu.contains("never print iron")
        && qemu.contains("vid:did=8086:100e")
        && qemu.contains("PCI census")
        && miri.contains("parse_mocked_rx_desc")
        && miri.contains("parse_mocked_rx_bd")
        && miri.contains("skip")
        && runbook.contains("ADR-013")
        && runbook.contains("NIC Selection")
        && runbook.contains("not iDRAC dedicated")
        && runbook.contains("r640_idrac_dedicated.md")
        && runbook.contains("Phase C")
        && runbook.contains("Phase 0")
        && runbook.contains("Phase D")
        && runbook.contains("Phase E")
        && runbook.contains(M7_HOST_NIC_QEMU_MARKER)
        && runbook.contains(M7_HOST_NIC_SCAFFOLD_MARKER)
        && runbook.contains("8086:100e")
        && runbook.contains("14e4:165f")
}

pub fn prop_host_nic_scaffold_package() -> bool {
    host_nic_surface_present()
        && host_nic_listen_does_not_claim_iron()
        && host_nic_scripts_present()
        && prop_bounded_poll_respects_budget()
        && prop_arena_reset_rewinds()
}

pub fn run_m7_host_nic_scaffold_gate() -> bool {
    prop_host_nic_scaffold_package()
}

#[cfg(test)]
#[path = "m7_host_nic_gate_test.rs"]
mod m7_host_nic_gate_test;
