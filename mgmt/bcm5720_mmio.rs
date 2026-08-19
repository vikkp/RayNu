//! BCM5720 (`14e4:165f`) MMIO/DMA — iron NIC `unsafe` module (ADR-013 Phase D).
//!
//! Pillar: [Z] [D]
//! Proven Core: **outside** (ADR-002 / ADR-013)
//!
//! All BAR0 MMIO, APE BAR2 (Linux `pci_ioremap_bar(..., BAR_2)`), mailbox,
//! SRAM window, and RX/TX ring DMA for the R640
//! census NIC live here. Bind **one** function: exact MAC match to the
//! parked SNP lease (R640: SNP is `01:00.1` / `:5a:3a`, not func 0).
//! If no MAC match: prefer `func == 1`, then `func == 0`. Do not start X710/i40e.
//!
//! Register map and bring-up: Broadcom Tigon3 / **Linux `tg3`**
//! (`drivers/net/ethernet/broadcom/tg3.c`) and BCM571X/BCM5720 Programmer’s
//! Guide ch. 7. BCM5720 (`14e4:165f`) is **not** `bnxt` — that driver is
//! BCM57416 10G. PRE-EBS SNP already has copper; peek `BMSR` **before**
//! `CORECLK_RESET` and inherit that link when `LSTATUS` is set (do not
//! BMCR_RESET a live PHY). Iron 2026-08-19: `phy_reset=pre` then
//! `CORECLK_RESET` still `bmsr=7949` / `lpa=0000` with working MDIO
//! (`ape-lock=yes id=0362:5f60`). **Skip `CORECLK_RESET`** — steal rings only;
//! BMCR_RESET the GPHY while MAC clocks stay firmware-owned. Clear CPMU EEE
//! LPI. Print APE `NCSI` from `APE_FW_FEATURES`. Iron 2026-08-19 skip-reset
//! EFI still `pre-reset bmsr=7949` / `lpa=0000` on func 1 with `ape-ncsi=yes`
//! after PRE-EBS SNP DHCP on `:3a`. Peek each candidate `BMSR` and **try
//! func 0 (NCSI/LOM1) then func 1** until `LSTATUS`; station stays the SNP
//! lease MAC. Wait `BMSR_LSTATUS` and set
//! `MAC_MODE` MII vs GMII (`tg3_adjust_link`). `RX_MODE_PROMISC` is on.
//! Poll-mode; MSI-X is not enabled (ADR-013). Host tests parse mocked RX BDs
//! only.

use crate::mgmt::pci_census::{pci_id_is_iron_census, IRON_CENSUS_DEVICE, IRON_CENSUS_VENDOR};
use core::sync::atomic::{AtomicBool, Ordering};

#[cfg(feature = "uefi-bin")]
use core::sync::atomic::{AtomicU32, AtomicU64};

#[cfg(feature = "uefi-bin")]
use crate::mgmt::e1000_mmio::{pci_read32, pci_write32};

pub const BCM5720_VENDOR: u16 = IRON_CENSUS_VENDOR;
pub const BCM5720_DEVICE: u16 = IRON_CENSUS_DEVICE;

pub const RING: usize = 32;
pub const PKT: usize = 2048;
pub const FRAME_MAX: usize = 1514;

const RXD_FLAG_END: u32 = 0x0004;
const RXD_FLAG_ERROR: u32 = 0x0400;
const RXD_ERR_MASK: u32 = 0xFFFF_0000;
const RXD_OPAQUE_RING_STD: u32 = 0x0001_0000;
const TXD_FLAG_END: u32 = 0x0004;
const TXD_LEN_SHIFT: u32 = 16;

const PCI_CMD_MEM: u16 = 1 << 1;
const PCI_CMD_BUSMASTER: u16 = 1 << 2;

const TG3PCI_MISC_HOST_CTRL: u32 = 0x68;
const MISC_HOST_CTRL_CLEAR_INT: u32 = 1 << 0;
const MISC_HOST_CTRL_MASK_PCI_INT: u32 = 1 << 1;
const MISC_HOST_CTRL_WORD_SWAP: u32 = 1 << 3;
const MISC_HOST_CTRL_INDIR_ACCESS: u32 = 1 << 7;
const MISC_HOST_CTRL_IRQ_MASK_MODE: u32 = 1 << 8;
const MISC_HOST_CTRL_CHIPREV: u32 = 0xFFFF_0000;

const TG3PCI_MEM_WIN_BASE_ADDR: u8 = 0x7C;
const TG3PCI_MEM_WIN_DATA: u8 = 0x84;

const TG3_64BIT_REG_LOW: u32 = 0x04;

const MAILBOX_RCV_STD_PROD_IDX: u32 = 0x268;
const MAILBOX_RCVRET_CON_IDX_0: u32 = 0x280;
const MAILBOX_SNDHOST_PROD_IDX_0: u32 = 0x300;

const MAC_MODE: u32 = 0x400;
const MAC_STATUS: u32 = 0x404;
const MAC_EVENT: u32 = 0x408;
const MAC_LED_CTRL: u32 = 0x40C;
const MAC_ADDR_0_HIGH: u32 = 0x410;
const MAC_ADDR_0_LOW: u32 = 0x414;
const MAC_RX_MTU_SIZE: u32 = 0x43C;
const MAC_MI_COM: u32 = 0x44C;
const MAC_MI_STAT: u32 = 0x450;
const MAC_MI_MODE: u32 = 0x454;
const MAC_TX_MODE: u32 = 0x45C;
const MAC_TX_LENGTHS: u32 = 0x464;
const MAC_RX_MODE: u32 = 0x468;
const MAC_HASH_REG_0: u32 = 0x470;
const MAC_RCV_RULE_CFG: u32 = 0x500;
const MAC_LOW_WMARK_MAX_RX_FRAME: u32 = 0x504;
const MAC_MODE_PORT_MODE_MASK: u32 = 0x0c;
const MAC_MODE_PORT_MODE_MII: u32 = 0x04;
const MAC_MODE_PORT_MODE_GMII: u32 = 0x08;
const MAC_MODE_HALF_DUPLEX: u32 = 0x02;
const MAC_MODE_RXSTAT_ENABLE: u32 = 0x800;
const MAC_MODE_TXSTAT_ENABLE: u32 = 0x4000;
const MAC_MODE_TDE_ENABLE: u32 = 0x20_0000;
const MAC_MODE_RDE_ENABLE: u32 = 0x40_0000;
const MAC_MODE_FHDE_ENABLE: u32 = 0x80_0000;
const MAC_EVENT_LNKSTATE_CHANGED: u32 = 0x1000;
const LED_CTRL_MODE_PHY_1: u32 = 0x800;
const TX_MODE_ENABLE: u32 = 0x02;
const TX_MODE_JMB_FRM_LEN: u32 = 0x40_0000;
const TX_MODE_CNT_DN_MODE: u32 = 0x80_0000;
const RX_MODE_ENABLE: u32 = 0x02;
const RX_MODE_PROMISC: u32 = 0x100;
const MI_COM_CMD_WRITE: u32 = 0x0400_0000;
const MI_COM_CMD_READ: u32 = 0x0800_0000;
const MI_COM_START: u32 = 0x2000_0000;
const MI_COM_BUSY: u32 = 0x2000_0000;
const MI_COM_PHY_ADDR_SHIFT: u32 = 21;
const MI_COM_REG_ADDR_SHIFT: u32 = 16;
const MAC_MI_MODE_BASE: u32 = 0x000C_0000;
const MAC_MI_MODE_500KHZ_CONST: u32 = 0x8000;
const MAC_MI_STAT_LNKSTAT_ATTN_ENAB: u32 = 0x01;
const TX_LENGTHS_DEFAULT: u32 = 0x2620;
const TX_LENGTHS_JMB_FRM_LEN_MSK: u32 = 0x00FF_0000;
const TX_LENGTHS_CNT_DWN_VAL_MSK: u32 = 0xFF00_0000;
const RCV_RULE_CFG_DEFAULT_CLASS: u32 = 0x08;
const RCVLPC_CONFIG_DEFAULT: u32 = 0x181;

const MII_BMCR: u32 = 0;
const MII_BMSR: u32 = 1;
const MII_PHYSID1: u32 = 2;
const MII_PHYSID2: u32 = 3;
const MII_ADVERTISE: u32 = 4;
const MII_LPA: u32 = 5;
const MII_CTRL1000: u32 = 9;
const MII_STAT1000: u32 = 10;
const BMCR_RESET: u16 = 0x8000;
const BMCR_ANRESTART: u16 = 0x0200;
const BMCR_ANENABLE: u16 = 0x1000;
const BMSR_LSTATUS: u16 = 0x0004;
const BMSR_ANEGCOMPLETE: u16 = 0x0020;
const ADVERTISE_COPPER: u16 = 0x0DE1;
const ADVERTISE_1000: u16 = 0x0300;
const MII_TG3_AUX_CTRL: u32 = 0x18;
const MII_TG3_AUXCTL_SHDWSEL_PWRCTL: u16 = 0x0002;
const MII_TG3_AUXCTL_SHDWSEL_MISC: u16 = 0x0007;
const MII_TG3_AUXCTL_MISC_WIRESPD_EN: u16 = 0x0010;
const MII_TG3_AUXCTL_MISC_FORCE_AMDIX: u16 = 0x0200;
const MII_TG3_AUXCTL_MISC_WREN: u16 = 0x8000;
const MII_TG3_AUXCTL_MISC_RDSEL_SHIFT: u16 = 12;
const TG3_CPMU_CTRL: u32 = 0x3600;
const CPMU_CTRL_LINK_IDLE_MODE: u32 = 0x200;
const CPMU_CTRL_LINK_AWARE_MODE: u32 = 0x400;
/// Iron `cpmu=00004000` — not idle. Leave this bit; do not treat as APD.
const CPMU_CTRL_LINK_SPEED_MODE: u32 = 0x4000;
const CPMU_CTRL_GPHY_10MB_RXONLY: u32 = 0x1_0000;
const TG3_CPMU_EEE_MODE: u32 = 0x36B0;
const TG3_CPMU_EEEMD_LPI_ENABLE: u32 = 0x80;
const SG_DIG_STATUS: u32 = 0x5B4;
const SG_DIG_IS_SERDES: u32 = 0x100;
const LPA_1000FULL: u16 = 0x0800;
const LPA_1000HALF: u16 = 0x0400;
const LPA_100FULL: u16 = 0x0100;
const LPA_100HALF: u16 = 0x0080;
const LPA_10FULL: u16 = 0x0040;

const SNDDATAC_MODE: u32 = 0x1000;
const SNDDATAI_MODE: u32 = 0x0C00;
const SNDBDS_MODE: u32 = 0x1400;
const SNDBDI_MODE: u32 = 0x1800;
const SNDBDC_MODE: u32 = 0x1C00;
const RCVLPC_MODE: u32 = 0x2000;
const RCVLPC_CONFIG: u32 = 0x2010;
const RCVDBDI_MODE: u32 = 0x2400;
const RCVDBDI_STD_BD: u32 = 0x2450;
const RCVDCC_MODE: u32 = 0x2800;
const RCVBDI_MODE: u32 = 0x2C00;
const RCVBDI_STD_THRESH: u32 = 0x2C18;
const RCVCC_MODE: u32 = 0x3000;
const RCVLSC_MODE: u32 = 0x3400;
const HOSTCC_MODE: u32 = 0x3C00;
const HOSTCC_RXCOL_TICKS: u32 = 0x3C08;
const HOSTCC_TXCOL_TICKS: u32 = 0x3C0C;
const HOSTCC_RXMAX_FRAMES: u32 = 0x3C10;
const HOSTCC_TXMAX_FRAMES: u32 = 0x3C14;
const HOSTCC_STAT_COAL_TICKS: u32 = 0x3C28;
const HOSTCC_STATUS_BLK_HOST_ADDR: u32 = 0x3C38;
const BUFMGR_MODE: u32 = 0x4400;
const BUFMGR_MB_RDMA_LOW_WATER: u32 = 0x4410;
const BUFMGR_MB_MACRX_LOW_WATER: u32 = 0x4414;
const BUFMGR_MB_HIGH_WATER: u32 = 0x4418;
const BUFMGR_DMA_LOW_WATER: u32 = 0x4434;
const BUFMGR_DMA_HIGH_WATER: u32 = 0x4438;
const RDMAC_MODE: u32 = 0x4800;
const WDMAC_MODE: u32 = 0x4C00;
const MEMARB_MODE: u32 = 0x4000;
const FTQ_RESET: u32 = 0x5C00;
const GRC_MODE: u32 = 0x6800;
const GRC_MISC_CFG: u32 = 0x6804;
const GRC_FASTBOOT_PC: u32 = 0x6894;

