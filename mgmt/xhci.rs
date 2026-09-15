//! Minimal xHCI host for USB BOT DurableLun (outside Proven Core).
//!
//! Pillar: [Z] [A] [D]
//! Proven Core: **outside** (ADR-002 / ADR-018)
//! VERIFICATION: L1 host tests for TRB packing + BOT config parse.
//! Live MMIO is UEFI-only and runs **after** ExitBootServices.
//!
//! QEMU `qemu-xhci` + `usb-storage` ≠ Intel PCH on the R640. Not
//! `ISO-INSTALL-OK`. Not iron `RAYNU-V-M8-DISK-PERSIST-OK`. Do not F11.
//!
//! ADR-004: persist backing is virtio-blk / BlockIo only.

use super::usb_bot::{
    next_bot_tag, store_usb_bot_diag, store_usb_bot_diag_unless_kept, store_usb_bot_ready,
    usb_bot_bring_up, usb_bot_last_bar, usb_bot_last_cmpl, usb_bot_last_portsc, usb_bot_last_stage,
    usb_bot_stage_name, UsbBotError, UsbBulk, CSW_LEN,
};

/// Same window as [`crate::mgmt::durable_lun::usb_is_esp_cruzer_window`].
const ESP_CRUZER_MIN_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const ESP_CRUZER_MAX_BYTES: u64 = 8 * 1024 * 1024 * 1024;

fn lun_is_esp_cruzer(bytes: u64) -> bool {
    bytes >= ESP_CRUZER_MIN_BYTES && bytes <= ESP_CRUZER_MAX_BYTES
}

pub const TRB_NORMAL: u32 = 1;
pub const TRB_SETUP: u32 = 2;
pub const TRB_DATA: u32 = 3;
pub const TRB_STATUS: u32 = 4;
pub const TRB_LINK: u32 = 6;
pub const TRB_ENABLE_SLOT: u32 = 9;
pub const TRB_DISABLE_SLOT: u32 = 10;
pub const TRB_ADDRESS_DEV: u32 = 11;
pub const TRB_CONFIG_EP: u32 = 12;
pub const TRB_RESET_EP: u32 = 14;
pub const TRB_SET_TR_DEQ: u32 = 16;
pub const TRB_EVENT_TRANSFER: u32 = 32;
pub const TRB_EVENT_CMD: u32 = 33;

pub const TRB_CYCLE: u32 = 1;
pub const TRB_IOC: u32 = 1 << 5;
pub const TRB_IDT: u32 = 1 << 6;
/// Interrupt on Short Packet. CSW is 13 bytes on a 512-byte HS bulk MPS.
/// Iron `73dc4d2e`: ISP on every bulk IN (including 512-byte READ) plus
/// auto EP-reset after ignored START STOP failed CSW. ISP only on CSW.
pub const TRB_ISP: u32 = 1 << 2;
pub const TRB_TC: u32 = 1 << 1;

pub const CMPL_SUCCESS: u8 = 1;
pub const CMPL_SHORT: u8 = 13;

pub const USBCMD_RS: u32 = 1;
pub const USBCMD_HCRST: u32 = 1 << 1;
pub const USBCMD_INTE: u32 = 1 << 2;
pub const USBSTS_HCH: u32 = 1;
pub const USBSTS_CNR: u32 = 1 << 11;

/// Intel PCH (C620 / Lewisburg `8086:a1af`) typically asks for 31 scratchpad
/// pages. QEMU `qemu-xhci` usually wants 0. Cap at 64×4 KiB `.bss`.
/// Iron COM2 `ac3b92cd`: `usb I/O fail err=1` `portsc=0` `cmpl=0` on BAR
/// `0x92b00000` — Cap before PORTSC (16-page budget). Not iron persist OK.
/// Iron COM2 `50b5d8bb` (Cap EFI): `scratch=34/64` then `err=3` Enum
/// `cmpl=0x11` (Parameter Error) `portsc=0`; CCS on p10/p11/p14 `0x206e1`
/// (PLS=Polling, PED=0). Not iron persist OK.
/// Iron COM2 `c6bdd671` (Enum EFI `53d1f7f7`): HS U0 `sc=0xe03` PED=1
/// speed=3 then Address Device `cmd=3` `cmpl=0x11` on p10/p11/p14 — Slot
/// Context DW1 Number of Ports was the port number (Hub=0). Not persist OK.
/// Iron COM2 `3473a0b9` (Slot DW1 EFI): Parameter Error gone; p10 Address
/// Device then p11/p14 Enable Slot all printed `cmpl=0x40`. That byte is
/// MaxSlots from [`xhci_reset_diag_cmpl`], not CC 64 — Address Device
/// never posted, command ring stayed busy, later CCS ports never got a
/// fair try. Toshiba unused. Leftover DRAM Everest is not persist.
pub const XHCI_SCRATCH_MAX: u32 = 64;

/// Do not cap the port walk at 16. Lewisburg `max_ports=26`.
pub const XHCI_PORT_SCAN_MAX: u8 = 31;

/// Enum `cmpl` command tag (low diag nibble of the packed word).
pub const XHCI_ENUM_CMD_RESET: u8 = 1;
pub const XHCI_ENUM_CMD_SLOT: u8 = 2;
pub const XHCI_ENUM_CMD_ADDR: u8 = 3;
pub const XHCI_ENUM_CMD_DESC: u8 = 4;
pub const XHCI_ENUM_CMD_CFG: u8 = 5;
/// SET_CONFIGURATION on EP0 (after parse, before Configure Endpoint).
/// Iron `96024edc` packed cmd=5 for both CONFIG_EP and SET_CONFIG.
pub const XHCI_ENUM_CMD_SETCFG: u8 = 6;

/// xHCI completion code 17 — Parameter Error (Address Device / Enable Slot).
pub const CMPL_PARAMETER: u8 = 17;
/// Command Ring Stopped / Command Aborted (after CRCR.CA).
pub const CMPL_CMD_STOPPED: u8 = 24;
pub const CMPL_CMD_ABORTED: u8 = 25;
/// Software: [`consume_event`] spun out. Not an xHCI CC.
/// Iron `3473a0b9` printed `0x40` because stamp used stale MaxSlots.
pub const CMPL_TIMEOUT: u8 = 0xFF;

/// Operational CRCR (xHCI 1.2 §5.4.5).
pub const CRCR_RCS: u64 = 1;
pub const CRCR_CA: u64 = 1 << 2;
pub const CRCR_CRR: u64 = 1 << 3;

/// Address Device issues SET_ADDRESS on the wire. Enable Slot does not.
/// `SPINS` was enough for Enable Slot on iron; Address Device was not.
pub const ADDR_SPINS: u32 = 50_000_000;
/// Bulk Transfer Event wait. Iron `6c278e85` (DESC retry): Toshiba p11
/// `0480:a004` INQUIRY/TUR/CAPACITY (≤36 B) succeeded (`lba=512`) then the
/// first 512-byte READ timed out (`err=8` `cmpl=0xff` at `off=0x200`).
/// Mechanical USB HDD first READ after CAPACITY can be seconds.
pub const BULK_SPINS: u32 = 100_000_000;

/// MSC BOT / UAS interface protocol (USB Mass Storage).
pub const USB_MSC_BOT: u8 = 0x50;
pub const USB_MSC_UAS: u8 = 0x62;
/// USB Hub bDeviceClass. Iron `06ca0f95` p14 `1604:10c0` class 09 proto 01.
pub const USB_CLASS_HUB: u8 = 0x09;
/// GET_DESC retries after Address Device. Iron `06ca0f95` p11 `cmd=4 cmpl=0`
/// (Toshiba `0480:a004` on p11 the prior recover flash). Not persist OK.
pub const XHCI_DESC_TRIES: u8 = 3;

/// xHCI USBLEGSUP (extended cap ID 1).
pub const USBLEGSUP_ID: u8 = 1;
pub const USBLEGSUP_BIOS_OWNED: u32 = 1 << 16;
pub const USBLEGSUP_OS_OWNED: u32 = 1 << 24;

/// HCSPARAMS2 Max Scratchpad Bufs = Hi[25:21] << 5 | Lo[31:27].
pub fn xhci_scratchpad_bufs(hcs2: u32) -> u32 {
    ((hcs2 >> 21) & 0x1F) << 5 | ((hcs2 >> 27) & 0x1F)
}

/// Inverse of [`xhci_scratchpad_bufs`] (host tests + HCS2 fixtures).
pub fn xhci_hcs2_with_scratch(n: u32) -> u32 {
    let lo = n & 0x1F;
    let hi = (n >> 5) & 0x1F;
    (hi << 21) | (lo << 27)
}

/// True when this controller's scratchpad fits our `.bss` pages.
pub fn xhci_scratchpad_supported(hcs2: u32) -> bool {
    xhci_scratchpad_bufs(hcs2) <= XHCI_SCRATCH_MAX
}

/// CAPLENGTH / HCSPARAMS1 / HCSPARAMS2 / HCCPARAMS1 snapshot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct XhciCapSnap {
    pub cap0: u32,
    pub hcs1: u32,
    pub hcs2: u32,
    pub hcc1: u32,
}

impl XhciCapSnap {
    pub fn caplen(self) -> u8 {
        self.cap0 as u8
    }

    pub fn max_slots(self) -> u8 {
        self.hcs1 as u8
    }

    pub fn max_ports(self) -> u8 {
        (self.hcs1 >> 24) as u8
    }

    pub fn scratch(self) -> u32 {
        xhci_scratchpad_bufs(self.hcs2)
    }

    /// HCCPARAMS1 CSZ — 64-byte contexts when set.
    pub fn csz(self) -> bool {
        (self.hcc1 & (1 << 2)) != 0
    }

    /// MMIO returned all-zero or all-ones — do not HCRST.
    pub fn mmio_dead(self) -> bool {
        self.cap0 == 0 || self.cap0 == 0xFFFF_FFFF
    }

    pub fn over_budget(self) -> bool {
        self.scratch() > XHCI_SCRATCH_MAX
    }
}

/// Read capability registers without starting the controller.
pub fn xhci_read_cap_snap(hw: &mut impl XhciHw) -> XhciCapSnap {
    XhciCapSnap {
        cap0: hw.read32(0),
        hcs1: hw.read32(0x04),
        hcs2: hw.read32(0x08),
        hcc1: hw.read32(0x10),
    }
}

/// Ports to walk after start (full HCSPARAMS1 MaxPorts, not 16).
pub fn xhci_scan_ports(max_ports: u8) -> u8 {
    max_ports.min(XHCI_PORT_SCAN_MAX)
}

/// CONFIG.MaxSlotsEn — program the hardware slot count, not 16.
pub fn xhci_config_slots(max_slots: u8) -> u8 {
    max_slots
}

pub fn portsc_pls(sc: u32) -> u32 {
    (sc >> 5) & 0xF
}

pub fn portsc_speed(sc: u32) -> u8 {
    ((sc >> 10) & 0xF) as u8
}

/// USB3-only PORTSC PLS: Polling / Recovery / Hot Reset / Compliance.
pub fn portsc_pls_training(pls: u32) -> bool {
    matches!(pls, 7 | 8 | 9 | 10)
}

/// PED + a real Port Speed + not still training. Speed=0 Address Device is
/// xHCI Parameter Error (`cmpl=0x11`).
pub fn portsc_link_ready(sc: u32) -> bool {
    sc & PORTSC_PED != 0 && portsc_speed(sc) != 0 && !portsc_pls_training(portsc_pls(sc))
}

/// Connected ports are always reset. Never inherit PED=1 / PLS=U0 from
/// firmware. xHCI HCRST does not fully clear PORTSC on some Intel PCH, and
/// Address Device on a device the UEFI driver already addressed times out
/// (`cmpl=0xff`). Iron `68e16633` scan was Polling `0x206e1` on p10/p11/p14
/// (HCRST *did* drop PED) — skip-if-U0 was not why p10 timed out — but it
/// is still a landmine if a later flash leaves the boot stick enabled.
pub fn port_must_reset(sc: u32) -> bool {
    sc & PORTSC_CCS != 0
}

/// Warm-reset (WPR) when the protocol cap, PLS, or SuperSpeed field says USB3.
pub fn port_reset_use_wpr(protocol_usb3: bool, sc: u32) -> bool {
    protocol_usb3 || portsc_pls_training(portsc_pls(sc)) || portsc_speed(sc) >= 4
}

/// DCI for bulk EP address n: OUT = 2n, IN = 2n+1.
pub fn bulk_ep_dci(ep: u8, dir_in: bool) -> u8 {
    if dir_in {
        ep.saturating_mul(2).saturating_add(1)
    } else {
        ep.saturating_mul(2)
    }
}

