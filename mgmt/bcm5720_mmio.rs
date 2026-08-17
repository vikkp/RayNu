//! BCM5720 (`14e4:165f`) MMIO/DMA — iron NIC `unsafe` module (ADR-013 Phase D).
//!
//! Pillar: [Z] [D]
//! Proven Core: **outside** (ADR-002 / ADR-013)
//!
//! All BAR0 MMIO, mailbox, SRAM window, and RX/TX ring DMA for the R640
//! census NIC live here. Bind **one** function: `01:00.0` (`func == 0`).
//! Do not bind `01:00.1`. Do not start X710/i40e.
//!
//! Register map: Broadcom Tigon3 / Linux `tg3` / iPXE `tg3.h`. Poll-mode;
//! MSI-X is not enabled (ADR-013). Host tests parse mocked RX BDs only.

use crate::mgmt::pci_census::{pci_id_is_iron_census, IRON_CENSUS_DEVICE, IRON_CENSUS_VENDOR};
use core::sync::atomic::{AtomicBool, Ordering};

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
const MAC_ADDR_0_HIGH: u32 = 0x410;
const MAC_ADDR_0_LOW: u32 = 0x414;
const MAC_RX_MTU_SIZE: u32 = 0x43C;
const MAC_TX_MODE: u32 = 0x45C;
const MAC_RX_MODE: u32 = 0x468;
const MAC_MODE_PORT_MODE_GMII: u32 = 0x08;
const MAC_MODE_RXSTAT_ENABLE: u32 = 0x800;
const MAC_MODE_TXSTAT_ENABLE: u32 = 0x4000;
const MAC_MODE_TDE_ENABLE: u32 = 0x20_0000;
const MAC_MODE_RDE_ENABLE: u32 = 0x40_0000;
const MAC_MODE_FHDE_ENABLE: u32 = 0x80_0000;
const TX_MODE_ENABLE: u32 = 0x02;
const RX_MODE_ENABLE: u32 = 0x02;

const RCVLPC_MODE: u32 = 0x2000;
const RCVDBDI_MODE: u32 = 0x2400;
const RCVDBDI_STD_BD: u32 = 0x2450;
const RCVCC_MODE: u32 = 0x3000;
const RCVLSC_MODE: u32 = 0x3400;
const SNDDATAI_MODE: u32 = 0x0C00;
const SNDBDS_MODE: u32 = 0x1400;
const SNDBDI_MODE: u32 = 0x1800;
const HOSTCC_MODE: u32 = 0x3C00;
const HOSTCC_STATUS_BLK_HOST_ADDR: u32 = 0x3C38;
const HOSTCC_RXCOL_TICKS: u32 = 0x3C08;
const HOSTCC_TXCOL_TICKS: u32 = 0x3C0C;
const HOSTCC_RXMAX_FRAMES: u32 = 0x3C10;
const HOSTCC_TXMAX_FRAMES: u32 = 0x3C14;
const BUFMGR_MODE: u32 = 0x4400;
const BUFMGR_MB_POOL_ADDR: u32 = 0x4408;
const BUFMGR_MB_POOL_SIZE: u32 = 0x440C;
const BUFMGR_MB_RDMA_LOW_WATER: u32 = 0x4410;
const BUFMGR_MB_MACRX_LOW_WATER: u32 = 0x4414;
const BUFMGR_MB_HIGH_WATER: u32 = 0x4418;
const BUFMGR_DMA_DESC_POOL_ADDR: u32 = 0x442C;
const BUFMGR_DMA_DESC_POOL_SIZE: u32 = 0x4430;
const BUFMGR_DMA_LOW_WATER: u32 = 0x4434;
const BUFMGR_DMA_HIGH_WATER: u32 = 0x4438;
const MEMARB_MODE: u32 = 0x4000;
const FTQ_RESET: u32 = 0x5C00;
const GRC_MODE: u32 = 0x6800;
const GRC_MISC_CFG: u32 = 0x6804;

const MODE_RESET: u32 = 0x01;
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

