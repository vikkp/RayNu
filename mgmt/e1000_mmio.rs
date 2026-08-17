//! QEMU e1000 (82540EM) MMIO/DMA — **the only NIC `unsafe` module** (ADR-013 C).
//!
//! Pillar: [Z]
//! Proven Core: **outside** (ADR-002 / ADR-013)
//!
//! All PCI config (`0xCF8`/`0xCFC`), BAR MMIO, and RX/TX ring DMA for the
//! management NIC live here. Safe helpers parse mocked descriptors so host
//! tests / Kani never touch real MMIO.
//!
//! Phase C binds **one** id: Intel `8086:100e` (QEMU `-device e1000`). Do not
//! guess Broadcom/X710/i40e here (Phase 0/D census).

use core::sync::atomic::{AtomicBool, Ordering};

/// Intel 82540EM as QEMU `e1000` (not `e1000e`).
pub const E1000_VENDOR: u16 = 0x8086;
pub const E1000_DEVICE: u16 = 0x100E;

pub const RX_DESC_DD: u8 = 1 << 0;
pub const RX_DESC_EOP: u8 = 1 << 1;
pub const TX_DESC_DD: u8 = 1 << 0;

const RX_DESC_CMD_EOP: u8 = 1 << 0;
const TX_CMD_EOP: u8 = 1 << 0;
const TX_CMD_IFCS: u8 = 1 << 1;
const TX_CMD_RS: u8 = 1 << 3;

/// RX/TX ring length (16-byte descriptors; `RDLEN`/`TDLEN` multiple of 128).
pub const RING: usize = 8;
pub const PKT: usize = 2048;
pub const FRAME_MAX: usize = 1514;

const REG_CTRL: u32 = 0x00000;
const REG_STATUS: u32 = 0x00008;
const REG_EERD: u32 = 0x00014;
const REG_ICR: u32 = 0x000C0;
const REG_IMS: u32 = 0x000D0;
const REG_IMC: u32 = 0x000D8;
const REG_RCTL: u32 = 0x00100;
const REG_TCTL: u32 = 0x00400;
const REG_TIPG: u32 = 0x00410;
const REG_RDBAL: u32 = 0x02800;
const REG_RDBAH: u32 = 0x02804;
const REG_RDLEN: u32 = 0x02808;
const REG_RDH: u32 = 0x02810;
const REG_RDT: u32 = 0x02818;
const REG_TDBAL: u32 = 0x03800;
const REG_TDBAH: u32 = 0x03804;
const REG_TDLEN: u32 = 0x03808;
const REG_TDH: u32 = 0x03810;
const REG_TDT: u32 = 0x03818;
const REG_MTA: u32 = 0x05200;
const REG_RAL: u32 = 0x05400;
const REG_RAH: u32 = 0x05404;

const CTRL_FD: u32 = 1 << 0;
const CTRL_SLU: u32 = 1 << 6;
const CTRL_RST: u32 = 1 << 26;
const STATUS_LU: u32 = 1 << 1;
const RCTL_EN: u32 = 1 << 1;
const RCTL_UPE: u32 = 1 << 3;
const RCTL_MPE: u32 = 1 << 4;
const RCTL_BAM: u32 = 1 << 15;
const RCTL_SECRC: u32 = 1 << 26;
const TCTL_EN: u32 = 1 << 1;
const TCTL_PSP: u32 = 1 << 3;
const TCTL_CT_10: u32 = 0x10 << 4;
const TCTL_COLD_40: u32 = 0x40 << 12;
const RAH_AV: u32 = 1 << 31;
const EERD_START: u32 = 1 << 0;
const EERD_DONE: u32 = 1 << 4;

const PCI_CONFIG_ADDR: u16 = 0xCF8;
const PCI_CONFIG_DATA: u16 = 0xCFC;
const PCI_CMD_MEM: u16 = 1 << 1;
const PCI_CMD_BUSMASTER: u16 = 1 << 2;

/// True when PCI id is the Phase C QEMU e1000 (82540EM).
pub fn pci_id_is_qemu_e1000(vendor: u16, device: u16) -> bool {
    vendor == E1000_VENDOR && device == E1000_DEVICE
}

/// Parse a mocked RX descriptor. Returns payload length or `None` if not a
/// complete good frame. Used by host tests / Kani — no MMIO.
///
/// INVARIANTS:
/// - `None` unless DD+EOP, zero errors, and `0 < length <= FRAME_MAX`
/// - Never panics
pub fn rx_desc_packet_len(status: u8, errors: u8, length: u16) -> Option<usize> {
    if status & RX_DESC_DD == 0 {
        return None;
    }
    if status & RX_DESC_EOP == 0 {
        return None;
    }
    if errors != 0 {
        return None;
    }
    let n = length as usize;
    if n == 0 || n > FRAME_MAX {
        return None;
    }
    Some(n)
}