/// EP Context DW1: EP Type[5:3] + CErr[2:1] + Max Packet Size[31:16].
pub fn ep_ctx_dw1(ep_type: u32, mps: u16) -> u32 {
    (ep_type << 3) | (3 << 1) | (u32::from(mps) << 16)
}

/// EP Context DW4 Average TRB Length. Bulk uses max packet.
pub fn ep_ctx_dw4_avg_trb(mps: u16) -> u32 {
    u32::from(mps)
}

/// Slot Context DW0: Speed[23:20] + Context Entries[31:27]. Speed 0 is reserved.
pub fn slot_ctx_dw0(speed: u8, context_entries: u8) -> Option<u32> {
    if speed == 0 || speed > 5 || context_entries == 0 {
        return None;
    }
    Some(u32::from(speed) << 20 | u32::from(context_entries) << 27)
}

/// Pack Enum `cmpl=` so COM2 is not `portsc=0 cmpl=0x11` with no port.
pub fn xhci_enum_diag_cmpl(cmpl: u8, cmd: u8, speed: u8, csz: bool) -> u64 {
    u64::from(cmpl) | (u64::from(cmd) << 8) | (u64::from(speed) << 16) | (u64::from(csz) << 24)
}

/// PORTSC-scan / reset_port LAST_CMPL. Low byte is MaxSlots (`0x40` on Lewisburg).
pub fn xhci_reset_diag_cmpl(max_slots: u8, max_ports: u8, op: u32, port: u8) -> u64 {
    u64::from(max_slots)
        | (u64::from(max_ports) << 8)
        | (u64::from(op) << 16)
        | (u64::from(port) << 24)
}

pub fn crcr_abort_bits(crcr: u64) -> u64 {
    crcr | CRCR_CA
}

pub fn crcr_is_running(crcr: u64) -> bool {
    crcr & CRCR_CRR != 0
}

pub fn crcr_restart(cmd_hpa: u64) -> u64 {
    (cmd_hpa & !0x3F) | CRCR_RCS
}

/// Slot ID[31:24] + Endpoint ID[20:16] for Reset Endpoint / Set TR Dequeue.
pub fn xhci_ep_cmd_extra(slot: u8, dci: u8) -> u32 {
    u32::from(slot) << 24 | u32::from(dci) << 16
}

/// Device descriptor bDeviceClass 09 is a hub, not BOT.
pub fn usb_dev_is_hub(class: u8) -> bool {
    class == USB_CLASS_HUB
}

/// Keep Cruzer / TooSmall / an earlier DESC/Xfer fail when a later port is a hub.
/// Iron `06ca0f95`: p11 GET_DESC `cmd=4 cmpl=0` then p14 hub `err=4` hid Toshiba.
/// Iron `73dc4d2e`: hub skip stamped `cmpl=0` over p11 CSW (`bot=csw`).
pub fn xhci_bring_up_keep_err(last: UsbBotError, e: UsbBotError) -> UsbBotError {
    match e {
        UsbBotError::Cruzer => UsbBotError::Cruzer,
        UsbBotError::TooSmall => {
            if last == UsbBotError::Cruzer {
                last
            } else {
                UsbBotError::TooSmall
            }
        }
        UsbBotError::Hub => {
            if last == UsbBotError::Reset {
                UsbBotError::Hub
            } else {
                last
            }
        }
        other => {
            if last == UsbBotError::Cruzer || last == UsbBotError::TooSmall {
                last
            } else {
                other
            }
        }
    }
}

/// Pack Enum `portsc=` as PORTSC | (port << 32).
pub fn xhci_enum_diag_portsc(sc: u32, port: u8) -> u64 {
    u64::from(sc) | (u64::from(port) << 32)
}

/// Pack CAPLENGTH+HCSPARAMS1 into the existing `portsc` diag word.
pub fn xhci_cap_diag_portsc(snap: XhciCapSnap) -> u64 {
    u64::from(snap.cap0) | (u64::from(snap.hcs1) << 32)
}

/// Pack HCSPARAMS2 + scratch count into the existing `cmpl` diag word.
pub fn xhci_cap_diag_cmpl(snap: XhciCapSnap) -> u64 {
    u64::from(snap.hcs2) | (u64::from(snap.scratch()) << 32)
}

/// Run bit only. Never OR [`USBCMD_INTE`] (firmware ISRs move ERDP).
pub fn xhci_run_usbcmd() -> u32 {
    USBCMD_RS
}

pub const PORTSC_CCS: u32 = 1;
pub const PORTSC_PED: u32 = 1 << 1;
pub const PORTSC_PR: u32 = 1 << 4;
pub const PORTSC_PP: u32 = 1 << 9;
pub const PORTSC_CSC: u32 = 1 << 17;
pub const PORTSC_WRC: u32 = 1 << 19;
pub const PORTSC_PRC: u32 = 1 << 21;
pub const PORTSC_WPR: u32 = 1 << 31;

pub const RING_TRBS: u16 = 256;
pub const SPINS: u32 = 5_000_000;

/// TRB control word: cycle + type + extra flags (IOC, IDT, …).
pub fn trb_ctrl(cycle: u32, trb_type: u32, extra: u32) -> u32 {
    (cycle & 1) | ((trb_type & 0x3F) << 10) | extra
}

pub fn trb_type(ctrl: u32) -> u32 {
    (ctrl >> 10) & 0x3F
}

pub fn trb_cmpl_code(status: u32) -> u8 {
    (status >> 24) as u8
}

fn put_u64(b: &mut [u8], off: usize, v: u64) {
    b[off..off + 8].copy_from_slice(&v.to_le_bytes());
}

fn put_u32(b: &mut [u8], off: usize, v: u32) {
    b[off..off + 4].copy_from_slice(&v.to_le_bytes());
}

fn get_u32(b: &[u8], off: usize) -> u32 {
    u32::from_le_bytes(b[off..off + 4].try_into().unwrap_or([0; 4]))
}

/// MMIO + DMA. Host tests pack TRBs without this; UEFI uses BAR0.
pub trait XhciHw {
    fn read32(&mut self, off: u32) -> u32;
    fn write32(&mut self, off: u32, val: u32);
    fn dma_read(&mut self, hpa: u64, buf: &mut [u8]);
    fn dma_write(&mut self, hpa: u64, buf: &[u8]);
}

fn write64(hw: &mut impl XhciHw, off: u32, val: u64) {
    hw.write32(off, val as u32);
    hw.write32(off + 4, (val >> 32) as u32);
}

fn read64(hw: &mut impl XhciHw, off: u32) -> u64 {
    u64::from(hw.read32(off)) | (u64::from(hw.read32(off + 4)) << 32)
}

fn write_trb(hw: &mut impl XhciHw, base: u64, idx: u16, ptr: u64, status: u32, ctrl: u32) {
    let mut t = [0u8; 16];
    put_u64(&mut t, 0, ptr);
    put_u32(&mut t, 8, status);
    put_u32(&mut t, 12, ctrl);
    hw.dma_write(base + u64::from(idx) * 16, &t);
}

fn read_trb(hw: &mut impl XhciHw, base: u64, idx: u16) -> [u8; 16] {
    let mut t = [0u8; 16];
    hw.dma_read(base + u64::from(idx) * 16, &mut t);
    t
}

#[derive(Clone, Copy)]
pub struct XhciCaps {
    pub op: u32,
    pub rt: u32,
    pub db: u32,
    pub max_slots: u8,
    pub max_ports: u8,
    pub csz: bool,
}

pub fn parse_caps(hw: &mut impl XhciHw) -> Result<XhciCaps, UsbBotError> {
    let snap = xhci_read_cap_snap(hw);
    parse_caps_from_snap(hw, snap)
}

pub fn parse_caps_from_snap(
    hw: &mut impl XhciHw,
    snap: XhciCapSnap,
) -> Result<XhciCaps, UsbBotError> {
    if snap.caplen() < 0x20 {
        return Err(UsbBotError::Cap);
    }
    Ok(XhciCaps {
        op: u32::from(snap.caplen()),
        rt: hw.read32(0x18) & !0x1F,
        db: hw.read32(0x14) & !0x3,
        max_slots: snap.max_slots(),
        max_ports: snap.max_ports(),
        csz: snap.csz(),
    })
}

fn ctx_size(csz: bool) -> usize {
    if csz {
        64
    } else {
        32
    }
}

pub struct Ring {
    pub base: u64,
    pub enq: u16,
    pub cycle: u32,
}

impl Ring {
    pub fn new(base: u64) -> Self {
        Self {
            base,
            enq: 0,
            cycle: 1,
        }
    }

    fn place(&mut self, hw: &mut impl XhciHw, ptr: u64, status: u32, ctrl: u32) {
        if self.enq == RING_TRBS - 1 {
            write_trb(
                hw,
                self.base,
                self.enq,
                self.base,
                0,
                trb_ctrl(self.cycle, TRB_LINK, TRB_TC),
            );
            self.enq = 0;
            self.cycle ^= 1;
        }
        write_trb(
            hw,
            self.base,
            self.enq,
            ptr,
            status,
            (ctrl & !1) | (self.cycle & 1),
        );
        self.enq = self.enq.saturating_add(1);
    }
}

pub struct EventRing {
    pub base: u64,
    pub deq: u16,
    pub cycle: u32,
}

impl EventRing {
    pub fn new(base: u64) -> Self {
        Self {
            base,
            deq: 0,
            cycle: 1,
        }
    }
}

fn wait_clear(hw: &mut impl XhciHw, off: u32, mask: u32) -> bool {
    for _ in 0..SPINS {
        if hw.read32(off) & mask == 0 {
            return true;
        }
    }
    false
}

fn wait_set(hw: &mut impl XhciHw, off: u32, mask: u32) -> bool {
    for _ in 0..SPINS {
        if hw.read32(off) & mask == mask {
            return true;
        }
    }
    false
}

fn handshake_legacy(hw: &mut impl XhciHw) {
    let hcc1 = hw.read32(0x10);
    let mut xecp = ((hcc1 >> 16) & 0xFFFF) * 4;
    for _ in 0..32 {
        if xecp < 0x20 {
            break;
        }
        let cap = hw.read32(xecp);
        if cap as u8 == USBLEGSUP_ID {
            hw.write32(xecp, cap | USBLEGSUP_OS_OWNED);
            for _ in 0..SPINS {
                if hw.read32(xecp) & USBLEGSUP_BIOS_OWNED == 0 {
                    break;
                }
            }
            return;
        }
        let next = ((cap >> 8) & 0xFF) * 4;
        if next == 0 {
            break;
        }
        xecp = xecp.saturating_add(next);
    }
}

/// xHCI §7.2 USB Supported Protocol: DWORD0 = ID/next/minor/major,
/// DWORD2 = Compatible Port Offset (7:0) + Count (15:8).
pub fn supported_protocol_matches(cap_dw0: u32, dw2: u32, port: u8) -> Option<bool> {
    if cap_dw0 as u8 != 2 {
        return None;
    }
    let major = (cap_dw0 >> 24) as u8;
    let off = dw2 as u8;
    let count = (dw2 >> 8) as u8;
    if off == 0 || count == 0 {
        return None;
    }
    if port >= off && port < off.saturating_add(count) {
        Some(major >= 3)
    } else {
        None
    }
}

fn port_is_usb3(hw: &mut impl XhciHw, port: u8) -> bool {
    let hcc1 = hw.read32(0x10);
    let mut xecp = ((hcc1 >> 16) & 0xFFFF) * 4;
    for _ in 0..32 {
        if xecp < 0x20 {
            break;
        }
        let cap = hw.read32(xecp);
        let dw2 = hw.read32(xecp + 8);
        if let Some(usb3) = supported_protocol_matches(cap, dw2, port) {
            return usb3;
        }
        let next = ((cap >> 8) & 0xFF) * 4;
        if next == 0 {
            break;
        }
        xecp = xecp.saturating_add(next);
    }
    false
}

fn portsc_off(op: u32, port: u8) -> u32 {
    op + 0x400 + u32::from(port.saturating_sub(1)) * 0x10
}

#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
fn serial_xhci_caps(snap: XhciCapSnap) {
    use crate::boot::serial;
    serial::write_str("boot: Stage 46 xhci cap caplen=0x");
    serial_hex32(snap.cap0);
    serial::write_str(" hcs1=0x");
    serial_hex32(snap.hcs1);
    serial::write_str(" hcs2=0x");
    serial_hex32(snap.hcs2);
    serial::write_str(" hcc1=0x");
    serial_hex32(snap.hcc1);
    serial::write_str(" slots=");
    serial_dec_u32(u32::from(snap.max_slots()));
    serial::write_str(" ports=");
    serial_dec_u32(u32::from(snap.max_ports()));
    serial::write_str(" scratch=");
    serial_dec_u32(snap.scratch());
    serial::write_str("/");
    serial_dec_u32(XHCI_SCRATCH_MAX);
    serial::write_str(" csz=");
    serial_dec_u32(u32::from(snap.csz()));
    if snap.mmio_dead() {
        serial::write_str(" mmio-dead");
    }
    if snap.over_budget() {
        serial::write_str(" over-budget");
    }
    serial::write_line(" (not ISO-INSTALL-OK)");
}