const MODE_ENABLE: u32 = 0x02;
const GRC_MODE_WSWAP_NONFRM_DATA: u32 = 0x04;
const GRC_MODE_WSWAP_DATA: u32 = 0x20;
const GRC_MODE_NOIRQ_ON_SENDS: u32 = 0x2000;
const GRC_MODE_NOIRQ_ON_RCV: u32 = 0x4000;
const GRC_MODE_HOST_STACKUP: u32 = 0x1_0000;
const GRC_MODE_HOST_SENDBDS: u32 = 0x2_0000;
const GRC_MODE_NO_TX_PHDR_CSUM: u32 = 0x10_0000;
const GRC_MODE_NO_RX_PHDR_CSUM: u32 = 0x80_0000;
const GRC_MISC_CFG_CORECLK_RESET: u32 = 0x01;
const GRC_MISC_CFG_PRESCALAR_SHIFT: u32 = 1;
/// Linux `tg3_chip_reset`: PCIe chips write bit 29 so CORECLK_RESET does not
/// drop the shared PCIe core (GRC bit 0 resets the device core only).
const GRC_MISC_CFG_PRESERVE_PCIE: u32 = 1 << 29;
const DEFAULT_MB_RDMA_LOW_WATER_5705: u32 = 0;
const DEFAULT_MB_MACRX_LOW_WATER_57765: u32 = 0x2A;
const DEFAULT_MB_HIGH_WATER_57765: u32 = 0xA0;
const DEFAULT_DMA_LOW_WATER: u32 = 0x05;
const DEFAULT_DMA_HIGH_WATER: u32 = 0x0A;
const TG3_RX_STD_DMA_SZ: u32 = 1536;
const RCVDBDI_MODE_INV_RING_SZ: u32 = 0x10;
const RDMAC_MODE_ENABLE: u32 = 0x02;
const RDMAC_MODE_TGTABORT_ENAB: u32 = 0x04;
const RDMAC_MODE_MSTABORT_ENAB: u32 = 0x08;
const RDMAC_MODE_PARITYERR_ENAB: u32 = 0x10;
const RDMAC_MODE_ADDROFLOW_ENAB: u32 = 0x20;
const RDMAC_MODE_FIFOOFLOW_ENAB: u32 = 0x40;
const RDMAC_MODE_FIFOURUN_ENAB: u32 = 0x80;
const RDMAC_MODE_FIFOOREAD_ENAB: u32 = 0x100;
const RDMAC_MODE_LNGREAD_ENAB: u32 = 0x200;
const RDMAC_MODE_FIFO_LONG_BURST: u32 = 0x0003_0000;
const WDMAC_MODE_ENABLE: u32 = 0x02;
const WDMAC_MODE_TGTABORT_ENAB: u32 = 0x04;
const WDMAC_MODE_MSTABORT_ENAB: u32 = 0x08;
const WDMAC_MODE_PARITYERR_ENAB: u32 = 0x10;
const WDMAC_MODE_ADDROFLOW_ENAB: u32 = 0x20;
const WDMAC_MODE_FIFOOFLOW_ENAB: u32 = 0x40;
const WDMAC_MODE_FIFOURUN_ENAB: u32 = 0x80;
const WDMAC_MODE_FIFOOREAD_ENAB: u32 = 0x100;
const WDMAC_MODE_LNGREAD_ENAB: u32 = 0x200;
const WDMAC_MODE_STATUS_TAG_FIX: u32 = 0x2000_0000;
const SNDDATAC_MODE_ENABLE: u32 = 0x02;
const SNDBDC_MODE_ENABLE: u32 = 0x02;
const SNDBDC_MODE_ATTN_ENABLE: u32 = 0x04;
const RCVDCC_MODE_ENABLE: u32 = 0x02;
const RCVDCC_MODE_ATTN_ENABLE: u32 = 0x04;
const RCVBDI_MODE_ENABLE: u32 = 0x02;
const RCVBDI_MODE_RCB_ATTN_ENAB: u32 = 0x04;

const NIC_SRAM_SEND_RCB: u32 = 0x100;
const NIC_SRAM_RCV_RET_RCB: u32 = 0x200;
const NIC_SRAM_STATS_BLK: u32 = 0x300;
const NIC_SRAM_FIRMWARE_MBOX: u32 = 0xB50;
const NIC_SRAM_DATA_CFG: u32 = 0xB58;
const NIC_SRAM_DATA_CFG_APE_ENABLE: u32 = 0x20_0000;
/// Linux `pci_ioremap_bar(pdev, BAR_2)` — APE, not BAR0. Config offset 0x18.
const PCI_BAR2_OFF: u8 = 0x18;
const TG3_APE_EVENT: u32 = 0x000C;
const APE_EVENT_1: u32 = 0x0000_0001;
/// Linux `tg3.h`: APE shared memory (same BAR as PER_LOCK on 5717+).
const TG3_APE_SEG_SIG: u32 = 0x4000;
const APE_SEG_SIG_MAGIC: u32 = 0x4150_4521;
const TG3_APE_FW_STATUS: u32 = 0x400C;
const APE_FW_STATUS_READY: u32 = 0x0000_0100;
const TG3_APE_FW_FEATURES: u32 = 0x4010;
const APE_FW_FEATURE_NCSI: u32 = 0x0000_0002;
const TG3_APE_PER_LOCK_REQ: u32 = 0x8400;
const TG3_APE_PER_LOCK_GRANT: u32 = 0x8420;
const APE_LOCK_REQ_DRIVER: u32 = 0x0000_1000;
const APE_LOCK_GRANT_DRIVER: u32 = 0x0000_1000;
const TG3_APE_LOCK_PHY0: u32 = 0;
const TG3_APE_LOCK_GRC: u32 = 1;
const TG3_APE_LOCK_PHY1: u32 = 2;
const TG3_APE_LOCK_PHY2: u32 = 3;
const TG3_APE_LOCK_MEM: u32 = 4;
const TG3_APE_LOCK_PHY3: u32 = 5;
const TG3_APE_LOCK_GPIO: u32 = 7;
const TG3_APE_HOST_SEG_SIG: u32 = 0x4200;
const APE_HOST_SEG_SIG_MAGIC: u32 = 0x484F_5354;
const TG3_APE_HOST_SEG_LEN: u32 = 0x4204;
const APE_HOST_SEG_LEN_MAGIC: u32 = 0x20;
const TG3_APE_HOST_INIT_COUNT: u32 = 0x4208;
const TG3_APE_HOST_DRIVER_ID: u32 = 0x420C;
const APE_HOST_DRIVER_ID_LINUX: u32 = 0xF000_0000;
const TG3_APE_HOST_BEHAVIOR: u32 = 0x4210;
const APE_HOST_BEHAV_NO_PHYLOCK: u32 = 0x0000_0001;
const TG3_APE_HOST_HEARTBEAT_INT_MS: u32 = 0x4214;
const APE_HOST_HEARTBEAT_INT_DISABLE: u32 = 0;
const TG3_APE_HOST_HEARTBEAT_COUNT: u32 = 0x4218;
const TG3_APE_HOST_DRVR_STATE: u32 = 0x421C;
const TG3_APE_HOST_DRVR_STATE_START: u32 = 0x0000_0001;
const TG3_APE_EVENT_STATUS: u32 = 0x4300;
const APE_EVENT_STATUS_DRIVER_EVNT: u32 = 0x0000_0010;
const APE_EVENT_STATUS_STATE_CHNGE: u32 = 0x0000_0500;
const APE_EVENT_STATUS_STATE_START: u32 = 0x0001_0000;
const APE_EVENT_STATUS_EVENT_PENDING: u32 = 0x8000_0000;
const MII_TG3_MISC_SHDW: u32 = 0x1C;
const MII_TG3_MISC_SHDW_WREN: u16 = 0x8000;
const MII_TG3_MISC_SHDW_APD_WKTM_84MS: u16 = 0x0001;
/// Linux `tg3_phy_toggle_apd(true)` only. Disable path must not set this.
#[allow(dead_code)]
const MII_TG3_MISC_SHDW_APD_ENABLE: u16 = 0x0100;
const MII_TG3_MISC_SHDW_APD_SEL: u16 = 0x2800;
const MII_TG3_MISC_SHDW_SCR5_C125OE: u16 = 0x0001;
const MII_TG3_MISC_SHDW_SCR5_DLLAPD: u16 = 0x0002;
const MII_TG3_MISC_SHDW_SCR5_SDTL: u16 = 0x0004;
const MII_TG3_MISC_SHDW_SCR5_DLPTLM: u16 = 0x0008;
const MII_TG3_MISC_SHDW_SCR5_LPED: u16 = 0x0010;
const MII_TG3_MISC_SHDW_SCR5_SEL: u16 = 0x1400;
const NIC_SRAM_FIRMWARE_MBOX_MAGIC1: u32 = 0x4B65_7654;
const NIC_SRAM_TX_BUFFER_DESC: u32 = 0x4000;
/// Firmware-owned on 5717_PLUS / BCM5720. Linux `tg3` writes this to
/// `RCVDBDI_STD_BD+NIC_ADDR` only when `!5717_PLUS`. Do not program 0x245C.
#[allow(dead_code)]
const NIC_SRAM_RX_BUFFER_DESC: u32 = 0x6000;

const TG3_BDINFO_HOST_ADDR: u32 = 0x00;
const TG3_BDINFO_MAXLEN_FLAGS: u32 = 0x08;
const TG3_BDINFO_NIC_ADDR: u32 = 0x0C;
const TG3_BDINFO_SIZE: u32 = 0x10;
const BDINFO_FLAGS_DISABLED: u32 = 0x02;
const BDINFO_FLAGS_MAXLEN_SHIFT: u32 = 16;
const TG3_HW_STATUS_SIZE: usize = 0x50;

/// True when PCI id is the Phase 0 iron pick (BCM5720).
pub fn pci_id_is_bcm5720(vendor: u16, device: u16) -> bool {
    pci_id_is_iron_census(vendor, device)
}

/// Why [`pick_bcm5720_pci`] / [`pick_bcm5720_try_order`] chose a BDF.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bcm5720PickReason {
    /// BAR0 MAC equals the parked SNP lease MAC.
    MacMatch,
    /// Candidate `BMSR_LSTATUS` set at scan (cable on this function).
    PreferLink,
    /// No MAC match / no live link; try function 0 first (Dell NCSI/LOM1).
    PreferFunc0,
    /// Retry (or sole remaining) function 1.
    PreferFunc1,
    /// No MAC match and neither function 0 nor 1; first candidate.
    FirstCand,
}

impl Bcm5720PickReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MacMatch => "matched SNP lease",
            Self::PreferLink => "link-up BMSR",
            Self::PreferFunc0 => "fallback=func0 (NCSI/LOM1)",
            Self::PreferFunc1 => "fallback=func1 (retry)",
            Self::FirstCand => "fallback=first (no MAC match)",
        }
    }
}

/// Chosen BCM5720 PCI function (host-testable picker).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bcm5720PciPick {
    pub bus: u8,
    pub dev: u8,
    pub func: u8,
    pub reason: Bcm5720PickReason,
}

pub const BCM5720_MAX_CAND: usize = 4;

fn pick_push(
    out: &mut [Bcm5720PciPick; BCM5720_MAX_CAND],
    n: &mut usize,
    bus: u8,
    dev: u8,
    func: u8,
    reason: Bcm5720PickReason,
) {
    if *n >= BCM5720_MAX_CAND {
        return;
    }
    for p in out.iter().take(*n) {
        if p.bus == bus && p.dev == dev && p.func == func {
            return;
        }
    }
    out[*n] = Bcm5720PciPick {
        bus,
        dev,
        func,
        reason,
    };
    *n += 1;
}

/// Bind order for dual-port BCM5720 (host-testable; no MMIO).
///
/// INVARIANTS:
/// - Exact MAC match first
/// - Then any candidate with `link_up`
/// - Then func 0 (NCSI/LOM1), then func 1, then leftovers
/// - Empty list → n=0
/// - Never panics
pub fn pick_bcm5720_try_order(
    cands: &[(u8, u8, u8, [u8; 6])],
    link_up: &[bool],
    want: [u8; 6],
) -> ([Bcm5720PciPick; BCM5720_MAX_CAND], usize) {
    let mut out = [Bcm5720PciPick {
        bus: 0,
        dev: 0,
        func: 0,
        reason: Bcm5720PickReason::FirstCand,
    }; BCM5720_MAX_CAND];
    let mut n = 0usize;
    for &(bus, dev, func, mac) in cands {
        if mac == want {
            pick_push(
                &mut out,
                &mut n,
                bus,
                dev,
                func,
                Bcm5720PickReason::MacMatch,
            );
        }
    }
    for (i, &(bus, dev, func, _)) in cands.iter().enumerate() {
        if i < link_up.len() && link_up[i] {
            pick_push(
                &mut out,
                &mut n,
                bus,
                dev,
                func,
                Bcm5720PickReason::PreferLink,
            );
        }
    }
    for &(bus, dev, func, _) in cands {
        if func == 0 {
            pick_push(
                &mut out,
                &mut n,
                bus,
                dev,
                func,
                Bcm5720PickReason::PreferFunc0,
            );
        }
    }
    for &(bus, dev, func, _) in cands {
        if func == 1 {
            pick_push(
                &mut out,
                &mut n,
                bus,
                dev,
                func,
                Bcm5720PickReason::PreferFunc1,
            );
        }
    }
    for &(bus, dev, func, _) in cands {
        pick_push(
            &mut out,
            &mut n,
            bus,
            dev,
            func,
            Bcm5720PickReason::FirstCand,
        );
    }
    (out, n)
}