/// TX descriptor completed (DD).
pub fn tx_desc_done(status: u8) -> bool {
    status & TX_DESC_DD != 0
}

/// Parse a mocked 16-byte RX descriptor (little-endian layout). Host/Miri/fuzz.
pub fn parse_mocked_rx_desc_bytes(raw: &[u8; 16]) -> Option<usize> {
    let length = u16::from_le_bytes([raw[8], raw[9]]);
    let status = raw[12];
    let errors = raw[13];
    rx_desc_packet_len(status, errors, length)
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
struct RxDesc {
    addr: u64,
    length: u16,
    csum: u16,
    status: u8,
    errors: u8,
    special: u16,
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
struct TxDesc {
    addr: u64,
    length: u16,
    cso: u8,
    cmd: u8,
    status: u8,
    css: u8,
    special: u16,
}

#[repr(C, align(4096))]
struct DmaArena {
    rx: [RxDesc; RING],
    tx: [TxDesc; RING],
    rx_buf: [[u8; PKT]; RING],
    tx_buf: [[u8; PKT]; RING],
}

impl DmaArena {
    const fn zeroed() -> Self {
        Self {
            rx: [RxDesc {
                addr: 0,
                length: 0,
                csum: 0,
                status: 0,
                errors: 0,
                special: 0,
            }; RING],
            tx: [TxDesc {
                addr: 0,
                length: 0,
                cso: 0,
                cmd: 0,
                status: 0,
                css: 0,
                special: 0,
            }; RING],
            rx_buf: [[0; PKT]; RING],
            tx_buf: [[0; PKT]; RING],
        }
    }
}

struct NicState {
    mmio: u64,
    mac: [u8; 6],
    rx_clean: usize,
    tx_use: usize,
    tx_clean: usize,
}

/// JUSTIFICATION (global): one mgmt NIC, BSP-only until ADR-013 Phase E arena.
/// `NIC_LOCK` is a ticket-style exclusive flag (not a sleeping lock).
static NIC_LOCK: AtomicBool = AtomicBool::new(false);
static mut DMA: DmaArena = DmaArena::zeroed();
static mut NIC: Option<NicState> = None;

fn with_nic<R>(f: impl FnOnce(&mut NicState, &mut DmaArena) -> R) -> Option<R> {
    if NIC_LOCK.swap(true, Ordering::Acquire) {
        return None;
    }
    // SAFETY: lock held; BSP-only mgmt path; DMA/NIC are the exclusive owner.
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

/// True when PCI scan finds `8086:100e`. Safe to call PRE-EBS (port I/O).
#[cfg(feature = "uefi-bin")]
pub fn qemu_e1000_present() -> bool {
    find_e1000().is_some()
}

#[cfg(not(feature = "uefi-bin"))]
pub fn qemu_e1000_present() -> bool {
    false
}

/// Reset + ring init. Identity-mapped BAR (UEFI page tables).
#[cfg(feature = "uefi-bin")]
pub fn init_e1000() -> Result<[u8; 6], E1000Error> {
    let (bus, dev, func, bar) = find_e1000().ok_or(E1000Error::NotFound)?;
    enable_bus_master(bus, dev, func);
    if bar == 0 {
        return Err(E1000Error::NoBar);
    }

    if NIC_LOCK.swap(true, Ordering::Acquire) {
        return Err(E1000Error::Busy);
    }

    // SAFETY: lock held; BAR programmed by firmware; identity-mapped MMIO.
    // DMA arena is .bss in the EFI image (identity-mapped RAM).
    // KANI-TARGET: mocked descriptor parse only — not this MMIO path.
    let mac = unsafe {
        let dma = &mut *core::ptr::addr_of_mut!(DMA);
        *dma = DmaArena::zeroed();
        let mac = hw_init(bar, dma)?;
        NIC = Some(NicState {
            mmio: bar,
            mac,
            rx_clean: 0,
            tx_use: 0,
            tx_clean: 0,
        });
        mac
    };
    NIC_LOCK.store(false, Ordering::Release);
    Ok(mac)
}

#[cfg(not(feature = "uefi-bin"))]
pub fn init_e1000() -> Result<[u8; 6], E1000Error> {
    Err(E1000Error::NotFound)
}

/// Copy one completed RX frame out of the DMA ring (poll mode).
pub fn receive_frame(out: &mut [u8]) -> Option<usize> {
    with_nic(|n, dma| unsafe { rx_one(n, dma, out) }).flatten()
}

/// Queue one Ethernet frame on the TX ring. Drops if the ring is full.
pub fn transmit_frame(frame: &[u8]) -> bool {
    with_nic(|n, dma| unsafe { tx_one(n, dma, frame) }).unwrap_or(false)
}

pub fn mac_address() -> Option<[u8; 6]> {
    with_nic(|n, _| n.mac)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum E1000Error {
    NotFound,
    NoBar,
    ResetTimeout,
    EepromTimeout,
    Busy,
}

#[cfg(feature = "uefi-bin")]
fn find_e1000() -> Option<(u8, u8, u8, u64)> {
    // QEMU q35 places e1000 on bus 0; scan 0..=1 only (lab).
    for bus in 0u8..=1 {
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
                if pci_id_is_qemu_e1000(vendor, device) {
                    let bar = pci_read32(bus, dev, func, 0x10) as u64;
                    let mmio = bar & !0xF;
                    return Some((bus, dev, func, mmio));
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

#[cfg(feature = "uefi-bin")]
fn enable_bus_master(bus: u8, dev: u8, func: u8) {
    let cmd = pci_read32(bus, dev, func, 0x04) as u16;
    let next = cmd | PCI_CMD_MEM | PCI_CMD_BUSMASTER;
    let rest = pci_read32(bus, dev, func, 0x04) & 0xFFFF_0000;
    pci_write32(bus, dev, func, 0x04, rest | u32::from(next));
}

#[cfg(feature = "uefi-bin")]
unsafe fn hw_init(mmio: u64, dma: &mut DmaArena) -> Result<[u8; 6], E1000Error> {
    mmio_write(mmio, REG_IMC, 0xFFFF_FFFF);
    let _ = mmio_read(mmio, REG_ICR);

    mmio_write(mmio, REG_CTRL, mmio_read(mmio, REG_CTRL) | CTRL_RST);
    tsc_spin_ms(10);
    let mut waited = 0u32;
    while mmio_read(mmio, REG_CTRL) & CTRL_RST != 0 {
        tsc_spin_ms(1);
        waited += 1;
        if waited > 100 {
            return Err(E1000Error::ResetTimeout);
        }
    }

    mmio_write(
        mmio,
        REG_CTRL,
        mmio_read(mmio, REG_CTRL) | CTRL_SLU | CTRL_FD,
    );
    mmio_write(mmio, REG_IMC, 0xFFFF_FFFF);
    let _ = mmio_read(mmio, REG_ICR);
    mmio_write(mmio, REG_IMS, 0);

    for i in 0..128u32 {
        mmio_write(mmio, REG_MTA + i * 4, 0);
    }

    let mac = read_mac(mmio)?;
    let ral = u32::from(mac[0])
        | u32::from(mac[1]) << 8
        | u32::from(mac[2]) << 16
        | u32::from(mac[3]) << 24;
    let rah = u32::from(mac[4]) | u32::from(mac[5]) << 8 | RAH_AV;
    mmio_write(mmio, REG_RAL, ral);
    mmio_write(mmio, REG_RAH, rah);

    let rx_phys = dma.rx.as_ptr() as u64;
    let tx_phys = dma.tx.as_ptr() as u64;
    for i in 0..RING {
        dma.rx[i].addr = dma.rx_buf[i].as_ptr() as u64;
        dma.rx[i].length = 0;
        dma.rx[i].status = 0;
        dma.rx[i].errors = 0;
        dma.tx[i].addr = dma.tx_buf[i].as_ptr() as u64;
        dma.tx[i].status = 0;
        dma.tx[i].cmd = 0;
        dma.tx[i].length = 0;
    }

    mmio_write(mmio, REG_RDBAL, rx_phys as u32);
    mmio_write(mmio, REG_RDBAH, (rx_phys >> 32) as u32);
    mmio_write(mmio, REG_RDLEN, (RING * core::mem::size_of::<RxDesc>()) as u32);
    mmio_write(mmio, REG_RDH, 0);
    mmio_write(mmio, REG_RDT, (RING - 1) as u32);

    mmio_write(mmio, REG_TDBAL, tx_phys as u32);
    mmio_write(mmio, REG_TDBAH, (tx_phys >> 32) as u32);
    mmio_write(mmio, REG_TDLEN, (RING * core::mem::size_of::<TxDesc>()) as u32);
    mmio_write(mmio, REG_TDH, 0);
    mmio_write(mmio, REG_TDT, 0);

    // TIPG: IPGT=10, IPGR1=8, IPGR2=6 (82540 copper).
    mmio_write(mmio, REG_TIPG, 0x0060_200A);
    mmio_write(
        mmio,
        REG_TCTL,
        TCTL_EN | TCTL_PSP | TCTL_CT_10 | TCTL_COLD_40,
    );
    mmio_write(
        mmio,
        REG_RCTL,
        RCTL_EN | RCTL_UPE | RCTL_MPE | RCTL_BAM | RCTL_SECRC,
    );

    tsc_spin_ms(20);
    let _ = mmio_read(mmio, REG_STATUS) & STATUS_LU;
    let _ = RX_DESC_CMD_EOP;
    Ok(mac)
}

#[cfg(feature = "uefi-bin")]
unsafe fn read_mac(mmio: u64) -> Result<[u8; 6], E1000Error> {
    let w0 = eeprom_read(mmio, 0)?;
    let w1 = eeprom_read(mmio, 1)?;
    let w2 = eeprom_read(mmio, 2)?;
    let mut mac = [
        w0 as u8,
        (w0 >> 8) as u8,
        w1 as u8,
        (w1 >> 8) as u8,
        w2 as u8,
        (w2 >> 8) as u8,
    ];
    if mac.iter().all(|&b| b == 0) || mac.iter().all(|&b| b == 0xff) {
        let ral = mmio_read(mmio, REG_RAL);
        let rah = mmio_read(mmio, REG_RAH);
        mac = [
            ral as u8,
            (ral >> 8) as u8,
            (ral >> 16) as u8,
            (ral >> 24) as u8,
            rah as u8,
            (rah >> 8) as u8,
        ];
    }
    if mac.iter().all(|&b| b == 0) {
        return Err(E1000Error::EepromTimeout);
    }
    Ok(mac)
}

#[cfg(feature = "uefi-bin")]
unsafe fn eeprom_read(mmio: u64, addr: u8) -> Result<u16, E1000Error> {
    mmio_write(
        mmio,
        REG_EERD,
        EERD_START | (u32::from(addr) << 8),
    );
    for _ in 0..1000 {
        let v = mmio_read(mmio, REG_EERD);
        if v & EERD_DONE != 0 {
            return Ok((v >> 16) as u16);
        }
        tsc_spin_ms(1);
    }
    Err(E1000Error::EepromTimeout)
}

unsafe fn rx_one(n: &mut NicState, dma: &mut DmaArena, out: &mut [u8]) -> Option<usize> {
    let i = n.rx_clean;
    let st = core::ptr::read_volatile(core::ptr::addr_of!(dma.rx[i].status));
    let err = core::ptr::read_volatile(core::ptr::addr_of!(dma.rx[i].errors));
    let len = core::ptr::read_volatile(core::ptr::addr_of!(dma.rx[i].length));
    let ncopy = rx_desc_packet_len(st, err, len)?;
    let ncopy = ncopy.min(out.len()).min(PKT);
    let src = dma.rx_buf[i].as_ptr();
    core::ptr::copy_nonoverlapping(src, out.as_mut_ptr(), ncopy);
    core::ptr::write_volatile(core::ptr::addr_of_mut!(dma.rx[i].status), 0);
    core::ptr::write_volatile(core::ptr::addr_of_mut!(dma.rx[i].errors), 0);
    core::ptr::write_volatile(core::ptr::addr_of_mut!(dma.rx[i].length), 0);
    core::ptr::write_volatile(
        core::ptr::addr_of_mut!(dma.rx[i].addr),
        dma.rx_buf[i].as_ptr() as u64,
    );
    n.rx_clean = (i + 1) % RING;
    let tail = n.rx_clean.wrapping_add(RING - 1) % RING;
    mmio_write(n.mmio, REG_RDT, tail as u32);
    Some(ncopy)
}

unsafe fn tx_one(n: &mut NicState, dma: &mut DmaArena, frame: &[u8]) -> bool {
    tx_reap(n, dma);
    let next = (n.tx_use + 1) % RING;
    if next == n.tx_clean {
        return false;
    }
    let len = frame.len().min(PKT).min(FRAME_MAX);
    if len == 0 {
        return false;
    }
    let i = n.tx_use;
    core::ptr::copy_nonoverlapping(frame.as_ptr(), dma.tx_buf[i].as_mut_ptr(), len);
    core::ptr::write_volatile(
        core::ptr::addr_of_mut!(dma.tx[i].addr),
        dma.tx_buf[i].as_ptr() as u64,
    );
    core::ptr::write_volatile(core::ptr::addr_of_mut!(dma.tx[i].length), len as u16);
    core::ptr::write_volatile(core::ptr::addr_of_mut!(dma.tx[i].cso), 0);
    core::ptr::write_volatile(core::ptr::addr_of_mut!(dma.tx[i].css), 0);
    core::ptr::write_volatile(core::ptr::addr_of_mut!(dma.tx[i].special), 0);
    core::ptr::write_volatile(core::ptr::addr_of_mut!(dma.tx[i].status), 0);
    core::sync::atomic::fence(Ordering::SeqCst);
    core::ptr::write_volatile(
        core::ptr::addr_of_mut!(dma.tx[i].cmd),
        TX_CMD_EOP | TX_CMD_IFCS | TX_CMD_RS,
    );
    n.tx_use = next;
    mmio_write(n.mmio, REG_TDT, n.tx_use as u32);
    true
}

unsafe fn tx_reap(n: &mut NicState, dma: &mut DmaArena) {
    while n.tx_clean != n.tx_use {
        let i = n.tx_clean;
        let st = core::ptr::read_volatile(core::ptr::addr_of!(dma.tx[i].status));
        if !tx_desc_done(st) {
            break;
        }
        core::ptr::write_volatile(core::ptr::addr_of_mut!(dma.tx[i].status), 0);
        n.tx_clean = (i + 1) % RING;
    }
}

#[cfg(feature = "uefi-bin")]
pub(crate) fn pci_read32(bus: u8, dev: u8, func: u8, offset: u8) -> u32 {
    let addr = 0x8000_0000u32
        | (u32::from(bus) << 16)
        | (u32::from(dev) << 11)
        | (u32::from(func) << 8)
        | (u32::from(offset) & 0xFC);
    // SAFETY: PCI config mechanism #1; unique addr/data pair; no MMIO.
    // KANI-TARGET: mocked ids via pci_id_is_qemu_e1000, not this port I/O.
    unsafe {
        outl(PCI_CONFIG_ADDR, addr);
        inl(PCI_CONFIG_DATA)
    }
}

#[cfg(feature = "uefi-bin")]
fn pci_write32(bus: u8, dev: u8, func: u8, offset: u8, value: u32) {
    let addr = 0x8000_0000u32
        | (u32::from(bus) << 16)
        | (u32::from(dev) << 11)
        | (u32::from(func) << 8)
        | (u32::from(offset) & 0xFC);
    // SAFETY: PCI config mechanism #1 write of command/BAR; offset 4-byte aligned.
    // KANI-TARGET: not executed under Kani.
    unsafe {
        outl(PCI_CONFIG_ADDR, addr);
        outl(PCI_CONFIG_DATA, value);
    }
}

#[inline]
unsafe fn mmio_read(base: u64, off: u32) -> u32 {
    // SAFETY: `base` is the firmware-assigned e1000 BAR; `off` is a known register.
    // KANI-TARGET: mocked RX/TX parse only.
    core::ptr::read_volatile((base + u64::from(off)) as *const u32)
}

#[inline]
unsafe fn mmio_write(base: u64, off: u32, val: u32) {
    // SAFETY: `base` is the firmware-assigned e1000 BAR; `off` is a known register.
    // KANI-TARGET: mocked RX/TX parse only.
    core::ptr::write_volatile((base + u64::from(off)) as *mut u32, val);
}

#[cfg(feature = "uefi-bin")]
#[inline]
unsafe fn outl(port: u16, val: u32) {
    core::arch::asm!(
        "out dx, eax",
        in("dx") port,
        in("eax") val,
        options(nomem, nostack, preserves_flags)
    );
}

#[cfg(feature = "uefi-bin")]
#[inline]
unsafe fn inl(port: u16) -> u32 {
    let val: u32;
    core::arch::asm!(
        "in eax, dx",
        out("eax") val,
        in("dx") port,
        options(nomem, nostack, preserves_flags)
    );
    val
}

/// ~1 ms spin without Boot Services (`stall` is invalid after EBS).
#[cfg(feature = "uefi-bin")]
fn tsc_spin_ms(ms: u32) {
    let ticks = (ms as u64).saturating_mul(2_100_000);
    let start = crate::arch::cpu::rdtsc();
    while crate::arch::cpu::rdtsc().wrapping_sub(start) < ticks {
        core::hint::spin_loop();
    }
}

#[cfg(test)]
#[path = "e1000_mmio_test.rs"]
mod e1000_mmio_test;