#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
fn serial_xhci_scratch_over(scratch: u32) {
    use crate::boot::serial;
    serial::write_str("boot: Stage 46 xhci scratch=");
    serial_dec_u32(scratch);
    serial::write_str(" > max=");
    serial_dec_u32(XHCI_SCRATCH_MAX);
    serial::write_line(" (Cap; leftover DRAM; not ISO-INSTALL-OK)");
}

#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
fn serial_xhci_ports(hw: &mut impl XhciHw, op: u32, ports: u8, caps: &XhciCaps) {
    use crate::boot::serial;
    serial::write_str("boot: Stage 46 xhci caplen=0x");
    serial_hex32(caps.op);
    serial::write_str(" slots=");
    serial_dec_u8(caps.max_slots);
    serial::write_str(" ports=");
    serial_dec_u8(caps.max_ports);
    serial::write_str(" scan=");
    serial_dec_u8(ports);
    let n = ports.min(16);
    for p in 1..=n {
        serial::write_str(" p");
        serial_dec_u8(p);
        serial::write_str("=0x");
        serial_hex32(hw.read32(portsc_off(op, p)));
    }
    serial::write_line(" (not ISO-INSTALL-OK)");
    serial::write_str("boot: Stage 46 xhci ccs");
    let mut any = false;
    for p in 1..=ports {
        let sc = hw.read32(portsc_off(op, p));
        if sc & PORTSC_CCS == 0 {
            continue;
        }
        any = true;
        serial::write_str(" p");
        serial_dec_u8(p);
        serial::write_str("=0x");
        serial_hex32(sc);
    }
    if !any {
        serial::write_str(" none");
    }
    serial::write_line(" (not ISO-INSTALL-OK)");
}

#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
fn serial_xhci_enum(port: u8, sc: u32, speed: u8, usb3: bool, csz: bool, cmd: u8, cmpl: u8) {
    use crate::boot::serial;
    serial::write_str("boot: Stage 46 xhci enum p");
    serial_dec_u8(port);
    serial::write_str(" sc=0x");
    serial_hex32(sc);
    serial::write_str(" speed=");
    serial_dec_u8(speed);
    serial::write_str(" usb3=");
    serial_dec_u8(u8::from(usb3));
    serial::write_str(" csz=");
    serial_dec_u8(u8::from(csz));
    serial::write_str(" cmd=");
    serial_dec_u8(cmd);
    serial::write_str(" cmpl=0x");
    serial_hex32(u32::from(cmpl));
    if cmpl == CMPL_TIMEOUT {
        serial::write_str(" timeout");
    }
    serial::write_line(" (not ISO-INSTALL-OK)");
}

#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
fn serial_xhci_dev(port: u8, vid: u16, pid: u16, class: u8, proto: u8) {
    use crate::boot::serial;
    serial::write_str("boot: Stage 46 xhci enum p");
    serial_dec_u8(port);
    serial::write_str(" vid=0x");
    serial_hex32(u32::from(vid));
    serial::write_str(" did=0x");
    serial_hex32(u32::from(pid));
    serial::write_str(" class=0x");
    serial_hex32(u32::from(class));
    serial::write_str(" proto=0x");
    serial_hex32(u32::from(proto));
    serial::write_line(" (not ISO-INSTALL-OK)");
}

#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
fn serial_xhci_uas(port: u8) {
    use crate::boot::serial;
    serial::write_str("boot: Stage 46 xhci enum p");
    serial_dec_u8(port);
    serial::write_line(" uas-no-bot (err=4; leftover DRAM; not ISO-INSTALL-OK)");
}

#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
fn serial_xhci_hub(port: u8) {
    use crate::boot::serial;
    serial::write_str("boot: Stage 46 xhci enum p");
    serial_dec_u8(port);
    serial::write_line(" hub skip (err=9; leftover DRAM; not ISO-INSTALL-OK)");
}

#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
fn serial_xhci_hcrst() {
    crate::boot::serial::write_line(
        "boot: Stage 46 xhci hcrst (own rings; leftover DRAM; not ISO-INSTALL-OK)",
    );
}

#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
fn serial_xhci_bot_eps(port: u8, ep_out: u8, ep_in: u8, cfg: u8) {
    use crate::boot::serial;
    serial::write_str("boot: Stage 46 xhci bot p");
    serial_dec_u8(port);
    serial::write_str(" iface=08/06/50 ep_out=");
    serial_dec_u8(ep_out);
    serial::write_str(" ep_in=");
    serial_dec_u8(ep_in);
    serial::write_str(" cfg=");
    serial_dec_u8(cfg);
    serial::write_line(" (not ISO-INSTALL-OK)");
}

#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
fn serial_xhci_setcfg(port: u8, cfg: u8) {
    use crate::boot::serial;
    serial::write_str("boot: Stage 46 xhci setcfg p");
    serial_dec_u8(port);
    serial::write_str(" val=");
    serial_dec_u8(cfg);
    serial::write_line(" (not ISO-INSTALL-OK)");
}

#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
fn serial_xhci_desc_retry(port: u8, n: u8) {
    use crate::boot::serial;
    serial::write_str("boot: Stage 46 xhci enum p");
    serial_dec_u8(port);
    serial::write_str(" desc retry n=");
    serial_dec_u8(n);
    serial::write_line(" (not ISO-INSTALL-OK)");
}

#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
fn serial_hex32(v: u32) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut buf = [0u8; 8];
    for i in 0..8 {
        buf[i] = HEX[((v >> (28 - i * 4)) & 0xF) as usize];
    }
    crate::boot::serial::write_str(core::str::from_utf8(&buf).unwrap_or("????????"));
}

#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
fn serial_dec_u8(v: u8) {
    if v >= 100 {
        crate::boot::serial::write_str("100+");
        return;
    }
    let mut buf = [0u8; 3];
    let mut n = 0usize;
    if v >= 10 {
        buf[n] = b'0' + (v / 10);
        n += 1;
    }
    buf[n] = b'0' + (v % 10);
    n += 1;
    crate::boot::serial::write_str(core::str::from_utf8(&buf[..n]).unwrap_or("?"));
}

#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
fn serial_dec_u32(v: u32) {
    if v == 0 {
        crate::boot::serial::write_str("0");
        return;
    }
    let mut digits = [0u8; 10];
    let mut n = 0usize;
    let mut x = v;
    while x > 0 && n < digits.len() {
        digits[n] = b'0' + (x % 10) as u8;
        n += 1;
        x /= 10;
    }
    let mut out = [0u8; 10];
    for i in 0..n {
        out[i] = digits[n - 1 - i];
    }
    crate::boot::serial::write_str(core::str::from_utf8(&out[..n]).unwrap_or("?"));
}

#[cfg(not(all(target_os = "uefi", feature = "uefi-bin")))]
fn serial_xhci_caps(_snap: XhciCapSnap) {}

#[cfg(not(all(target_os = "uefi", feature = "uefi-bin")))]
fn serial_xhci_scratch_over(_scratch: u32) {}

#[cfg(not(all(target_os = "uefi", feature = "uefi-bin")))]
fn serial_xhci_ports(_hw: &mut impl XhciHw, _op: u32, _ports: u8, _caps: &XhciCaps) {}

#[cfg(not(all(target_os = "uefi", feature = "uefi-bin")))]
fn serial_xhci_enum(_port: u8, _sc: u32, _speed: u8, _usb3: bool, _csz: bool, _cmd: u8, _cmpl: u8) {
}

#[cfg(not(all(target_os = "uefi", feature = "uefi-bin")))]
fn serial_xhci_dev(_port: u8, _vid: u16, _pid: u16, _class: u8, _proto: u8) {}

#[cfg(not(all(target_os = "uefi", feature = "uefi-bin")))]
fn serial_xhci_uas(_port: u8) {}

#[cfg(not(all(target_os = "uefi", feature = "uefi-bin")))]
fn serial_xhci_hub(_port: u8) {}

#[cfg(not(all(target_os = "uefi", feature = "uefi-bin")))]
fn serial_xhci_hcrst() {}

#[cfg(not(all(target_os = "uefi", feature = "uefi-bin")))]
fn serial_xhci_bot_eps(_port: u8, _ep_out: u8, _ep_in: u8, _cfg: u8) {}

#[cfg(not(all(target_os = "uefi", feature = "uefi-bin")))]
fn serial_xhci_setcfg(_port: u8, _cfg: u8) {}

#[cfg(not(all(target_os = "uefi", feature = "uefi-bin")))]
fn serial_xhci_desc_retry(_port: u8, _n: u8) {}

fn doorbell(hw: &mut impl XhciHw, db: u32, slot: u8, target: u8) {
    hw.write32(db + u32::from(slot) * 4, u32::from(target));
}

fn advance_event(hw: &mut impl XhciHw, caps: &XhciCaps, ev: &mut EventRing) {
    let erdp_off = caps.rt + 0x38;
    ev.deq = ev.deq.saturating_add(1);
    if ev.deq == RING_TRBS {
        ev.deq = 0;
        ev.cycle ^= 1;
    }
    write64(hw, erdp_off, ev.base + u64::from(ev.deq) * 16 | (1 << 3));
}

/// Drop already-posted events (port-status, extra short-packet) so the next
/// BOT round-trip does not retire a stale Transfer Event as success.
fn drain_events(hw: &mut impl XhciHw, caps: &XhciCaps, ev: &mut EventRing) {
    for _ in 0..RING_TRBS {
        let t = read_trb(hw, ev.base, ev.deq);
        let ctrl = get_u32(&t, 12);
        if (ctrl & 1) != (ev.cycle & 1) {
            break;
        }
        advance_event(hw, caps, ev);
    }
}

fn xhci_event_err(want_type: u32) -> UsbBotError {
    if want_type == TRB_EVENT_TRANSFER {
        UsbBotError::Xfer
    } else {
        UsbBotError::Enum
    }
}

fn consume_event(
    hw: &mut impl XhciHw,
    caps: &XhciCaps,
    ev: &mut EventRing,
    want_type: u32,
    spins_max: u32,
) -> Result<[u8; 16], UsbBotError> {
    let mut spins = 0u32;
    loop {
        let t = read_trb(hw, ev.base, ev.deq);
        let ctrl = get_u32(&t, 12);
        if (ctrl & 1) == (ev.cycle & 1) {
            let ty = trb_type(ctrl);
            advance_event(hw, caps, ev);
            if ty == want_type {
                let code = trb_cmpl_code(get_u32(&t, 8));
                if code != CMPL_SUCCESS && code != CMPL_SHORT {
                    let err = xhci_event_err(want_type);
                    store_usb_bot_diag(
                        err,
                        usb_bot_last_bar(),
                        usb_bot_last_portsc(),
                        u64::from(code),
                    );
                    return Err(err);
                }
                return Ok(t);
            }
            continue;
        }
        spins = spins.saturating_add(1);
        if spins > spins_max {
            let err = xhci_event_err(want_type);
            store_usb_bot_diag(
                err,
                usb_bot_last_bar(),
                usb_bot_last_portsc(),
                u64::from(CMPL_TIMEOUT),
            );
            return Err(err);
        }
    }
}

fn cmd(
    hw: &mut impl XhciHw,
    caps: &XhciCaps,
    cmd_ring: &mut Ring,
    ev: &mut EventRing,
    ptr: u64,
    extra_and_type: u32,
) -> Result<[u8; 16], UsbBotError> {
    cmd_wait(hw, caps, cmd_ring, ev, ptr, extra_and_type, SPINS)
}

fn cmd_wait(
    hw: &mut impl XhciHw,
    caps: &XhciCaps,
    cmd_ring: &mut Ring,
    ev: &mut EventRing,
    ptr: u64,
    extra_and_type: u32,
    spins_max: u32,
) -> Result<[u8; 16], UsbBotError> {
    cmd_ring.place(hw, ptr, 0, extra_and_type);
    doorbell(hw, caps.db, 0, 0);
    consume_event(hw, caps, ev, TRB_EVENT_CMD, spins_max)
}