/// First bind attempt from [`pick_bcm5720_try_order`] (no live-link hints).
///
/// INVARIANTS:
/// - Exact MAC match wins
/// - Else func 0, then func 1, then the first candidate
/// - Empty list → `None`
/// - Never panics
pub fn pick_bcm5720_pci(cands: &[(u8, u8, u8, [u8; 6])], want: [u8; 6]) -> Option<Bcm5720PciPick> {
    let (ord, n) = pick_bcm5720_try_order(cands, &[], want);
    if n == 0 {
        None
    } else {
        Some(ord[0])
    }
}

/// Station address for smoltcp and `MAC_ADDR_*` after reset.
///
/// INVARIANTS:
/// - Non-zero `lease` always wins (PRE-EBS SNP DHCP/ARP identity)
/// - All-zero `lease` → `peeked` BAR0 MAC
/// - Never panics
///
/// Iron 2026-08-18: SNP leased `:3a` on `01:00.1`; BAR0 peek was `:39`.
/// Programming the peek left ARP on the wrong station → curl timeout.
pub fn station_mac(peeked: [u8; 6], lease: [u8; 6]) -> [u8; 6] {
    if lease.iter().any(|&b| b != 0) {
        lease
    } else {
        peeked
    }
}

/// IEEE 802.3 `BMSR_LSTATUS` (MII reg 1 bit 2). Latched low — callers read twice.
///
/// INVARIANTS:
/// - Bit 2 set → copper link up
/// - Never panics
pub fn bmsr_link_up(bmsr: u16) -> bool {
    bmsr & BMSR_LSTATUS != 0
}

/// IEEE 802.3 `BMSR_ANEGCOMPLETE` (MII reg 1 bit 5).
///
/// Iron 2026-08-19: `bmsr=7949` was link-down **and** AN incomplete.
pub fn bmsr_an_complete(bmsr: u16) -> bool {
    bmsr & BMSR_ANEGCOMPLETE != 0
}

/// Inherit firmware SNP copper instead of `CORECLK_RESET` when `BMSR_LSTATUS` is set.
///
/// Iron 2026-08-19: PRE-EBS SNP had DHCP/HTTP; post-reset `bmsr=7949` / `lpa=0000`.
///
/// INVARIANTS:
/// - True iff `bmsr_link_up`
/// - Never panics
pub fn inherit_snp_phy(pre_bmsr: u16) -> bool {
    bmsr_link_up(pre_bmsr)
}

/// Iron 2026-08-19: `phy_reset=pre` then `CORECLK_RESET` still `lpa=0000`.
/// Skip chip reset; steal rings; BMCR_RESET only when `LSTATUS` is clear.
///
/// INVARIANTS:
/// - Always true on this iron path
/// - Never panics
pub fn skip_coreclk_reset() -> bool {
    true
}

/// Linux `APE_FW_FEATURE_NCSI` (`tg3.h` bit 1).
///
/// INVARIANTS:
/// - True iff bit 1 set
/// - Never panics
pub fn ape_ncsi_enabled(fw_features: u32) -> bool {
    fw_features & APE_FW_FEATURE_NCSI != 0
}

/// Iron `cpmu=00004000` is Linux `CPMU_CTRL_LINK_SPEED_MODE`, not idle/APD.
///
/// INVARIANTS:
/// - True iff speed-mode set and idle/aware clear
/// - Never panics
pub fn cpmu_is_link_speed_mode(cpmu: u32) -> bool {
    cpmu & CPMU_CTRL_LINK_SPEED_MODE != 0
        && cpmu & (CPMU_CTRL_LINK_IDLE_MODE | CPMU_CTRL_LINK_AWARE_MODE) == 0
}

/// Linux `tp->phy_ape_lock` for `ENABLE_APE`: func0→PHY0, func1→PHY1, …
///
/// Iron 2026-08-19: `ape=yes` on `01:00.1` → lock PHY1 (`2`).
///
/// INVARIANTS:
/// - Func 0/1/2/3 → 0/2/3/5
/// - Other func → PHY3
/// - Never panics
pub fn ape_phy_lock_num(func: u8) -> u32 {
    match func {
        0 => TG3_APE_LOCK_PHY0,
        1 => TG3_APE_LOCK_PHY1,
        2 => TG3_APE_LOCK_PHY2,
        _ => TG3_APE_LOCK_PHY3,
    }
}

/// Linux `TG3_APE_PER_LOCK_REQ + 4 * locknum` (5717+).
pub fn ape_per_lock_req(locknum: u32) -> u32 {
    TG3_APE_PER_LOCK_REQ + 4 * locknum
}

/// Linux `TG3_APE_PER_LOCK_GRANT + 4 * locknum` (5717+).
pub fn ape_per_lock_grant(locknum: u32) -> u32 {
    TG3_APE_PER_LOCK_GRANT + 4 * locknum
}

/// Request/grant bit for an APE mutex.
///
/// INVARIANTS:
/// - PHY locks (0,2,3,5) → `APE_LOCK_REQ_DRIVER` (`0x1000`)
/// - Else func 0 → `0x1000`, else `1 << pci_fn`
/// - Never panics
pub fn ape_lock_req_bit(locknum: u32, pci_fn: u8) -> u32 {
    match locknum {
        TG3_APE_LOCK_PHY0 | TG3_APE_LOCK_PHY1 | TG3_APE_LOCK_PHY2 | TG3_APE_LOCK_PHY3 => {
            APE_LOCK_REQ_DRIVER
        }
        _ => {
            if pci_fn == 0 {
                APE_LOCK_REQ_DRIVER
            } else {
                1u32 << pci_fn
            }
        }
    }
}

/// Linux `tg3_ape_lock_init` grant bit (PHY locks = DRIVER; GRC/MEM/GPIO = fn bit).
pub fn ape_lock_init_grant_bit(locknum: u32, pci_fn: u8) -> u32 {
    match locknum {
        TG3_APE_LOCK_PHY0 | TG3_APE_LOCK_PHY1 | TG3_APE_LOCK_PHY2 | TG3_APE_LOCK_PHY3 => {
            APE_LOCK_GRANT_DRIVER
        }
        TG3_APE_LOCK_GRC | TG3_APE_LOCK_MEM | TG3_APE_LOCK_GPIO => {
            if pci_fn == 0 {
                APE_LOCK_GRANT_DRIVER
            } else {
                1u32 << pci_fn
            }
        }
        _ => {
            if pci_fn == 0 {
                APE_LOCK_GRANT_DRIVER
            } else {
                1u32 << pci_fn
            }
        }
    }
}

/// Linux `tg3_ape_send_event` guards: `APE_SEG_SIG_MAGIC` + `APE_FW_STATUS_READY`.
///
/// INVARIANTS:
/// - True iff magic `0x41504521` and ready bit `0x100`
/// - Never panics
pub fn ape_fw_ready(seg_sig: u32, fw_status: u32) -> bool {
    seg_sig == APE_SEG_SIG_MAGIC && fw_status & APE_FW_STATUS_READY != 0
}

/// Decode a PCI memory BAR from config dwords (host-testable).
///
/// INVARIANTS:
/// - I/O BAR (bit 0) → 0
/// - 32-bit mem → `lo & !0xF`
/// - 64-bit mem (bits 2:1 = 10b) → `(hi << 32) | (lo & !0xF)`
/// - Never panics
pub fn pci_mem_bar_addr(lo: u32, hi: u32) -> u64 {
    if lo & 1 != 0 {
        return 0;
    }
    let low = u64::from(lo) & !0xF;
    if lo & 0x6 == 0x4 {
        (u64::from(hi) << 32) | low
    } else {
        low
    }
}

/// 5717_PLUS MDIO address: `pci_fn + 1`, plus 7 if `SG_DIG_IS_SERDES`.
/// Linux `tg3_mdio_init`.
///
/// INVARIANTS:
/// - Copper (no serdes bit) → func+1 (R640 func 1 → PHY 2)
/// - Serdes bit → func+8
/// - Never panics
pub fn phy_addr_5717_plus(func: u8, sg_dig_status: u32) -> u32 {
    let mut phy_addr = u32::from(func) + 1; // pci_fn + 1 (func 1 → PHY 2)
    if sg_dig_status & SG_DIG_IS_SERDES != 0 {
        phy_addr += 7;
    }
    phy_addr
}

/// Decode copper partner ability (`MII_LPA` + `MII_STAT1000`).
/// Linux `tg3_adjust_link` uses phydev speed/duplex the same way.
///
/// INVARIANTS:
/// - Preference: 1000/full > 1000/half > 100/full > 100/half > 10/full > 10/half
/// - Never panics
pub fn decode_phy_link(lpa: u16, stat1000: u16) -> (u16, bool) {
    if stat1000 & LPA_1000FULL != 0 {
        return (1000, true);
    }
    if stat1000 & LPA_1000HALF != 0 {
        return (1000, false);
    }
    if lpa & LPA_100FULL != 0 {
        return (100, true);
    }
    if lpa & LPA_100HALF != 0 {
        return (100, false);
    }
    if lpa & LPA_10FULL != 0 {
        return (10, true);
    }
    (10, false)
}

/// `MAC_MODE` after link: GMII at 1000, MII at 10/100; half-duplex bit.
/// Includes TDE/RDE/FHDE + RX/TX stats (same as `enable_mac`).
///
/// INVARIANTS:
/// - `speed_mbps >= 1000` → GMII (`0x08`), else MII (`0x04`)
/// - Half duplex → `MAC_MODE_HALF_DUPLEX`
/// - Never panics
pub fn mac_mode_from_link(speed_mbps: u16, full_duplex: bool) -> u32 {
    let port = (if speed_mbps >= 1000 {
        MAC_MODE_PORT_MODE_GMII
    } else {
        MAC_MODE_PORT_MODE_MII
    }) & MAC_MODE_PORT_MODE_MASK;
    let hd = if full_duplex { 0 } else { MAC_MODE_HALF_DUPLEX };
    MAC_MODE_RXSTAT_ENABLE
        | MAC_MODE_TXSTAT_ENABLE
        | MAC_MODE_TDE_ENABLE
        | MAC_MODE_RDE_ENABLE
        | MAC_MODE_FHDE_ENABLE
        | port
        | hd
}

/// Parse a mocked 32-byte Tigon3 RX return BD. Host/Miri/fuzz — no MMIO.
///
/// INVARIANTS:
/// - `None` unless END, no ERROR/ERR bits, and `0 < length <= FRAME_MAX`
/// - Never panics
pub fn rx_bd_packet_len(idx_len: u32, type_flags: u32, err_vlan: u32) -> Option<usize> {
    let flags = type_flags & 0xFFFF;
    if flags & RXD_FLAG_END == 0 {
        return None;
    }
    if flags & RXD_FLAG_ERROR != 0 {
        return None;
    }
    if err_vlan & RXD_ERR_MASK != 0 {
        return None;
    }
    let n = (idx_len & 0xFFFF) as usize;
    if n == 0 || n > FRAME_MAX {
        return None;
    }
    Some(n)
}

/// Parse a mocked 32-byte RX BD (little-endian packed layout).
pub fn parse_mocked_rx_bd_bytes(raw: &[u8; 32]) -> Option<usize> {
    let idx_len = u32::from_le_bytes([raw[8], raw[9], raw[10], raw[11]]);
    let type_flags = u32::from_le_bytes([raw[12], raw[13], raw[14], raw[15]]);
    let err_vlan = u32::from_le_bytes([raw[20], raw[21], raw[22], raw[23]]);
    rx_bd_packet_len(idx_len, type_flags, err_vlan)
}