const NIC_SRAM_SEND_RCB: u32 = 0x100;
const NIC_SRAM_RCV_RET_RCB: u32 = 0x200;
const NIC_SRAM_STATS_BLK: u32 = 0x300;
const NIC_SRAM_FIRMWARE_MBOX: u32 = 0xB50;
const NIC_SRAM_FIRMWARE_MBOX_MAGIC1: u32 = 0x4B65_7654;
const NIC_SRAM_TX_BUFFER_DESC: u32 = 0x4000;
const NIC_SRAM_RX_BUFFER_DESC: u32 = 0x6000;
const NIC_SRAM_MBUF_POOL_BASE: u32 = 0x8000;
const NIC_SRAM_MBUF_POOL_SIZE96: u32 = 0x1_8000;
const NIC_SRAM_DMA_DESC_POOL_BASE: u32 = 0x2000;
const NIC_SRAM_DMA_DESC_POOL_SIZE: u32 = 0x2000;

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

/// True when PCI scan finds `14e4:165f` function 0.
#[cfg(feature = "uefi-bin")]
pub fn bcm5720_present() -> bool {
    find_bcm5720().is_some()
}

#[cfg(not(feature = "uefi-bin"))]
pub fn bcm5720_present() -> bool {
    false
}

/// Reset + ring init. Identity-mapped BAR (UEFI page tables). Prefers `func=0`.
#[cfg(feature = "uefi-bin")]
pub fn init_bcm5720() -> Result<[u8; 6], Bcm5720Error> {
    let (bus, dev, func, bar) = find_bcm5720().ok_or(Bcm5720Error::NotFound)?;
    enable_bus_master(bus, dev, func);
    if bar == 0 {
        return Err(Bcm5720Error::NoBar);
    }
    if NIC_LOCK.swap(true, Ordering::Acquire) {
        return Err(Bcm5720Error::Busy);
    }
    // SAFETY: lock held; BAR programmed by firmware; identity-mapped MMIO.
    // DMA arena is .bss in the EFI image (identity-mapped RAM).
    // KANI-TARGET: mocked RX BD parse only — not this MMIO path.
    let result = unsafe {
        let dma = &mut *core::ptr::addr_of_mut!(DMA);
        *dma = DmaArena::zeroed();
        match hw_init(bar, bus, dev, func, dma) {
            Ok(mac) => {
                NIC = Some(NicState {
                    mmio: bar,
                    mac,
                    rx_rcb_ptr: 0,
                    rx_std_prod: (RING as u16).saturating_sub(1),
                    tx_prod: 0,
                    tx_cons: 0,
                });
                Ok(mac)
            }
            Err(e) => Err(e),
        }
    };
    NIC_LOCK.store(false, Ordering::Release);
    result
}