fn abort_cmd_ring(
    hw: &mut impl XhciHw,
    caps: &XhciCaps,
    mem: &XhciMem,
    cmd_ring: &mut Ring,
    ev: &mut EventRing,
) {
    let off = caps.op + 0x18;
    let cur = read64(hw, off);
    write64(hw, off, crcr_abort_bits(cur));
    for _ in 0..SPINS {
        let crcr = read64(hw, off);
        if !crcr_is_running(crcr) {
            break;
        }
    }
    drain_events(hw, caps, ev);
    zero_page(hw, mem.cmd);
    *cmd_ring = Ring::new(mem.cmd);
    write64(hw, off, crcr_restart(mem.cmd));
}

/// Address Device that never completes leaves CRR=1. The next CCS port's
/// Enable Slot then times out on a busy ring (iron `3473a0b9` p11/p14).
fn recover_enum(
    hw: &mut impl XhciHw,
    caps: &XhciCaps,
    mem: &XhciMem,
    cmd_ring: &mut Ring,
    ev: &mut EventRing,
    slot: u8,
) {
    abort_cmd_ring(hw, caps, mem, cmd_ring, ev);
    if slot != 0 {
        let _ = cmd(
            hw,
            caps,
            cmd_ring,
            ev,
            0,
            trb_ctrl(0, TRB_DISABLE_SLOT, u32::from(slot) << 24),
        );
        abort_cmd_ring(hw, caps, mem, cmd_ring, ev);
        let z = [0u8; 8];
        hw.dma_write(mem.dcbaa + u64::from(slot) * 8, &z);
        zero_page(hw, mem.inctx);
        zero_page(hw, mem.devctx);
        zero_page(hw, mem.ep0);
    }
}

fn event_slot(ev: &[u8; 16]) -> u8 {
    (get_u32(ev, 12) >> 24) as u8
}

/// Identity-mapped pages the live driver owns.
pub struct XhciMem {
    pub dcbaa: u64,
    pub scratch_array: u64,
    pub cmd: u64,
    pub evt: u64,
    pub erst: u64,
    pub inctx: u64,
    pub devctx: u64,
    pub ep0: u64,
    pub bulk_out: u64,
    pub bulk_in: u64,
    pub bounce: u64,
    pub scratch0: u64,
}

fn zero_page(hw: &mut impl XhciHw, hpa: u64) {
    let z = [0u8; 256];
    for i in 0..16u64 {
        hw.dma_write(hpa + i * 256, &z);
    }
}

fn xhci_start(
    hw: &mut impl XhciHw,
    mem: &XhciMem,
) -> Result<(XhciCaps, Ring, EventRing), UsbBotError> {
    handshake_legacy(hw);
    let snap = xhci_read_cap_snap(hw);
    store_usb_bot_diag(
        UsbBotError::Cap,
        0,
        xhci_cap_diag_portsc(snap),
        xhci_cap_diag_cmpl(snap),
    );
    serial_xhci_caps(snap);
    if snap.mmio_dead() {
        return Err(UsbBotError::Cap);
    }
    let caps = parse_caps_from_snap(hw, snap)?;
    if caps.max_slots == 0 || caps.max_ports == 0 {
        return Err(UsbBotError::Cap);
    }
    let usbsts = caps.op + 4;
    let usbcmd = caps.op;
    if hw.read32(usbcmd) & USBCMD_RS != 0 {
        hw.write32(usbcmd, 0);
        if !wait_set(hw, usbsts, USBSTS_HCH) {
            return Err(UsbBotError::Reset);
        }
    }
    hw.write32(usbcmd, USBCMD_HCRST);
    if !wait_clear(hw, usbcmd, USBCMD_HCRST) {
        return Err(UsbBotError::Reset);
    }
    if !wait_clear(hw, usbsts, USBSTS_CNR) {
        return Err(UsbBotError::Reset);
    }
    serial_xhci_hcrst();
    handshake_legacy(hw);
    let slots = xhci_config_slots(caps.max_slots);
    hw.write32(caps.op + 0x38, u32::from(slots));
    zero_page(hw, mem.dcbaa);
    zero_page(hw, mem.cmd);
    zero_page(hw, mem.evt);
    zero_page(hw, mem.erst);
    zero_page(hw, mem.inctx);
    zero_page(hw, mem.devctx);
    zero_page(hw, mem.ep0);
    zero_page(hw, mem.bulk_out);
    zero_page(hw, mem.bulk_in);
    zero_page(hw, mem.bounce);
    let hcs2 = hw.read32(0x08);
    let scratch = xhci_scratchpad_bufs(hcs2);
    if scratch > XHCI_SCRATCH_MAX {
        serial_xhci_scratch_over(scratch);
        store_usb_bot_diag(
            UsbBotError::Cap,
            0,
            xhci_cap_diag_portsc(snap),
            xhci_cap_diag_cmpl(snap),
        );
        return Err(UsbBotError::Cap);
    }
    if scratch != 0 {
        zero_page(hw, mem.scratch_array);
        for i in 0..scratch {
            let page = mem.scratch0.saturating_add(u64::from(i) * 4096);
            zero_page(hw, page);
            let mut ptr = [0u8; 8];
            put_u64(&mut ptr, 0, page);
            hw.dma_write(mem.scratch_array.saturating_add(u64::from(i) * 8), &ptr);
        }
        let mut dc0 = [0u8; 8];
        put_u64(&mut dc0, 0, mem.scratch_array);
        hw.dma_write(mem.dcbaa, &dc0);
    }
    write64(hw, caps.op + 0x30, mem.dcbaa);
    write64(hw, caps.op + 0x18, mem.cmd | 1);
    let mut erst = [0u8; 16];
    put_u64(&mut erst, 0, mem.evt);
    put_u32(&mut erst, 8, u32::from(RING_TRBS));
    hw.dma_write(mem.erst, &erst);
    hw.write32(caps.rt + 0x28, 1);
    write64(hw, caps.rt + 0x30, mem.erst);
    write64(hw, caps.rt + 0x38, mem.evt | (1 << 3));
    // Poll the event ring. INTE + leftover firmware ISRs can move ERDP
    // after EBS (allocator / STI) and wedge BOT keep-detect.
    hw.write32(caps.rt + 0x20, 0);
    hw.write32(usbcmd, xhci_run_usbcmd());
    if !wait_clear(hw, usbsts, USBSTS_HCH) {
        return Err(UsbBotError::Reset);
    }
    let ports = xhci_scan_ports(caps.max_ports);
    // PED is RW1CS: never write the previous PORTSC word back (that clears PED).
    // Do not blast-reset every port here — QEMU xhci_port_reset is a no-op when
    // CCS=0, and a second reset in try_port is enough once CCS latches after PP.
    for port in 1..=ports {
        hw.write32(portsc_off(caps.op, port), PORTSC_PP | PORTSC_CSC);
    }
    let mut saw_ccs = false;
    let mut p1 = 0u32;
    let mut p_last = 0u32;
    for _ in 0..SPINS {
        for port in 1..=ports {
            let sc = hw.read32(portsc_off(caps.op, port));
            if port == 1 {
                p1 = sc;
            }
            if port == ports {
                p_last = sc;
            }
            if sc & PORTSC_CCS != 0 {
                saw_ccs = true;
                break;
            }
        }
        if saw_ccs {
            break;
        }
    }
    store_usb_bot_diag(
        UsbBotError::Reset,
        0,
        u64::from(p1) | (u64::from(p_last) << 32),
        u64::from(caps.max_slots) | (u64::from(caps.max_ports) << 8) | (u64::from(caps.op) << 16),
    );
    serial_xhci_ports(hw, caps.op, ports, &caps);
    if !saw_ccs {
        return Err(UsbBotError::Reset);
    }
    Ok((caps, Ring::new(mem.cmd), EventRing::new(mem.evt)))
}

/// Slot Context DW1 (xHCI 1.2 Table 6-8):
/// Root Hub Port Number is **23:16**. Number of Ports is **31:24** and is
/// hubs only. Writing the port number into 31:24 with Hub=0 is Intel
/// Parameter Error (`cmpl=0x11`) on Address Device. QEMU 8.2 reads 23:16.
pub fn slot_ctx_dw1_port(port: u8) -> u32 {
    u32::from(port) << 16
}

/// Number of Ports field (DW1 31:24). Must stay 0 unless Hub=1.
pub fn slot_ctx_dw1_num_ports(dw1: u32) -> u8 {
    (dw1 >> 24) as u8
}

fn ep0_max_packet(speed: u8) -> u16 {
    match speed {
        2 => 8,
        4 | 5 => 512,
        _ => 64,
    }
}

fn wait_port_link(hw: &mut impl XhciHw, off: u32) -> bool {
    for _ in 0..SPINS {
        if portsc_link_ready(hw.read32(off)) {
            return true;
        }
    }
    false
}

fn issue_port_reset(hw: &mut impl XhciHw, off: u32, wpr: bool) -> bool {
    hw.write32(off, PORTSC_PP | PORTSC_CSC | PORTSC_PRC | PORTSC_WRC);
    let rst = if wpr { PORTSC_WPR } else { PORTSC_PR };
    hw.write32(off, PORTSC_PP | rst);
    wait_set(hw, off, PORTSC_PRC) || wait_set(hw, off, PORTSC_WRC)
}

fn reset_port(hw: &mut impl XhciHw, caps: &XhciCaps, port: u8) -> Result<u32, UsbBotError> {
    let off = portsc_off(caps.op, port);
    let mut sc = hw.read32(off);
    let cmpl = xhci_reset_diag_cmpl(caps.max_slots, caps.max_ports, caps.op, port);
    store_usb_bot_diag_unless_kept(
        UsbBotError::Reset,
        usb_bot_last_bar(),
        xhci_enum_diag_portsc(sc, port),
        cmpl,
    );
    if sc & PORTSC_CCS == 0 {
        return Err(UsbBotError::Reset);
    }
    // Always reset. Never inherit PED=1 / U0 (`port_must_reset`).
    let usb3 = port_reset_use_wpr(port_is_usb3(hw, port), sc);
    if !issue_port_reset(hw, off, usb3) && !issue_port_reset(hw, off, !usb3) {
        sc = hw.read32(off);
        store_usb_bot_diag_unless_kept(
            UsbBotError::Reset,
            usb_bot_last_bar(),
            xhci_enum_diag_portsc(sc, port),
            cmpl,
        );
        return Err(UsbBotError::Reset);
    }
    hw.write32(off, PORTSC_PP | PORTSC_PRC | PORTSC_WRC | PORTSC_CSC);
    if !wait_port_link(hw, off) {
        sc = hw.read32(off);
        store_usb_bot_diag_unless_kept(
            UsbBotError::Reset,
            usb_bot_last_bar(),
            xhci_enum_diag_portsc(sc, port),
            cmpl,
        );
        return Err(UsbBotError::Reset);
    }
    sc = hw.read32(off);
    store_usb_bot_diag_unless_kept(
        UsbBotError::Reset,
        usb_bot_last_bar(),
        xhci_enum_diag_portsc(sc, port),
        cmpl,
    );
    Ok(sc)
}

fn reset_ep0(
    hw: &mut impl XhciHw,
    caps: &XhciCaps,
    mem: &XhciMem,
    cmd_ring: &mut Ring,
    ev: &mut EventRing,
    ep0: &mut Ring,
    slot: u8,
) {
    drain_events(hw, caps, ev);
    let extra = xhci_ep_cmd_extra(slot, 1);
    let _ = cmd(hw, caps, cmd_ring, ev, 0, trb_ctrl(0, TRB_RESET_EP, extra));
    zero_page(hw, mem.ep0);
    *ep0 = Ring::new(mem.ep0);
    let _ = cmd(
        hw,
        caps,
        cmd_ring,
        ev,
        mem.ep0 | 1,
        trb_ctrl(0, TRB_SET_TR_DEQ, extra),
    );
}

fn control_in_retry(
    hw: &mut impl XhciHw,
    caps: &XhciCaps,
    mem: &XhciMem,
    cmd_ring: &mut Ring,
    ep0: &mut Ring,
    ev: &mut EventRing,
    slot: u8,
    bounce: u64,
    setup: [u8; 8],
    data: &mut [u8],
    port: u8,
) -> Result<(), UsbBotError> {
    let mut last = UsbBotError::Enum;
    for n in 0..XHCI_DESC_TRIES {
        drain_events(hw, caps, ev);
        match control_in(hw, caps, ep0, ev, slot, bounce, setup, data) {
            Ok(()) => return Ok(()),
            Err(e) => {
                last = e;
                if n + 1 < XHCI_DESC_TRIES {
                    serial_xhci_desc_retry(port, n + 1);
                    reset_ep0(hw, caps, mem, cmd_ring, ev, ep0, slot);
                }
            }
        }
    }
    Err(last)
}