#[repr(C)]
#[derive(Clone, Copy)]
struct RxBd {
    addr_hi: u32,
    addr_lo: u32,
    idx_len: u32,
    type_flags: u32,
    ip_tcp_csum: u32,
    err_vlan: u32,
    reserved: u32,
    opaque: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct TxBd {
    addr_hi: u32,
    addr_lo: u32,
    len_flags: u32,
    vlan_tag: u32,
}

#[repr(C)]
struct HwStatus {
    status: u32,
    status_tag: u32,
    rx_jumbo_consumer: u16,
    rx_consumer: u16,
    rx_mini_consumer: u16,
    reserved: u16,
    idx: [StatusIdx; 16],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct StatusIdx {
    rx_producer: u16,
    tx_consumer: u16,
}

#[repr(C, align(4096))]
struct DmaArena {
    status: HwStatus,
    rx_std: [RxBd; RING],
    rx_rcb: [RxBd; RING],
    tx: [TxBd; RING],
    rx_buf: [[u8; PKT]; RING],
    tx_buf: [[u8; PKT]; RING],
}

impl DmaArena {
    const fn zeroed() -> Self {
        const RX: RxBd = RxBd {
            addr_hi: 0,
            addr_lo: 0,
            idx_len: 0,
            type_flags: 0,
            ip_tcp_csum: 0,
            err_vlan: 0,
            reserved: 0,
            opaque: 0,
        };
        const TX: TxBd = TxBd {
            addr_hi: 0,
            addr_lo: 0,
            len_flags: 0,
            vlan_tag: 0,
        };
        const IDX: StatusIdx = StatusIdx {
            rx_producer: 0,
            tx_consumer: 0,
        };
        Self {
            status: HwStatus {
                status: 0,
                status_tag: 0,
                rx_jumbo_consumer: 0,
                rx_consumer: 0,
                rx_mini_consumer: 0,
                reserved: 0,
                idx: [IDX; 16],
            },
            rx_std: [RX; RING],
            rx_rcb: [RX; RING],
            tx: [TX; RING],
            rx_buf: [[0; PKT]; RING],
            tx_buf: [[0; PKT]; RING],
        }
    }
}

struct NicState {
    mmio: u64,
    mac: [u8; 6],
    rx_rcb_ptr: u16,
    rx_std_prod: u16,
    tx_prod: u16,
    tx_cons: u16,
}

static NIC_LOCK: AtomicBool = AtomicBool::new(false);
static mut DMA: DmaArena = DmaArena::zeroed();
static mut NIC: Option<NicState> = None;
/// APE BAR2 + lock id for the one bound mgmt function. Written once in
/// `hw_init`; MDIO leaves read them. Justification: phy_read/write cannot
/// take `&mut NicState` without threading it through every helper.
#[cfg(feature = "uefi-bin")]
static APE_MMIO: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "uefi-bin")]
static APE_LOCKNUM: AtomicU32 = AtomicU32::new(0);
#[cfg(feature = "uefi-bin")]
static APE_PCI_FN: AtomicU32 = AtomicU32::new(0);
#[cfg(feature = "uefi-bin")]
static APE_EVT_SENT: AtomicBool = AtomicBool::new(false);

fn misc_host_ctrl() -> u32 {
    MISC_HOST_CTRL_CLEAR_INT
        | MISC_HOST_CTRL_MASK_PCI_INT
        | MISC_HOST_CTRL_WORD_SWAP
        | MISC_HOST_CTRL_INDIR_ACCESS
        | MISC_HOST_CTRL_IRQ_MASK_MODE
}

fn with_nic<R>(f: impl FnOnce(&mut NicState, &mut DmaArena) -> R) -> Option<R> {
    if NIC_LOCK.swap(true, Ordering::Acquire) {
        return None;
    }
    // SAFETY: lock held; BSP-only mgmt path; DMA/NIC exclusive owner.
    // KANI-TARGET: lock excludes concurrent with_nic (host tests are single-threaded).
    let out = unsafe {
        match (*core::ptr::addr_of_mut!(NIC)).as_mut() {
            Some(n) => Some(f(n, &mut *core::ptr::addr_of_mut!(DMA))),
            None => None,
        }
    };
    NIC_LOCK.store(false, Ordering::Release);
    out
}

/// True when PCI scan finds `14e4:165f` (any function).
#[cfg(feature = "uefi-bin")]
pub fn bcm5720_present() -> bool {
    any_bcm5720_bar().is_some()
}

#[cfg(not(feature = "uefi-bin"))]
pub fn bcm5720_present() -> bool {
    false
}

/// Reset + ring init. Identity-mapped BAR (UEFI page tables).
/// Tries each BCM5720 function until copper `LSTATUS` (func 0 / NCSI first).
#[cfg(feature = "uefi-bin")]
pub fn init_bcm5720(prefer_mac: [u8; 6]) -> Result<[u8; 6], Bcm5720Error> {
    const MAX: usize = BCM5720_MAX_CAND;
    let mut cands = [(0u8, 0u8, 0u8, [0u8; 6]); MAX];
    let mut bars = [0u64; MAX];
    let mut bmsr = [0u16; MAX];
    let n = scan_bcm5720(&mut cands, &mut bars, &mut bmsr);
    if n == 0 {
        return Err(Bcm5720Error::NotFound);
    }
    let mut link = [false; MAX];
    for i in 0..n {
        link[i] = bmsr_link_up(bmsr[i]);
    }
    let (order, ntry) = pick_bcm5720_try_order(&cands[..n], &link[..n], prefer_mac);
    if ntry == 0 {
        return Err(Bcm5720Error::NotFound);
    }
    if NIC_LOCK.swap(true, Ordering::Acquire) {
        return Err(Bcm5720Error::Busy);
    }
    // SAFETY: lock held; BAR programmed by firmware; identity-mapped MMIO.
    // DMA arena is .bss in the EFI image (identity-mapped RAM).
    // KANI-TARGET: mocked RX BD parse only — not this MMIO path.
    let result = unsafe {
        let dma = &mut *core::ptr::addr_of_mut!(DMA);
        let mut last_ok: Option<([u8; 6], u64)> = None;
        let mut last_err = Bcm5720Error::NotFound;
        let mut prev_bar: Option<u64> = None;
        for k in 0..ntry {
            let pick = order[k];
            let mut bar = 0u64;
            for i in 0..n {
                if cands[i].0 == pick.bus && cands[i].1 == pick.dev && cands[i].2 == pick.func {
                    bar = bars[i];
                    break;
                }
            }
            if bar == 0 {
                last_err = Bcm5720Error::NoBar;
                continue;
            }
            if let Some(prev) = prev_bar {
                halt_mac_dma(prev);
            }
            serial_step("boot: HOST-NIC BCM5720 try pci=");
            write_hex_u8(pick.bus);
            crate::boot::serial::write_byte(b':');
            write_hex_u8(pick.dev);
            crate::boot::serial::write_byte(b'.');
            write_hex_u8(pick.func);
            crate::boot::serial::write_str(" MAC=");
            write_mac({
                let mut m = [0u8; 6];
                for i in 0..n {
                    if cands[i].0 == pick.bus && cands[i].1 == pick.dev && cands[i].2 == pick.func {
                        m = cands[i].3;
                        break;
                    }
                }
                m
            });
            crate::boot::serial::write_byte(b' ');
            serial_step(pick.reason.as_str());
            crate::boot::serial::write_byte(b'\n');
            enable_bus_master(pick.bus, pick.dev, pick.func);
            *dma = DmaArena::zeroed();
            match hw_init(bar, pick.bus, pick.dev, pick.func, dma, prefer_mac) {
                Ok((mac, up)) => {
                    if up {
                        NIC = Some(NicState {
                            mmio: bar,
                            mac,
                            rx_rcb_ptr: 0,
                            rx_std_prod: (RING as u16).saturating_sub(1),
                            tx_prod: 0,
                            tx_cons: 0,
                        });
                        last_ok = Some((mac, bar));
                        break;
                    }
                    prev_bar = Some(bar);
                    last_ok = Some((mac, bar));
                }
                Err(e) => {
                    last_err = e;
                    prev_bar = Some(bar);
                    if k + 1 < ntry {
                        serial_step("boot: HOST-NIC BCM5720 try next func\n");
                    }
                    continue;
                }
            }
            if k + 1 < ntry {
                serial_step("boot: HOST-NIC BCM5720 try next func\n");
            }
        }
        if let Some((mac, bar)) = last_ok {
            NIC = Some(NicState {
                mmio: bar,
                mac,
                rx_rcb_ptr: 0,
                rx_std_prod: (RING as u16).saturating_sub(1),
                tx_prod: 0,
                tx_cons: 0,
            });
            Ok(mac)
        } else {
            Err(last_err)
        }
    };
    NIC_LOCK.store(false, Ordering::Release);
    result
}

#[cfg(not(feature = "uefi-bin"))]
pub fn init_bcm5720(_prefer_mac: [u8; 6]) -> Result<[u8; 6], Bcm5720Error> {
    Err(Bcm5720Error::NotFound)
}

pub fn receive_frame(out: &mut [u8]) -> Option<usize> {
    with_nic(|n, dma| {
        // SAFETY: lock held; DMA arena is the exclusive BCM5720 ring owner.
        // KANI-TARGET: parse_mocked_rx_bd_bytes / rx_bd_packet_len, not this MMIO path.
        unsafe { rx_one(n, dma, out) }
    })
    .flatten()
}

pub fn transmit_frame(frame: &[u8]) -> bool {
    with_nic(|n, dma| {
        // SAFETY: lock held; DMA arena is the exclusive BCM5720 ring owner.
        // KANI-TARGET: parse_mocked_rx_bd_bytes, not this MMIO path.
        unsafe { tx_one(n, dma, frame) }
    })
    .unwrap_or(false)
}