#[cfg(not(feature = "uefi-bin"))]
pub fn init_bcm5720() -> Result<[u8; 6], Bcm5720Error> {
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
fn find_bcm5720() -> Option<(u8, u8, u8, u64)> {
    let mut fallback = None;
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
                if pci_id_is_bcm5720(vendor, device) && func == 0 {
                    let bar = (pci_read32(bus, dev, func, 0x10) as u64) & !0xF;
                    if bus == 1 && dev == 0 {
                        return Some((bus, dev, func, bar));
                    }
                    if fallback.is_none() {
                        fallback = Some((bus, dev, func, bar));
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
    fallback
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
) -> Result<[u8; 6], Bcm5720Error> {
    pci_write32(bus, dev, func, TG3PCI_MISC_HOST_CTRL as u8, misc_host_ctrl());
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
    chip_reset(mmio, bus, dev, func)?;
    mmio_write(mmio, MEMARB_MODE, MODE_ENABLE);
    mmio_write(mmio, TG3PCI_MISC_HOST_CTRL, misc_host_ctrl());

    let fw = wait_fw_magic(bus, dev, func);
    serial_step(if fw {
        "boot: HOST-NIC BCM5720 fw-magic=yes\n"
    } else {
        "boot: HOST-NIC BCM5720 fw-magic=timeout (continuing)\n"
    });

    let grc = GRC_MODE_WSWAP_NONFRM_DATA
        | GRC_MODE_WSWAP_DATA
        | GRC_MODE_NOIRQ_ON_SENDS
        | GRC_MODE_NOIRQ_ON_RCV
        | GRC_MODE_HOST_STACKUP
        | GRC_MODE_HOST_SENDBDS
        | GRC_MODE_NO_TX_PHDR_CSUM
        | GRC_MODE_NO_RX_PHDR_CSUM;
    mmio_write(mmio, GRC_MODE, grc);
    mmio_write(mmio, GRC_MISC_CFG, 65 << GRC_MISC_CFG_PRESCALAR_SHIFT);

    mmio_write(mmio, BUFMGR_MB_POOL_ADDR, NIC_SRAM_MBUF_POOL_BASE);
    mmio_write(mmio, BUFMGR_MB_POOL_SIZE, NIC_SRAM_MBUF_POOL_SIZE96);
    mmio_write(mmio, BUFMGR_DMA_DESC_POOL_ADDR, NIC_SRAM_DMA_DESC_POOL_BASE);
    mmio_write(mmio, BUFMGR_DMA_DESC_POOL_SIZE, NIC_SRAM_DMA_DESC_POOL_SIZE);
    mmio_write(mmio, BUFMGR_MB_RDMA_LOW_WATER, 0x50);
    mmio_write(mmio, BUFMGR_MB_MACRX_LOW_WATER, 0x20);
    mmio_write(mmio, BUFMGR_MB_HIGH_WATER, 0x60);
    mmio_write(mmio, BUFMGR_DMA_LOW_WATER, 0x05);
    mmio_write(mmio, BUFMGR_DMA_HIGH_WATER, 0x0A);
    mmio_write(mmio, BUFMGR_MODE, MODE_ENABLE);
    if !wait_bits(mmio, BUFMGR_MODE, MODE_ENABLE, 200) {
        serial_step("boot: WARN — HOST-NIC BCM5720 bufmgr timeout (continuing)\n");
    }
    mmio_write(mmio, FTQ_RESET, 0xFFFF_FFFF);
    mmio_write(mmio, FTQ_RESET, 0);
    let _ = wait_eq(mmio, FTQ_RESET, 0, 200);

    init_rings(dma);
    program_rings(mmio, bus, dev, func, dma);
    enable_mac(mmio, mac_before)?;

    mailbox_write(mmio, MAILBOX_RCV_STD_PROD_IDX, (RING as u32).saturating_sub(1));
    mailbox_write(mmio, MAILBOX_RCVRET_CON_IDX_0, 0);
    mailbox_write(mmio, MAILBOX_SNDHOST_PROD_IDX_0, 0);

    serial_step("boot: HOST-NIC BCM5720 rings armed (poll-mode, MSI-X off)\n");
    Ok(mac_before)
}

#[cfg(feature = "uefi-bin")]
unsafe fn chip_reset(mmio: u64, bus: u8, dev: u8, func: u8) -> Result<(), Bcm5720Error> {
    serial_step("boot: HOST-NIC BCM5720 reset…\n");
    mmio_write(mmio, GRC_MISC_CFG, GRC_MISC_CFG_CORECLK_RESET);
    let _ = pci_read32(bus, dev, func, 0x04);
    tsc_spin_ms(150);
    pci_write32(bus, dev, func, TG3PCI_MISC_HOST_CTRL as u8, misc_host_ctrl());
    enable_bus_master(bus, dev, func);
    tsc_spin_ms(20);
    let id = pci_read32(bus, dev, func, 0);
    if id as u16 != BCM5720_VENDOR {
        return Err(Bcm5720Error::ResetTimeout);
    }
    Ok(())
}

#[cfg(feature = "uefi-bin")]
fn wait_fw_magic(bus: u8, dev: u8, func: u8) -> bool {
    sram_write(bus, dev, func, NIC_SRAM_FIRMWARE_MBOX, NIC_SRAM_FIRMWARE_MBOX_MAGIC1);
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
    mmio_write(mmio, HOSTCC_RXCOL_TICKS, 0x48);
    mmio_write(mmio, HOSTCC_TXCOL_TICKS, 0x48);
    mmio_write(mmio, HOSTCC_RXMAX_FRAMES, 0x01);
    mmio_write(mmio, HOSTCC_TXMAX_FRAMES, 0x01);

    mmio_write64(mmio, RCVDBDI_STD_BD + TG3_BDINFO_HOST_ADDR, std);
    mmio_write(
        mmio,
        RCVDBDI_STD_BD + TG3_BDINFO_MAXLEN_FLAGS,
        (RING as u32) << BDINFO_FLAGS_MAXLEN_SHIFT,
    );
    mmio_write(mmio, RCVDBDI_STD_BD + TG3_BDINFO_NIC_ADDR, NIC_SRAM_RX_BUFFER_DESC);

    for off in (NIC_SRAM_SEND_RCB..NIC_SRAM_RCV_RET_RCB).step_by(TG3_BDINFO_SIZE as usize) {
        sram_write(bus, dev, func, off + TG3_BDINFO_MAXLEN_FLAGS, BDINFO_FLAGS_DISABLED);
    }
    for off in (NIC_SRAM_RCV_RET_RCB..NIC_SRAM_STATS_BLK).step_by(TG3_BDINFO_SIZE as usize) {
        sram_write(bus, dev, func, off + TG3_BDINFO_MAXLEN_FLAGS, BDINFO_FLAGS_DISABLED);
    }
    set_bdinfo(bus, dev, func, NIC_SRAM_SEND_RCB, tx, RING as u32, NIC_SRAM_TX_BUFFER_DESC);
    set_bdinfo(bus, dev, func, NIC_SRAM_RCV_RET_RCB, rcb, RING as u32, 0);

    enable_block(mmio, RCVLPC_MODE);
    enable_block(mmio, RCVDBDI_MODE);
    enable_block(mmio, RCVCC_MODE);
    enable_block(mmio, RCVLSC_MODE);
    enable_block(mmio, SNDDATAI_MODE);
    enable_block(mmio, SNDBDI_MODE);
    enable_block(mmio, SNDBDS_MODE);
    enable_block(mmio, HOSTCC_MODE);
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
    sram_write(bus, dev, func, base + TG3_BDINFO_HOST_ADDR + 4, mapping as u32);
    sram_write(
        bus,
        dev,
        func,
        base + TG3_BDINFO_MAXLEN_FLAGS,
        maxlen << BDINFO_FLAGS_MAXLEN_SHIFT,
    );
    sram_write(bus, dev, func, base + TG3_BDINFO_NIC_ADDR, nic_addr);
}

#[cfg(feature = "uefi-bin")]
unsafe fn enable_block(mmio: u64, reg: u32) {
    mmio_write(mmio, reg, MODE_RESET);
    tsc_spin_ms(1);
    mmio_write(mmio, reg, MODE_ENABLE);
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
    let mac_mode = MAC_MODE_PORT_MODE_GMII
        | MAC_MODE_RXSTAT_ENABLE
        | MAC_MODE_TXSTAT_ENABLE
        | MAC_MODE_TDE_ENABLE
        | MAC_MODE_RDE_ENABLE
        | MAC_MODE_FHDE_ENABLE;
    mmio_write(mmio, MAC_MODE, mac_mode);
    mmio_write(mmio, MAC_RX_MODE, RX_MODE_ENABLE);
    mmio_write(mmio, MAC_TX_MODE, TX_MODE_ENABLE);
    let _ = mmio_read(mmio, MAC_STATUS);
    serial_step("boot: HOST-NIC BCM5720 MAC=");
    write_mac(mac);
    crate::boot::serial::write_byte(b'\n');
    Ok(mac)
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
    core::ptr::write_volatile(core::ptr::addr_of_mut!(dma.tx[i].addr_hi), (addr >> 32) as u32);
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
fn tsc_spin_ms(ms: u32) {
    let ticks = (ms as u64).saturating_mul(2_100_000);
    let start = crate::arch::cpu::rdtsc();
    while crate::arch::cpu::rdtsc().wrapping_sub(start) < ticks {
        core::hint::spin_loop();
    }
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