fn control_in(
    hw: &mut impl XhciHw,
    caps: &XhciCaps,
    ep0: &mut Ring,
    ev: &mut EventRing,
    slot: u8,
    bounce: u64,
    setup: [u8; 8],
    data: &mut [u8],
) -> Result<(), UsbBotError> {
    let z = [0u8; 4096];
    hw.dma_write(bounce, &z[..data.len().min(4096)]);
    ep0.place(
        hw,
        u64::from_le_bytes(setup),
        8,
        trb_ctrl(0, TRB_SETUP, TRB_IDT) | (3u32 << 16),
    );
    ep0.place(
        hw,
        bounce,
        data.len() as u32,
        trb_ctrl(0, TRB_DATA, 1 << 16),
    );
    ep0.place(hw, 0, 0, trb_ctrl(0, TRB_STATUS, TRB_IOC));
    doorbell(hw, caps.db, slot, 1);
    consume_event(hw, caps, ev, TRB_EVENT_TRANSFER, SPINS)?;
    hw.dma_read(bounce, data);
    Ok(())
}

fn control_nodata(
    hw: &mut impl XhciHw,
    caps: &XhciCaps,
    ep0: &mut Ring,
    ev: &mut EventRing,
    slot: u8,
    setup: [u8; 8],
) -> Result<(), UsbBotError> {
    ep0.place(
        hw,
        u64::from_le_bytes(setup),
        8,
        trb_ctrl(0, TRB_SETUP, TRB_IDT),
    );
    ep0.place(hw, 0, 0, trb_ctrl(0, TRB_STATUS, TRB_IOC) | (1 << 16));
    doorbell(hw, caps.db, slot, 1);
    consume_event(hw, caps, ev, TRB_EVENT_TRANSFER, SPINS)?;
    Ok(())
}

fn control_nodata_retry(
    hw: &mut impl XhciHw,
    caps: &XhciCaps,
    mem: &XhciMem,
    cmd_ring: &mut Ring,
    ep0: &mut Ring,
    ev: &mut EventRing,
    slot: u8,
    setup: [u8; 8],
    port: u8,
) -> Result<(), UsbBotError> {
    let mut last = UsbBotError::Xfer;
    for n in 0..XHCI_DESC_TRIES {
        drain_events(hw, caps, ev);
        match control_nodata(hw, caps, ep0, ev, slot, setup) {
            Ok(()) => return Ok(()),
            Err(e) => {
                last = e;
                if n + 1 < XHCI_DESC_TRIES {
                    serial_xhci_desc_retry(port, n + 1);
                    reset_ep0(hw, caps, mem, cmd_ring, ev, ep0, slot);
                }
            }
        }
    }
    Err(last)
}

fn setup_get_desc(ty: u8, len: u16) -> [u8; 8] {
    let mut s = [0u8; 8];
    s[0] = 0x80;
    s[1] = 6;
    s[3] = ty;
    s[6] = len as u8;
    s[7] = (len >> 8) as u8;
    s
}

fn setup_set_config(cfg: u8) -> [u8; 8] {
    let mut s = [0u8; 8];
    s[1] = 9;
    s[2] = cfg;
    s
}

/// `(has_bot, has_uas)` from configuration descriptor interface protocols.
pub fn cfg_msc_protos(cfg: &[u8]) -> (bool, bool) {
    if cfg.len() < 9 {
        return (false, false);
    }
    let total = usize::from(u16::from_le_bytes([cfg[2], cfg[3]])).min(cfg.len());
    let mut i = 0usize;
    let mut bot = false;
    let mut uas = false;
    while i + 2 <= total {
        let len = cfg[i] as usize;
        if len < 2 || i + len > total {
            break;
        }
        if cfg[i + 1] == 4 && len >= 9 {
            let proto = cfg[i + 7];
            if cfg[i + 5] == 8 && cfg[i + 6] == 6 {
                if proto == USB_MSC_BOT {
                    bot = true;
                }
                if proto == USB_MSC_UAS {
                    uas = true;
                }
            }
        }
        i += len;
    }
    (bot, uas)
}

/// Find SCSI BOT bulk endpoints in a configuration descriptor.
pub fn parse_bot_eps(cfg: &[u8]) -> Option<(u8, u8, u16, u16, u8)> {
    if cfg.len() < 9 {
        return None;
    }
    let total = usize::from(u16::from_le_bytes([cfg[2], cfg[3]])).min(cfg.len());
    let mut i = 0usize;
    let mut iface_ok = false;
    let cfg_val = cfg[5];
    let mut ep_out = 0u8;
    let mut ep_in = 0u8;
    let mut mps_out = 512u16;
    let mut mps_in = 512u16;
    while i + 2 <= total {
        let len = cfg[i] as usize;
        if len < 2 || i + len > total {
            break;
        }
        let ty = cfg[i + 1];
        if ty == 4 && len >= 9 {
            iface_ok = cfg[i + 5] == 8 && cfg[i + 6] == 6 && cfg[i + 7] == USB_MSC_BOT;
        }
        if ty == 5 && len >= 7 && iface_ok {
            let addr = cfg[i + 2];
            let attr = cfg[i + 3];
            let mps = u16::from_le_bytes([cfg[i + 4], cfg[i + 5]]);
            if attr & 3 == 2 {
                if addr & 0x80 != 0 {
                    ep_in = addr & 0xF;
                    mps_in = mps;
                } else {
                    ep_out = addr & 0xF;
                    mps_out = mps;
                }
            }
        }
        i += len;
    }
    if iface_ok && ep_out != 0 && ep_in != 0 {
        Some((ep_out, ep_in, mps_out, mps_in, cfg_val))
    } else {
        None
    }
}

struct LiveXhci {
    mmio: u64,
    caps: XhciCaps,
    ev: EventRing,
    cmd: Ring,
    bulk_out: Ring,
    bulk_in: Ring,
    slot: u8,
    dci_out: u8,
    dci_in: u8,
    bounce: u64,
    bulk_out_hpa: u64,
    bulk_in_hpa: u64,
    lba: u32,
    tag: u32,
}

fn reset_bulk_ep(
    hw: &mut impl XhciHw,
    caps: &XhciCaps,
    cmd_ring: &mut Ring,
    ev: &mut EventRing,
    ring: &mut Ring,
    hpa: u64,
    slot: u8,
    dci: u8,
) {
    drain_events(hw, caps, ev);
    let extra = xhci_ep_cmd_extra(slot, dci);
    let _ = cmd(hw, caps, cmd_ring, ev, 0, trb_ctrl(0, TRB_RESET_EP, extra));
    zero_page(hw, hpa);
    *ring = Ring::new(hpa);
    let _ = cmd(
        hw,
        caps,
        cmd_ring,
        ev,
        hpa | 1,
        trb_ctrl(0, TRB_SET_TR_DEQ, extra),
    );
}

/// ISP only on the 13-byte CSW short packet. Full-size READ/WRITE IN uses IOC.
pub fn bulk_in_trb_flags(len: usize) -> u32 {
    if len == CSW_LEN {
        TRB_IOC | TRB_ISP
    } else {
        TRB_IOC
    }
}

impl UsbBulk for LiveXhci {
    fn bulk_out(&mut self, data: &[u8]) -> Result<(), UsbBotError> {
        if data.is_empty() || data.len() > 4096 {
            return Err(UsbBotError::Xfer);
        }
        let mut hw = MmioXhci { base: self.mmio };
        drain_events(&mut hw, &self.caps, &mut self.ev);
        hw.dma_write(self.bounce, data);
        self.bulk_out.place(
            &mut hw,
            self.bounce,
            data.len() as u32,
            trb_ctrl(0, TRB_NORMAL, TRB_IOC),
        );
        doorbell(&mut hw, self.caps.db, self.slot, self.dci_out);
        consume_event(
            &mut hw,
            &self.caps,
            &mut self.ev,
            TRB_EVENT_TRANSFER,
            BULK_SPINS,
        )
        .map(|_| ())
    }

    fn bulk_in(&mut self, data: &mut [u8]) -> Result<usize, UsbBotError> {
        if data.is_empty() || data.len() > 4096 {
            return Err(UsbBotError::Xfer);
        }
        let mut hw = MmioXhci { base: self.mmio };
        drain_events(&mut hw, &self.caps, &mut self.ev);
        let z = [0u8; 4096];
        hw.dma_write(self.bounce, &z[..data.len()]);
        self.bulk_in.place(
            &mut hw,
            self.bounce,
            data.len() as u32,
            trb_ctrl(0, TRB_NORMAL, bulk_in_trb_flags(data.len())),
        );
        doorbell(&mut hw, self.caps.db, self.slot, self.dci_in);
        match consume_event(
            &mut hw,
            &self.caps,
            &mut self.ev,
            TRB_EVENT_TRANSFER,
            BULK_SPINS,
        ) {
            Ok(_) => {
                hw.dma_read(self.bounce, data);
                Ok(data.len())
            }
            Err(e) => Err(e),
        }
    }

    fn recover_pipes(&mut self) {
        let mut hw = MmioXhci { base: self.mmio };
        let caps = self.caps;
        let slot = self.slot;
        let dci_out = self.dci_out;
        let dci_in = self.dci_in;
        let hpa_out = self.bulk_out_hpa;
        let hpa_in = self.bulk_in_hpa;
        reset_bulk_ep(
            &mut hw,
            &caps,
            &mut self.cmd,
            &mut self.ev,
            &mut self.bulk_out,
            hpa_out,
            slot,
            dci_out,
        );
        reset_bulk_ep(
            &mut hw,
            &caps,
            &mut self.cmd,
            &mut self.ev,
            &mut self.bulk_in,
            hpa_in,
            slot,
            dci_in,
        );
    }
}

struct MmioXhci {
    base: u64,
}

impl XhciHw for MmioXhci {
    fn read32(&mut self, off: u32) -> u32 {
        // SAFETY: firmware-assigned xHCI BAR; identity map after EBS.
        // KANI-TARGET: host tests pack TRBs, not this MMIO.
        unsafe { core::ptr::read_volatile((self.base + u64::from(off)) as *const u32) }
    }

    fn write32(&mut self, off: u32, val: u32) {
        // SAFETY: firmware-assigned xHCI BAR.
        // KANI-TARGET: host tests pack TRBs, not this MMIO.
        unsafe {
            core::ptr::write_volatile((self.base + u64::from(off)) as *mut u32, val);
        }
    }

    fn dma_read(&mut self, hpa: u64, buf: &mut [u8]) {
        if buf.is_empty() {
            return;
        }
        // SAFETY: ring/bounce pages in this EFI image.
        unsafe {
            for (i, b) in buf.iter_mut().enumerate() {
                *b = core::ptr::read_volatile((hpa as *const u8).add(i));
            }
        }
    }

    fn dma_write(&mut self, hpa: u64, buf: &[u8]) {
        if buf.is_empty() {
            return;
        }
        // SAFETY: ring/bounce pages in this EFI image.
        unsafe {
            for (i, b) in buf.iter().enumerate() {
                core::ptr::write_volatile((hpa as *mut u8).add(i), *b);
            }
        }
        core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
    }
}

fn stamp_enum(
    hw: &mut impl XhciHw,
    caps: &XhciCaps,
    mmio: u64,
    port: u8,
    cmd: u8,
    err: UsbBotError,
) -> UsbBotError {
    let sc = hw.read32(portsc_off(caps.op, port));
    let speed = portsc_speed(sc);
    let cmpl = usb_bot_last_cmpl() as u8;
    store_usb_bot_diag(
        err,
        mmio,
        xhci_enum_diag_portsc(sc, port),
        xhci_enum_diag_cmpl(cmpl, cmd, speed, caps.csz),
    );
    serial_xhci_enum(
        port,
        sc,
        speed,
        port_reset_use_wpr(port_is_usb3(hw, port), sc),
        caps.csz,
        cmd,
        cmpl,
    );
    err
}