pub fn mac_address() -> Option<[u8; 6]> {
    with_nic(|n, _| n.mac)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bcm5720Error {
    NotFound,
    NoBar,
    ResetTimeout,
    MacInvalid,
    Busy,
}

#[cfg(feature = "uefi-bin")]
fn any_bcm5720_bar() -> Option<(u8, u8, u8, u64)> {
    for bus in 0u8..=15 {
        for dev in 0u8..32 {
            for func in 0u8..8 {
                let id = pci_read32(bus, dev, func, 0);
                if id == 0xFFFF_FFFF {
                    if func == 0 {
                        break;
                    }
                    continue;
                }
                let vendor = id as u16;
                let device = (id >> 16) as u16;
                if pci_id_is_bcm5720(vendor, device) {
                    let bar = (pci_read32(bus, dev, func, 0x10) as u64) & !0xF;
                    if bar != 0 {
                        return Some((bus, dev, func, bar));
                    }
                }
                if func == 0 {
                    let ht = (pci_read32(bus, dev, func, 0x0C) >> 16) as u8;
                    if ht & 0x80 == 0 {
                        break;
                    }
                }
            }
        }
    }
    None
}

/// Scan func 0 **and** 1. Peek MAC + `BMSR` (no `CORECLK_RESET`).
#[cfg(feature = "uefi-bin")]
fn scan_bcm5720(
    cands: &mut [(u8, u8, u8, [u8; 6]); BCM5720_MAX_CAND],
    bars: &mut [u64; BCM5720_MAX_CAND],
    bmsr: &mut [u16; BCM5720_MAX_CAND],
) -> usize {
    let mut n = 0usize;
    for bus in 0u8..=15 {
        for dev in 0u8..32 {
            for func in 0u8..8 {
                let id = pci_read32(bus, dev, func, 0);
                if id == 0xFFFF_FFFF {
                    if func == 0 {
                        break;
                    }
                    continue;
                }
                let vendor = id as u16;
                let device = (id >> 16) as u16;
                if pci_id_is_bcm5720(vendor, device) && n < BCM5720_MAX_CAND {
                    let bar = (pci_read32(bus, dev, func, 0x10) as u64) & !0xF;
                    if bar != 0 {
                        enable_mem(bus, dev, func);
                        // SAFETY: firmware BAR0; peek MAC_ADDR + BMSR only — no CORECLK_RESET.
                        // KANI-TARGET: pick_bcm5720_try_order (host), not this MMIO peek.
                        let mac = unsafe { peek_mac(bar, bus, dev, func) };
                        let phy = unsafe { peek_phy_bmsr(bar, func) };
                        serial_step("boot: HOST-NIC BCM5720 cand pci=");
                        write_hex_u8(bus);
                        crate::boot::serial::write_byte(b':');
                        write_hex_u8(dev);
                        crate::boot::serial::write_byte(b'.');
                        write_hex_u8(func);
                        crate::boot::serial::write_str(" MAC=");
                        write_mac(mac);
                        serial_step(" bmsr=");
                        write_hex_u16(phy);
                        crate::boot::serial::write_byte(b'\n');
                        cands[n] = (bus, dev, func, mac);
                        bars[n] = bar;
                        bmsr[n] = phy;
                        n += 1;
                    }
                }
                if func == 0 {
                    let ht = (pci_read32(bus, dev, func, 0x0C) >> 16) as u8;
                    if ht & 0x80 == 0 {
                        break;
                    }
                }
            }
        }
    }
    n
}

#[cfg(feature = "uefi-bin")]
unsafe fn peek_mac(mmio: u64, bus: u8, dev: u8, func: u8) -> [u8; 6] {
    pci_write32(
        bus,
        dev,
        func,
        TG3PCI_MISC_HOST_CTRL as u8,
        misc_host_ctrl(),
    );
    mmio_write(mmio, TG3PCI_MISC_HOST_CTRL, misc_host_ctrl());
    read_mac(mmio)
}

#[cfg(feature = "uefi-bin")]
fn enable_mem(bus: u8, dev: u8, func: u8) {
    let cmd = pci_read32(bus, dev, func, 0x04) as u16;
    let next = cmd | PCI_CMD_MEM;
    let rest = pci_read32(bus, dev, func, 0x04) & 0xFFFF_0000;
    pci_write32(bus, dev, func, 0x04, rest | u32::from(next));
}

#[cfg(feature = "uefi-bin")]
fn enable_bus_master(bus: u8, dev: u8, func: u8) {
    let cmd = pci_read32(bus, dev, func, 0x04) as u16;
    let next = cmd | PCI_CMD_MEM | PCI_CMD_BUSMASTER;
    let rest = pci_read32(bus, dev, func, 0x04) & 0xFFFF_0000;
    pci_write32(bus, dev, func, 0x04, rest | u32::from(next));
}

#[cfg(feature = "uefi-bin")]
unsafe fn hw_init(
    mmio: u64,
    bus: u8,
    dev: u8,
    func: u8,
    dma: &mut DmaArena,
    prefer_mac: [u8; 6],
) -> Result<([u8; 6], bool), Bcm5720Error> {
    pci_write32(
        bus,
        dev,
        func,
        TG3PCI_MISC_HOST_CTRL as u8,
        misc_host_ctrl(),
    );
    mmio_write(mmio, TG3PCI_MISC_HOST_CTRL, misc_host_ctrl());

    let rev = (mmio_read(mmio, TG3PCI_MISC_HOST_CTRL) & MISC_HOST_CTRL_CHIPREV) >> 16;
    serial_step("boot: HOST-NIC BCM5720 pci=");
    write_hex_u8(bus);
    crate::boot::serial::write_byte(b':');
    write_hex_u8(dev);
    crate::boot::serial::write_byte(b'.');
    write_hex_u8(func);
    crate::boot::serial::write_str(" bar0=0x");
    write_hex_u32(mmio as u32);
    crate::boot::serial::write_str(" rev=0x");
    write_hex_u32(rev);
    crate::boot::serial::write_byte(b'\n');

    let mac_before = read_mac(mmio);
    let station = station_mac(mac_before, prefer_mac);
    if station != mac_before {
        serial_step("boot: HOST-NIC BCM5720 station SNP-lease MAC=");
        write_mac(station);
        serial_step(" (BAR0 peeked ");
        write_mac(mac_before);
        serial_step(")\n");
    }

    bind_ape(bus, dev, func);
    let pre_bmsr = peek_phy_bmsr(mmio, func);
    let nic_cfg = sram_read(bus, dev, func, NIC_SRAM_DATA_CFG);
    serial_step("boot: HOST-NIC BCM5720 pre-reset bmsr=");
    write_hex_u16(pre_bmsr);
    serial_step(if nic_cfg & NIC_SRAM_DATA_CFG_APE_ENABLE != 0 {
        " ape=yes"
    } else {
        " ape=no"
    });
    serial_step(" ape-bar=0x");
    write_hex_u32(APE_MMIO.load(Ordering::Relaxed) as u32);
    serial_step(" ape-fw=");
    serial_step(if ape_fw_ready_now() { "ready" } else { "no" });
    serial_step(" ape-evt=");
    serial_step(if APE_EVT_SENT.load(Ordering::Relaxed) {
        "sent"
    } else {
        "skip"
    });
    serial_step(" ape-ncsi=");
    serial_step(if ape_ncsi_now() { "yes" } else { "no" });
    serial_step(" ape-lock=");
    serial_step(if APE_MMIO.load(Ordering::Relaxed) == 0 {
        "skip\n"
    } else if ape_lock_now() {
        ape_unlock_now();
        "yes\n"
    } else {
        "timeout\n"
    });

    let inherit = inherit_snp_phy(pre_bmsr);
    halt_mac_dma(mmio);
    if inherit {
        serial_step("boot: HOST-NIC BCM5720 inherit SNP PHY (skip CORECLK_RESET)\n");
    } else if skip_coreclk_reset() {
        // Iron: phy_reset=pre then CORECLK_RESET still lpa=0000. Keep GPHY analog.
        serial_step("boot: HOST-NIC BCM5720 skip CORECLK_RESET (keep GPHY analog)\n");
        setup_copper_phy(mmio, func);
    } else {
        setup_copper_phy(mmio, func);
        chip_reset(mmio, bus, dev, func)?;
        let fw = wait_fw_magic(bus, dev, func);
        serial_step(if fw {
            "boot: HOST-NIC BCM5720 fw-magic=yes\n"
        } else {
            "boot: HOST-NIC BCM5720 fw-magic=timeout (continuing)\n"
        });
    }
    mmio_write(mmio, MEMARB_MODE, MODE_ENABLE);
    mmio_write(mmio, TG3PCI_MISC_HOST_CTRL, misc_host_ctrl());

    let grc = GRC_MODE_WSWAP_NONFRM_DATA
        | GRC_MODE_WSWAP_DATA
        | GRC_MODE_NOIRQ_ON_SENDS
        | GRC_MODE_NOIRQ_ON_RCV
        | GRC_MODE_HOST_STACKUP
        | GRC_MODE_HOST_SENDBDS
        | GRC_MODE_NO_TX_PHDR_CSUM
        | GRC_MODE_NO_RX_PHDR_CSUM;
    mmio_write(mmio, GRC_MODE, grc);
    // Prescalar only (bits 7:0). Preserve GRC_MISC_CFG_PRESERVE_PCIE.
    let mut misc = mmio_read(mmio, GRC_MISC_CFG);
    misc &= !0xFF;
    misc |= 65 << GRC_MISC_CFG_PRESCALAR_SHIFT;
    mmio_write(mmio, GRC_MISC_CFG, misc);

    // 5750_PLUS / BCM5720: firmware owns the mbuf pool. Linux tg3 does
    // nothing at 0x4408/0x440C/0x442C/0x4430. Watermarks are 57765_PLUS
    // (PG Table 38): MACRX 0x2A, high 0xA0 — not the 5700 0x20/0x60 defaults.
    mmio_write(
        mmio,
        BUFMGR_MB_RDMA_LOW_WATER,
        DEFAULT_MB_RDMA_LOW_WATER_5705,
    );
    mmio_write(
        mmio,
        BUFMGR_MB_MACRX_LOW_WATER,
        DEFAULT_MB_MACRX_LOW_WATER_57765,
    );
    mmio_write(mmio, BUFMGR_MB_HIGH_WATER, DEFAULT_MB_HIGH_WATER_57765);
    mmio_write(mmio, BUFMGR_DMA_LOW_WATER, DEFAULT_DMA_LOW_WATER);
    mmio_write(mmio, BUFMGR_DMA_HIGH_WATER, DEFAULT_DMA_HIGH_WATER);
    mmio_write(mmio, BUFMGR_MODE, MODE_ENABLE);
    if !wait_bits(mmio, BUFMGR_MODE, MODE_ENABLE, 200) {
        serial_step("boot: WARN — HOST-NIC BCM5720 bufmgr timeout (continuing)\n");
    }
    mmio_write(mmio, FTQ_RESET, 0xFFFF_FFFF);
    mmio_write(mmio, FTQ_RESET, 0);
    let _ = wait_eq(mmio, FTQ_RESET, 0, 200);

    init_rings(dma);
    program_rings(mmio, bus, dev, func, dma);
    enable_mac(mmio, station)?;
    enable_dma_engines(mmio);
    enable_hostcc(mmio);
    enable_completion_blocks(mmio);
    if !inherit && !skip_coreclk_reset() {
        restart_copper_an(mmio, func);
    }
    let up = wait_phy_link_and_adjust_mac(mmio, func);

    mailbox_write(
        mmio,
        MAILBOX_RCV_STD_PROD_IDX,
        (RING as u32).saturating_sub(1),
    );
    mailbox_write(mmio, MAILBOX_RCVRET_CON_IDX_0, 0);
    mailbox_write(mmio, MAILBOX_SNDHOST_PROD_IDX_0, 0);

    serial_step("boot: HOST-NIC BCM5720 rings armed (poll-mode, MSI-X off)\n");
    Ok((station, up))
}

/// Linux `tg3_write_sig_pre_reset` then `tg3_chip_reset` (PG 7.1/7.2).
/// MAGIC1 goes to SRAM 0xB50 **before** CORECLK_RESET. Bit 29 keeps PCIe.
/// Retained for the `!skip_coreclk_reset()` fallback (iron skips it).
#[cfg(feature = "uefi-bin")]
unsafe fn chip_reset(mmio: u64, bus: u8, dev: u8, func: u8) -> Result<(), Bcm5720Error> {
    serial_step("boot: HOST-NIC BCM5720 reset…\n");
    mmio_write(mmio, GRC_FASTBOOT_PC, 0);
    sram_write(
        bus,
        dev,
        func,
        NIC_SRAM_FIRMWARE_MBOX,
        NIC_SRAM_FIRMWARE_MBOX_MAGIC1,
    );
    mmio_write(
        mmio,
        GRC_MISC_CFG,
        GRC_MISC_CFG_CORECLK_RESET | GRC_MISC_CFG_PRESERVE_PCIE,
    );
    let _ = pci_read32(bus, dev, func, 0x04);
    tsc_spin_ms(20);
    pci_write32(
        bus,
        dev,
        func,
        TG3PCI_MISC_HOST_CTRL as u8,
        misc_host_ctrl(),
    );
    mmio_write(mmio, TG3PCI_MISC_HOST_CTRL, misc_host_ctrl());
    enable_bus_master(bus, dev, func);
    tsc_spin_ms(20);
    let id = pci_read32(bus, dev, func, 0);
    if id as u16 != BCM5720_VENDOR {
        return Err(Bcm5720Error::ResetTimeout);
    }
    Ok(())
}

/// Linux `tg3_poll_fw`: wait for `~MAGIC1`. Timeout is **not** fatal
/// (some boards have no running firmware); we WARN and continue.
#[cfg(feature = "uefi-bin")]
fn wait_fw_magic(bus: u8, dev: u8, func: u8) -> bool {
    for _ in 0..200 {
        let v = sram_read(bus, dev, func, NIC_SRAM_FIRMWARE_MBOX);
        if v == !NIC_SRAM_FIRMWARE_MBOX_MAGIC1 {
            return true;
        }
        tsc_spin_ms(10);
    }
    false
}

#[cfg(feature = "uefi-bin")]
unsafe fn init_rings(dma: &mut DmaArena) {
    for i in 0..RING {
        let addr = dma.rx_buf[i].as_ptr() as u64;
        dma.rx_std[i] = RxBd {
            addr_hi: (addr >> 32) as u32,
            addr_lo: addr as u32,
            idx_len: ((PKT as u32) - 64) & 0xFFFF,
            type_flags: RXD_FLAG_END,
            ip_tcp_csum: 0,
            err_vlan: 0,
            reserved: 0,
            opaque: RXD_OPAQUE_RING_STD | (i as u32),
        };
        dma.rx_rcb[i] = RxBd {
            addr_hi: 0,
            addr_lo: 0,
            idx_len: 0,
            type_flags: 0,
            ip_tcp_csum: 0,
            err_vlan: 0,
            reserved: 0,
            opaque: 0,
        };
        let taddr = dma.tx_buf[i].as_ptr() as u64;
        dma.tx[i] = TxBd {
            addr_hi: (taddr >> 32) as u32,
            addr_lo: taddr as u32,
            len_flags: 0,
            vlan_tag: 0,
        };
    }
    dma.status = DmaArena::zeroed().status;
}

#[cfg(feature = "uefi-bin")]
unsafe fn program_rings(mmio: u64, bus: u8, dev: u8, func: u8, dma: &mut DmaArena) {
    let std = dma.rx_std.as_ptr() as u64;
    let rcb = dma.rx_rcb.as_ptr() as u64;
    let tx = dma.tx.as_ptr() as u64;
    let st = core::ptr::addr_of!(dma.status) as u64;

    mmio_write64(mmio, HOSTCC_STATUS_BLK_HOST_ADDR, st);

    // 57765_PLUS STD BD: (ring_size << 16) | (1536 << 2). Linux tg3.
    mmio_write64(mmio, RCVDBDI_STD_BD + TG3_BDINFO_HOST_ADDR, std);
    mmio_write(
        mmio,
        RCVDBDI_STD_BD + TG3_BDINFO_MAXLEN_FLAGS,
        (RING as u32) << BDINFO_FLAGS_MAXLEN_SHIFT | (TG3_RX_STD_DMA_SZ << 2),
    );
    // 5717_PLUS: do not program RCVDBDI_STD_BD+NIC_ADDR (0x245C).

    // RING=32 → thresh = max(RING/8, 1) = 4. Must stay < RING-1.
    mmio_write(mmio, RCVBDI_STD_THRESH, 4);

    for off in (NIC_SRAM_SEND_RCB..NIC_SRAM_RCV_RET_RCB).step_by(TG3_BDINFO_SIZE as usize) {
        sram_write(
            bus,
            dev,
            func,
            off + TG3_BDINFO_MAXLEN_FLAGS,
            BDINFO_FLAGS_DISABLED,
        );
    }
    for off in (NIC_SRAM_RCV_RET_RCB..NIC_SRAM_STATS_BLK).step_by(TG3_BDINFO_SIZE as usize) {
        sram_write(
            bus,
            dev,
            func,
            off + TG3_BDINFO_MAXLEN_FLAGS,
            BDINFO_FLAGS_DISABLED,
        );
    }
    set_bdinfo(
        bus,
        dev,
        func,
        NIC_SRAM_SEND_RCB,
        tx,
        RING as u32,
        NIC_SRAM_TX_BUFFER_DESC,
    );
    set_bdinfo(bus, dev, func, NIC_SRAM_RCV_RET_RCB, rcb, RING as u32, 0);
}

#[cfg(feature = "uefi-bin")]
fn set_bdinfo(bus: u8, dev: u8, func: u8, base: u32, mapping: u64, maxlen: u32, nic_addr: u32) {
    sram_write(
        bus,
        dev,
        func,
        base + TG3_BDINFO_HOST_ADDR,
        (mapping >> 32) as u32,
    );
    sram_write(
        bus,
        dev,
        func,
        base + TG3_BDINFO_HOST_ADDR + 4,
        mapping as u32,
    );
    sram_write(
        bus,
        dev,
        func,
        base + TG3_BDINFO_MAXLEN_FLAGS,
        maxlen << BDINFO_FLAGS_MAXLEN_SHIFT,
    );
    sram_write(bus, dev, func, base + TG3_BDINFO_NIC_ADDR, nic_addr);
}

/// Linux `tg3_reset_hw`: disable HOSTCC, wait, program ticks, then ENABLE.
/// Do **not** RESET after programming ticks (that wipes them).
#[cfg(feature = "uefi-bin")]
unsafe fn enable_hostcc(mmio: u64) {
    mmio_write(mmio, HOSTCC_MODE, 0);
    tsc_spin_ms(1);
    for _ in 0..200 {
        if mmio_read(mmio, HOSTCC_MODE) & MODE_ENABLE == 0 {
            break;
        }
        tsc_spin_ms(1);
    }
    mmio_write(mmio, HOSTCC_RXCOL_TICKS, 0x48);
    mmio_write(mmio, HOSTCC_TXCOL_TICKS, 0x14);
    mmio_write(mmio, HOSTCC_RXMAX_FRAMES, 1);
    mmio_write(mmio, HOSTCC_TXMAX_FRAMES, 1);
    mmio_write(mmio, HOSTCC_STAT_COAL_TICKS, 0);
    mmio_write(mmio, HOSTCC_MODE, MODE_ENABLE);
}

/// Linux `tg3`: WDMAC + RDMAC with abort bits. 5720 also STATUS_TAG_FIX
/// and PCIe FIFO_LONG_BURST. Without these, frames never DMA.
#[cfg(feature = "uefi-bin")]
unsafe fn enable_dma_engines(mmio: u64) {
    let wdmac = WDMAC_MODE_ENABLE
        | WDMAC_MODE_TGTABORT_ENAB
        | WDMAC_MODE_MSTABORT_ENAB
        | WDMAC_MODE_PARITYERR_ENAB
        | WDMAC_MODE_ADDROFLOW_ENAB
        | WDMAC_MODE_FIFOOFLOW_ENAB
        | WDMAC_MODE_FIFOURUN_ENAB
        | WDMAC_MODE_FIFOOREAD_ENAB
        | WDMAC_MODE_LNGREAD_ENAB
        | WDMAC_MODE_STATUS_TAG_FIX;
    mmio_write(mmio, WDMAC_MODE, wdmac);
    tsc_spin_ms(1);
    let rdmac = RDMAC_MODE_ENABLE
        | RDMAC_MODE_TGTABORT_ENAB
        | RDMAC_MODE_MSTABORT_ENAB
        | RDMAC_MODE_PARITYERR_ENAB
        | RDMAC_MODE_ADDROFLOW_ENAB
        | RDMAC_MODE_FIFOOFLOW_ENAB
        | RDMAC_MODE_FIFOURUN_ENAB
        | RDMAC_MODE_FIFOOREAD_ENAB
        | RDMAC_MODE_LNGREAD_ENAB
        | RDMAC_MODE_FIFO_LONG_BURST;
    mmio_write(mmio, RDMAC_MODE, rdmac);
    tsc_spin_ms(1);
}

/// Completions + producer engines. `RCVDBDI` is not `RCVBDI` (0x2400 vs 0x2C00).
#[cfg(feature = "uefi-bin")]
unsafe fn enable_completion_blocks(mmio: u64) {
    mmio_write(
        mmio,
        RCVDCC_MODE,
        RCVDCC_MODE_ENABLE | RCVDCC_MODE_ATTN_ENABLE,
    );
    mmio_write(mmio, SNDDATAC_MODE, SNDDATAC_MODE_ENABLE);
    mmio_write(
        mmio,
        SNDBDC_MODE,
        SNDBDC_MODE_ENABLE | SNDBDC_MODE_ATTN_ENABLE,
    );
    mmio_write(
        mmio,
        RCVBDI_MODE,
        RCVBDI_MODE_ENABLE | RCVBDI_MODE_RCB_ATTN_ENAB,
    );
    mmio_write(mmio, RCVDBDI_MODE, MODE_ENABLE | RCVDBDI_MODE_INV_RING_SZ);
    mmio_write(mmio, SNDDATAI_MODE, MODE_ENABLE);
    mmio_write(mmio, SNDBDI_MODE, MODE_ENABLE);
    mmio_write(mmio, SNDBDS_MODE, MODE_ENABLE);
    mmio_write(mmio, RCVCC_MODE, MODE_ENABLE);
    mmio_write(mmio, RCVLPC_MODE, MODE_ENABLE);
    mmio_write(mmio, RCVLSC_MODE, MODE_ENABLE);
}

#[cfg(feature = "uefi-bin")]
unsafe fn enable_mac(mmio: u64, mac: [u8; 6]) -> Result<[u8; 6], Bcm5720Error> {
    if mac.iter().all(|&b| b == 0) {
        return Err(Bcm5720Error::MacInvalid);
    }
    let addr_high = u32::from(mac[0]) << 8 | u32::from(mac[1]);
    let addr_low = u32::from(mac[2]) << 24
        | u32::from(mac[3]) << 16
        | u32::from(mac[4]) << 8
        | u32::from(mac[5]);
    for i in 0..4u32 {
        mmio_write(mmio, MAC_ADDR_0_HIGH + i * 8, addr_high);
        mmio_write(mmio, MAC_ADDR_0_LOW + i * 8, addr_low);
    }
    mmio_write(mmio, MAC_RX_MTU_SIZE, 1536);
    let mut tx_len = TX_LENGTHS_DEFAULT;
    tx_len |=
        mmio_read(mmio, MAC_TX_LENGTHS) & (TX_LENGTHS_JMB_FRM_LEN_MSK | TX_LENGTHS_CNT_DWN_VAL_MSK);
    mmio_write(mmio, MAC_TX_LENGTHS, tx_len);
    mmio_write(mmio, MAC_RCV_RULE_CFG, RCV_RULE_CFG_DEFAULT_CLASS);
    mmio_write(mmio, RCVLPC_CONFIG, RCVLPC_CONFIG_DEFAULT);
    for i in 0..4u32 {
        mmio_write(mmio, MAC_HASH_REG_0 + i * 4, 0xFFFF_FFFF);
    }
    mmio_write(mmio, MAC_LOW_WMARK_MAX_RX_FRAME, 1);
    mmio_write(mmio, MAC_LED_CTRL, LED_CTRL_MODE_PHY_1);
    let mac_mode = MAC_MODE_PORT_MODE_GMII
        | MAC_MODE_RXSTAT_ENABLE
        | MAC_MODE_TXSTAT_ENABLE
        | MAC_MODE_TDE_ENABLE
        | MAC_MODE_RDE_ENABLE
        | MAC_MODE_FHDE_ENABLE;
    mmio_write(mmio, MAC_MODE, mac_mode);
    mmio_write(mmio, MAC_RX_MODE, RX_MODE_ENABLE | RX_MODE_PROMISC);
    let mut tx_mode = TX_MODE_ENABLE;
    tx_mode |= mmio_read(mmio, MAC_TX_MODE) & (TX_MODE_JMB_FRM_LEN | TX_MODE_CNT_DN_MODE);
    mmio_write(mmio, MAC_TX_MODE, tx_mode);
    mmio_write(mmio, MAC_MI_STAT, MAC_MI_STAT_LNKSTAT_ATTN_ENAB);
    mmio_write(mmio, MAC_EVENT, MAC_EVENT_LNKSTATE_CHANGED);
    let _ = mmio_read(mmio, MAC_STATUS);
    serial_step("boot: HOST-NIC BCM5720 MAC=");
    write_mac(mac);
    crate::boot::serial::write_byte(b'\n');
    Ok(mac)
}

/// Linux `tg3_phy_reset` (called from `tg3_reset_hw` **before** chip reset).
/// Iron 2026-08-19: AN-only left `bmsr=7949` (LSTATUS=0, ANEGCOMPLETE=0).
/// MDIO timeout is not fatal.
#[cfg(feature = "uefi-bin")]
unsafe fn setup_copper_phy(mmio: u64, func: u8) {
    mmio_write(
        mmio,
        MAC_MI_MODE,
        MAC_MI_MODE_BASE | MAC_MI_MODE_500KHZ_CONST,
    );
    tsc_spin_ms(1);

    let sg_dig = mmio_read(mmio, SG_DIG_STATUS);
    let phy_addr = phy_addr_5717_plus(func, sg_dig);
    serial_step("boot: HOST-NIC BCM5720 phy_addr=");
    write_hex_u8(phy_addr as u8);
    serial_step(if sg_dig & SG_DIG_IS_SERDES != 0 {
        " serdes=yes"
    } else {
        " serdes=no"
    });
    serial_step(" sgdig=");
    write_hex_u32(sg_dig);
    serial_step("\n");

    // Linux `tg3_reset_hw` / eeprom path: GPHY must not sit in CPMU idle.
    let mut cpmu = mmio_read(mmio, TG3_CPMU_CTRL);
    cpmu &= !(CPMU_CTRL_LINK_IDLE_MODE | CPMU_CTRL_LINK_AWARE_MODE | CPMU_CTRL_GPHY_10MB_RXONLY);
    mmio_write(mmio, TG3_CPMU_CTRL, cpmu);
    let eee = mmio_read(mmio, TG3_CPMU_EEE_MODE) & !TG3_CPMU_EEEMD_LPI_ENABLE;
    mmio_write(mmio, TG3_CPMU_EEE_MODE, eee);

    let rst = phy_bmcr_reset(mmio, phy_addr);
    let pwr = phy_auxctl_write(mmio, phy_addr, MII_TG3_AUXCTL_SHDWSEL_PWRCTL, 0);
    let apd = phy_disable_apd(mmio, phy_addr);
    let mdix = phy_enable_automdix_wirespeed(mmio, phy_addr);
    let an = phy_write(mmio, phy_addr, MII_ADVERTISE, ADVERTISE_COPPER)
        && phy_write(mmio, phy_addr, MII_CTRL1000, ADVERTISE_1000)
        && phy_write(mmio, phy_addr, MII_BMCR, BMCR_ANENABLE | BMCR_ANRESTART);

    serial_step("boot: HOST-NIC BCM5720 phy_reset=pre");
    serial_step(if rst { " yes" } else { " timeout" });
    serial_step(" pwrctl=");
    serial_step(if pwr { "clr" } else { "timeout" });
    serial_step(" apd=");
    serial_step(if apd { "off" } else { "timeout" });
    serial_step(" mdix=");
    serial_step(if mdix { "yes" } else { "timeout" });
    serial_step(" eee=off");
    serial_step(if an { " phy=yes" } else { " phy=timeout" });
    serial_step(" id=");
    match (
        phy_read(mmio, phy_addr, MII_PHYSID1),
        phy_read(mmio, phy_addr, MII_PHYSID2),
    ) {
        (Some(hi), Some(lo)) => {
            write_hex_u16(hi);
            serial_step(":");
            write_hex_u16(lo);
        }
        _ => serial_step("----:----"),
    }
    serial_step("\n");
}

/// Linux `tg3_setup_phy(false)` after `tg3_chip_reset`: PWRCTL + AN restart,
/// no `BMCR_RESET` (that already ran before CORECLK_RESET).
#[cfg(feature = "uefi-bin")]
unsafe fn restart_copper_an(mmio: u64, func: u8) {
    mmio_write(
        mmio,
        MAC_MI_MODE,
        MAC_MI_MODE_BASE | MAC_MI_MODE_500KHZ_CONST,
    );
    tsc_spin_ms(1);
    let mut cpmu = mmio_read(mmio, TG3_CPMU_CTRL);
    cpmu &= !(CPMU_CTRL_LINK_IDLE_MODE | CPMU_CTRL_LINK_AWARE_MODE | CPMU_CTRL_GPHY_10MB_RXONLY);
    mmio_write(mmio, TG3_CPMU_CTRL, cpmu);
    let sg_dig = mmio_read(mmio, SG_DIG_STATUS);
    let phy_addr = phy_addr_5717_plus(func, sg_dig);
    let pwr = phy_auxctl_write(mmio, phy_addr, MII_TG3_AUXCTL_SHDWSEL_PWRCTL, 0);
    let an = phy_write(mmio, phy_addr, MII_ADVERTISE, ADVERTISE_COPPER)
        && phy_write(mmio, phy_addr, MII_CTRL1000, ADVERTISE_1000)
        && phy_write(mmio, phy_addr, MII_BMCR, BMCR_ANENABLE | BMCR_ANRESTART);
    serial_step("boot: HOST-NIC BCM5720 an-restart=");
    serial_step(if an { "yes" } else { "timeout" });
    serial_step(" pwrctl=");
    serial_step(if pwr { "clr" } else { "timeout" });
    serial_step("\n");
}

/// Linux `tg3_phy_toggle_apd(false)` — Auto Power Down keeps analog off
/// so AN never completes (`bmsr=7949` / `lpa=0000`).
///
/// 5720 (not 5784): SCR5 still sets `DLLAPD`. APD shadow gets `APD_SEL|WKTM`
/// **without** `MII_TG3_MISC_SHDW_APD_ENABLE`.
#[cfg(feature = "uefi-bin")]
unsafe fn phy_disable_apd(mmio: u64, phy: u32) -> bool {
    let scr5 = MII_TG3_MISC_SHDW_SCR5_LPED
        | MII_TG3_MISC_SHDW_SCR5_DLPTLM
        | MII_TG3_MISC_SHDW_SCR5_SDTL
        | MII_TG3_MISC_SHDW_SCR5_C125OE
        | MII_TG3_MISC_SHDW_SCR5_DLLAPD;
    phy_write(
        mmio,
        phy,
        MII_TG3_MISC_SHDW,
        MII_TG3_MISC_SHDW_SCR5_SEL | scr5 | MII_TG3_MISC_SHDW_WREN,
    ) && phy_write(
        mmio,
        phy,
        MII_TG3_MISC_SHDW,
        MII_TG3_MISC_SHDW_APD_SEL | MII_TG3_MISC_SHDW_APD_WKTM_84MS | MII_TG3_MISC_SHDW_WREN,
    )
}

#[cfg(feature = "uefi-bin")]
unsafe fn peek_phy_bmsr(mmio: u64, func: u8) -> u16 {
    mmio_write(
        mmio,
        MAC_MI_MODE,
        MAC_MI_MODE_BASE | MAC_MI_MODE_500KHZ_CONST,
    );
    tsc_spin_ms(1);
    let sg_dig = mmio_read(mmio, SG_DIG_STATUS);
    let phy_addr = phy_addr_5717_plus(func, sg_dig);
    let _ = phy_read(mmio, phy_addr, MII_BMSR);
    phy_read(mmio, phy_addr, MII_BMSR).unwrap_or(0)
}

/// Stop UNDI TX/RX/coalescing before we steal the rings (inherit path).
#[cfg(feature = "uefi-bin")]
unsafe fn halt_mac_dma(mmio: u64) {
    mmio_write(mmio, MAC_RX_MODE, 0);
    mmio_write(mmio, MAC_TX_MODE, 0);
    mmio_write(mmio, HOSTCC_MODE, 0);
    tsc_spin_ms(1);
}

/// Linux `tg3_bmcr_reset`: write BMCR_RESET, poll until the bit clears.
#[cfg(feature = "uefi-bin")]
unsafe fn phy_bmcr_reset(mmio: u64, phy: u32) -> bool {
    if !phy_write(mmio, phy, MII_BMCR, BMCR_RESET) {
        return false;
    }
    for _ in 0..5000 {
        if let Some(bmcr) = phy_read(mmio, phy, MII_BMCR) {
            if bmcr & BMCR_RESET == 0 {
                tsc_spin_us(40);
                return true;
            }
        }
        tsc_spin_us(10);
    }
    false
}

#[cfg(feature = "uefi-bin")]
unsafe fn phy_auxctl_write(mmio: u64, phy: u32, reg: u16, set: u16) -> bool {
    let mut v = set | reg;
    if reg == MII_TG3_AUXCTL_SHDWSEL_MISC {
        v |= MII_TG3_AUXCTL_MISC_WREN;
    }
    phy_write(mmio, phy, MII_TG3_AUX_CTRL, v)
}

#[cfg(feature = "uefi-bin")]
unsafe fn phy_auxctl_read(mmio: u64, phy: u32, reg: u16) -> Option<u16> {
    if !phy_write(
        mmio,
        phy,
        MII_TG3_AUX_CTRL,
        (reg << MII_TG3_AUXCTL_MISC_RDSEL_SHIFT) | MII_TG3_AUXCTL_SHDWSEL_MISC,
    ) {
        return None;
    }
    phy_read(mmio, phy, MII_TG3_AUX_CTRL)
}

/// Linux `tg3_phy_toggle_automdix(true)` + `tg3_phy_set_wirespeed`.
#[cfg(feature = "uefi-bin")]
unsafe fn phy_enable_automdix_wirespeed(mmio: u64, phy: u32) -> bool {
    let misc = phy_auxctl_read(mmio, phy, MII_TG3_AUXCTL_SHDWSEL_MISC).unwrap_or(0);
    phy_auxctl_write(
        mmio,
        phy,
        MII_TG3_AUXCTL_SHDWSEL_MISC,
        misc | MII_TG3_AUXCTL_MISC_FORCE_AMDIX | MII_TG3_AUXCTL_MISC_WIRESPD_EN,
    )
}

/// Linux `tg3_setup_phy` / `tg3_adjust_link`: `BMSR_LSTATUS` is latched low
/// (read twice). Then `MAC_MODE` MII vs GMII from speed. Soft-fail.
#[cfg(feature = "uefi-bin")]
unsafe fn wait_phy_link_and_adjust_mac(mmio: u64, func: u8) -> bool {
    let sg_dig = mmio_read(mmio, SG_DIG_STATUS);
    let phy_addr = phy_addr_5717_plus(func, sg_dig);
    let mut last_bmsr: u16 = 0;
    let mut up = false;
    // ~8s: BMCR_RESET restarts 1000BASE-T AN (often 1–4s; 4s was not enough
    // when the analog front-end was still isolated).
    for _ in 0..400 {
        let _ = phy_read(mmio, phy_addr, MII_BMSR); // discard latch
        if let Some(bmsr) = phy_read(mmio, phy_addr, MII_BMSR) {
            last_bmsr = bmsr;
            if bmsr_link_up(bmsr) {
                up = true;
                break;
            }
        }
        tsc_spin_ms(20);
    }
    let mac_status = mmio_read(mmio, MAC_STATUS);
    if up {
        let (speed, full) = match (
            phy_read(mmio, phy_addr, MII_LPA),
            phy_read(mmio, phy_addr, MII_STAT1000),
        ) {
            (Some(lpa), Some(stat1000)) => decode_phy_link(lpa, stat1000),
            _ => (1000, true),
        };
        mmio_write(mmio, MAC_MODE, mac_mode_from_link(speed, full));
        tsc_spin_us(40);
        mmio_write(mmio, MAC_RX_MODE, RX_MODE_ENABLE | RX_MODE_PROMISC);
        serial_step("boot: HOST-NIC BCM5720 link=up speed=");
        write_u16_dec(speed);
        serial_step(if full {
            " duplex=full\n"
        } else {
            " duplex=half\n"
        });
    } else {
        mmio_write(mmio, MAC_RX_MODE, RX_MODE_ENABLE | RX_MODE_PROMISC);
        serial_step("boot: HOST-NIC BCM5720 link=timeout bmsr=");
        write_hex_u16(last_bmsr);
        serial_step(" bmcr=");
        match phy_read(mmio, phy_addr, MII_BMCR) {
            Some(v) => write_hex_u16(v),
            None => serial_step("----"),
        }
        serial_step(" lpa=");
        match phy_read(mmio, phy_addr, MII_LPA) {
            Some(v) => write_hex_u16(v),
            None => serial_step("----"),
        }
        serial_step(" s1000=");
        match phy_read(mmio, phy_addr, MII_STAT1000) {
            Some(v) => write_hex_u16(v),
            None => serial_step("----"),
        }
        serial_step(" mac_status=");
        write_hex_u32(mac_status);
        serial_step(" cpmu=");
        write_hex_u32(mmio_read(mmio, TG3_CPMU_CTRL));
        serial_step("\n");
    }
    up
}

#[cfg(feature = "uefi-bin")]
unsafe fn phy_wait(mmio: u64) -> bool {
    for _ in 0..500 {
        if mmio_read(mmio, MAC_MI_COM) & MI_COM_BUSY == 0 {
            return true;
        }
        tsc_spin_us(10);
    }
    false
}

#[cfg(feature = "uefi-bin")]
unsafe fn phy_write(mmio: u64, phy: u32, reg: u32, val: u16) -> bool {
    ape_lock_now();
    mmio_write(
        mmio,
        MAC_MI_COM,
        MI_COM_START
            | MI_COM_CMD_WRITE
            | (phy << MI_COM_PHY_ADDR_SHIFT)
            | (reg << MI_COM_REG_ADDR_SHIFT)
            | u32::from(val),
    );
    let ok = phy_wait(mmio);
    ape_unlock_now();
    ok
}

#[cfg(feature = "uefi-bin")]
unsafe fn phy_read(mmio: u64, phy: u32, reg: u32) -> Option<u16> {
    ape_lock_now();
    mmio_write(
        mmio,
        MAC_MI_COM,
        MI_COM_START
            | MI_COM_CMD_READ
            | (phy << MI_COM_PHY_ADDR_SHIFT)
            | (reg << MI_COM_REG_ADDR_SHIFT),
    );
    let out = if !phy_wait(mmio) {
        None
    } else {
        Some((mmio_read(mmio, MAC_MI_COM) & 0xFFFF) as u16)
    };
    ape_unlock_now();
    out
}

/// Linux `pci_ioremap_bar(pdev, BAR_2)` then `tg3_ape_lock_init` + driver START.
#[cfg(feature = "uefi-bin")]
fn bind_ape(bus: u8, dev: u8, func: u8) {
    APE_EVT_SENT.store(false, Ordering::Relaxed);
    let nic_cfg = sram_read(bus, dev, func, NIC_SRAM_DATA_CFG);
    if nic_cfg & NIC_SRAM_DATA_CFG_APE_ENABLE == 0 {
        APE_MMIO.store(0, Ordering::Relaxed);
        return;
    }
    let lo = pci_read32(bus, dev, func, PCI_BAR2_OFF);
    let hi = pci_read32(bus, dev, func, PCI_BAR2_OFF + 4);
    let bar = pci_mem_bar_addr(lo, hi);
    if bar == 0 || lo == 0xFFFF_FFFF {
        APE_MMIO.store(0, Ordering::Relaxed);
        return;
    }
    enable_mem(bus, dev, func);
    APE_MMIO.store(bar, Ordering::Relaxed);
    APE_LOCKNUM.store(ape_phy_lock_num(func), Ordering::Relaxed);
    APE_PCI_FN.store(u32::from(func), Ordering::Relaxed);
    // SAFETY: firmware BAR2 APE window; identity-mapped like BAR0.
    // KANI-TARGET: ape_phy_lock_num / ape_lock_req_bit / pci_mem_bar_addr (host).
    unsafe {
        ape_lock_init();
        ape_host_driver_start();
    }
}

#[cfg(feature = "uefi-bin")]
unsafe fn ape_write(off: u32, val: u32) {
    let base = APE_MMIO.load(Ordering::Relaxed);
    if base == 0 {
        return;
    }
    mmio_write(base, off, val);
}

#[cfg(feature = "uefi-bin")]
unsafe fn ape_read(off: u32) -> u32 {
    let base = APE_MMIO.load(Ordering::Relaxed);
    if base == 0 {
        return 0;
    }
    mmio_read(base, off)
}

/// Linux `tg3_ape_lock_init`: drop stale grants on PHY0..=GPIO.
#[cfg(feature = "uefi-bin")]
unsafe fn ape_lock_init() {
    let fn_ = APE_PCI_FN.load(Ordering::Relaxed) as u8;
    for locknum in TG3_APE_LOCK_PHY0..=TG3_APE_LOCK_GPIO {
        let bit = ape_lock_init_grant_bit(locknum, fn_);
        ape_write(ape_per_lock_grant(locknum), bit);
    }
}

#[cfg(feature = "uefi-bin")]
fn ape_fw_ready_now() -> bool {
    let base = APE_MMIO.load(Ordering::Relaxed);
    if base == 0 {
        return false;
    }
    // SAFETY: APE_MMIO is BAR2 from bind_ape.
    // KANI-TARGET: ape_fw_ready (host).
    unsafe { ape_fw_ready(ape_read(TG3_APE_SEG_SIG), ape_read(TG3_APE_FW_STATUS)) }
}

#[cfg(feature = "uefi-bin")]
fn ape_ncsi_now() -> bool {
    let base = APE_MMIO.load(Ordering::Relaxed);
    if base == 0 {
        return false;
    }
    // SAFETY: APE_MMIO is BAR2 from bind_ape.
    // KANI-TARGET: ape_ncsi_enabled (host).
    unsafe { ape_ncsi_enabled(ape_read(TG3_APE_FW_FEATURES)) }
}

#[cfg(feature = "uefi-bin")]
fn ape_lock_now() -> bool {
    let base = APE_MMIO.load(Ordering::Relaxed);
    if base == 0 {
        return true;
    }
    let locknum = APE_LOCKNUM.load(Ordering::Relaxed);
    let fn_ = APE_PCI_FN.load(Ordering::Relaxed) as u8;
    let bit = ape_lock_req_bit(locknum, fn_);
    let req = ape_per_lock_req(locknum);
    let gnt = ape_per_lock_grant(locknum);
    // SAFETY: APE_MMIO is BAR2 from bind_ape.
    // KANI-TARGET: ape_lock_req_bit (host).
    unsafe {
        ape_write(req, bit);
        for _ in 0..100 {
            if ape_read(gnt) == bit {
                return true;
            }
            tsc_spin_us(10);
        }
        ape_write(gnt, bit);
    }
    false
}

#[cfg(feature = "uefi-bin")]
fn ape_unlock_now() {
    let base = APE_MMIO.load(Ordering::Relaxed);
    if base == 0 {
        return;
    }
    let locknum = APE_LOCKNUM.load(Ordering::Relaxed);
    let fn_ = APE_PCI_FN.load(Ordering::Relaxed) as u8;
    let bit = ape_lock_req_bit(locknum, fn_);
    unsafe {
        ape_write(ape_per_lock_grant(locknum), bit);
    }
}

/// Linux `tg3_ape_driver_state_change(RESET_KIND_INIT)` (no heartbeat timer).
#[cfg(feature = "uefi-bin")]
unsafe fn ape_host_driver_start() {
    ape_write(TG3_APE_HOST_HEARTBEAT_COUNT, 1);
    ape_write(TG3_APE_HOST_SEG_SIG, APE_HOST_SEG_SIG_MAGIC);
    ape_write(TG3_APE_HOST_SEG_LEN, APE_HOST_SEG_LEN_MAGIC);
    let n = ape_read(TG3_APE_HOST_INIT_COUNT).wrapping_add(1);
    ape_write(TG3_APE_HOST_INIT_COUNT, n);
    ape_write(TG3_APE_HOST_DRIVER_ID, APE_HOST_DRIVER_ID_LINUX);
    ape_write(TG3_APE_HOST_BEHAVIOR, APE_HOST_BEHAV_NO_PHYLOCK);
    ape_write(
        TG3_APE_HOST_HEARTBEAT_INT_MS,
        APE_HOST_HEARTBEAT_INT_DISABLE,
    );
    ape_write(TG3_APE_HOST_DRVR_STATE, TG3_APE_HOST_DRVR_STATE_START);
    let ev =
        APE_EVENT_STATUS_STATE_START | APE_EVENT_STATUS_DRIVER_EVNT | APE_EVENT_STATUS_STATE_CHNGE;
    APE_EVT_SENT.store(ape_send_event(ev), Ordering::Relaxed);
}

/// Linux `tg3_ape_send_event`: skip if APE FW not ready; wait EVENT_PENDING clear.
#[cfg(feature = "uefi-bin")]
unsafe fn ape_send_event(event: u32) -> bool {
    if !ape_fw_ready(ape_read(TG3_APE_SEG_SIG), ape_read(TG3_APE_FW_STATUS)) {
        return false;
    }
    let fn_ = APE_PCI_FN.load(Ordering::Relaxed) as u8;
    let bit = ape_lock_req_bit(TG3_APE_LOCK_MEM, fn_);
    let req = ape_per_lock_req(TG3_APE_LOCK_MEM);
    let gnt = ape_per_lock_grant(TG3_APE_LOCK_MEM);
    let mut locked = false;
    for _ in 0..2000 {
        ape_write(req, bit);
        if ape_read(gnt) == bit {
            if ape_read(TG3_APE_EVENT_STATUS) & APE_EVENT_STATUS_EVENT_PENDING == 0 {
                locked = true;
                break;
            }
            ape_write(gnt, bit);
        }
        tsc_spin_us(10);
    }
    if !locked {
        return false;
    }
    ape_write(TG3_APE_EVENT_STATUS, event | APE_EVENT_STATUS_EVENT_PENDING);
    ape_write(gnt, bit);
    ape_write(TG3_APE_EVENT, APE_EVENT_1);
    true
}

#[cfg(feature = "uefi-bin")]
unsafe fn read_mac(mmio: u64) -> [u8; 6] {
    let hi = mmio_read(mmio, MAC_ADDR_0_HIGH);
    let lo = mmio_read(mmio, MAC_ADDR_0_LOW);
    [
        (hi >> 8) as u8,
        hi as u8,
        (lo >> 24) as u8,
        (lo >> 16) as u8,
        (lo >> 8) as u8,
        lo as u8,
    ]
}

unsafe fn rx_one(n: &mut NicState, dma: &mut DmaArena, out: &mut [u8]) -> Option<usize> {
    let hw_prod = core::ptr::read_volatile(core::ptr::addr_of!(dma.status.idx[0].rx_producer));
    if hw_prod == n.rx_rcb_ptr {
        return None;
    }
    let i = n.rx_rcb_ptr as usize % RING;
    let idx_len = core::ptr::read_volatile(core::ptr::addr_of!(dma.rx_rcb[i].idx_len));
    let flags = core::ptr::read_volatile(core::ptr::addr_of!(dma.rx_rcb[i].type_flags));
    let err = core::ptr::read_volatile(core::ptr::addr_of!(dma.rx_rcb[i].err_vlan));
    let opaque = core::ptr::read_volatile(core::ptr::addr_of!(dma.rx_rcb[i].opaque));
    n.rx_rcb_ptr = n.rx_rcb_ptr.wrapping_add(1);
    mailbox_write(n.mmio, MAILBOX_RCVRET_CON_IDX_0, u32::from(n.rx_rcb_ptr));
    let ncopy = rx_bd_packet_len(idx_len, flags, err)?;
    let std = (opaque & 0xFFFF) as usize % RING;
    let ncopy = ncopy.min(out.len()).min(PKT);
    core::ptr::copy_nonoverlapping(dma.rx_buf[std].as_ptr(), out.as_mut_ptr(), ncopy);
    n.rx_std_prod = (n.rx_std_prod + 1) % RING as u16;
    mailbox_write(n.mmio, MAILBOX_RCV_STD_PROD_IDX, u32::from(n.rx_std_prod));
    Some(ncopy)
}

unsafe fn tx_one(n: &mut NicState, dma: &mut DmaArena, frame: &[u8]) -> bool {
    let hw_cons = core::ptr::read_volatile(core::ptr::addr_of!(dma.status.idx[0].tx_consumer));
    n.tx_cons = hw_cons;
    let prod = n.tx_prod as usize % RING;
    let cons = n.tx_cons as usize % RING;
    if prod.wrapping_add(1) % RING == cons {
        return false;
    }
    let len = frame.len().min(PKT).min(FRAME_MAX);
    if len == 0 {
        return false;
    }
    let i = n.tx_prod as usize % RING;
    core::ptr::copy_nonoverlapping(frame.as_ptr(), dma.tx_buf[i].as_mut_ptr(), len);
    let addr = dma.tx_buf[i].as_ptr() as u64;
    core::ptr::write_volatile(
        core::ptr::addr_of_mut!(dma.tx[i].addr_hi),
        (addr >> 32) as u32,
    );
    core::ptr::write_volatile(core::ptr::addr_of_mut!(dma.tx[i].addr_lo), addr as u32);
    core::ptr::write_volatile(core::ptr::addr_of_mut!(dma.tx[i].vlan_tag), 0);
    core::sync::atomic::fence(Ordering::SeqCst);
    core::ptr::write_volatile(
        core::ptr::addr_of_mut!(dma.tx[i].len_flags),
        (len as u32) << TXD_LEN_SHIFT | TXD_FLAG_END,
    );
    n.tx_prod = n.tx_prod.wrapping_add(1);
    mailbox_write(n.mmio, MAILBOX_SNDHOST_PROD_IDX_0, u32::from(n.tx_prod));
    true
}

unsafe fn mailbox_write(mmio: u64, base: u32, val: u32) {
    mmio_write(mmio, base + TG3_64BIT_REG_LOW, val);
}

#[cfg(feature = "uefi-bin")]
fn sram_write(bus: u8, dev: u8, func: u8, off: u32, val: u32) {
    pci_write32(bus, dev, func, TG3PCI_MEM_WIN_BASE_ADDR, off);
    pci_write32(bus, dev, func, TG3PCI_MEM_WIN_DATA, val);
    pci_write32(bus, dev, func, TG3PCI_MEM_WIN_BASE_ADDR, 0);
}

#[cfg(feature = "uefi-bin")]
fn sram_read(bus: u8, dev: u8, func: u8, off: u32) -> u32 {
    pci_write32(bus, dev, func, TG3PCI_MEM_WIN_BASE_ADDR, off);
    let v = pci_read32(bus, dev, func, TG3PCI_MEM_WIN_DATA);
    pci_write32(bus, dev, func, TG3PCI_MEM_WIN_BASE_ADDR, 0);
    v
}

#[inline]
unsafe fn mmio_read(base: u64, off: u32) -> u32 {
    // SAFETY: `base` is the firmware-assigned BCM5720 BAR0; `off` is a tg3 register.
    // KANI-TARGET: mocked RX BD parse only.
    core::ptr::read_volatile((base + u64::from(off)) as *const u32)
}

#[inline]
unsafe fn mmio_write(base: u64, off: u32, val: u32) {
    // SAFETY: `base` is the firmware-assigned BCM5720 BAR0; `off` is a tg3 register.
    // KANI-TARGET: mocked RX BD parse only.
    core::ptr::write_volatile((base + u64::from(off)) as *mut u32, val);
}

#[inline]
unsafe fn mmio_write64(base: u64, off: u32, val: u64) {
    mmio_write(base, off, (val >> 32) as u32);
    mmio_write(base, off + 4, val as u32);
}

#[cfg(feature = "uefi-bin")]
unsafe fn wait_bits(mmio: u64, reg: u32, bits: u32, tries: u32) -> bool {
    for _ in 0..tries {
        if mmio_read(mmio, reg) & bits == bits {
            return true;
        }
        tsc_spin_ms(1);
    }
    false
}

#[cfg(feature = "uefi-bin")]
unsafe fn wait_eq(mmio: u64, reg: u32, want: u32, tries: u32) -> bool {
    for _ in 0..tries {
        if mmio_read(mmio, reg) == want {
            return true;
        }
        tsc_spin_ms(1);
    }
    false
}

#[cfg(feature = "uefi-bin")]
fn tsc_spin_us(us: u32) {
    let ticks = (us as u64).saturating_mul(2_100);
    let start = crate::arch::cpu::rdtsc();
    while crate::arch::cpu::rdtsc().wrapping_sub(start) < ticks {
        core::hint::spin_loop();
    }
}

#[cfg(feature = "uefi-bin")]
fn tsc_spin_ms(ms: u32) {
    tsc_spin_us(ms.saturating_mul(1000));
}

#[cfg(feature = "uefi-bin")]
fn serial_step(s: &str) {
    crate::boot::serial::write_str(s);
}

#[cfg(feature = "uefi-bin")]
fn write_mac(mac: [u8; 6]) {
    for (i, b) in mac.iter().enumerate() {
        if i > 0 {
            crate::boot::serial::write_byte(b':');
        }
        write_hex_u8(*b);
    }
}

#[cfg(feature = "uefi-bin")]
fn write_hex_u8(b: u8) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    crate::boot::serial::write_byte(HEX[(b >> 4) as usize]);
    crate::boot::serial::write_byte(HEX[(b & 0xf) as usize]);
}

