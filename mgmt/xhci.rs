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
    csw_ok, next_bot_tag, restore_usb_bot_diag, stamp_scsi_cdb, store_usb_bot_diag,
    store_usb_bot_diag_unless_kept, store_usb_bot_ready, store_usb_bot_stage, usb_bot_bring_up,
    usb_bot_last_bar, usb_bot_last_cmpl, usb_bot_last_err, usb_bot_last_portsc, Cbw, UsbBotError,
    UsbBulk, BOT_STAGE_CBW, BOT_STAGE_CSW, BOT_STAGE_DATA, CBW_LEN, CSW_LEN, SCSI_READ_CAPACITY_10,
};

#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
use super::usb_bot::{
    usb_bot_last_scsi, usb_bot_last_stage, usb_bot_scsi_name, usb_bot_stage_name,
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
/// No-Op Command (xHCI 6.4.6 type 23). Transfer No-Op is type 8.
/// Iron p11first COM2: first Enable Slot after HCRST never posted
/// (`cmd=2 cmpl=0xff`, abort crr=0) then p14 hub enumerated. Prime the
/// command ring so Toshiba is not the lost first doorbell.
pub const TRB_NO_OP_CMD: u32 = 23;
pub const TRB_ENABLE_SLOT: u32 = 9;
pub const TRB_DISABLE_SLOT: u32 = 10;
pub const TRB_ADDRESS_DEV: u32 = 11;
pub const TRB_CONFIG_EP: u32 = 12;
/// Evaluate Context (xHCI 4.6.7 / 6.4.6 type 13). After GET_DEVICE,
/// software must update EP0 Max Packet Size from `bMaxPacketSize0`.
/// Address Device leaves the USB2 default; GET_CONFIG without this is
/// the ep0-stop F11 (`cmd=4` after Toshiba named). Not Configure Endpoint.
pub const TRB_EVALUATE_CTX: u32 = 13;
pub const TRB_RESET_EP: u32 = 14;
/// Stop Endpoint (xHCI 4.6.9). Timeout leaves EP Running; Reset Endpoint
/// is Halted-only and returns Context State Error (`cmpl=0x13`).
pub const TRB_STOP_EP: u32 = 15;
pub const TRB_SET_TR_DEQ: u32 = 16;
/// Reset Device (xHCI 4.6.11 / 6.4.6 type 17). Port reset does not
/// change slot state. Address Device on an already-Addressed slot is
/// Context State Error (`cmpl=0x13`). Keep the slot id.
pub const TRB_RESET_DEV: u32 = 17;
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
/// Chain (xHCI 6.4.6 bit 4). Iron cfg-desc (`b5338458`): CH on SETUP+DATA
/// never posted GET_DEVICE on Lewisburg (no `vid=`). Control TDs stay
/// unchained; the bit remains for tests / bulk LINK.
pub const TRB_CH: u32 = 1 << 4;

pub const CMPL_SUCCESS: u8 = 1;
pub const CMPL_SHORT: u8 = 13;
/// Context State Error (xHCI Table 6-91). Iron first-cbw: Reset Endpoint
/// on a Running bulk EP after CAPACITY CBW timeout stamped `cmpl=0x13`.
pub const CMPL_CONTEXT_STATE: u8 = 19;

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
/// Evaluate Context EP0 MPS. COM2 `xhci eval` — not packed `cmd=4` (GET_DESC).
pub const XHCI_ENUM_CMD_EVAL: u8 = 7;
/// Reset Device before re-Address. COM2 `xhci rstdev` — not packed `cmd=3`.
pub const XHCI_ENUM_CMD_RSTDEV: u8 = 8;

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
/// Iron p11first COM2: first Enable Slot after HCRST used `SPINS` and
/// printed `cmd=2 cmpl=0xff` (doorbell lost; abort crr=0). Wait
/// ADDR_SPINS and retry Enable Slot on the same port.
pub const ADDR_SPINS: u32 = 50_000_000;
/// GET_DESC / SET_CONFIG on EP0. Iron ep0-stop COM2: Toshiba device
/// descriptor lived (18-byte exact) then 9-byte config GET_DESC
/// `cmd=4 cmpl=0xff` `bot=? scsi=?`. HS MPS=64 + wLength=9 babbles if
/// the device returns wTotalLength; Intel may skip STATUS after a short
/// DATA stage. Unchained SETUP/DATA/STATUS; complete on DATA immediately
/// then Stop leftover STATUS. Do not match leftover SETUP.
pub const DESC_SPINS: u32 = ADDR_SPINS;
/// Default control endpoint DCI.
pub const XHCI_EP0_DCI: u8 = 1;
/// Bulk Transfer Event wait. Iron `6c278e85` (DESC retry): Toshiba p11
/// `0480:a004` INQUIRY/TUR/CAPACITY (≤36 B) succeeded (`lba=512`) then the
/// first 512-byte READ timed out (`err=8` `cmpl=0xff` at `off=0x200`).
/// Mechanical USB HDD first READ after CAPACITY can be seconds.
pub const BULK_SPINS: u32 = 100_000_000;
/// First bulk CBW after GET_MAX_LUN (INQUIRY, then CAPACITY, then READ).
/// Iron epst COM2: INQUIRY/CAPACITY lived, READ CBW `cmpl=0xff`.
/// Iron firstread COM2: `epst`/`botrst`/`maxlun` then INQUIRY CBW `cmpl=0xff`.
/// Iron firstcbw COM2: `xhci firstcbw` then INQUIRY still `cmpl=0xff` —
/// Stop+rearm before the unused first CBW did not retire a Transfer Event.
/// Toshiba spinning HDD can NAK the first bulk CBW for seconds.
pub const FIRST_READ_SPINS: u32 = 800_000_000;

/// MSC BOT / UAS interface protocol (USB Mass Storage).
pub const USB_MSC_BOT: u8 = 0x50;
pub const USB_MSC_UAS: u8 = 0x62;
/// USB Hub bDeviceClass. Iron `06ca0f95` p14 `1604:10c0` class 09 proto 01.
pub const USB_CLASS_HUB: u8 = 0x09;
/// GET_DESC retries after Address Device. Iron `06ca0f95` p11 `cmd=4 cmpl=0`
/// (Toshiba `0480:a004` on p11 the prior recover flash). Not persist OK.
pub const XHCI_DESC_TRIES: u8 = 3;
/// One HS EP0 packet. Iron ep0-stop: 9-byte GET_CONFIG `cmd=4` (babble).
/// Cfg-desc 256 never ran — CH broke GET_DEVICE first.
pub const USB_CFG_DESC_MPS: u16 = 64;
/// Second GET_CONFIGURATION if `wTotalLength` exceeds one EP0 packet.
pub const USB_CFG_DESC_MAX: u16 = 256;
/// Iron `96024edc` / recover: Toshiba USB HDD BOT on p11.
pub const USB_VID_TOSHIBA: u16 = 0x0480;
pub const USB_DID_TOSHIBA_LUN: u16 = 0xa004;

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

/// Command tag from a packed Enum `cmpl=` (bits 15:8).
pub fn xhci_enum_diag_cmd(packed: u64) -> u8 {
    ((packed >> 8) & 0xFF) as u8
}

/// Command Completion Code 0 is Invalid (xHCI Table 6-91). Iron capoverlap
/// COM2: SET_CONFIG lived then CONFIG_EP `cmd=5 cmpl=0` `err=3 bot=?`.
/// Do not treat that leftover as the Configure Endpoint result.
pub fn cmd_cc_invalid(code: u8) -> bool {
    code == 0
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

/// Command Ring Stopped / Command Aborted after CRCR.CA (xHCI Table 6-91).
pub fn cmd_cc_ring_stopped(code: u8) -> bool {
    code == CMPL_CMD_STOPPED || code == CMPL_CMD_ABORTED
}

/// Iron cmdptr COM2: p10 Address Device `cmd=3 cmpl=0xff` then p11 Enable
/// Slot `cmd=2 cmpl=0xff`. Toshiba is p11; p10 has never named a device.
/// Walk 11..=max then 1..=10 so SET_ADDRESS on the stuck port cannot
/// wedge CRR before Toshiba.
pub const XHCI_ENUM_TOSHIBA_PORT: u8 = 11;

/// Port at `idx` (0-based) in Toshiba-first enum order.
pub fn xhci_enum_port_at(max_ports: u8, idx: u8) -> Option<u8> {
    if max_ports == 0 || idx >= max_ports {
        return None;
    }
    let first = XHCI_ENUM_TOSHIBA_PORT;
    if max_ports < first {
        return Some(idx + 1);
    }
    let tail = first - 1;
    let head = max_ports - tail;
    if idx < head {
        Some(first + idx)
    } else {
        Some(idx - head + 1)
    }
}

/// Slot ID[31:24] + Endpoint ID[20:16] for Stop/Reset Endpoint / Set TR Dequeue.
pub fn xhci_ep_cmd_extra(slot: u8, dci: u8) -> u32 {
    u32::from(slot) << 24 | u32::from(dci) << 16
}

/// Reset Endpoint only when Stop Endpoint did not succeed (Halted / already
/// Stopped). Iron first-cbw `cmpl=0x13`: Reset on Running after CBW timeout.
pub fn xhci_ep_recover_need_reset(stop_ok: bool) -> bool {
    !stop_ok
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

/// Transfer Event Slot ID (xHCI 6.4.2.1 DW3[31:24]).
pub fn xhci_event_slot(ctrl: u32) -> u8 {
    (ctrl >> 24) as u8
}

/// Transfer Event Endpoint ID / DCI (xHCI 6.4.2.1 DW3[20:16]).
pub fn xhci_event_dci(ctrl: u32) -> u8 {
    ((ctrl >> 16) & 0x1F) as u8
}

/// Transfer Event TRB Pointer (xHCI 6.4.2.1 DW0-1, bits 63:4).
/// Iron `cbd9bf47`: leftover EP0 SETUP/DATA must not retire GET_DESC STATUS;
/// leftover CSW IN (same DCI as a later READ IN) must not retire that IN.
pub fn xhci_event_trb_ptr(ev: &[u8; 16]) -> u64 {
    get_u64(ev, 0) & !0xF
}

/// Command Completion Event matches the command TRB we doorbell'd
/// (xHCI 6.4.2.2 Command TRB Pointer). Iron capoverlap COM2: p11 Toshiba
/// `0480:a004` was named, then leftover Invalid (CC=0) from p10 Address
/// Device abort retired CONFIG_EP (`cmd=5 cmpl=0` `bot=?`). recover_enum
/// Disable Slot'd the Toshiba slot and the mapper printed need-media —
/// looks like "no device". Do not match any command event.
///
/// Iron p11first COM2: first Enable Slot after HCRST never posted. Some
/// Lewisburg completions arrive with Command TRB Pointer 0 on Success.
/// Accept ptr=0 only on Success so leftover Invalid (CC=0) still cannot
/// retire CONFIG_EP.
pub fn xhci_cmd_event_matches(ev: &[u8; 16], want_ptr: u64) -> bool {
    if want_ptr == 0 {
        return false;
    }
    let p = xhci_event_trb_ptr(ev);
    if p == (want_ptr & !0xF) {
        return true;
    }
    p == 0 && trb_cmpl_code(get_u32(ev, 8)) == CMPL_SUCCESS
}

/// Iron p11first COM2: first Enable Slot timed out (`cmd=2 cmpl=0xff`)
/// then abort crr=0 and p14 hub enumerated. Retry Enable Slot on the
/// same CCS port after abort; do not walk to the hub.
pub fn xhci_retry_enable_slot(first_posted: bool) -> bool {
    !first_posted
}

/// Iron slotretry COM2: No-Op + Enable Slot + Address Device lived on p11
/// (`xhci nop`, no `cmd=2`). GET_DESC retried n=1,n=2 then `cmd=4 cmpl=0xff`
/// with no Transfer Event — EP0 drain, **not** Evaluate Context (`cmd=7`).
/// abort crr=0; walking to p14 threw away the live slot. Keep DCBAA; do not
/// Disable Slot; do not start p14/p10 after a DESC miss.
pub fn xhci_desc_fail_stops_walk(packed_cmpl: u64) -> bool {
    xhci_enum_diag_cmd(packed_cmpl) == XHCI_ENUM_CMD_DESC
}

/// Iron rstdev/udiskkick COM2 (`9f251ff6`): p11 Toshiba `0480:a004` named,
/// eval/setcfg/CONFIG_EP Running, `firstcbw overlap`, `usb rw wait` then
/// recover_enum zeroed DCBAA and the walk named p14 hub `1604:10c0` + p10
/// ADDR `cmd=3 cmpl=0xff`. Packed fail `cmpl=0x303ff` is p10, not the p11
/// `bot=csw scsi=inquiry`. Guestio (`f2c55be4`) reached `usb I/O ready` on
/// this named path — walking after a named MSC is a regression.
/// Keep DCBAA; do not Disable Slot; do not start p14/p10. Cruzer still walks.
pub fn xhci_named_msc_fail_stops_walk(e: UsbBotError, named: bool) -> bool {
    named
        && !matches!(
            e,
            UsbBotError::Cruzer | UsbBotError::Hub | UsbBotError::Reset | UsbBotError::Cap
        )
}

/// Keep the live slot after a named MSC BOT/enum fail. Zeroing DCBAA /
/// Disable Slot then walking p14 is the rstdev COM2 regression.
pub fn xhci_keep_slot_after_named_msc() -> bool {
    true
}

static MSC_NAMED: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);

/// Record that GET_DESC named a non-hub MSC on the current CCS port.
pub fn xhci_note_msc_named(named: bool) {
    MSC_NAMED.store(named, core::sync::atomic::Ordering::SeqCst);
}

/// True after [`xhci_note_msc_named`] on this port (cleared at try_port).
pub fn xhci_msc_was_named() -> bool {
    MSC_NAMED.load(core::sync::atomic::Ordering::SeqCst)
}

/// Packed `cmd=4` + `cmpl=0xff` is GET_DESC with no Transfer Event, not Eval.
pub fn xhci_desc_timeout_is_ep0_xfer(cmd: u8, cmpl: u8) -> bool {
    cmd == XHCI_ENUM_CMD_DESC && cmpl == CMPL_TIMEOUT
}

/// Keep the Enable Slot id after GET_DESC timeout. Zeroing DCBAA / Disable
/// Slot throws away Slotretry's win.
pub fn xhci_keep_slot_after_desc() -> bool {
    true
}

/// Iron stallquiet COM2: descabort keep-slot then port-reset + Address
/// Device printed `cmd=3 cmpl=0x13` (`err=3`). Slot was already Addressed
/// from the first Address Device; USB reset does not drop xHC slot state.
/// Reset Device (type 17) returns Default, then Address Device is legal.
/// Do not Disable Slot. Do not walk p14.
pub fn xhci_addr_context_state_needs_reset_device(cmd: u8, cmpl: u8) -> bool {
    cmd == XHCI_ENUM_CMD_ADDR && cmpl == CMPL_CONTEXT_STATE
}

/// Iron descabort COM2 (`b0c2b678`): bring-up 512-byte READ lived
/// (`FIRST_READ_SPINS`) then Alpine `vda` 298 GiB mixed `last_st=0x0`
/// with seek/4K `last_st=0x1`; `sfdisk` I/O error. Guest live BOT must
/// keep the long wait — `end_first_read` must not drop to `BULK_SPINS`.
/// Iron guestio COM2 (`f2c55be4`): three `vda` completions then leftover
/// apk stall dump during ISO mount — not a BOT timeout.
/// Iron stopwalk COM2 (`90af2c5b`): FIRST_READ_SPINS lived; 4K/`nlb=8`
/// still `last_st=0x1`. Guest chunk is one native LBA (`usb_bot_guest_chunk`).
pub fn xhci_guest_rw_long_wait() -> bool {
    true
}

/// Rate-limit `lun rw fail` so COM2 names CBW/DATA/CSW without a flood.
pub fn xhci_rw_fail_should_print(n: u32) -> bool {
    n < 8 || n % 64 == 0
}

/// SETUP TRB flags: Immediate Data + TRT (IN=3, no-data=0). No Chain —
/// iron cfg-desc CH on SETUP+DATA timed out GET_DEVICE on Lewisburg.
pub fn control_setup_flags(data_in: bool) -> u32 {
    let trt = if data_in { 3u32 << 16 } else { 0 };
    trb_ctrl(0, TRB_SETUP, TRB_IDT) | trt
}

/// DATA IN TRB: IOC + ISP + DIR=IN. No Chain. Short packet vs HS MPS=64
/// posts; leftover STATUS is Stopped by the caller.
pub fn control_data_in_flags() -> u32 {
    trb_ctrl(0, TRB_DATA, TRB_IOC | TRB_ISP | (1 << 16))
}

/// STATUS OUT (control IN data stage): IOC, DIR=OUT.
pub fn control_status_out_flags() -> u32 {
    trb_ctrl(0, TRB_STATUS, TRB_IOC)
}

/// True when the Transfer Event completes the control TD (DATA or STATUS).
/// Leftover SETUP must not match (`cbd9bf47`).
pub fn xhci_control_event_matches(ev: &[u8; 16], data_ptr: u64, status_ptr: u64) -> bool {
    let p = xhci_event_trb_ptr(ev);
    p == (data_ptr & !0xF) || p == (status_ptr & !0xF)
}

/// Configuration descriptor `wTotalLength` as the device advertised.
pub fn usb_cfg_w_total_raw(cfg: &[u8]) -> u16 {
    if cfg.len() < 4 || cfg[1] != 2 {
        return 0;
    }
    u16::from_le_bytes([cfg[2], cfg[3]])
}

/// Configuration descriptor wTotalLength, capped to the buffer, or 0.
pub fn usb_cfg_w_total(cfg: &[u8]) -> u16 {
    usb_cfg_w_total_raw(cfg).min(cfg.len() as u16)
}

/// Add Context flags A0+A1 for Evaluate Context EP0 MPS (xHCI 4.6.7).
pub fn evaluate_ep0_add_flags() -> u32 {
    0x3
}

/// Add Context flags A0+A1+bulk DCIs for Configure Endpoint (xHCI 4.6.6).
/// Iron maxlun COM2: A0+A3+A4 only (no A1, reconstructed Slot) then INQUIRY
/// CBW `cmpl=0xff`. Copy Output Slot/EP0 into Input and keep EP0 at the
/// live dequeue so Evaluate Context's rewind to `mem.ep0|1` does not leave
/// HW chewing completed GET_DEVICE TRBs while bulk OUT is doorbell'd.
pub fn config_ep_add_flags(dci_out: u8, dci_in: u8) -> u32 {
    1 | (1 << 1) | (1u32 << dci_out) | (1u32 << dci_in)
}

/// Output Device Context: Slot at 0, DCI n at cs*n (xHCI 6.1).
pub fn output_ep_ctx_off(cs: usize, dci: u8) -> usize {
    cs.saturating_mul(usize::from(dci))
}

/// Input Context: Control at 0, Slot at cs, DCI n at cs*(1+n) (xHCI 6.2.5).
pub fn input_ep_ctx_off(cs: usize, dci: u8) -> usize {
    cs.saturating_mul(1 + usize::from(dci))
}

/// EP State in EP Context DW0[2:0]. 0 Disabled, 1 Running, 2 Halted, 3 Stopped.
pub fn ep_ctx_state(dw0: u32) -> u8 {
    (dw0 & 7) as u8
}

pub const EP_STATE_DISABLED: u8 = 0;
pub const EP_STATE_RUNNING: u8 = 1;
pub const EP_STATE_HALTED: u8 = 2;
pub const EP_STATE_STOPPED: u8 = 3;

/// Input EP Context EP State is reserved — must be 0 (xHCI 6.2.3.1).
pub fn ep_ctx_input_clear_state(dw0: u32) -> u32 {
    dw0 & !7
}

/// Slot Context Speed[23:20].
pub fn slot_ctx_speed(dw0: u32) -> u8 {
    ((dw0 >> 20) & 0xF) as u8
}

/// Update Context Entries[31:27] without clobbering Route String / Speed.
pub fn slot_ctx_set_entries(dw0: u32, entries: u8) -> u32 {
    (dw0 & !(0x1F << 27)) | (u32::from(entries) << 27)
}

/// Input Slot State[31:27] of DW3 is reserved (xHCI 6.2.2).
pub fn slot_ctx_input_clear_state(dw3: u32) -> u32 {
    dw3 & !(0x1F << 27)
}

/// Context Size: 32 bytes when HCCPARAMS1 CSZ=0, 64 when CSZ=1.
pub fn xhci_ctx_size(csz: bool) -> usize {
    if csz {
        64
    } else {
        32
    }
}

/// Pack Configure Endpoint Input Context from the live Output Device Context.
/// Iron maxlun COM2 reconstructed Slot DW0/DW1 only (USB address / Route
/// String dropped) and skipped A1.
pub fn fill_config_ep_input(
    ic: &mut [u8],
    out: &[u8],
    cs: usize,
    speed: u8,
    port: u8,
    dci_out: u8,
    dci_in: u8,
    ep0_deq: u64,
    bulk_out: u64,
    bulk_in: u64,
    mps_out: u16,
    mps_in: u16,
    ep0_mps: u16,
) {
    if cs == 0 || ic.len() < cs.saturating_mul(6) {
        return;
    }
    let hi = core::cmp::max(dci_out, dci_in);
    put_u32(ic, 4, config_ep_add_flags(dci_out, dci_in));
    if out.len() >= cs {
        ic[cs..cs + cs].copy_from_slice(&out[..cs]);
        let mut dw0 = slot_ctx_set_entries(get_u32(ic, cs), hi);
        if slot_ctx_speed(dw0) == 0 {
            dw0 = slot_ctx_dw0(speed, hi).unwrap_or(dw0);
            put_u32(ic, cs + 4, slot_ctx_dw1_port(port));
        }
        put_u32(ic, cs, dw0);
        put_u32(
            ic,
            cs + 12,
            slot_ctx_input_clear_state(get_u32(ic, cs + 12)),
        );
    } else if let Some(dw0) = slot_ctx_dw0(speed, hi) {
        put_u32(ic, cs, dw0);
        put_u32(ic, cs + 4, slot_ctx_dw1_port(port));
    }
    let ep0_dst = input_ep_ctx_off(cs, 1);
    let ep0_src = output_ep_ctx_off(cs, 1);
    if out.len() >= ep0_src + cs {
        ic[ep0_dst..ep0_dst + cs].copy_from_slice(&out[ep0_src..ep0_src + cs]);
        put_u32(ic, ep0_dst, ep_ctx_input_clear_state(get_u32(ic, ep0_dst)));
    } else {
        put_u32(ic, ep0_dst + 4, ep_ctx_dw1(4, ep0_mps));
    }
    put_u64(ic, ep0_dst + 8, ep0_deq);
    let out_off = input_ep_ctx_off(cs, dci_out);
    put_u32(ic, out_off + 4, ep_ctx_dw1(2, mps_out));
    put_u64(ic, out_off + 8, bulk_out | 1);
    put_u32(ic, out_off + 16, ep_ctx_dw4_avg_trb(mps_out));
    let in_off = input_ep_ctx_off(cs, dci_in);
    put_u32(ic, in_off + 4, ep_ctx_dw1(6, mps_in));
    put_u64(ic, in_off + 8, bulk_in | 1);
    put_u32(ic, in_off + 16, ep_ctx_dw4_avg_trb(mps_in));
}

/// EP0 Max Packet Size from the device descriptor, with a speed fallback.
pub fn usb_ep0_mps_from_desc(speed: u8, b_mps0: u8) -> u16 {
    match speed {
        4 | 5 => ep0_max_packet(speed),
        _ => match b_mps0 {
            8 | 16 | 32 | 64 => u16::from(b_mps0),
            _ => ep0_max_packet(speed),
        },
    }
}

/// Iron `96024edc`: Toshiba `0480:a004` BOT `ep_out=2 ep_in=1 cfg=1` mps=512.
/// GET_CONFIG skip only for this vid/did — do not guess other devices.
pub fn toshiba_bot_eps(vid: u16, did: u16) -> Option<(u8, u8, u16, u16, u8)> {
    if vid == USB_VID_TOSHIBA && did == USB_DID_TOSHIBA_LUN {
        Some((2, 1, 512, 512, 1))
    } else {
        None
    }
}

/// True when a Transfer Event belongs to this slot + bulk DCI.
/// Iron `96024edc`: READ CBW `cmpl=0` after CAPACITY CSW — a leftover bulk-IN
/// event (Toshiba `ep_in=1` → DCI 3) must not retire the CBW OUT (DCI 4).
pub fn xhci_xfer_matches(ctrl: u32, slot: u8, dci: u8) -> bool {
    xhci_event_slot(ctrl) == slot && xhci_event_dci(ctrl) == dci
}

/// Pack bulk Transfer Event `cmpl=` as CC | (dci << 8) | (slot << 16).
pub fn xhci_xfer_diag_cmpl(code: u8, dci: u8, slot: u8) -> u64 {
    u64::from(code) | (u64::from(dci) << 8) | (u64::from(slot) << 16)
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

fn get_u64(b: &[u8], off: usize) -> u64 {
    u64::from_le_bytes(b[off..off + 8].try_into().unwrap_or([0; 8]))
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
    xhci_ctx_size(csz)
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

    /// Next TRB the xHC should process (HPA | DCS). Used as EP0 dequeue in
    /// Configure Endpoint A1 so Evaluate Context cannot rewind to TRB 0.
    pub fn tr_dequeue(&self) -> u64 {
        self.base + u64::from(self.enq) * 16 | u64::from(self.cycle & 1)
    }

    fn place(&mut self, hw: &mut impl XhciHw, ptr: u64, status: u32, ctrl: u32) -> u64 {
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
        let hpa = self.base + u64::from(self.enq) * 16;
        write_trb(
            hw,
            self.base,
            self.enq,
            ptr,
            status,
            (ctrl & !1) | (self.cycle & 1),
        );
        self.enq = self.enq.saturating_add(1);
        hpa
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
    if cmd == XHCI_ENUM_CMD_DESC && cmpl == CMPL_TIMEOUT {
        serial::write_line(
            " ep0 timeout (not Evaluate Context; leftover DRAM; not ISO-INSTALL-OK)",
        );
        return;
    }
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
fn serial_xhci_config(port: u8) {
    use crate::boot::serial;
    serial::write_str("boot: Stage 46 xhci config p");
    serial_dec_u8(port);
    serial::write_line(" (not ISO-INSTALL-OK)");
}

#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
fn serial_xhci_cfgretry(port: u8, n: u8) {
    use crate::boot::serial;
    serial::write_str("boot: Stage 46 xhci cfgretry p");
    serial_dec_u8(port);
    serial::write_str(" n=");
    serial_dec_u8(n);
    serial::write_line(" (not ISO-INSTALL-OK)");
}

#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
fn serial_xhci_cmdptr(port: u8) {
    use crate::boot::serial;
    serial::write_str("boot: Stage 46 xhci cmdptr p");
    serial_dec_u8(port);
    serial::write_line(" (not ISO-INSTALL-OK)");
}

#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
fn serial_xhci_toshiba_named(port: u8) {
    use crate::boot::serial;
    serial::write_str("boot: Stage 46 xhci toshiba p");
    serial_dec_u8(port);
    serial::write_line(" named; CONFIG_EP leftover — not I/O ready (not ISO-INSTALL-OK)");
}

#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
fn serial_xhci_p11first() {
    use crate::boot::serial;
    serial::write_line("boot: Stage 46 xhci p11first (not ISO-INSTALL-OK)");
}

#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
fn serial_xhci_nop() {
    use crate::boot::serial;
    serial::write_line("boot: Stage 46 xhci nop (not ISO-INSTALL-OK)");
}

#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
fn serial_xhci_slotretry(port: u8) {
    use crate::boot::serial;
    serial::write_str("boot: Stage 46 xhci slotretry p");
    serial_dec_u8(port);
    serial::write_line(" (not ISO-INSTALL-OK)");
}

#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
fn serial_xhci_descabort(port: u8) {
    use crate::boot::serial;
    serial::write_str("boot: Stage 46 xhci descabort p");
    serial_dec_u8(port);
    serial::write_line(" keep-slot (not Disable Slot; not ISO-INSTALL-OK)");
}

#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
fn serial_xhci_rstdev(port: u8) {
    use crate::boot::serial;
    serial::write_str("boot: Stage 46 xhci rstdev p");
    serial_dec_u8(port);
    serial::write_line(" (Reset Device; not Disable Slot; not ISO-INSTALL-OK)");
}

#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
fn serial_xhci_stopwalk(port: u8) {
    use crate::boot::serial;
    serial::write_str("boot: Stage 46 xhci stopwalk p");
    serial_dec_u8(port);
    serial::write_line(" keep-slot (named MSC; not p14; not ISO-INSTALL-OK)");
}

#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
fn serial_xhci_abort(crr: u8) {
    use crate::boot::serial;
    serial::write_str("boot: Stage 46 xhci abort crr=");
    serial_dec_u8(crr);
    serial::write_line(" (not ISO-INSTALL-OK)");
}

#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
fn serial_xhci_eval(port: u8, mps: u16, ok: bool) {
    use crate::boot::serial;
    serial::write_str("boot: Stage 46 xhci eval p");
    serial_dec_u8(port);
    serial::write_str(" mps=");
    serial_dec_u32(u32::from(mps));
    if ok {
        serial::write_line(" (not ISO-INSTALL-OK)");
    } else {
        serial::write_line(" fail (leftover DRAM; not ISO-INSTALL-OK)");
    }
}

#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
fn serial_xhci_cfg_skip(port: u8, vid: u16, did: u16) {
    use crate::boot::serial;
    serial::write_str("boot: Stage 46 xhci cfg-skip p");
    serial_dec_u8(port);
    serial::write_str(" toshiba 0x");
    serial_hex32(u32::from(vid));
    serial::write_str(":0x");
    serial_hex32(u32::from(did));
    serial::write_line(" (not ISO-INSTALL-OK)");
}

/// Iron norearm COM2: GET_MAX_LUN `val=0` then `epst ep0=3` before first CBW.
/// Skip the optional class IN so COM2 prints `xhci maxlun pN skip`.
#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
fn serial_xhci_maxlun(port: u8) {
    use crate::boot::serial;
    serial::write_str("boot: Stage 46 xhci maxlun p");
    serial_dec_u8(port);
    serial::write_line(" skip (not ISO-INSTALL-OK)");
}

#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
fn serial_xhci_epst(port: u8, ep0: u8, bulk_out: u8, bulk_in: u8) {
    use crate::boot::serial;
    serial::write_str("boot: Stage 46 xhci epst p");
    serial_dec_u8(port);
    serial::write_str(" ep0=");
    serial_dec_u8(ep0);
    serial::write_str(" out=");
    serial_dec_u8(bulk_out);
    serial::write_str(" in=");
    serial_dec_u8(bulk_in);
    serial::write_line(" (not ISO-INSTALL-OK)");
}

#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
fn serial_xhci_botrst(port: u8, ok: bool) {
    use crate::boot::serial;
    serial::write_str("boot: Stage 46 xhci botrst p");
    serial_dec_u8(port);
    if ok {
        serial::write_line(" (not ISO-INSTALL-OK)");
    } else {
        serial::write_line(" fail (continue; leftover DRAM; not ISO-INSTALL-OK)");
    }
}

#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
fn serial_xhci_firstcbw(port: u8) {
    use crate::boot::serial;
    serial::write_str("boot: Stage 46 xhci firstcbw p");
    serial_dec_u8(port);
    serial::write_line(" overlap (not ISO-INSTALL-OK)");
}

#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
fn serial_xhci_firstread(port: u8) {
    use crate::boot::serial;
    serial::write_str("boot: Stage 46 xhci firstread p");
    serial_dec_u8(port);
    serial::write_line(" overlap (not ISO-INSTALL-OK)");
}

#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
fn serial_xhci_capoverlap(port: u8) {
    use crate::boot::serial;
    serial::write_str("boot: Stage 46 xhci capoverlap p");
    serial_dec_u8(port);
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
fn serial_hex64(v: u64) {
    serial_hex32((v >> 32) as u32);
    serial_hex32(v as u32);
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
fn serial_xhci_config(_port: u8) {}

#[cfg(not(all(target_os = "uefi", feature = "uefi-bin")))]
fn serial_xhci_cfgretry(_port: u8, _n: u8) {}

#[cfg(not(all(target_os = "uefi", feature = "uefi-bin")))]
fn serial_xhci_cmdptr(_port: u8) {}

#[cfg(not(all(target_os = "uefi", feature = "uefi-bin")))]
fn serial_xhci_toshiba_named(_port: u8) {}

#[cfg(not(all(target_os = "uefi", feature = "uefi-bin")))]
fn serial_xhci_p11first() {}

#[cfg(not(all(target_os = "uefi", feature = "uefi-bin")))]
fn serial_xhci_nop() {}

#[cfg(not(all(target_os = "uefi", feature = "uefi-bin")))]
fn serial_xhci_slotretry(_port: u8) {}

#[cfg(not(all(target_os = "uefi", feature = "uefi-bin")))]
fn serial_xhci_descabort(_port: u8) {}

#[cfg(not(all(target_os = "uefi", feature = "uefi-bin")))]
fn serial_xhci_rstdev(_port: u8) {}

#[cfg(not(all(target_os = "uefi", feature = "uefi-bin")))]
fn serial_xhci_stopwalk(_port: u8) {}

#[cfg(not(all(target_os = "uefi", feature = "uefi-bin")))]
fn serial_xhci_abort(_crr: u8) {}

#[cfg(not(all(target_os = "uefi", feature = "uefi-bin")))]
fn serial_xhci_desc_retry(_port: u8, _n: u8) {}

#[cfg(not(all(target_os = "uefi", feature = "uefi-bin")))]
fn serial_xhci_maxlun(_port: u8) {}

#[cfg(not(all(target_os = "uefi", feature = "uefi-bin")))]
fn serial_xhci_epst(_port: u8, _ep0: u8, _bulk_out: u8, _bulk_in: u8) {}

#[cfg(not(all(target_os = "uefi", feature = "uefi-bin")))]
fn serial_xhci_botrst(_port: u8, _ok: bool) {}

#[cfg(not(all(target_os = "uefi", feature = "uefi-bin")))]
fn serial_xhci_firstcbw(_port: u8) {}

#[cfg(not(all(target_os = "uefi", feature = "uefi-bin")))]
fn serial_xhci_firstread(_port: u8) {}

#[cfg(not(all(target_os = "uefi", feature = "uefi-bin")))]
fn serial_xhci_capoverlap(_port: u8) {}

#[cfg(not(all(target_os = "uefi", feature = "uefi-bin")))]
fn serial_xhci_eval(_port: u8, _mps: u16, _ok: bool) {}

#[cfg(not(all(target_os = "uefi", feature = "uefi-bin")))]
fn serial_xhci_cfg_skip(_port: u8, _vid: u16, _did: u16) {}

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

fn consume_transfer(
    hw: &mut impl XhciHw,
    caps: &XhciCaps,
    ev: &mut EventRing,
    slot: u8,
    dci: u8,
    want_ptr: u64,
    spins_max: u32,
) -> Result<[u8; 16], UsbBotError> {
    consume_posted(
        hw,
        caps,
        ev,
        TRB_EVENT_TRANSFER,
        slot,
        dci,
        want_ptr,
        spins_max,
    )
}

/// Control IN: STATUS completes the TD, or DATA success/short immediately.
/// Do not spin `DESC_SPINS` after DATA — leftover STATUS would block the
/// next SETUP (ep0-stop: GET_DEVICE lived, GET_CONFIG `cmd=4`). Caller
/// Stops EP0 when the event is DATA. Leftover SETUP is ignored.
fn consume_control(
    hw: &mut impl XhciHw,
    caps: &XhciCaps,
    ev: &mut EventRing,
    slot: u8,
    data_ptr: u64,
    status_ptr: u64,
    spins_max: u32,
) -> Result<[u8; 16], UsbBotError> {
    let data = data_ptr & !0xF;
    let status = status_ptr & !0xF;
    let mut spins = 0u32;
    loop {
        let t = read_trb(hw, ev.base, ev.deq);
        let ctrl = get_u32(&t, 12);
        if (ctrl & 1) == (ev.cycle & 1) {
            let ty = trb_type(ctrl);
            advance_event(hw, caps, ev);
            if ty != TRB_EVENT_TRANSFER || !xhci_xfer_matches(ctrl, slot, XHCI_EP0_DCI) {
                continue;
            }
            let p = xhci_event_trb_ptr(&t);
            let code = trb_cmpl_code(get_u32(&t, 8));
            if p == data {
                if code != CMPL_SUCCESS && code != CMPL_SHORT {
                    store_usb_bot_diag(
                        UsbBotError::Xfer,
                        usb_bot_last_bar(),
                        usb_bot_last_portsc(),
                        xhci_xfer_diag_cmpl(code, xhci_event_dci(ctrl), xhci_event_slot(ctrl)),
                    );
                    return Err(UsbBotError::Xfer);
                }
                return Ok(t);
            }
            if p != status {
                continue;
            }
            if code != CMPL_SUCCESS && code != CMPL_SHORT {
                store_usb_bot_diag(
                    UsbBotError::Xfer,
                    usb_bot_last_bar(),
                    usb_bot_last_portsc(),
                    xhci_xfer_diag_cmpl(code, xhci_event_dci(ctrl), xhci_event_slot(ctrl)),
                );
                return Err(UsbBotError::Xfer);
            }
            return Ok(t);
        }
        spins = spins.saturating_add(1);
        if spins > spins_max {
            store_usb_bot_diag(
                UsbBotError::Xfer,
                usb_bot_last_bar(),
                usb_bot_last_portsc(),
                u64::from(CMPL_TIMEOUT),
            );
            return Err(UsbBotError::Xfer);
        }
    }
}

fn consume_posted(
    hw: &mut impl XhciHw,
    caps: &XhciCaps,
    ev: &mut EventRing,
    want_type: u32,
    slot: u8,
    dci: u8,
    want_ptr: u64,
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
                if want_type == TRB_EVENT_TRANSFER
                    && dci != 0
                    && !xhci_xfer_matches(ctrl, slot, dci)
                {
                    continue;
                }
                if want_type == TRB_EVENT_TRANSFER
                    && want_ptr != 0
                    && xhci_event_trb_ptr(&t) != (want_ptr & !0xF)
                {
                    continue;
                }
                // Iron capoverlap: leftover CC=0 from p10 abort must not
                // retire p11 CONFIG_EP. Match Command TRB Pointer.
                if want_type == TRB_EVENT_CMD
                    && want_ptr != 0
                    && !xhci_cmd_event_matches(&t, want_ptr)
                {
                    continue;
                }
                let code = trb_cmpl_code(get_u32(&t, 8));
                if want_type == TRB_EVENT_CMD && want_ptr == 0 && cmd_cc_invalid(code) {
                    continue;
                }
                if code != CMPL_SUCCESS && code != CMPL_SHORT {
                    let err = xhci_event_err(want_type);
                    let cmpl = if want_type == TRB_EVENT_TRANSFER {
                        xhci_xfer_diag_cmpl(code, xhci_event_dci(ctrl), xhci_event_slot(ctrl))
                    } else {
                        u64::from(code)
                    };
                    store_usb_bot_diag(err, usb_bot_last_bar(), usb_bot_last_portsc(), cmpl);
                    return Err(err);
                }
                return Ok(t);
            }
            continue;
        }
        spins = spins.saturating_add(1);
        maybe_serial_xhci_rw_wait(spins, spins_max);
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

/// Wait for bulk OUT and bulk IN Transfer Events in either order.
/// Iron skipmaxlun COM2: sequential CBW wait never posted; some BOT
/// devices do not complete the CBW TD until the DATA IN pipe is primed.
/// Do not require TRB pointer match — leftover EP0 events are skipped
/// by slot+DCI. Unmatched events are dropped (same as consume_posted).
fn consume_bulk_pair(
    hw: &mut impl XhciHw,
    caps: &XhciCaps,
    ev: &mut EventRing,
    slot: u8,
    dci_out: u8,
    dci_in: u8,
    spins_max: u32,
) -> Result<(), UsbBotError> {
    let mut got_out = false;
    let mut got_in = false;
    let mut spins = 0u32;
    loop {
        if got_out && got_in {
            return Ok(());
        }
        let t = read_trb(hw, ev.base, ev.deq);
        let ctrl = get_u32(&t, 12);
        if (ctrl & 1) == (ev.cycle & 1) {
            let ty = trb_type(ctrl);
            advance_event(hw, caps, ev);
            if ty != TRB_EVENT_TRANSFER {
                continue;
            }
            let code = trb_cmpl_code(get_u32(&t, 8));
            if code != CMPL_SUCCESS && code != CMPL_SHORT {
                store_usb_bot_diag(
                    UsbBotError::Xfer,
                    usb_bot_last_bar(),
                    usb_bot_last_portsc(),
                    xhci_xfer_diag_cmpl(code, xhci_event_dci(ctrl), xhci_event_slot(ctrl)),
                );
                return Err(UsbBotError::Xfer);
            }
            bulk_pair_take(&mut got_out, &mut got_in, ctrl, slot, dci_out, dci_in);
            continue;
        }
        spins = spins.saturating_add(1);
        if spins > spins_max {
            if got_out {
                store_usb_bot_stage(BOT_STAGE_DATA);
            }
            store_usb_bot_diag(
                UsbBotError::Xfer,
                usb_bot_last_bar(),
                usb_bot_last_portsc(),
                u64::from(CMPL_TIMEOUT),
            );
            return Err(UsbBotError::Xfer);
        }
    }
}

/// Record a Transfer Event as CBW OUT and/or DATA IN for overlapped BOT.
pub fn bulk_pair_take(
    got_out: &mut bool,
    got_in: &mut bool,
    ctrl: u32,
    slot: u8,
    dci_out: u8,
    dci_in: u8,
) {
    if xhci_xfer_matches(ctrl, slot, dci_out) {
        *got_out = true;
    } else if xhci_xfer_matches(ctrl, slot, dci_in) {
        *got_in = true;
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
    let trb_ptr = cmd_ring.place(hw, ptr, 0, extra_and_type);
    doorbell(hw, caps.db, 0, 0);
    consume_posted(hw, caps, ev, TRB_EVENT_CMD, 0, 0, trb_ptr, spins_max)
}

fn abort_cmd_ring(
    hw: &mut impl XhciHw,
    caps: &XhciCaps,
    mem: &XhciMem,
    cmd_ring: &mut Ring,
    ev: &mut EventRing,
) -> bool {
    let off = caps.op + 0x18;
    let cur = read64(hw, off);
    write64(hw, off, crcr_abort_bits(cur));
    // Iron cmdptr COM2: Address Device used ADDR_SPINS then abort polled
    // only SPINS; CRR stayed 1, CRCR restart was ignored, p11 Enable Slot
    // timed out. Wait as long as Address Device. Command Ring Stopped
    // points at the *aborted* TRB — do not require cmdptr match.
    for _ in 0..ADDR_SPINS {
        if !crcr_is_running(read64(hw, off)) {
            break;
        }
        let t = read_trb(hw, ev.base, ev.deq);
        let ctrl = get_u32(&t, 12);
        if (ctrl & 1) == (ev.cycle & 1) {
            advance_event(hw, caps, ev);
            if trb_type(ctrl) == TRB_EVENT_CMD && cmd_cc_ring_stopped(trb_cmpl_code(get_u32(&t, 8)))
            {
                break;
            }
        }
    }
    drain_events(hw, caps, ev);
    zero_page(hw, mem.cmd);
    *cmd_ring = Ring::new(mem.cmd);
    write64(hw, off, crcr_restart(mem.cmd));
    !crcr_is_running(read64(hw, off))
}

/// Prime the command ring with a No-Op after HCRST+RS. Iron p11first COM2:
/// the first Enable Slot doorbell was lost (`cmd=2 cmpl=0xff`); abort then
/// p14 enumerated. A failed No-Op aborts so Toshiba is not the lost first
/// command. Do not stamp BOT diag — this is warmup, not enum fail.
fn prime_cmd_ring(
    hw: &mut impl XhciHw,
    caps: &XhciCaps,
    mem: &XhciMem,
    cmd_ring: &mut Ring,
    ev: &mut EventRing,
) {
    serial_xhci_nop();
    hold_bot_diag(|| {
        let extra = trb_ctrl(0, TRB_NO_OP_CMD, 0);
        if cmd_wait(hw, caps, cmd_ring, ev, 0, extra, ADDR_SPINS).is_err() {
            recover_enum(hw, caps, mem, cmd_ring, ev, 0);
        }
    });
}

/// Enable Slot after HCRST. Iron p11first COM2: first doorbell lost on p11
/// (`cmd=2 cmpl=0xff`, abort crr=0) then the walk named p14 hub instead of
/// Toshiba. Wait ADDR_SPINS; on fail abort and retry once on the same port.
fn enable_slot(
    hw: &mut impl XhciHw,
    caps: &XhciCaps,
    mem: &XhciMem,
    cmd_ring: &mut Ring,
    ev: &mut EventRing,
    port: u8,
) -> Result<[u8; 16], UsbBotError> {
    let extra = trb_ctrl(0, TRB_ENABLE_SLOT, 0);
    match cmd_wait(hw, caps, cmd_ring, ev, 0, extra, ADDR_SPINS) {
        Ok(ev_en) => Ok(ev_en),
        Err(_) => {
            recover_enum(hw, caps, mem, cmd_ring, ev, 0);
            serial_xhci_slotretry(port);
            cmd_wait(hw, caps, cmd_ring, ev, 0, extra, ADDR_SPINS).map_err(|e2| {
                recover_enum(hw, caps, mem, cmd_ring, ev, 0);
                e2
            })
        }
    }
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
    // Iron ep0-eval COM2: p11 CAPACITY CBW `err=8` then p14 hub Disable Slot
    // stamped `cmpl=0` over the BOT diag. Keep the p11 `cmpl`.
    // Iron cmdptr COM2: Disable Slot *wait* skipped Command Ring Stopped
    // (wrong TRB pointer) and p11 Enable Slot timed out. Abort+restart
    // only; leak the slot (64 MaxSlots).
    hold_bot_diag(|| {
        let crr_clear = abort_cmd_ring(hw, caps, mem, cmd_ring, ev);
        serial_xhci_abort(if crr_clear { 0 } else { 1 });
        if slot != 0 {
            let z = [0u8; 8];
            hw.dma_write(mem.dcbaa + u64::from(slot) * 8, &z);
            zero_page(hw, mem.inctx);
            zero_page(hw, mem.devctx);
            zero_page(hw, mem.ep0);
        }
    });
}

/// Abort the command ring; keep DCBAA[slot]. Iron slotretry COM2: GET_DESC
/// timeout then recover_enum zeroed the live slot and the walk named p14.
fn abort_keep_slot(
    hw: &mut impl XhciHw,
    caps: &XhciCaps,
    mem: &XhciMem,
    cmd_ring: &mut Ring,
    ev: &mut EventRing,
) {
    hold_bot_diag(|| {
        let crr_clear = abort_cmd_ring(hw, caps, mem, cmd_ring, ev);
        serial_xhci_abort(if crr_clear { 0 } else { 1 });
    });
}

/// After GET_DESC named a non-hub MSC, do not zero DCBAA. Iron rstdev COM2
/// walked p14 after Toshiba BOT fail and clobbered `bot=csw scsi=inquiry`.
fn recover_named_or_drop(
    hw: &mut impl XhciHw,
    caps: &XhciCaps,
    mem: &XhciMem,
    cmd_ring: &mut Ring,
    ev: &mut EventRing,
    slot: u8,
    port: u8,
    named: bool,
) {
    if named && xhci_keep_slot_after_named_msc() {
        abort_keep_slot(hw, caps, mem, cmd_ring, ev);
        serial_xhci_stopwalk(port);
    } else {
        recover_enum(hw, caps, mem, cmd_ring, ev, slot);
    }
}

fn fill_address_input(
    inctx: &mut [u8; 4096],
    cs: usize,
    slot_dw0: u32,
    port: u8,
    speed: u8,
    ep0_hpa: u64,
) {
    put_u32(inctx, 4, 0x3);
    put_u32(inctx, cs, slot_dw0);
    put_u32(inctx, cs + 4, slot_ctx_dw1_port(port));
    let mps0 = u32::from(ep0_max_packet(speed));
    put_u32(inctx, cs * 2 + 4, (4u32 << 3) | (3 << 1) | (mps0 << 16));
    put_u64(inctx, cs * 2 + 8, ep0_hpa | 1);
}

/// Address Device on an already-enabled slot. Do not Enable Slot again.
fn address_device(
    hw: &mut impl XhciHw,
    caps: &XhciCaps,
    mem: &XhciMem,
    cmd_ring: &mut Ring,
    ev: &mut EventRing,
    port: u8,
    slot: u8,
    speed: u8,
    mmio: u64,
) -> Result<(), UsbBotError> {
    let Some(slot_dw0) = slot_ctx_dw0(speed, 1) else {
        store_usb_bot_diag(UsbBotError::Enum, mmio, xhci_enum_diag_portsc(0, port), 0);
        return Err(stamp_enum(
            hw,
            caps,
            mmio,
            port,
            XHCI_ENUM_CMD_ADDR,
            UsbBotError::Enum,
        ));
    };
    zero_page(hw, mem.inctx);
    zero_page(hw, mem.devctx);
    zero_page(hw, mem.ep0);
    let cs = ctx_size(caps.csz);
    let mut inctx = [0u8; 4096];
    fill_address_input(&mut inctx, cs, slot_dw0, port, speed, mem.ep0);
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
        abort_keep_slot(hw, caps, mem, cmd_ring, ev);
        stamp_enum(hw, caps, mmio, port, XHCI_ENUM_CMD_ADDR, e)
    })?;
    Ok(())
}

/// xHCI 4.6.11: slot Addressed/Configured → Default. Keep DCBAA.
fn reset_device(
    hw: &mut impl XhciHw,
    caps: &XhciCaps,
    mem: &XhciMem,
    cmd_ring: &mut Ring,
    ev: &mut EventRing,
    port: u8,
    slot: u8,
    mmio: u64,
) -> Result<(), UsbBotError> {
    cmd_wait(
        hw,
        caps,
        cmd_ring,
        ev,
        0,
        trb_ctrl(0, TRB_RESET_DEV, u32::from(slot) << 24),
        ADDR_SPINS,
    )
    .map_err(|e| {
        abort_keep_slot(hw, caps, mem, cmd_ring, ev);
        stamp_enum(hw, caps, mmio, port, XHCI_ENUM_CMD_RSTDEV, e)
    })?;
    Ok(())
}

/// GET_DEVICE on a live slot. Iron slotretry COM2: no Transfer Event after
/// Address Device (`cmd=4 cmpl=0xff`) — EP0 drain, not Evaluate Context.
/// Abort the command ring, keep DCBAA, retry EP0; then port-reset + Reset
/// Device + Address Device on the **same** slot. Iron stallquiet COM2:
/// port-reset + Address Device without Reset Device printed `cmd=3
/// cmpl=0x13`. Do not Disable Slot. Do not Enable Slot again.
fn get_device_desc(
    hw: &mut impl XhciHw,
    caps: &XhciCaps,
    mem: &XhciMem,
    cmd_ring: &mut Ring,
    ev: &mut EventRing,
    ep0: &mut Ring,
    slot: u8,
    port: u8,
    mmio: u64,
) -> Result<[u8; 18], UsbBotError> {
    let mut dev = [0u8; 18];
    let setup = setup_get_desc(1, 18);
    if let Err(e) = control_in_retry(
        hw, caps, mem, cmd_ring, ep0, ev, slot, mem.bounce, setup, &mut dev, port,
    ) {
        // Split COM2: packed cmd=4 + no Transfer Event is EP0 GET_DESC,
        // not Evaluate Context (cmd=7 / `xhci eval`).
        let _ = stamp_enum(hw, caps, mmio, port, XHCI_ENUM_CMD_DESC, e);
    } else {
        return Ok(dev);
    }
    abort_keep_slot(hw, caps, mem, cmd_ring, ev);
    reset_ep0(hw, caps, mem, cmd_ring, ev, ep0, slot);
    serial_xhci_descabort(port);
    if control_in_retry(
        hw, caps, mem, cmd_ring, ep0, ev, slot, mem.bounce, setup, &mut dev, port,
    )
    .is_ok()
    {
        return Ok(dev);
    }
    let sc = reset_port(hw, caps, port)
        .map_err(|e| stamp_enum(hw, caps, mmio, port, XHCI_ENUM_CMD_RESET, e))?;
    let speed = portsc_speed(sc);
    serial_xhci_rstdev(port);
    reset_device(hw, caps, mem, cmd_ring, ev, port, slot, mmio)?;
    address_device(hw, caps, mem, cmd_ring, ev, port, slot, speed, mmio)?;
    *ep0 = Ring::new(mem.ep0);
    serial_xhci_descabort(port);
    control_in_retry(
        hw, caps, mem, cmd_ring, ep0, ev, slot, mem.bounce, setup, &mut dev, port,
    )?;
    Ok(dev)
}

fn event_slot(ev: &[u8; 16]) -> u8 {
    xhci_event_slot(get_u32(ev, 12))
}

/// Identity-mapped pages the live driver owns.
#[derive(Clone, Copy)]
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
    pub bounce_in: u64,
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
    zero_page(hw, mem.bounce_in);
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

fn recover_ep(
    hw: &mut impl XhciHw,
    caps: &XhciCaps,
    cmd_ring: &mut Ring,
    ev: &mut EventRing,
    ring: &mut Ring,
    hpa: u64,
    slot: u8,
    dci: u8,
) {
    hold_bot_diag(|| {
        drain_events(hw, caps, ev);
        let extra = xhci_ep_cmd_extra(slot, dci);
        // Timeout: EP is Running. Stop Endpoint then Set TR Dequeue.
        // Reset Endpoint is Halted-only (iron first-cbw `cmpl=0x13`).
        let stop_ok = cmd(hw, caps, cmd_ring, ev, 0, trb_ctrl(0, TRB_STOP_EP, extra)).is_ok();
        if xhci_ep_recover_need_reset(stop_ok) {
            let _ = cmd(hw, caps, cmd_ring, ev, 0, trb_ctrl(0, TRB_RESET_EP, extra));
        }
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
        drain_events(hw, caps, ev);
    });
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
    recover_ep(hw, caps, cmd_ring, ev, ep0, mem.ep0, slot, XHCI_EP0_DCI);
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
            Ok(status_done) => {
                if !status_done {
                    // DATA-complete without STATUS: Stop leftover STATUS TRB.
                    // Iron norearm COM2: GET_MAX_LUN took this path → EP0
                    // Stopped (`epst ep0=3`) immediately before the first CBW.
                    // Optional GET_MAX_LUN is skipped in try_port so this Stop
                    // does not run there.
                    reset_ep0(hw, caps, mem, cmd_ring, ev, ep0, slot);
                }
                return Ok(());
            }
            Err(e) => {
                last = e;
                if n + 1 < XHCI_DESC_TRIES {
                    serial_xhci_desc_retry(port, n + 1);
                    if should_reset_ep0(e) {
                        reset_ep0(hw, caps, mem, cmd_ring, ev, ep0, slot);
                    }
                }
            }
        }
    }
    Err(last)
}

/// Recover EP0 before GET_DESC retry. Timeout leaves EP0 Running — Stop
/// Endpoint (not Reset). Iron stop-ep COM2: device desc lived, then config
/// `cmd=4 cmpl=0xff` `bot=? scsi=?` because timeout retries stacked TRBs.
/// Iron `6ba076cc`: do not Reset Endpoint on timeout (Halted-only).
fn should_reset_ep0(err: UsbBotError) -> bool {
    matches!(err, UsbBotError::Xfer | UsbBotError::Enum)
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
) -> Result<bool, UsbBotError> {
    let z = [0u8; 4096];
    hw.dma_write(bounce, &z[..data.len().min(4096)]);
    ep0.place(hw, u64::from_le_bytes(setup), 8, control_setup_flags(true));
    let data_ptr = ep0.place(hw, bounce, data.len() as u32, control_data_in_flags());
    let status = ep0.place(hw, 0, 0, control_status_out_flags());
    doorbell(hw, caps.db, slot, XHCI_EP0_DCI);
    let ev_trb = consume_control(hw, caps, ev, slot, data_ptr, status, DESC_SPINS)?;
    hw.dma_read(bounce, data);
    Ok(xhci_event_trb_ptr(&ev_trb) == (status & !0xF))
}

fn control_nodata(
    hw: &mut impl XhciHw,
    caps: &XhciCaps,
    ep0: &mut Ring,
    ev: &mut EventRing,
    slot: u8,
    setup: [u8; 8],
) -> Result<(), UsbBotError> {
    ep0.place(hw, u64::from_le_bytes(setup), 8, control_setup_flags(false));
    let status = ep0.place(hw, 0, 0, trb_ctrl(0, TRB_STATUS, TRB_IOC) | (1 << 16));
    doorbell(hw, caps.db, slot, XHCI_EP0_DCI);
    consume_transfer(hw, caps, ev, slot, XHCI_EP0_DCI, status, DESC_SPINS)?;
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
                    if should_reset_ep0(e) {
                        reset_ep0(hw, caps, mem, cmd_ring, ev, ep0, slot);
                    }
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

/// MSC BOT Get Max LUN (class IN 0xFE, 1 byte). Optional (USB MSC 3.2).
/// Iron norearm COM2: GET_MAX_LUN `val=0` then `epst ep0=3` before first CBW
/// (`control_in_retry` DATA-then-Stop). try_port skips the class IN; packet
/// stays in-tree (gate + pack test). Not a BOT-reset / INQUIRY revert.
fn setup_get_max_lun() -> [u8; 8] {
    let mut s = [0u8; 8];
    s[0] = 0xA1;
    s[1] = 0xFE;
    s[6] = 1;
    s
}

/// MSC Bulk-Only Mass Storage Reset (class OUT 0xFF, interface 0).
/// Iron maxlun COM2: GET_MAX_LUN `val=0` then INQUIRY CBW `cmpl=0xff`.
/// USB MSC 3.1: Reset then Clear Feature ENDPOINT_HALT on both bulk EPs
/// before the first CBW. Not a CH / GET_MAX_LUN revert.
fn setup_msc_bot_reset() -> [u8; 8] {
    let mut s = [0u8; 8];
    s[0] = 0x21;
    s[1] = 0xFF;
    s
}

/// CLEAR_FEATURE ENDPOINT_HALT. `ep_addr` is the USB endpoint address
/// (IN has 0x80).
fn setup_clear_halt(ep_addr: u8) -> [u8; 8] {
    let mut s = [0u8; 8];
    s[0] = 0x02;
    s[1] = 1;
    s[4] = ep_addr;
    s
}

/// xHCI 4.6.7 / 4.8.2.1: after GET_DEVICE, Evaluate Context with EP0 MPS
/// from `bMaxPacketSize0`. Missing this is the ep0-stop GET_CONFIG hang.
fn evaluate_ep0_mps(
    hw: &mut impl XhciHw,
    caps: &XhciCaps,
    mem: &XhciMem,
    cmd_ring: &mut Ring,
    ev: &mut EventRing,
    slot: u8,
    port: u8,
    speed: u8,
    mps: u16,
) -> Result<(), UsbBotError> {
    let Some(slot_dw0) = slot_ctx_dw0(speed, 1) else {
        return Err(UsbBotError::Enum);
    };
    zero_page(hw, mem.inctx);
    let cs = ctx_size(caps.csz);
    let mut inctx = [0u8; 4096];
    put_u32(&mut inctx, 4, evaluate_ep0_add_flags());
    put_u32(&mut inctx, cs, slot_dw0);
    put_u32(&mut inctx, cs + 4, slot_ctx_dw1_port(port));
    put_u32(&mut inctx, cs * 2 + 4, ep_ctx_dw1(4, mps));
    put_u64(&mut inctx, cs * 2 + 8, mem.ep0 | 1);
    hw.dma_write(mem.inctx, &inctx);
    cmd_wait(
        hw,
        caps,
        cmd_ring,
        ev,
        mem.inctx,
        trb_ctrl(0, TRB_EVALUATE_CTX, u32::from(slot) << 24),
        ADDR_SPINS,
    )?;
    Ok(())
}

/// One-packet GET_CONFIG, then `wTotalLength` if needed. Toshiba BOT
/// layout from iron `96024edc` if GET_CONFIG still fails — that is the
/// ep0-stop F11 escape, not a CH revert.
fn read_bot_eps_or_toshiba(
    hw: &mut impl XhciHw,
    caps: &XhciCaps,
    mem: &XhciMem,
    cmd_ring: &mut Ring,
    ep0: &mut Ring,
    ev: &mut EventRing,
    slot: u8,
    port: u8,
    vid: u16,
    did: u16,
    mps0: u16,
) -> Result<(u8, u8, u16, u16, u8), UsbBotError> {
    let mut cfg = [0u8; 256];
    let first = mps0.max(8).min(USB_CFG_DESC_MPS);
    let n1 = first as usize;
    let mut last = UsbBotError::Enum;
    match control_in_retry(
        hw,
        caps,
        mem,
        cmd_ring,
        ep0,
        ev,
        slot,
        mem.bounce,
        setup_get_desc(2, first),
        &mut cfg[..n1],
        port,
    ) {
        Ok(()) => {
            let raw = usb_cfg_w_total_raw(&cfg);
            if raw > first && raw <= USB_CFG_DESC_MAX {
                let n2 = raw as usize;
                if let Err(e) = control_in_retry(
                    hw,
                    caps,
                    mem,
                    cmd_ring,
                    ep0,
                    ev,
                    slot,
                    mem.bounce,
                    setup_get_desc(2, raw),
                    &mut cfg[..n2],
                    port,
                ) {
                    last = e;
                }
            }
            let total = usb_cfg_w_total(&cfg);
            if total >= 9 {
                if let Some(eps) = parse_bot_eps(&cfg[..total as usize]) {
                    return Ok(eps);
                }
                let (_bot, uas) = cfg_msc_protos(&cfg[..total as usize]);
                if uas {
                    serial_xhci_uas(port);
                    return Err(UsbBotError::Bot);
                }
                last = UsbBotError::Bot;
            }
        }
        Err(e) => last = e,
    }
    if let Some(eps) = toshiba_bot_eps(vid, did) {
        serial_xhci_cfg_skip(port, vid, did);
        return Ok(eps);
    }
    Err(last)
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
    mem: XhciMem,
    ev: EventRing,
    cmd: Ring,
    ep0: Ring,
    bulk_out: Ring,
    bulk_in: Ring,
    slot: u8,
    dci_out: u8,
    dci_in: u8,
    bounce: u64,
    bounce_in: u64,
    bulk_out_hpa: u64,
    bulk_in_hpa: u64,
    cs: usize,
    port: u8,
    ep_out: u8,
    ep_in: u8,
    long_bulk: bool,
    lba: u32,
    tag: u32,
}

fn hold_bot_diag<R>(f: impl FnOnce() -> R) -> R {
    let err = usb_bot_last_err();
    let bar = usb_bot_last_bar();
    let portsc = usb_bot_last_portsc();
    let cmpl = usb_bot_last_cmpl();
    let r = f();
    restore_usb_bot_diag(err, bar, portsc, cmpl);
    r
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
    recover_ep(hw, caps, cmd_ring, ev, ring, hpa, slot, dci);
}

/// ISP only on the 13-byte CSW short packet. Full-size READ/WRITE IN uses IOC.
pub fn bulk_in_trb_flags(len: usize) -> u32 {
    if len == CSW_LEN {
        TRB_IOC | TRB_ISP
    } else {
        TRB_IOC
    }
}

/// After CAPACITY CSW, before the first 512-byte READ, and after SET_CONFIG
/// before the first CBW. Iron `6c278e85` timed out READ data; `96024edc`
/// failed READ CBW (`cmpl=0`). Iron ep0-eval: SET_CONFIG held then CAPACITY
/// CBW `err=8` — Toshiba spinning HDD needs more than 20M spins.
const BOT_SETTLE_SPINS: u32 = 80_000_000;

impl UsbBulk for LiveXhci {
    fn bulk_out(&mut self, data: &[u8]) -> Result<(), UsbBotError> {
        if data.is_empty() || data.len() > 4096 {
            return Err(UsbBotError::Xfer);
        }
        let mut hw = MmioXhci { base: self.mmio };
        drain_events(&mut hw, &self.caps, &mut self.ev);
        hw.dma_write(self.bounce, data);
        let trb = self.bulk_out.place(
            &mut hw,
            self.bounce,
            data.len() as u32,
            trb_ctrl(0, TRB_NORMAL, TRB_IOC),
        );
        doorbell(&mut hw, self.caps.db, self.slot, self.dci_out);
        let spins = self.bulk_wait();
        consume_transfer(
            &mut hw,
            &self.caps,
            &mut self.ev,
            self.slot,
            self.dci_out,
            trb,
            spins,
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
        hw.dma_write(self.bounce_in, &z[..data.len()]);
        let trb = self.bulk_in.place(
            &mut hw,
            self.bounce_in,
            data.len() as u32,
            trb_ctrl(0, TRB_NORMAL, bulk_in_trb_flags(data.len())),
        );
        doorbell(&mut hw, self.caps.db, self.slot, self.dci_in);
        let spins = self.bulk_wait();
        match consume_transfer(
            &mut hw,
            &self.caps,
            &mut self.ev,
            self.slot,
            self.dci_in,
            trb,
            spins,
        ) {
            Ok(_) => {
                hw.dma_read(self.bounce_in, data);
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

    fn settle(&mut self) {
        for _ in 0..BOT_SETTLE_SPINS {
            core::hint::spin_loop();
        }
    }

    fn prepare_first_cbw(&mut self) {
        // Iron firstcbw COM2: rearm+Clear Halt+FIRST_READ_SPINS then INQUIRY
        // still `cmpl=0xff`. Epst boot (CONFIG_EP dequeue, no Stop) got
        // INQUIRY+CAPACITY. Set TR Deq is fire-and-forget. Iron skipmaxlun
        // COM2: `norearm` + `epst ep0=1` then sequential CBW `cmpl=0xff`.
        self.arm_first_bulk(false);
    }

    fn overlapped_in(&mut self, tag: u32, cdb: &[u8], buf: &mut [u8]) -> Result<(), UsbBotError> {
        // Iron overlap COM2: INQUIRY overlap lived (`xhci firstcbw overlap`)
        // then sequential CAPACITY CBW `cmpl=0xff`. Overlap every IN BOT
        // command (CAPACITY + READ too). Keep skip GET_MAX_LUN / norearm.
        if buf.is_empty() || buf.len() > 4096 {
            return Err(UsbBotError::Xfer);
        }
        if cdb.first().copied().unwrap_or(0) == SCSI_READ_CAPACITY_10 {
            serial_xhci_capoverlap(self.port);
        }
        let data_len = buf.len() as u32;
        let cbw = Cbw::scsi(tag, data_len, true, 0, cdb);
        stamp_scsi_cdb(cdb);
        store_usb_bot_stage(BOT_STAGE_CBW);
        let mut hw = MmioXhci { base: self.mmio };
        drain_events(&mut hw, &self.caps, &mut self.ev);
        hw.dma_write(self.bounce, &cbw.bytes);
        let _trb_out = self.bulk_out.place(
            &mut hw,
            self.bounce,
            CBW_LEN as u32,
            trb_ctrl(0, TRB_NORMAL, TRB_IOC | TRB_ISP),
        );
        let z = [0u8; 4096];
        hw.dma_write(self.bounce_in, &z[..buf.len()]);
        let _trb_in = self.bulk_in.place(
            &mut hw,
            self.bounce_in,
            data_len,
            trb_ctrl(0, TRB_NORMAL, TRB_IOC | TRB_ISP),
        );
        doorbell(&mut hw, self.caps.db, self.slot, self.dci_out);
        doorbell(&mut hw, self.caps.db, self.slot, self.dci_in);
        let spins = self.bulk_wait();
        consume_bulk_pair(
            &mut hw,
            &self.caps,
            &mut self.ev,
            self.slot,
            self.dci_out,
            self.dci_in,
            spins,
        )?;
        hw.dma_read(self.bounce_in, buf);
        store_usb_bot_stage(BOT_STAGE_CSW);
        let mut csw = [0u8; CSW_LEN];
        let n = UsbBulk::bulk_in(self, &mut csw)?;
        if n < CSW_LEN || !csw_ok(&csw) {
            return Err(UsbBotError::Bot);
        }
        Ok(())
    }

    fn prepare_first_read(&mut self) {
        // Iron epst COM2: INQUIRY/CAPACITY lived; READ CBW `cmpl=0xff`.
        self.arm_first_bulk(true);
    }

    fn end_first_read(&mut self) {
        self.long_bulk = false;
    }
}

impl LiveXhci {
    fn bulk_wait(&self) -> u32 {
        if self.long_bulk || xhci_guest_rw_long_wait() {
            FIRST_READ_SPINS
        } else {
            BULK_SPINS
        }
    }

    fn arm_first_bulk(&mut self, read: bool) {
        // Iron firstcbw COM2: Stop+rearm before unused bulk CBW, then
        // INQUIRY `cmpl=0xff` at FIRST_READ_SPINS. Keep CONFIG_EP dequeue.
        // recover_pipes stays on CBW timeout retry only.
        self.long_bulk = true;
        let mut hw = MmioXhci { base: self.mmio };
        let mut outctx = [0u8; 4096];
        hw.dma_read(self.mem.devctx, &mut outctx);
        let cs = self.cs;
        let port = self.port;
        serial_xhci_epst(
            port,
            ep_ctx_state(get_u32(&outctx, output_ep_ctx_off(cs, 1))),
            ep_ctx_state(get_u32(&outctx, output_ep_ctx_off(cs, self.dci_out))),
            ep_ctx_state(get_u32(&outctx, output_ep_ctx_off(cs, self.dci_in))),
        );
        self.settle();
        if read {
            serial_xhci_firstread(port);
        } else {
            serial_xhci_firstcbw(port);
        }
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
    xhci_note_msc_named(false);
    let sc = reset_port(hw, caps, port)
        .map_err(|e| stamp_enum(hw, caps, mmio, port, XHCI_ENUM_CMD_RESET, e))?;
    let speed = portsc_speed(sc);
    if slot_ctx_dw0(speed, 1).is_none() {
        store_usb_bot_diag(UsbBotError::Enum, mmio, xhci_enum_diag_portsc(sc, port), 0);
        return Err(stamp_enum(
            hw,
            caps,
            mmio,
            port,
            XHCI_ENUM_CMD_ADDR,
            UsbBotError::Enum,
        ));
    }
    let ev_en = enable_slot(hw, caps, mem, cmd_ring, ev, port)
        .map_err(|e| stamp_enum(hw, caps, mmio, port, XHCI_ENUM_CMD_SLOT, e))?;
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
    let cs = ctx_size(caps.csz);
    let mut dc = [0u8; 8];
    put_u64(&mut dc, 0, mem.devctx);
    hw.dma_write(mem.dcbaa + u64::from(slot) * 8, &dc);
    address_device(hw, caps, mem, cmd_ring, ev, port, slot, speed, mmio)?;
    let mut ep0 = Ring::new(mem.ep0);
    let dev = get_device_desc(hw, caps, mem, cmd_ring, ev, &mut ep0, slot, port, mmio)
        .map_err(|e| stamp_enum(hw, caps, mmio, port, XHCI_ENUM_CMD_DESC, e))?;
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
    xhci_note_msc_named(true);
    let named = true;
    let vid = u16::from_le_bytes([dev[8], dev[9]]);
    let did = u16::from_le_bytes([dev[10], dev[11]]);
    let ep0_mps = usb_ep0_mps_from_desc(speed, dev[7]);
    drain_events(hw, caps, ev);
    if evaluate_ep0_mps(hw, caps, mem, cmd_ring, ev, slot, port, speed, ep0_mps).is_err() {
        abort_cmd_ring(hw, caps, mem, cmd_ring, ev);
        reset_ep0(hw, caps, mem, cmd_ring, ev, &mut ep0, slot);
        serial_xhci_eval(port, ep0_mps, false);
    } else {
        serial_xhci_eval(port, ep0_mps, true);
    }
    let (ep_out, ep_in, mps_out, mps_in, cfg_val) = match read_bot_eps_or_toshiba(
        hw, caps, mem, cmd_ring, &mut ep0, ev, slot, port, vid, did, ep0_mps,
    ) {
        Ok(eps) => eps,
        Err(e) => {
            recover_named_or_drop(hw, caps, mem, cmd_ring, ev, slot, port, named);
            return Err(stamp_enum(hw, caps, mmio, port, XHCI_ENUM_CMD_DESC, e));
        }
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
        recover_named_or_drop(hw, caps, mem, cmd_ring, ev, slot, port, named);
        stamp_enum(hw, caps, mmio, port, XHCI_ENUM_CMD_SETCFG, e)
    })?;
    serial_xhci_setcfg(port, cfg_val);
    let dci_out = bulk_ep_dci(ep_out, false);
    let dci_in = bulk_ep_dci(ep_in, true);
    let mut outctx = [0u8; 4096];
    hw.dma_read(mem.devctx, &mut outctx);
    zero_page(hw, mem.inctx);
    let mut ic = [0u8; 4096];
    fill_config_ep_input(
        &mut ic,
        &outctx,
        cs,
        speed,
        port,
        dci_out,
        dci_in,
        ep0.tr_dequeue(),
        mem.bulk_out,
        mem.bulk_in,
        mps_out,
        mps_in,
        ep0_mps,
    );
    hw.dma_write(mem.inctx, &ic);
    zero_page(hw, mem.bulk_out);
    zero_page(hw, mem.bulk_in);
    // Iron capoverlap COM2: SET_CONFIG lived then CONFIG_EP `cmd=5 cmpl=0`
    // `err=3 bot=? scsi=?`. Leftover Invalid command event (CC=0) can retire
    // Configure Endpoint. Drain, skip CC=0, retry once with a fresh ring.
    serial_xhci_config(port);
    serial_xhci_cmdptr(port);
    drain_events(hw, caps, ev);
    let mut cfg_err = UsbBotError::Enum;
    let mut cfg_ok = false;
    for n in 0u8..2 {
        if n > 0 {
            serial_xhci_cfgretry(port, n);
            abort_cmd_ring(hw, caps, mem, cmd_ring, ev);
            hw.dma_read(mem.devctx, &mut outctx);
            zero_page(hw, mem.inctx);
            fill_config_ep_input(
                &mut ic,
                &outctx,
                cs,
                speed,
                port,
                dci_out,
                dci_in,
                ep0.tr_dequeue(),
                mem.bulk_out,
                mem.bulk_in,
                mps_out,
                mps_in,
                ep0_mps,
            );
            hw.dma_write(mem.inctx, &ic);
            zero_page(hw, mem.bulk_out);
            zero_page(hw, mem.bulk_in);
        }
        match cmd_wait(
            hw,
            caps,
            cmd_ring,
            ev,
            mem.inctx,
            trb_ctrl(0, TRB_CONFIG_EP, u32::from(slot) << 24),
            ADDR_SPINS,
        ) {
            Ok(_) => {
                cfg_ok = true;
                break;
            }
            Err(e) => cfg_err = e,
        }
    }
    if !cfg_ok {
        if toshiba_bot_eps(vid, did).is_some() {
            serial_xhci_toshiba_named(port);
        }
        recover_named_or_drop(hw, caps, mem, cmd_ring, ev, slot, port, named);
        return Err(stamp_enum(hw, caps, mmio, port, XHCI_ENUM_CMD_CFG, cfg_err));
    }
    drain_events(hw, caps, ev);
    hw.dma_read(mem.devctx, &mut outctx);
    serial_xhci_epst(
        port,
        ep_ctx_state(get_u32(&outctx, output_ep_ctx_off(cs, 1))),
        ep_ctx_state(get_u32(&outctx, output_ep_ctx_off(cs, dci_out))),
        ep_ctx_state(get_u32(&outctx, output_ep_ctx_off(cs, dci_in))),
    );
    let rst_ok = control_nodata_retry(
        hw,
        caps,
        mem,
        cmd_ring,
        &mut ep0,
        ev,
        slot,
        setup_msc_bot_reset(),
        port,
    )
    .is_ok();
    if !rst_ok {
        reset_ep0(hw, caps, mem, cmd_ring, ev, &mut ep0, slot);
    }
    serial_xhci_botrst(port, rst_ok);
    if control_nodata_retry(
        hw,
        caps,
        mem,
        cmd_ring,
        &mut ep0,
        ev,
        slot,
        setup_clear_halt(ep_out),
        port,
    )
    .is_err()
    {
        reset_ep0(hw, caps, mem, cmd_ring, ev, &mut ep0, slot);
    }
    if control_nodata_retry(
        hw,
        caps,
        mem,
        cmd_ring,
        &mut ep0,
        ev,
        slot,
        setup_clear_halt(ep_in | 0x80),
        port,
    )
    .is_err()
    {
        reset_ep0(hw, caps, mem, cmd_ring, ev, &mut ep0, slot);
    }
    // Iron norearm COM2: GET_MAX_LUN `val=0` then `epst ep0=3 out=1 in=1`
    // before `xhci firstcbw norearm`. `control_in_retry` returns on DATA
    // (STATUS often still pending) and Stop+Set TR Deq (`reset_ep0`) so EP0
    // is Stopped immediately before the first CBW. GET_MAX_LUN is optional
    // (USB MSC 3.2). Skip the class IN so EP0 stays Running. Packet stays
    // in-tree (gate + pack test). Not a BOT-reset / INQUIRY revert.
    let _ = setup_get_max_lun();
    serial_xhci_maxlun(port);
    let mut live = LiveXhci {
        mmio,
        caps: *caps,
        mem: *mem,
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
        ep0: Ring {
            base: ep0.base,
            enq: ep0.enq,
            cycle: ep0.cycle,
        },
        bulk_out: Ring::new(mem.bulk_out),
        bulk_in: Ring::new(mem.bulk_in),
        slot,
        dci_out,
        dci_in,
        bounce: mem.bounce,
        bounce_in: mem.bounce_in,
        bulk_out_hpa: mem.bulk_out,
        bulk_in_hpa: mem.bulk_in,
        cs,
        port,
        ep_out,
        ep_in,
        long_bulk: false,
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
            // Iron rstdev COM2: INQUIRY CSW wait then recover_enum + p14/p10.
            // Guestio reached usb I/O ready on this named slot. Keep DCBAA,
            // abort, recover bulk pipes, retry BOT once. Do not walk.
            if named && xhci_keep_slot_after_named_msc() {
                abort_keep_slot(hw, caps, mem, &mut live.cmd, &mut live.ev);
                live.recover_pipes();
                match usb_bot_bring_up(&mut live, min_bytes) {
                    Ok((bytes, lba)) => {
                        if lun_is_esp_cruzer(bytes) {
                            recover_enum(hw, caps, mem, cmd_ring, ev, slot);
                            return Err(UsbBotError::Cruzer);
                        }
                        live.lba = lba;
                        live.tag = next_bot_tag();
                        store_usb_bot_ready(bytes, lba);
                        return Ok(live);
                    }
                    Err(e2) => {
                        serial_xhci_stopwalk(port);
                        let sc = hw.read32(portsc_off(caps.op, port));
                        store_usb_bot_diag(
                            e2,
                            mmio,
                            xhci_enum_diag_portsc(sc, port),
                            usb_bot_last_cmpl(),
                        );
                        return Err(e2);
                    }
                }
            }
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
    xhci_note_msc_named(false);
    let (caps, mut cmd_ring, mut ev) = xhci_start(hw, mem)?;
    let mut last = UsbBotError::Reset;
    let mut any_ccs = false;
    let ports = xhci_scan_ports(caps.max_ports);
    serial_xhci_p11first();
    prime_cmd_ring(hw, &caps, mem, &mut cmd_ring, &mut ev);
    for idx in 0..ports {
        let Some(port) = xhci_enum_port_at(ports, idx) else {
            break;
        };
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
            Err(e) => {
                // Iron slotretry COM2: p11 GET_DESC EP0 timeout then p14 hub.
                // Keep the live slot; do not start p14/p10.
                // Iron rstdev COM2: Toshiba named then BOT CSW wait then p14.
                if xhci_desc_fail_stops_walk(usb_bot_last_cmpl())
                    || xhci_named_msc_fail_stops_walk(e, xhci_msc_was_named())
                {
                    store_usb_bot_diag(e, mmio, usb_bot_last_portsc(), usb_bot_last_cmpl());
                    return Err(e);
                }
                last = xhci_bring_up_keep_err(last, e);
            }
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
static mut BOUNCE_IN: Page = Page([0; 4096]);
#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
static mut SCRATCH0: [Page; XHCI_SCRATCH_MAX as usize] =
    [Page([0; 4096]); XHCI_SCRATCH_MAX as usize];

#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
static mut LIVE: Option<LiveXhci> = None;
#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
static LIVE_LOCK: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);
#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
static RW_FAIL_N: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);
#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
static RW_OK_N: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);

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
            bounce_in: core::ptr::addr_of_mut!(BOUNCE_IN) as u64,
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
        serial_xhci_rw_busy(off, write);
        return false;
    }
    // SAFETY: lock held; LIVE set by xhci_init_pci.
    let ok = unsafe {
        match LIVE.as_mut() {
            Some(live) => {
                if buf.len() > 4096 {
                    false
                } else {
                    live.long_bulk = xhci_guest_rw_long_wait();
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
                            serial_xhci_rw_ok(off, write);
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
    let n = RW_FAIL_N.fetch_add(1, core::sync::atomic::Ordering::AcqRel);
    if !xhci_rw_fail_should_print(n) {
        return;
    }
    use crate::boot::serial;
    serial::write_str_nowait("boot: Stage 46 durable LUN usb rw fail off=0x");
    serial_hex64_nowait(off);
    serial::write_str_nowait(if write { " wr=1 err=" } else { " wr=0 err=" });
    serial_dec_u8_nowait(err as u8);
    serial::write_str_nowait(" cmpl=0x");
    serial_hex32_nowait(usb_bot_last_cmpl() as u32);
    serial::write_str_nowait(" bot=");
    serial::write_str_nowait(usb_bot_stage_name(usb_bot_last_stage()));
    serial::write_str_nowait(" scsi=");
    serial::write_str_nowait(usb_bot_scsi_name(usb_bot_last_scsi()));
    serial::write_str_nowait(" n=");
    serial_dec_u8_nowait(n.min(250) as u8);
    serial::write_line_nowait(" (not ISO-INSTALL-OK)");
}

#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
fn serial_xhci_rw_busy(off: u64, write: bool) {
    let n = RW_FAIL_N.fetch_add(1, core::sync::atomic::Ordering::AcqRel);
    if !xhci_rw_fail_should_print(n) {
        return;
    }
    use crate::boot::serial;
    serial::write_str_nowait("boot: Stage 46 durable LUN usb rw busy off=0x");
    serial_hex64_nowait(off);
    serial::write_str_nowait(if write { " wr=1 n=" } else { " wr=0 n=" });
    serial_dec_u8_nowait(n.min(250) as u8);
    serial::write_line_nowait(" (not ISO-INSTALL-OK)");
}

#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
fn serial_xhci_rw_ok(off: u64, write: bool) {
    let n = RW_OK_N.fetch_add(1, core::sync::atomic::Ordering::AcqRel);
    if !xhci_rw_fail_should_print(n) {
        return;
    }
    use crate::boot::serial;
    serial::write_str_nowait("boot: Stage 46 durable LUN usb rw ok off=0x");
    serial_hex64_nowait(off);
    serial::write_str_nowait(if write { " wr=1 n=" } else { " wr=0 n=" });
    serial_dec_u8_nowait(n.min(250) as u8);
    serial::write_line_nowait(" (not ISO-INSTALL-OK)");
}

#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
fn maybe_serial_xhci_rw_wait(spins: u32, spins_max: u32) {
    if spins != BULK_SPINS || spins_max <= BULK_SPINS {
        return;
    }
    use crate::boot::serial;
    serial::write_str_nowait("boot: Stage 46 durable LUN usb rw wait spins=");
    serial_dec_u32_nowait(spins);
    serial::write_line_nowait(" (not ISO-INSTALL-OK)");
}

#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
fn serial_hex32_nowait(v: u32) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut buf = [0u8; 8];
    for i in 0..8 {
        buf[i] = HEX[((v >> (28 - i * 4)) & 0xF) as usize];
    }
    crate::boot::serial::write_str_nowait(core::str::from_utf8(&buf).unwrap_or("????????"));
}

#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
fn serial_hex64_nowait(v: u64) {
    serial_hex32_nowait((v >> 32) as u32);
    serial_hex32_nowait(v as u32);
}

#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
fn serial_dec_u8_nowait(v: u8) {
    if v >= 100 {
        crate::boot::serial::write_str_nowait("100+");
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
    crate::boot::serial::write_str_nowait(core::str::from_utf8(&buf[..n]).unwrap_or("?"));
}

#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
fn serial_dec_u32_nowait(v: u32) {
    if v == 0 {
        crate::boot::serial::write_str_nowait("0");
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
    crate::boot::serial::write_str_nowait(core::str::from_utf8(&out[..n]).unwrap_or("?"));
}

#[cfg(not(all(target_os = "uefi", feature = "uefi-bin")))]
fn maybe_serial_xhci_rw_wait(_spins: u32, _spins_max: u32) {}

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
    use crate::mgmt::usb_bot::{
        store_usb_bot_diag, usb_bot_keep_xfer_diag, usb_bot_last_cmpl, usb_bot_last_err,
        usb_bot_last_portsc, usb_bot_last_scsi, usb_bot_last_stage, usb_bot_scsi_name,
        usb_bot_stage_name,
    };

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
        assert_eq!(
            toshiba_bot_eps(USB_VID_TOSHIBA, USB_DID_TOSHIBA_LUN),
            Some((2, 1, 512, 512, 1))
        );
        assert_eq!(toshiba_bot_eps(0x0781, 0x5567), None);
        assert_eq!(usb_ep0_mps_from_desc(3, 64), 64);
        assert_eq!(usb_ep0_mps_from_desc(3, 0), 64);
        assert_eq!(usb_ep0_mps_from_desc(2, 8), 8);
        assert_eq!(evaluate_ep0_add_flags(), 0x3);
        assert_eq!(TRB_EVALUATE_CTX, 13);
        assert_eq!(XHCI_ENUM_CMD_EVAL, 7);
        assert_eq!(config_ep_add_flags(4, 3), 0x1B);
        assert_eq!(xhci_ctx_size(false), 32);
        assert_eq!(xhci_ctx_size(true), 64);
        assert_eq!(ep_ctx_state(1), EP_STATE_RUNNING);
        assert_eq!(ep_ctx_input_clear_state(1), 0);
    }

    #[test]
    fn config_ep_input_copies_output_slot_and_clears_ep_state() {
        // Iron maxlun COM2: reconstructed Slot + A0+A3+A4 then INQUIRY CBW
        // timeout. Input must copy Output Slot (keep Speed), set Context
        // Entries=4, include A1 with live EP0 dequeue, EP State=0 in Input.
        let cs = xhci_ctx_size(false);
        let mut out = [0u8; 4096];
        put_u32(&mut out, 0, slot_ctx_dw0(3, 1).expect("slot"));
        put_u32(&mut out, 4, slot_ctx_dw1_port(11));
        put_u32(&mut out, 12, 5);
        put_u32(
            &mut out,
            output_ep_ctx_off(cs, 1),
            u32::from(EP_STATE_RUNNING),
        );
        put_u32(&mut out, output_ep_ctx_off(cs, 1) + 4, ep_ctx_dw1(4, 64));
        put_u64(&mut out, output_ep_ctx_off(cs, 1) + 8, 0x2000 | 1);
        let mut ic = [0u8; 4096];
        let ring = Ring::new(0x2000);
        fill_config_ep_input(
            &mut ic,
            &out,
            cs,
            3,
            11,
            4,
            3,
            ring.tr_dequeue(),
            0x3000,
            0x4000,
            512,
            512,
            64,
        );
        assert_eq!(get_u32(&ic, 4), config_ep_add_flags(4, 3));
        assert_eq!(slot_ctx_speed(get_u32(&ic, cs)), 3);
        assert_eq!(get_u32(&ic, cs) >> 27, 4);
        assert_eq!(slot_ctx_dw1_num_ports(get_u32(&ic, cs + 4)), 0);
        assert_eq!(get_u32(&ic, cs + 4) >> 16, 11);
        assert_eq!(get_u32(&ic, cs + 12), 5);
        assert_eq!(
            ep_ctx_state(get_u32(&ic, input_ep_ctx_off(cs, 1))),
            EP_STATE_DISABLED
        );
        assert_eq!(get_u64(&ic, input_ep_ctx_off(cs, 1) + 8), ring.tr_dequeue());
        assert_eq!(ring.tr_dequeue(), 0x2000 | 1);
        assert_eq!(get_u32(&ic, input_ep_ctx_off(cs, 4) + 4) >> 16, 512);
        assert_eq!(get_u64(&ic, input_ep_ctx_off(cs, 4) + 8), 0x3000 | 1);
        assert_eq!(get_u32(&ic, input_ep_ctx_off(cs, 3) + 4) >> 16, 512);
        let rst = setup_msc_bot_reset();
        assert_eq!(rst[0], 0x21);
        assert_eq!(rst[1], 0xFF);
        assert_eq!(rst[6], 0);
        let halt_out = setup_clear_halt(2);
        assert_eq!(halt_out[0], 0x02);
        assert_eq!(halt_out[1], 1);
        assert_eq!(halt_out[4], 2);
        assert_eq!(setup_clear_halt(0x81)[4], 0x81);
        assert_eq!(
            slot_ctx_set_entries(slot_ctx_dw0(3, 1).unwrap(), 4) >> 27,
            4
        );
        assert_eq!(output_ep_ctx_off(32, 4), 128);
        assert_eq!(input_ep_ctx_off(32, 4), 160);
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
    fn leftover_setup_event_does_not_retire_status_trb() {
        // Iron `cbd9bf47`: GET_DESC waited for any EP0 Transfer Event.
        // Leftover SETUP/DATA (or a prior STATUS) retired the wait; config
        // GET_DESC then timed out (`cmd=4 cmpl=0xff`). Wait the STATUS TRB.
        let mut hw = FakeMem { mem: [0; 4096] };
        let caps = XhciCaps {
            op: 0,
            rt: 0,
            db: 0,
            max_slots: 64,
            max_ports: 26,
            csz: false,
        };
        let extra = (1u32 << 24) | (u32::from(XHCI_EP0_DCI) << 16);
        write_trb(
            &mut hw,
            0,
            0,
            0x2000,
            u32::from(CMPL_SUCCESS) << 24,
            trb_ctrl(1, TRB_EVENT_TRANSFER, extra),
        );
        write_trb(
            &mut hw,
            0,
            1,
            0x2020,
            u32::from(CMPL_SUCCESS) << 24,
            trb_ctrl(1, TRB_EVENT_TRANSFER, extra),
        );
        let mut any = EventRing::new(0);
        let old =
            consume_transfer(&mut hw, &caps, &mut any, 1, XHCI_EP0_DCI, 0, 8).expect("any event");
        assert_eq!(xhci_event_trb_ptr(&old), 0x2000);

        let mut hw = FakeMem { mem: [0; 4096] };
        write_trb(
            &mut hw,
            0,
            0,
            0x2000,
            u32::from(CMPL_SUCCESS) << 24,
            trb_ctrl(1, TRB_EVENT_TRANSFER, extra),
        );
        write_trb(
            &mut hw,
            0,
            1,
            0x2020,
            u32::from(CMPL_SUCCESS) << 24,
            trb_ctrl(1, TRB_EVENT_TRANSFER, extra),
        );
        let mut ev = EventRing::new(0);
        let got =
            consume_transfer(&mut hw, &caps, &mut ev, 1, XHCI_EP0_DCI, 0x2020, 8).expect("status");
        assert_eq!(xhci_event_trb_ptr(&got), 0x2020);

        // Iron ep0-stop: leftover SETUP must not complete control; DATA short
        // packet without STATUS still completes (Intel skip-STATUS / 9-byte
        // GET_CONFIG `cmd=4 cmpl=0xff`).
        let mut hw = FakeMem { mem: [0; 4096] };
        write_trb(
            &mut hw,
            0,
            0,
            0x2000,
            u32::from(CMPL_SUCCESS) << 24,
            trb_ctrl(1, TRB_EVENT_TRANSFER, extra),
        );
        write_trb(
            &mut hw,
            0,
            1,
            0x2010,
            u32::from(CMPL_SHORT) << 24,
            trb_ctrl(1, TRB_EVENT_TRANSFER, extra),
        );
        let mut ev = EventRing::new(0);
        let got = consume_control(&mut hw, &caps, &mut ev, 1, 0x2010, 0x2020, 8).expect("data");
        assert_eq!(xhci_event_trb_ptr(&got), 0x2010);
        assert!(xhci_control_event_matches(&got, 0x2010, 0x2020));
        let mut setup_only = [0u8; 16];
        put_u64(&mut setup_only, 0, 0x2000);
        assert!(!xhci_control_event_matches(&setup_only, 0x2010, 0x2020));

        // Same bulk DCI leftover CSW IN vs later READ IN.
        let mut hw = FakeMem { mem: [0; 4096] };
        let in_extra = (1u32 << 24) | (3u32 << 16);
        write_trb(
            &mut hw,
            0,
            0,
            0x4000,
            u32::from(CMPL_SUCCESS) << 24,
            trb_ctrl(1, TRB_EVENT_TRANSFER, in_extra),
        );
        write_trb(
            &mut hw,
            0,
            1,
            0x4020,
            u32::from(CMPL_SUCCESS) << 24,
            trb_ctrl(1, TRB_EVENT_TRANSFER, in_extra),
        );
        let mut ev = EventRing::new(0);
        let got = consume_transfer(&mut hw, &caps, &mut ev, 1, 3, 0x4020, 8).expect("read in");
        assert_eq!(xhci_event_trb_ptr(&got), 0x4020);
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
        assert_eq!(XHCI_ENUM_CMD_EVAL, 7);
        assert!(cmd_cc_invalid(0));
        assert!(!cmd_cc_invalid(CMPL_SUCCESS));
        // Iron capoverlap COM2: fail `cmpl=0x30500` is CC=0 + cmd=5 + speed=3.
        assert_eq!(
            xhci_enum_diag_cmpl(0, XHCI_ENUM_CMD_CFG, 3, false),
            0x0003_0500
        );
        // Leftover p10 abort CC=0 must not match p11 CONFIG_EP TRB.
        let mut leftover_cc0 = [0u8; 16];
        put_u64(&mut leftover_cc0, 0, 0x1000);
        let mut config_ep_trb = [0u8; 16];
        put_u64(&mut config_ep_trb, 0, 0x2000);
        assert!(!xhci_cmd_event_matches(&leftover_cc0, 0x2000));
        assert!(xhci_cmd_event_matches(&config_ep_trb, 0x2000));
        assert!(!xhci_cmd_event_matches(&config_ep_trb, 0));
        // Iron p11first: Lewisburg Success with Command TRB Pointer 0
        // must match; leftover Invalid CC=0 ptr=0 must not.
        let leftover_cc0_ptr0 = [0u8; 16];
        assert!(!xhci_cmd_event_matches(&leftover_cc0_ptr0, 0x2000));
        let mut success_ptr0 = [0u8; 16];
        put_u32(&mut success_ptr0, 8, u32::from(CMPL_SUCCESS) << 24);
        assert!(xhci_cmd_event_matches(&success_ptr0, 0x2000));
        assert!(!xhci_cmd_event_matches(&success_ptr0, 0));
        assert_eq!(TRB_NO_OP_CMD, 23);
        assert!(xhci_retry_enable_slot(false));
        assert!(!xhci_retry_enable_slot(true));
        // Iron slotretry COM2: packed fail `cmpl=0x303ff` is p10 ADDR (cmd=3),
        // not the p11 GET_DESC timeout (`cmd=4`). Stop the walk on cmd=4;
        // keep the live slot (no Disable Slot / no p14).
        let desc_to = xhci_enum_diag_cmpl(CMPL_TIMEOUT, XHCI_ENUM_CMD_DESC, 3, false);
        assert_eq!(xhci_enum_diag_cmd(desc_to), XHCI_ENUM_CMD_DESC);
        assert!(xhci_desc_fail_stops_walk(desc_to));
        assert!(!xhci_desc_fail_stops_walk(0x0003_03ff));
        assert!(!xhci_desc_fail_stops_walk(xhci_enum_diag_cmpl(
            CMPL_TIMEOUT,
            XHCI_ENUM_CMD_SLOT,
            3,
            false
        )));
        // Iron rstdev COM2: packed p10 ADDR `0x303ff` must not hide a named
        // p11 BOT fail. Stop the walk on named Xfer/Bot/Enum; Cruzer walks.
        assert!(xhci_named_msc_fail_stops_walk(UsbBotError::Xfer, true));
        assert!(xhci_named_msc_fail_stops_walk(UsbBotError::Bot, true));
        assert!(xhci_named_msc_fail_stops_walk(UsbBotError::Enum, true));
        assert!(!xhci_named_msc_fail_stops_walk(UsbBotError::Cruzer, true));
        assert!(!xhci_named_msc_fail_stops_walk(UsbBotError::Hub, true));
        assert!(!xhci_named_msc_fail_stops_walk(UsbBotError::Xfer, false));
        assert!(xhci_keep_slot_after_named_msc());
        xhci_note_msc_named(true);
        assert!(xhci_msc_was_named());
        assert!(xhci_named_msc_fail_stops_walk(UsbBotError::Xfer, xhci_msc_was_named()));
        xhci_note_msc_named(false);
        assert!(!xhci_msc_was_named());
        assert!(xhci_desc_timeout_is_ep0_xfer(
            XHCI_ENUM_CMD_DESC,
            CMPL_TIMEOUT
        ));
        assert!(!xhci_desc_timeout_is_ep0_xfer(
            XHCI_ENUM_CMD_EVAL,
            CMPL_TIMEOUT
        ));
        assert_eq!(XHCI_ENUM_CMD_EVAL, 7);
        assert_eq!(XHCI_ENUM_CMD_RSTDEV, 8);
        assert_eq!(TRB_RESET_DEV, 17);
        assert_ne!(XHCI_ENUM_CMD_DESC, XHCI_ENUM_CMD_EVAL);
        assert!(xhci_keep_slot_after_desc());
        // Iron stallquiet COM2: `cmd=3 cmpl=0x13` after port-reset Address
        // Device on an Addressed slot. Reset Device first.
        assert!(xhci_addr_context_state_needs_reset_device(
            XHCI_ENUM_CMD_ADDR,
            CMPL_CONTEXT_STATE
        ));
        assert!(!xhci_addr_context_state_needs_reset_device(
            XHCI_ENUM_CMD_DESC,
            CMPL_CONTEXT_STATE
        ));
        assert!(!xhci_addr_context_state_needs_reset_device(
            XHCI_ENUM_CMD_ADDR,
            CMPL_TIMEOUT
        ));
        assert!(xhci_guest_rw_long_wait());
        assert!(xhci_rw_fail_should_print(0));
        assert!(xhci_rw_fail_should_print(7));
        assert!(!xhci_rw_fail_should_print(8));
        assert!(xhci_rw_fail_should_print(64));
        assert!(cmd_cc_ring_stopped(CMPL_CMD_STOPPED));
        assert!(cmd_cc_ring_stopped(CMPL_CMD_ABORTED));
        assert!(!cmd_cc_ring_stopped(0));
        assert_eq!(xhci_enum_port_at(26, 0), Some(11));
        assert_eq!(xhci_enum_port_at(26, 1), Some(12));
        assert_eq!(xhci_enum_port_at(26, 15), Some(26));
        assert_eq!(xhci_enum_port_at(26, 16), Some(1));
        assert_eq!(xhci_enum_port_at(26, 25), Some(10));
        assert_eq!(xhci_enum_port_at(8, 0), Some(1));
        assert_eq!(XHCI_ENUM_TOSHIBA_PORT, 11);
        assert_eq!(TRB_EVALUATE_CTX, 13);
        assert_eq!(TRB_CONFIG_EP, 12);
        assert_eq!(bulk_ep_dci(2, false), 4);
        assert_eq!(bulk_ep_dci(1, true), 3);
        assert_eq!(ep_ctx_dw1(2, 512) >> 16, 512);
        assert_eq!(ep_ctx_dw4_avg_trb(512), 512);
        // Toshiba p11: leftover CSW IN (DCI 3) must not match CBW OUT (DCI 4).
        let csw_in = trb_ctrl(1, TRB_EVENT_TRANSFER, 0) | (1u32 << 24) | (3u32 << 16);
        let cbw_out = trb_ctrl(1, TRB_EVENT_TRANSFER, 0) | (1u32 << 24) | (4u32 << 16);
        assert!(!xhci_xfer_matches(csw_in, 1, 4));
        assert!(xhci_xfer_matches(cbw_out, 1, 4));
        assert_eq!(xhci_event_dci(csw_in), 3);
        // Iron skipmaxlun: overlapped INQUIRY accepts OUT then IN, or IN then OUT.
        let mut got_out = false;
        let mut got_in = false;
        bulk_pair_take(&mut got_out, &mut got_in, csw_in, 1, 4, 3);
        assert!(!got_out && got_in);
        bulk_pair_take(&mut got_out, &mut got_in, cbw_out, 1, 4, 3);
        assert!(got_out && got_in);
        got_out = false;
        got_in = false;
        bulk_pair_take(&mut got_out, &mut got_in, cbw_out, 1, 4, 3);
        bulk_pair_take(&mut got_out, &mut got_in, csw_in, 1, 4, 3);
        assert!(got_out && got_in);
        assert_eq!(xhci_event_dci(cbw_out), 4);
        assert_eq!(xhci_xfer_diag_cmpl(0, 4, 1), 0x0001_0400);
        assert_eq!(BOT_SETTLE_SPINS > SPINS, true);
        assert_eq!(DESC_SPINS, ADDR_SPINS);
        assert_eq!(XHCI_EP0_DCI, 1);
        assert_eq!(TRB_CH, 1 << 4);
        assert_eq!(USB_CFG_DESC_MPS, 64);
        assert_eq!(USB_CFG_DESC_MAX, 256);
        let setup_in = control_setup_flags(true);
        assert_eq!(setup_in & TRB_CH, 0);
        assert_ne!(setup_in & TRB_IDT, 0);
        assert_eq!((setup_in >> 16) & 3, 3);
        let data_in = control_data_in_flags();
        assert_eq!(data_in & TRB_CH, 0);
        assert_ne!(data_in & TRB_ISP, 0);
        assert_ne!(data_in & TRB_IOC, 0);
        assert_eq!(control_status_out_flags() & TRB_CH, 0);
        assert_eq!(usb_cfg_w_total(&[9, 2, 32, 0]), 4); // min(32, len=4) on short slice
        assert_eq!(usb_cfg_w_total_raw(&[9, 2, 32, 0]), 32);
        let mut cfg256 = [0u8; 32];
        cfg256[0] = 9;
        cfg256[1] = 2;
        cfg256[2] = 32;
        assert_eq!(usb_cfg_w_total(&cfg256), 32);
        assert_eq!(usb_cfg_w_total(&[9, 1, 18, 0]), 0);
        assert_eq!(setup_get_desc(2, USB_CFG_DESC_MPS)[6], 64);
        assert_eq!(setup_get_desc(2, USB_CFG_DESC_MPS)[7], 0);
        let maxlun = setup_get_max_lun();
        assert_eq!(maxlun[0], 0xA1);
        assert_eq!(maxlun[1], 0xFE);
        assert_eq!(maxlun[6], 1);
        assert_eq!(setup_msc_bot_reset()[1], 0xFF);
        assert_eq!(config_ep_add_flags(4, 3) & (1 << 1), 2);
        assert_eq!(BOT_SETTLE_SPINS >= 80_000_000, true);
        assert_eq!(setup_get_desc(2, USB_CFG_DESC_MAX)[6], 0);
        assert_eq!(setup_get_desc(2, USB_CFG_DESC_MAX)[7], 1);
        // Iron `cbd9bf47`: leftover SETUP TRB must not match STATUS TRB.
        let mut setup_ev = [0u8; 16];
        put_u64(&mut setup_ev, 0, 0x2000);
        let mut status_ev = [0u8; 16];
        put_u64(&mut status_ev, 0, 0x2020);
        assert_eq!(xhci_event_trb_ptr(&setup_ev), 0x2000);
        assert_eq!(xhci_event_trb_ptr(&status_ev), 0x2020);
        assert_ne!(
            xhci_event_trb_ptr(&setup_ev),
            xhci_event_trb_ptr(&status_ev)
        );
        // Same DCI leftover CSW IN vs later READ IN.
        let mut csw_ptr = [0u8; 16];
        put_u64(&mut csw_ptr, 0, 0x4000);
        let mut read_ptr = [0u8; 16];
        put_u64(&mut read_ptr, 0, 0x4020);
        assert_ne!(xhci_event_trb_ptr(&csw_ptr), xhci_event_trb_ptr(&read_ptr));
        store_usb_bot_diag(UsbBotError::Xfer, 0, 0, u64::from(CMPL_TIMEOUT));
        assert!(should_reset_ep0(UsbBotError::Xfer));
        store_usb_bot_diag(UsbBotError::Xfer, 0, 0, 6);
        assert!(should_reset_ep0(UsbBotError::Xfer));
        assert!(!should_reset_ep0(UsbBotError::Hub));
        assert_eq!(xhci_reset_diag_cmpl(64, 26, 0x80, 14), 0x0e80_1a40);
        assert_eq!(CMPL_PARAMETER, 17);
        assert_eq!(CMPL_TIMEOUT, 0xFF);
        assert_eq!(ADDR_SPINS > SPINS, true);
        assert_eq!(BULK_SPINS > ADDR_SPINS, true);
        assert_eq!(FIRST_READ_SPINS > BULK_SPINS, true);
        assert_eq!(TRB_ISP, 1 << 2);
        assert_eq!(bulk_in_trb_flags(CSW_LEN), TRB_IOC | TRB_ISP);
        assert_eq!(bulk_in_trb_flags(512), TRB_IOC);
        assert_eq!(usb_bot_stage_name(2), "data");
        assert!(usb_bot_last_stage() <= 3);
        assert_eq!(usb_bot_scsi_name(usb_bot_last_scsi()).is_empty(), false);
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
        assert_eq!(TRB_STOP_EP, 15);
        assert_eq!(TRB_SET_TR_DEQ, 16);
        assert_eq!(TRB_RESET_DEV, 17);
        assert_eq!(CMPL_CONTEXT_STATE, 19);
        assert!(!xhci_ep_recover_need_reset(true));
        assert!(xhci_ep_recover_need_reset(false));
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
        // Iron first-cbw: Reset Endpoint CC 19 must not clobber CBW timeout.
        super::hold_bot_diag(|| {
            store_usb_bot_diag(UsbBotError::Enum, 0, 0, u64::from(CMPL_CONTEXT_STATE));
        });
        assert_eq!(usb_bot_last_err(), UsbBotError::Xfer as u8);
        assert_eq!(usb_bot_last_cmpl(), 0xff);
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