fn try_port(
    hw: &mut impl XhciHw,
    caps: &XhciCaps,
    mem: &XhciMem,
    cmd_ring: &mut Ring,
    ev: &mut EventRing,
    port: u8,
    min_bytes: u64,
    mmio: u64,
) -> Result<LiveXhci, UsbBotError> {
    let sc = reset_port(hw, caps, port)
        .map_err(|e| stamp_enum(hw, caps, mmio, port, XHCI_ENUM_CMD_RESET, e))?;
    let speed = portsc_speed(sc);
    let Some(slot_dw0) = slot_ctx_dw0(speed, 1) else {
        store_usb_bot_diag(UsbBotError::Enum, mmio, xhci_enum_diag_portsc(sc, port), 0);
        return Err(stamp_enum(
            hw,
            caps,
            mmio,
            port,
            XHCI_ENUM_CMD_ADDR,
            UsbBotError::Enum,
        ));
    };
    let ev_en = cmd(hw, caps, cmd_ring, ev, 0, trb_ctrl(0, TRB_ENABLE_SLOT, 0)).map_err(|e| {
        recover_enum(hw, caps, mem, cmd_ring, ev, 0);
        stamp_enum(hw, caps, mmio, port, XHCI_ENUM_CMD_SLOT, e)
    })?;
    let slot = event_slot(&ev_en);
    if slot == 0 {
        recover_enum(hw, caps, mem, cmd_ring, ev, 0);
        return Err(stamp_enum(
            hw,
            caps,
            mmio,
            port,
            XHCI_ENUM_CMD_SLOT,
            UsbBotError::Enum,
        ));
    }
    let mut dc = [0u8; 8];
    put_u64(&mut dc, 0, mem.devctx);
    hw.dma_write(mem.dcbaa + u64::from(slot) * 8, &dc);
    zero_page(hw, mem.inctx);
    zero_page(hw, mem.devctx);
    zero_page(hw, mem.ep0);
    let cs = ctx_size(caps.csz);
    let mut inctx = [0u8; 4096];
    put_u32(&mut inctx, 4, 0x3);
    put_u32(&mut inctx, cs, slot_dw0);
    put_u32(&mut inctx, cs + 4, slot_ctx_dw1_port(port));
    let mps0 = u32::from(ep0_max_packet(speed));
    put_u32(
        &mut inctx,
        cs * 2 + 4,
        (4u32 << 3) | (3 << 1) | (mps0 << 16),
    );
    put_u64(&mut inctx, cs * 2 + 8, mem.ep0 | 1);
    hw.dma_write(mem.inctx, &inctx);
    cmd_wait(
        hw,
        caps,
        cmd_ring,
        ev,
        mem.inctx,
        trb_ctrl(0, TRB_ADDRESS_DEV, u32::from(slot) << 24),
        ADDR_SPINS,
    )
    .map_err(|e| {
        recover_enum(hw, caps, mem, cmd_ring, ev, slot);
        stamp_enum(hw, caps, mmio, port, XHCI_ENUM_CMD_ADDR, e)
    })?;
    let mut ep0 = Ring::new(mem.ep0);
    let mut dev = [0u8; 18];
    control_in_retry(
        hw,
        caps,
        mem,
        cmd_ring,
        &mut ep0,
        ev,
        slot,
        mem.bounce,
        setup_get_desc(1, 18),
        &mut dev,
        port,
    )
    .map_err(|e| {
        recover_enum(hw, caps, mem, cmd_ring, ev, slot);
        stamp_enum(hw, caps, mmio, port, XHCI_ENUM_CMD_DESC, e)
    })?;
    serial_xhci_dev(
        port,
        u16::from_le_bytes([dev[8], dev[9]]),
        u16::from_le_bytes([dev[10], dev[11]]),
        dev[4],
        dev[6],
    );
    if usb_dev_is_hub(dev[4]) {
        serial_xhci_hub(port);
        recover_enum(hw, caps, mem, cmd_ring, ev, slot);
        return Err(UsbBotError::Hub);
    }
    let mut cfg9 = [0u8; 9];
    control_in_retry(
        hw,
        caps,
        mem,
        cmd_ring,
        &mut ep0,
        ev,
        slot,
        mem.bounce,
        setup_get_desc(2, 9),
        &mut cfg9,
        port,
    )
    .map_err(|e| {
        recover_enum(hw, caps, mem, cmd_ring, ev, slot);
        stamp_enum(hw, caps, mmio, port, XHCI_ENUM_CMD_DESC, e)
    })?;
    let total = u16::from_le_bytes([cfg9[2], cfg9[3]]).min(256);
    let mut cfg = [0u8; 256];
    control_in_retry(
        hw,
        caps,
        mem,
        cmd_ring,
        &mut ep0,
        ev,
        slot,
        mem.bounce,
        setup_get_desc(2, total),
        &mut cfg[..total as usize],
        port,
    )
    .map_err(|e| {
        recover_enum(hw, caps, mem, cmd_ring, ev, slot);
        stamp_enum(hw, caps, mmio, port, XHCI_ENUM_CMD_DESC, e)
    })?;
    let Some((ep_out, ep_in, mps_out, mps_in, cfg_val)) = parse_bot_eps(&cfg[..total as usize])
    else {
        let (_bot, uas) = cfg_msc_protos(&cfg[..total as usize]);
        if uas {
            serial_xhci_uas(port);
        }
        recover_enum(hw, caps, mem, cmd_ring, ev, slot);
        return Err(UsbBotError::Bot);
    };
    serial_xhci_bot_eps(port, ep_out, ep_in, cfg_val);
    // xHCI 4.3.5: SET_CONFIGURATION on EP0, then Configure Endpoint.
    // Iron `96024edc`: CONFIG_EP first then SET_CONFIG `cmd=5 cmpl=0 err=8`.
    control_nodata_retry(
        hw,
        caps,
        mem,
        cmd_ring,
        &mut ep0,
        ev,
        slot,
        setup_set_config(cfg_val),
        port,
    )
    .map_err(|e| {
        recover_enum(hw, caps, mem, cmd_ring, ev, slot);
        stamp_enum(hw, caps, mmio, port, XHCI_ENUM_CMD_SETCFG, e)
    })?;
    serial_xhci_setcfg(port, cfg_val);
    let dci_out = bulk_ep_dci(ep_out, false);
    let dci_in = bulk_ep_dci(ep_in, true);
    let hi = core::cmp::max(dci_out, dci_in);
    zero_page(hw, mem.inctx);
    let mut ic = [0u8; 4096];
    put_u32(&mut ic, 4, 1 | (1 << dci_out) | (1 << dci_in));
    put_u32(&mut ic, cs, slot_ctx_dw0(speed, hi).unwrap_or(slot_dw0));
    put_u32(&mut ic, cs + 4, slot_ctx_dw1_port(port));
    let out_off = cs * (1 + dci_out as usize);
    put_u32(&mut ic, out_off + 4, ep_ctx_dw1(2, mps_out));
    put_u64(&mut ic, out_off + 8, mem.bulk_out | 1);
    put_u32(&mut ic, out_off + 16, ep_ctx_dw4_avg_trb(mps_out));
    let in_off = cs * (1 + dci_in as usize);
    put_u32(&mut ic, in_off + 4, ep_ctx_dw1(6, mps_in));
    put_u64(&mut ic, in_off + 8, mem.bulk_in | 1);
    put_u32(&mut ic, in_off + 16, ep_ctx_dw4_avg_trb(mps_in));
    hw.dma_write(mem.inctx, &ic);
    zero_page(hw, mem.bulk_out);
    zero_page(hw, mem.bulk_in);
    cmd(
        hw,
        caps,
        cmd_ring,
        ev,
        mem.inctx,
        trb_ctrl(0, TRB_CONFIG_EP, u32::from(slot) << 24),
    )
    .map_err(|e| {
        recover_enum(hw, caps, mem, cmd_ring, ev, slot);
        stamp_enum(hw, caps, mmio, port, XHCI_ENUM_CMD_CFG, e)
    })?;
    let mut live = LiveXhci {
        mmio,
        caps: *caps,
        ev: EventRing {
            base: ev.base,
            deq: ev.deq,
            cycle: ev.cycle,
        },
        cmd: Ring {
            base: cmd_ring.base,
            enq: cmd_ring.enq,
            cycle: cmd_ring.cycle,
        },
        bulk_out: Ring::new(mem.bulk_out),
        bulk_in: Ring::new(mem.bulk_in),
        slot,
        dci_out,
        dci_in,
        bounce: mem.bounce,
        bulk_out_hpa: mem.bulk_out,
        bulk_in_hpa: mem.bulk_in,
        lba: 512,
        tag: 10,
    };
    match usb_bot_bring_up(&mut live, min_bytes) {
        Ok((bytes, lba)) => {
            if lun_is_esp_cruzer(bytes) {
                recover_enum(hw, caps, mem, cmd_ring, ev, slot);
                return Err(UsbBotError::Cruzer);
            }
            live.lba = lba;
            live.tag = next_bot_tag();
            store_usb_bot_ready(bytes, lba);
            Ok(live)
        }
        Err(e) => {
            let sc = hw.read32(portsc_off(caps.op, port));
            store_usb_bot_diag(
                e,
                mmio,
                xhci_enum_diag_portsc(sc, port),
                usb_bot_last_cmpl(),
            );
            recover_enum(hw, caps, mem, cmd_ring, ev, slot);
            Err(e)
        }
    }
}

fn xhci_bring_up(
    hw: &mut impl XhciHw,
    mem: &XhciMem,
    min_bytes: u64,
    mmio: u64,
) -> Result<LiveXhci, UsbBotError> {
    let (caps, mut cmd_ring, mut ev) = xhci_start(hw, mem)?;
    let mut last = UsbBotError::Reset;
    let mut any_ccs = false;
    let ports = xhci_scan_ports(caps.max_ports);
    for port in 1..=ports {
        let sc = hw.read32(portsc_off(caps.op, port));
        if sc & PORTSC_CCS == 0 {
            continue;
        }
        any_ccs = true;
        match try_port(
            hw,
            &caps,
            mem,
            &mut cmd_ring,
            &mut ev,
            port,
            min_bytes,
            mmio,
        ) {
            Ok(live) => return Ok(live),
            Err(e) => last = xhci_bring_up_keep_err(last, e),
        }
    }
    if !any_ccs {
        return Err(UsbBotError::Reset);
    }
    store_usb_bot_diag(last, mmio, usb_bot_last_portsc(), usb_bot_last_cmpl());
    Err(last)
}

#[repr(C, align(4096))]
#[derive(Clone, Copy)]
struct Page([u8; 4096]);

#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
static mut DCBAA: Page = Page([0; 4096]);
#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
static mut SCRATCH_ARRAY: Page = Page([0; 4096]);
#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
static mut CMD: Page = Page([0; 4096]);
#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
static mut EVT: Page = Page([0; 4096]);
#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
static mut ERST: Page = Page([0; 4096]);
#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
static mut INCTX: Page = Page([0; 4096]);
#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
static mut DEVCTX: Page = Page([0; 4096]);
#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
static mut EP0: Page = Page([0; 4096]);
#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
static mut BULKOUT: Page = Page([0; 4096]);
#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
static mut BULKIN: Page = Page([0; 4096]);
#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
static mut BOUNCE: Page = Page([0; 4096]);
#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
static mut SCRATCH0: [Page; XHCI_SCRATCH_MAX as usize] =
    [Page([0; 4096]); XHCI_SCRATCH_MAX as usize];

#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
static mut LIVE: Option<LiveXhci> = None;
#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
static LIVE_LOCK: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);
#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
static RW_FAIL_NOTED: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);

#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
fn xhci_bar(bus: u8, dev: u8, func: u8) -> u64 {
    use crate::mgmt::e1000_mmio::{pci_read32, pci_write32};
    let cmd = pci_read32(bus, dev, func, 0x04) as u16;
    let next = cmd | (1 << 1) | (1 << 2);
    let rest = pci_read32(bus, dev, func, 0x04) & 0xFFFF_0000;
    pci_write32(bus, dev, func, 0x04, rest | u32::from(next));
    let bar0 = pci_read32(bus, dev, func, 0x10);
    let lo = u64::from(bar0 & !0xF);
    if bar0 & 0x4 != 0 {
        let bar1 = pci_read32(bus, dev, func, 0x14);
        lo | (u64::from(bar1) << 32)
    } else {
        lo
    }
}