#[cfg(feature = "uefi-bin")]
fn write_hex_u16(n: u16) {
    write_hex_u8((n >> 8) as u8);
    write_hex_u8(n as u8);
}

#[cfg(feature = "uefi-bin")]
fn write_u16_dec(n: u16) {
    if n == 0 {
        crate::boot::serial::write_byte(b'0');
        return;
    }
    let mut buf = [0u8; 5];
    let mut i = 5;
    let mut v = n;
    while v > 0 {
        i -= 1;
        buf[i] = b'0' + (v % 10) as u8;
        v /= 10;
    }
    for b in &buf[i..] {
        crate::boot::serial::write_byte(*b);
    }
}

#[cfg(feature = "uefi-bin")]
fn write_hex_u32(n: u32) {
    write_hex_u8((n >> 24) as u8);
    write_hex_u8((n >> 16) as u8);
    write_hex_u8((n >> 8) as u8);
    write_hex_u8(n as u8);
}

const _: () = assert!(core::mem::size_of::<HwStatus>() == TG3_HW_STATUS_SIZE);
const _: () = assert!(core::mem::size_of::<RxBd>() == 32);
const _: () = assert!(core::mem::size_of::<TxBd>() == 16);

#[cfg(test)]
#[path = "bcm5720_mmio_test.rs"]
mod bcm5720_mmio_test;