/// Bring up the first eligible USB BOT LUN on this xHCI BDF (post-EBS).
#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
pub fn xhci_init_pci(bus: u8, dev: u8, func: u8, min_bytes: u64) -> Result<u64, UsbBotError> {
    if LIVE_LOCK.swap(true, core::sync::atomic::Ordering::Acquire) {
        return Err(UsbBotError::Enum);
    }
    let bar = xhci_bar(bus, dev, func);
    store_usb_bot_diag(UsbBotError::Cap, bar, 0, 0);
    if bar == 0 {
        LIVE_LOCK.store(false, core::sync::atomic::Ordering::Release);
        return Err(UsbBotError::Cap);
    }
    // SAFETY: BSP-only; pages are .bss in this image.
    let mem = unsafe {
        XhciMem {
            dcbaa: core::ptr::addr_of_mut!(DCBAA) as u64,
            scratch_array: core::ptr::addr_of_mut!(SCRATCH_ARRAY) as u64,
            cmd: core::ptr::addr_of_mut!(CMD) as u64,
            evt: core::ptr::addr_of_mut!(EVT) as u64,
            erst: core::ptr::addr_of_mut!(ERST) as u64,
            inctx: core::ptr::addr_of_mut!(INCTX) as u64,
            devctx: core::ptr::addr_of_mut!(DEVCTX) as u64,
            ep0: core::ptr::addr_of_mut!(EP0) as u64,
            bulk_out: core::ptr::addr_of_mut!(BULKOUT) as u64,
            bulk_in: core::ptr::addr_of_mut!(BULKIN) as u64,
            bounce: core::ptr::addr_of_mut!(BOUNCE) as u64,
            scratch0: core::ptr::addr_of_mut!(SCRATCH0) as u64,
        }
    };
    let mut hw = MmioXhci { base: bar };
    match xhci_bring_up(&mut hw, &mem, min_bytes, bar) {
        Ok(live) => {
            let bytes = super::usb_bot::usb_bot_ns_bytes();
            // SAFETY: lock held; single LIVE writer.
            unsafe {
                LIVE = Some(live);
            }
            LIVE_LOCK.store(false, core::sync::atomic::Ordering::Release);
            Ok(bytes)
        }
        Err(e) => {
            store_usb_bot_diag(
                e,
                bar,
                super::usb_bot::usb_bot_last_portsc(),
                super::usb_bot::usb_bot_last_cmpl(),
            );
            LIVE_LOCK.store(false, core::sync::atomic::Ordering::Release);
            Err(e)
        }
    }
}

#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
pub fn xhci_live_rw(off: u64, buf: &mut [u8], write: bool) -> bool {
    if LIVE_LOCK.swap(true, core::sync::atomic::Ordering::Acquire) {
        return false;
    }
    // SAFETY: lock held; LIVE set by xhci_init_pci.
    let ok = unsafe {
        match LIVE.as_mut() {
            Some(live) => {
                if buf.len() > 4096 {
                    false
                } else {
                    let mut slice = [0u8; 4096];
                    if write {
                        slice[..buf.len()].copy_from_slice(buf);
                    }
                    let mut tag = live.tag;
                    let lba = live.lba;
                    let r = super::usb_bot::usb_bot_rw(
                        live,
                        &mut tag,
                        lba,
                        off,
                        &mut slice[..buf.len()],
                        write,
                    );
                    live.tag = tag;
                    match r {
                        Ok(()) => {
                            if !write {
                                buf.copy_from_slice(&slice[..buf.len()]);
                            }
                            true
                        }
                        Err(e) => {
                            store_usb_bot_diag(
                                e,
                                live.mmio,
                                usb_bot_last_portsc(),
                                usb_bot_last_cmpl(),
                            );
                            serial_xhci_rw_fail(off, write, e);
                            false
                        }
                    }
                }
            }
            None => false,
        }
    };
    LIVE_LOCK.store(false, core::sync::atomic::Ordering::Release);
    ok
}

#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
fn serial_xhci_rw_fail(off: u64, write: bool, err: UsbBotError) {
    if RW_FAIL_NOTED.swap(true, core::sync::atomic::Ordering::AcqRel) {
        return;
    }
    use crate::boot::serial;
    serial::write_str("boot: Stage 46 durable LUN usb rw fail off=0x");
    serial_hex32(off as u32);
    serial::write_str(if write { " wr=1 err=" } else { " wr=0 err=" });
    serial_dec_u8(err as u8);
    serial::write_str(" cmpl=0x");
    serial_hex32(usb_bot_last_cmpl() as u32);
    serial::write_str(" bot=");
    serial::write_str(usb_bot_stage_name(usb_bot_last_stage()));
    serial::write_line(" (not ISO-INSTALL-OK)");
}

#[cfg(not(all(target_os = "uefi", feature = "uefi-bin")))]
pub fn xhci_init_pci(_bus: u8, _dev: u8, _func: u8, _min_bytes: u64) -> Result<u64, UsbBotError> {
    Err(UsbBotError::Cap)
}

#[cfg(not(all(target_os = "uefi", feature = "uefi-bin")))]
pub fn xhci_live_rw(_off: u64, _buf: &mut [u8], _write: bool) -> bool {
    false
}

#[cfg(test)]
mod xhci_pack_test {
    use super::*;
    use crate::mgmt::usb_bot::{usb_bot_keep_xfer_diag, usb_bot_last_err};

    #[test]
    fn trb_ctrl_packs_type_and_cycle() {
        let c = trb_ctrl(1, TRB_SETUP, TRB_IDT);
        assert_eq!(trb_type(c), TRB_SETUP);
        assert_eq!(c & 1, 1);
        assert_ne!(c & TRB_IDT, 0);
        assert_eq!(trb_cmpl_code(1u32 << 24), CMPL_SUCCESS);
    }

    #[test]
    fn qemu_xhci_protocol_caps_split_usb2_usb3() {
        // qemu-xhci p3=4,p2=4: USB3 ports 1–4 (major 3), USB2 ports 5–8 (major 2).
        let usb3 = (3u32 << 24) | 2;
        let usb3_dw2 = 4u32 << 8 | 1;
        let usb2 = (2u32 << 24) | 2;
        let usb2_dw2 = 4u32 << 8 | 5;
        assert_eq!(supported_protocol_matches(usb3, usb3_dw2, 1), Some(true));
        assert_eq!(supported_protocol_matches(usb3, usb3_dw2, 4), Some(true));
        assert_eq!(supported_protocol_matches(usb3, usb3_dw2, 5), None);
        assert_eq!(supported_protocol_matches(usb2, usb2_dw2, 5), Some(false));
        assert_eq!(supported_protocol_matches(usb2, usb2_dw2, 8), Some(false));
        assert_eq!(supported_protocol_matches(usb2, usb2_dw2, 1), None);
    }

    #[test]
    fn slot_ctx_dw1_port_is_root_hub_port_only() {
        // Iron Enum EFI `c6bdd671`: Address Device Parameter Error when
        // Number of Ports (31:24) copied the port number (10/11/14) with Hub=0.
        assert_eq!(slot_ctx_dw1_port(1), 0x0001_0000);
        assert_eq!(slot_ctx_dw1_port(5), 0x0005_0000);
        assert_eq!(slot_ctx_dw1_port(10), 0x000a_0000);
        assert_eq!(slot_ctx_dw1_port(14), 0x000e_0000);
        assert_eq!(slot_ctx_dw1_num_ports(slot_ctx_dw1_port(14)), 0);
        let dual = u32::from(14u8) << 24 | u32::from(14u8) << 16;
        assert_ne!(slot_ctx_dw1_port(14), dual);
    }

    #[test]
    fn intel_pch_scratchpad_and_usblegsup_run_without_inte() {
        assert_eq!(xhci_scratchpad_bufs(0), 0);
        assert_eq!(xhci_scratchpad_bufs(4u32 << 27), 4);
        assert_eq!(xhci_scratchpad_bufs(31u32 << 27), 31);
        assert_eq!(xhci_scratchpad_bufs(xhci_hcs2_with_scratch(31)), 31);
        assert_eq!(xhci_scratchpad_bufs(xhci_hcs2_with_scratch(64)), 64);
        assert!(xhci_scratchpad_supported(4u32 << 27));
        assert!(xhci_scratchpad_supported(31u32 << 27));
        assert!(xhci_scratchpad_supported(xhci_hcs2_with_scratch(
            XHCI_SCRATCH_MAX
        )));
        assert!(!xhci_scratchpad_supported(xhci_hcs2_with_scratch(
            XHCI_SCRATCH_MAX + 1
        )));
        assert_eq!(xhci_run_usbcmd(), USBCMD_RS);
        assert_eq!(xhci_run_usbcmd() & USBCMD_INTE, 0);
        assert_eq!(USBLEGSUP_ID, 1);
        let cap = u32::from(USBLEGSUP_ID) | USBLEGSUP_BIOS_OWNED;
        assert_eq!(cap | USBLEGSUP_OS_OWNED, cap | (1 << 24));
        struct Leg {
            hcc1: u32,
            leg: u32,
        }
        impl XhciHw for Leg {
            fn read32(&mut self, off: u32) -> u32 {
                if off == 0x10 {
                    self.hcc1
                } else if off == 0x20 {
                    self.leg
                } else {
                    0
                }
            }
            fn write32(&mut self, off: u32, val: u32) {
                if off == 0x20 {
                    self.leg = val & !USBLEGSUP_BIOS_OWNED;
                }
            }
            fn dma_read(&mut self, _hpa: u64, buf: &mut [u8]) {
                buf.fill(0);
            }
            fn dma_write(&mut self, _hpa: u64, _buf: &[u8]) {}
        }
        let mut hw = Leg {
            hcc1: (0x20 / 4) << 16,
            leg: u32::from(USBLEGSUP_ID) | USBLEGSUP_BIOS_OWNED,
        };
        handshake_legacy(&mut hw);
        assert_ne!(hw.leg & USBLEGSUP_OS_OWNED, 0);
    }

    #[test]
    fn cap_snap_packs_diag_and_rejects_dead_mmio() {
        struct Caps {
            cap0: u32,
            hcs1: u32,
            hcs2: u32,
        }
        impl XhciHw for Caps {
            fn read32(&mut self, off: u32) -> u32 {
                match off {
                    0 => self.cap0,
                    0x04 => self.hcs1,
                    0x08 => self.hcs2,
                    0x10 => 0,
                    0x14 => 0x1000,
                    0x18 => 0x2000,
                    _ => 0,
                }
            }
            fn write32(&mut self, _off: u32, _val: u32) {}
            fn dma_read(&mut self, _hpa: u64, buf: &mut [u8]) {
                buf.fill(0);
            }
            fn dma_write(&mut self, _hpa: u64, _buf: &[u8]) {}
        }
        let mut dead = Caps {
            cap0: 0,
            hcs1: 0,
            hcs2: 0,
        };
        let snap0 = xhci_read_cap_snap(&mut dead);
        assert!(snap0.mmio_dead());
        assert!(parse_caps_from_snap(&mut dead, snap0).is_err());

        let mut ones = Caps {
            cap0: 0xFFFF_FFFF,
            hcs1: 0xFFFF_FFFF,
            hcs2: 0xFFFF_FFFF,
        };
        let snap_ff = xhci_read_cap_snap(&mut ones);
        assert!(snap_ff.mmio_dead());
        assert!(snap_ff.over_budget());

        // Lewisburg-shaped: caplen 0x20, 64 slots, 22 ports, 31 scratch.
        let mut pch = Caps {
            cap0: 0x20,
            hcs1: 64 | (22u32 << 24),
            hcs2: 31u32 << 27,
        };
        let snap = xhci_read_cap_snap(&mut pch);
        assert!(!snap.mmio_dead());
        assert_eq!(snap.caplen(), 0x20);
        assert_eq!(snap.max_slots(), 64);
        assert_eq!(snap.max_ports(), 22);
        assert_eq!(snap.scratch(), 31);
        assert!(!snap.over_budget());
        assert_eq!(
            xhci_cap_diag_portsc(snap),
            u64::from(snap.cap0) | (u64::from(snap.hcs1) << 32)
        );
        assert_eq!(
            xhci_cap_diag_cmpl(snap),
            u64::from(snap.hcs2) | (31u64 << 32)
        );
        let caps = parse_caps_from_snap(&mut pch, snap).expect("pch caps");
        assert_eq!(caps.max_slots, 64);
        assert_eq!(caps.max_ports, 22);
        assert_eq!(caps.op, 0x20);
    }

    #[test]
    fn parse_qemu_like_bot_config() {
        // config(9) + iface(9) + bulk OUT + bulk IN
        let mut cfg = [0u8; 32];
        cfg[0] = 9;
        cfg[1] = 2;
        cfg[2] = 32;
        cfg[3] = 0;
        cfg[4] = 1;
        cfg[5] = 1;
        cfg[9] = 9;
        cfg[10] = 4;
        cfg[14] = 8;
        cfg[15] = 6;
        cfg[16] = 0x50;
        cfg[18] = 7;
        cfg[19] = 5;
        cfg[20] = 0x01;
        cfg[21] = 2;
        cfg[22] = 0x00;
        cfg[23] = 0x02; // 512
        cfg[25] = 7;
        cfg[26] = 5;
        cfg[27] = 0x82;
        cfg[28] = 2;
        cfg[29] = 0x00;
        cfg[30] = 0x02;
        let (out, inn, mps_o, mps_i, cfgv) = parse_bot_eps(&cfg).expect("bot");
        assert_eq!(out, 1);
        assert_eq!(inn, 2);
        assert_eq!(mps_o, 512);
        assert_eq!(mps_i, 512);
        assert_eq!(cfgv, 1);
        assert_eq!(cfg_msc_protos(&cfg), (true, false));
    }

    #[test]
    fn parse_toshiba_bot_eps_out2_in1() {
        // Iron `96024edc`: `xhci bot p11 iface=08/06/50 ep_out=2 ep_in=1 cfg=1`.
        let mut cfg = [0u8; 32];
        cfg[0] = 9;
        cfg[1] = 2;
        cfg[2] = 32;
        cfg[5] = 1;
        cfg[9] = 9;
        cfg[10] = 4;
        cfg[14] = 8;
        cfg[15] = 6;
        cfg[16] = USB_MSC_BOT;
        cfg[18] = 7;
        cfg[19] = 5;
        cfg[20] = 0x02;
        cfg[21] = 2;
        cfg[22] = 0x00;
        cfg[23] = 0x02;
        cfg[25] = 7;
        cfg[26] = 5;
        cfg[27] = 0x81;
        cfg[28] = 2;
        cfg[29] = 0x00;
        cfg[30] = 0x02;
        let (out, inn, _, _, cfgv) = parse_bot_eps(&cfg).expect("bot");
        assert_eq!(out, 2);
        assert_eq!(inn, 1);
        assert_eq!(cfgv, 1);
        assert_eq!(bulk_ep_dci(out, false), 4);
        assert_eq!(bulk_ep_dci(inn, true), 3);
    }

    #[test]
    fn parse_uas_only_config_is_not_bot() {
        let mut cfg = [0u8; 18];
        cfg[0] = 9;
        cfg[1] = 2;
        cfg[2] = 18;
        cfg[3] = 0;
        cfg[4] = 1;
        cfg[5] = 1;
        cfg[9] = 9;
        cfg[10] = 4;
        cfg[14] = 8;
        cfg[15] = 6;
        cfg[16] = USB_MSC_UAS;
        assert_eq!(parse_bot_eps(&cfg), None);
        assert_eq!(cfg_msc_protos(&cfg), (false, true));
    }

    #[test]
    fn parse_bot_plus_uas_prefers_bot() {
        let mut cfg = [0u8; 41];
        cfg[0] = 9;
        cfg[1] = 2;
        cfg[2] = 41;
        cfg[4] = 2;
        cfg[5] = 1;
        cfg[9] = 9;
        cfg[10] = 4;
        cfg[14] = 8;
        cfg[15] = 6;
        cfg[16] = USB_MSC_UAS;
        cfg[18] = 9;
        cfg[19] = 4;
        cfg[23] = 8;
        cfg[24] = 6;
        cfg[25] = USB_MSC_BOT;
        cfg[27] = 7;
        cfg[28] = 5;
        cfg[29] = 0x01;
        cfg[30] = 2;
        cfg[31] = 0x00;
        cfg[32] = 0x02;
        cfg[34] = 7;
        cfg[35] = 5;
        cfg[36] = 0x82;
        cfg[37] = 2;
        cfg[38] = 0x00;
        cfg[39] = 0x02;
        assert_eq!(cfg_msc_protos(&cfg), (true, true));
        let (out, inn, _, _, _) = parse_bot_eps(&cfg).expect("bot");
        assert_eq!(out, 1);
        assert_eq!(inn, 2);
    }

    struct FakeMem {
        mem: [u8; 4096],
    }

    impl XhciHw for FakeMem {
        fn read32(&mut self, _off: u32) -> u32 {
            0
        }
        fn write32(&mut self, _off: u32, _val: u32) {}
        fn dma_read(&mut self, hpa: u64, buf: &mut [u8]) {
            let o = hpa as usize;
            buf.copy_from_slice(&self.mem[o..o + buf.len()]);
        }
        fn dma_write(&mut self, hpa: u64, buf: &[u8]) {
            let o = hpa as usize;
            self.mem[o..o + buf.len()].copy_from_slice(buf);
        }
    }

    #[test]
    fn transfer_ring_places_link_before_wrap() {
        let mut hw = FakeMem { mem: [0; 4096] };
        let mut ring = Ring::new(0);
        for i in 0..RING_TRBS {
            ring.place(
                &mut hw,
                0x1000 + u64::from(i),
                8,
                trb_ctrl(0, TRB_NORMAL, TRB_IOC),
            );
        }
        let link = read_trb(&mut hw, 0, RING_TRBS - 1);
        assert_eq!(trb_type(get_u32(&link, 12)), TRB_LINK);
        assert_ne!(get_u32(&link, 12) & TRB_TC, 0);
        assert_eq!(ring.enq, 1);
        assert_eq!(ring.cycle, 0);
        assert_eq!(USBCMD_INTE, 1 << 2);
    }

    #[test]
    fn iron_lewisburg_enum_portsc_polling_is_not_link_ready() {
        // Iron Cap EFI COM2 `50b5d8bb` / `67db8a68`: p10/p11/p14 PORTSC=0x000206e1
        // (CCS=1, PED=0, PLS=Polling, Speed=FS). Address Device with that
        // snapshot is xHCI Parameter Error (`cmpl=0x11`). Not persist OK.
        const IRON_POLL: u32 = 0x0002_06e1;
        assert_eq!(portsc_pls(IRON_POLL), 7);
        assert_eq!(portsc_speed(IRON_POLL), 1);
        assert!(IRON_POLL & PORTSC_CCS != 0);
        assert_eq!(IRON_POLL & PORTSC_PED, 0);
        assert!(portsc_pls_training(portsc_pls(IRON_POLL)));
        assert!(!portsc_link_ready(IRON_POLL));
        assert!(port_reset_use_wpr(false, IRON_POLL));
        assert!(port_reset_use_wpr(true, IRON_POLL));
        assert!(slot_ctx_dw0(portsc_speed(IRON_POLL), 1).is_some());
        assert!(slot_ctx_dw0(0, 1).is_none());
        assert_eq!(xhci_scan_ports(26), 26);
        assert_eq!(xhci_config_slots(64), 64);
        assert!(XHCI_PORT_SCAN_MAX > 16);

        let snap = XhciCapSnap {
            cap0: 0x0100_0080,
            hcs1: 0x1a00_0840,
            hcs2: 0x1420_0054,
            hcc1: 1 << 2,
        };
        assert_eq!(snap.caplen(), 0x80);
        assert_eq!(snap.max_slots(), 64);
        assert_eq!(snap.max_ports(), 26);
        assert_eq!(snap.scratch(), 34);
        assert!(!snap.over_budget());
        assert!(snap.csz());
        assert_eq!(xhci_scan_ports(snap.max_ports()), 26);
        assert_eq!(xhci_config_slots(snap.max_slots()), 64);
        assert_eq!(
            xhci_enum_diag_cmpl(CMPL_PARAMETER, XHCI_ENUM_CMD_ADDR, 1, true) as u8,
            CMPL_PARAMETER
        );
        assert_eq!(xhci_enum_diag_portsc(IRON_POLL, 10) >> 32, 10);
        // Iron CSW `68e16633`: fail `portsc=0x0000000e00000e03` is packed
        // port **14** + PORTSC `0xe03`, not "LUN stuck on p10".
        const IRON_HS_U0: u32 = 0x0000_0e03;
        assert_eq!(xhci_enum_diag_portsc(IRON_HS_U0, 14), 0x0000_000e_0000_0e03);
        assert_eq!(xhci_enum_diag_portsc(IRON_HS_U0, 10), 0x0000_000a_0000_0e03);
        assert!(port_must_reset(IRON_POLL));
        assert!(port_must_reset(IRON_HS_U0));
        assert!(!port_must_reset(0));

        // Enum EFI after WPR: HS U0 PED=1. Address Device still Parameter Error
        // because DW1 Number of Ports was 10/11/14.
        assert!(portsc_link_ready(IRON_HS_U0));
        assert_eq!(portsc_speed(IRON_HS_U0), 3);
        assert_eq!(portsc_pls(IRON_HS_U0), 0);
        assert_eq!(slot_ctx_dw1_num_ports(slot_ctx_dw1_port(14)), 0);
        assert_eq!(XHCI_ENUM_CMD_ADDR, 3);
        assert_eq!(XHCI_ENUM_CMD_CFG, 5);
        assert_eq!(XHCI_ENUM_CMD_SETCFG, 6);
        assert_eq!(bulk_ep_dci(2, false), 4);
        assert_eq!(bulk_ep_dci(1, true), 3);
        assert_eq!(ep_ctx_dw1(2, 512) >> 16, 512);
        assert_eq!(ep_ctx_dw4_avg_trb(512), 512);
        assert_eq!(xhci_reset_diag_cmpl(64, 26, 0x80, 14), 0x0e80_1a40);
        assert_eq!(CMPL_PARAMETER, 17);
        assert_eq!(CMPL_TIMEOUT, 0xFF);
        assert_eq!(ADDR_SPINS > SPINS, true);
        assert_eq!(BULK_SPINS > ADDR_SPINS, true);
        assert_eq!(TRB_ISP, 1 << 2);
        assert_eq!(bulk_in_trb_flags(CSW_LEN), TRB_IOC | TRB_ISP);
        assert_eq!(bulk_in_trb_flags(512), TRB_IOC);
        assert_eq!(usb_bot_stage_name(2), "data");
        assert!(usb_bot_last_stage() <= 3);
        // Iron Slot DW1 EFI `3473a0b9`: stamp_enum printed MaxSlots as cmpl.
        let stale = xhci_reset_diag_cmpl(64, 26, 0x80, 10);
        assert_eq!(stale as u8, 0x40);
        assert_ne!(stale as u8, CMPL_TIMEOUT);
        assert_eq!(crcr_abort_bits(0) & CRCR_CA, CRCR_CA);
        assert!(crcr_is_running(CRCR_CRR));
        assert_eq!(crcr_restart(0x1000) & CRCR_RCS, CRCR_RCS);
        assert_eq!(USB_MSC_BOT, 0x50);
        assert_eq!(USB_MSC_UAS, 0x62);
        assert_eq!(xhci_event_err(TRB_EVENT_TRANSFER), UsbBotError::Xfer);
        assert_eq!(xhci_event_err(TRB_ENABLE_SLOT), UsbBotError::Enum);
    }

    #[test]
    fn usb_hub_skip_does_not_hide_desc_fail() {
        // Iron `06ca0f95`: p11 GET_DESC `cmd=4 cmpl=0`; p14 hub `1604:10c0`
        // class 09 became `err=4` and leftover DRAM Everest ran. ISO isolation
        // is proven (`last_st=0x0`, `ISO-INSTALL-OK` leftover). Not persist.
        assert!(usb_dev_is_hub(USB_CLASS_HUB));
        assert!(!usb_dev_is_hub(0));
        assert_eq!(USB_CLASS_HUB, 9);
        assert_eq!(XHCI_DESC_TRIES, 3);
        assert_eq!(TRB_RESET_EP, 14);
        assert_eq!(TRB_SET_TR_DEQ, 16);
        assert_eq!(UsbBotError::Hub as u8, 9);
        assert_eq!(xhci_ep_cmd_extra(3, 1), 3u32 << 24 | 1u32 << 16);
        assert_eq!(
            xhci_bring_up_keep_err(UsbBotError::Xfer, UsbBotError::Hub),
            UsbBotError::Xfer
        );
        assert_eq!(
            xhci_bring_up_keep_err(UsbBotError::Reset, UsbBotError::Hub),
            UsbBotError::Hub
        );
        assert_eq!(
            xhci_bring_up_keep_err(UsbBotError::Cruzer, UsbBotError::Hub),
            UsbBotError::Cruzer
        );
        assert_eq!(
            xhci_bring_up_keep_err(UsbBotError::Reset, UsbBotError::Xfer),
            UsbBotError::Xfer
        );
        store_usb_bot_diag(UsbBotError::Xfer, 0x92b0_0000, 0x0000_000b_0000_0e03, 0xff);
        assert!(usb_bot_keep_xfer_diag(usb_bot_last_err()));
        assert!(usb_bot_keep_xfer_diag(UsbBotError::Enum as u8));
        // Hub skip must not stamp — even when LAST_ERR is Reset, not Xfer.
        // Iron `68e16633`: p14 hub wrote `cmpl=0` + packed port 14 over p11 BOT.
        assert_eq!(usb_bot_last_err(), UsbBotError::Xfer as u8);
        assert_eq!(usb_bot_last_cmpl(), 0xff);
        assert_eq!(usb_bot_last_portsc(), 0x0000_000b_0000_0e03);
        let mut hub_cfg = [0u8; 18];
        hub_cfg[0] = 9;
        hub_cfg[1] = 2;
        hub_cfg[2] = 18;
        hub_cfg[9] = 9;
        hub_cfg[10] = 4;
        hub_cfg[14] = USB_CLASS_HUB;
        assert_eq!(parse_bot_eps(&hub_cfg), None);
        assert_eq!(cfg_msc_protos(&hub_cfg), (false, false));
    }
}
