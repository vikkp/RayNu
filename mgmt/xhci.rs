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
    next_bot_tag, store_usb_bot_diag, store_usb_bot_ready, usb_bot_bring_up, UsbBotError, UsbBulk,
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
pub const TRB_EVENT_TRANSFER: u32 = 32;
pub const TRB_EVENT_CMD: u32 = 33;

pub const TRB_CYCLE: u32 = 1;
pub const TRB_IOC: u32 = 1 << 5;
pub const TRB_IDT: u32 = 1 << 6;
pub const TRB_TC: u32 = 1 << 1;

pub const CMPL_SUCCESS: u8 = 1;
pub const CMPL_SHORT: u8 = 13;

pub const USBCMD_RS: u32 = 1;
pub const USBCMD_HCRST: u32 = 1 << 1;
pub const USBCMD_INTE: u32 = 1 << 2;
pub const USBSTS_HCH: u32 = 1;
pub const USBSTS_CNR: u32 = 1 << 11;

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
    let cap0 = hw.read32(0);
    let caplen = cap0 as u8;
    if caplen < 0x20 {
        return Err(UsbBotError::Cap);
    }
    let hcs1 = hw.read32(0x04);
    let hcc1 = hw.read32(0x10);
    Ok(XhciCaps {
        op: u32::from(caplen),
        rt: hw.read32(0x18),
        db: hw.read32(0x14),
        max_slots: hcs1 as u8,
        max_ports: (hcs1 >> 24) as u8,
        csz: (hcc1 & (1 << 2)) != 0,
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
        if cap as u8 == 1 {
            hw.write32(xecp, cap | (1 << 24));
            for _ in 0..SPINS {
                if hw.read32(xecp) & (1 << 16) == 0 {
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

fn doorbell(hw: &mut impl XhciHw, db: u32, slot: u8, target: u8) {
    hw.write32(db + u32::from(slot) * 4, u32::from(target));
}

fn consume_event(
    hw: &mut impl XhciHw,
    caps: &XhciCaps,
    ev: &mut EventRing,
    want_type: u32,
) -> Result<[u8; 16], UsbBotError> {
    let erdp_off = caps.rt + 0x38;
    let mut spins = 0u32;
    loop {
        let t = read_trb(hw, ev.base, ev.deq);
        let ctrl = get_u32(&t, 12);
        if (ctrl & 1) == (ev.cycle & 1) {
            let ty = trb_type(ctrl);
            ev.deq = ev.deq.saturating_add(1);
            if ev.deq == RING_TRBS {
                ev.deq = 0;
                ev.cycle ^= 1;
            }
            write64(hw, erdp_off, ev.base + u64::from(ev.deq) * 16 | (1 << 3));
            if ty == want_type {
                let code = trb_cmpl_code(get_u32(&t, 8));
                if code != CMPL_SUCCESS && code != CMPL_SHORT {
                    store_usb_bot_diag(UsbBotError::Enum, 0, 0, u64::from(code));
                    return Err(UsbBotError::Enum);
                }
                return Ok(t);
            }
            continue;
        }
        spins = spins.saturating_add(1);
        if spins > SPINS {
            return Err(UsbBotError::Enum);
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
    cmd_ring.place(hw, ptr, 0, extra_and_type);
    doorbell(hw, caps.db, 0, 0);
    consume_event(hw, caps, ev, TRB_EVENT_CMD)
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
    let caps = parse_caps(hw)?;
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
    let slots = core::cmp::min(caps.max_slots, 16);
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
    let scratch = ((hcs2 >> 21) & 0x1F) << 5 | ((hcs2 >> 27) & 0x1F);
    if scratch > 1 {
        return Err(UsbBotError::Cap);
    }
    if scratch != 0 {
        zero_page(hw, mem.scratch_array);
        zero_page(hw, mem.scratch0);
        let mut ptr = [0u8; 8];
        put_u64(&mut ptr, 0, mem.scratch0);
        hw.dma_write(mem.scratch_array, &ptr);
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
    hw.write32(caps.rt + 0x20, 1 << 1);
    hw.write32(usbcmd, USBCMD_RS | USBCMD_INTE);
    if !wait_clear(hw, usbsts, USBSTS_HCH) {
        return Err(UsbBotError::Reset);
    }
    let ports = core::cmp::min(caps.max_ports, 16);
    // PED is RW1CS: never write the previous PORTSC word back (that clears PED).
    for port in 1..=ports {
        hw.write32(portsc_off(caps.op, port), PORTSC_PP | PORTSC_CSC);
    }
    for port in 1..=ports {
        let rst = if port_is_usb3(hw, port) {
            PORTSC_WPR
        } else {
            PORTSC_PR
        };
        hw.write32(
            portsc_off(caps.op, port),
            PORTSC_PP | rst | PORTSC_CSC | PORTSC_PRC | PORTSC_WRC,
        );
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
            if sc & (PORTSC_CCS | PORTSC_PED) != 0 {
                saw_ccs = true;
                break;
            }
        }
        if saw_ccs {
            break;
        }
    }
    // last_cmpl: max_slots | max_ports<<8 | caplength<<16 so a fail names HCSPARAMS.
    // last_portsc: port 1 (often USB3) | last implemented port (often USB2).
    store_usb_bot_diag(
        UsbBotError::Reset,
        0,
        u64::from(p1) | (u64::from(p_last) << 32),
        u64::from(caps.max_slots) | (u64::from(caps.max_ports) << 8) | (u64::from(caps.op) << 16),
    );
    if !saw_ccs {
        return Err(UsbBotError::Reset);
    }
    Ok((caps, Ring::new(mem.cmd), EventRing::new(mem.evt)))
}

fn speed_from_portsc(portsc: u32) -> u8 {
    ((portsc >> 10) & 0xF) as u8
}

fn ep0_max_packet(speed: u8) -> u16 {
    match speed {
        2 => 8,
        4 | 5 => 512,
        _ => 64,
    }
}

fn reset_port(hw: &mut impl XhciHw, caps: &XhciCaps, port: u8) -> Result<u32, UsbBotError> {
    let off = portsc_off(caps.op, port);
    let mut sc = hw.read32(off);
    store_usb_bot_diag(UsbBotError::Reset, 0, u64::from(sc), u64::from(port));
    if sc & PORTSC_CCS == 0 {
        return Err(UsbBotError::Reset);
    }
    if sc & PORTSC_PP == 0 {
        hw.write32(off, PORTSC_PP | PORTSC_CSC);
        sc = hw.read32(off);
    }
    let rst = if port_is_usb3(hw, port) {
        PORTSC_WPR
    } else {
        PORTSC_PR
    };
    hw.write32(off, PORTSC_PP | rst | PORTSC_CSC | PORTSC_PRC | PORTSC_WRC);
    if !wait_set(hw, off, PORTSC_PRC) && !wait_set(hw, off, PORTSC_WRC) {
        return Err(UsbBotError::Reset);
    }
    sc = hw.read32(off);
    hw.write32(off, sc | PORTSC_PRC | PORTSC_WRC | PORTSC_CSC);
    sc = hw.read32(off);
    store_usb_bot_diag(UsbBotError::Reset, 0, u64::from(sc), u64::from(port));
    if sc & PORTSC_PED == 0 {
        return Err(UsbBotError::Reset);
    }
    Ok(sc)
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
    consume_event(hw, caps, ev, TRB_EVENT_TRANSFER)?;
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
    consume_event(hw, caps, ev, TRB_EVENT_TRANSFER)?;
    Ok(())
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
            iface_ok = cfg[i + 5] == 8 && cfg[i + 6] == 6 && cfg[i + 7] == 0x50;
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
    bulk_out: Ring,
    bulk_in: Ring,
    slot: u8,
    dci_out: u8,
    dci_in: u8,
    bounce: u64,
    lba: u32,
    tag: u32,
}

impl UsbBulk for LiveXhci {
    fn bulk_out(&mut self, data: &[u8]) -> Result<(), UsbBotError> {
        if data.is_empty() || data.len() > 4096 {
            return Err(UsbBotError::Xfer);
        }
        let mut hw = MmioXhci { base: self.mmio };
        hw.dma_write(self.bounce, data);
        self.bulk_out.place(
            &mut hw,
            self.bounce,
            data.len() as u32,
            trb_ctrl(0, TRB_NORMAL, TRB_IOC),
        );
        doorbell(&mut hw, self.caps.db, self.slot, self.dci_out);
        consume_event(&mut hw, &self.caps, &mut self.ev, TRB_EVENT_TRANSFER)?;
        Ok(())
    }

    fn bulk_in(&mut self, data: &mut [u8]) -> Result<usize, UsbBotError> {
        if data.is_empty() || data.len() > 4096 {
            return Err(UsbBotError::Xfer);
        }
        let mut hw = MmioXhci { base: self.mmio };
        let z = [0u8; 4096];
        hw.dma_write(self.bounce, &z[..data.len()]);
        self.bulk_in.place(
            &mut hw,
            self.bounce,
            data.len() as u32,
            trb_ctrl(0, TRB_NORMAL, TRB_IOC),
        );
        doorbell(&mut hw, self.caps.db, self.slot, self.dci_in);
        consume_event(&mut hw, &self.caps, &mut self.ev, TRB_EVENT_TRANSFER)?;
        hw.dma_read(self.bounce, data);
        Ok(data.len())
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
    let sc = reset_port(hw, caps, port)?;
    let speed = speed_from_portsc(sc);
    let ev_en = cmd(hw, caps, cmd_ring, ev, 0, trb_ctrl(0, TRB_ENABLE_SLOT, 0))?;
    let slot = event_slot(&ev_en);
    if slot == 0 {
        return Err(UsbBotError::Enum);
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
    put_u32(&mut inctx, cs, (u32::from(speed) << 20) | (1u32 << 27));
    put_u32(&mut inctx, cs + 4, u32::from(port) << 24);
    let mps0 = u32::from(ep0_max_packet(speed));
    put_u32(
        &mut inctx,
        cs * 2 + 4,
        (4u32 << 3) | (3 << 1) | (mps0 << 16),
    );
    put_u64(&mut inctx, cs * 2 + 8, mem.ep0 | 1);
    hw.dma_write(mem.inctx, &inctx);
    cmd(
        hw,
        caps,
        cmd_ring,
        ev,
        mem.inctx,
        trb_ctrl(0, TRB_ADDRESS_DEV, u32::from(slot) << 24),
    )?;
    let mut ep0 = Ring::new(mem.ep0);
    let mut dev = [0u8; 18];
    control_in(
        hw,
        caps,
        &mut ep0,
        ev,
        slot,
        mem.bounce,
        setup_get_desc(1, 18),
        &mut dev,
    )?;
    let mut cfg9 = [0u8; 9];
    control_in(
        hw,
        caps,
        &mut ep0,
        ev,
        slot,
        mem.bounce,
        setup_get_desc(2, 9),
        &mut cfg9,
    )?;
    let total = u16::from_le_bytes([cfg9[2], cfg9[3]]).min(256);
    let mut cfg = [0u8; 256];
    control_in(
        hw,
        caps,
        &mut ep0,
        ev,
        slot,
        mem.bounce,
        setup_get_desc(2, total),
        &mut cfg[..total as usize],
    )?;
    let Some((ep_out, ep_in, mps_out, mps_in, cfg_val)) = parse_bot_eps(&cfg[..total as usize])
    else {
        let _ = cmd(
            hw,
            caps,
            cmd_ring,
            ev,
            0,
            trb_ctrl(0, TRB_DISABLE_SLOT, u32::from(slot) << 24),
        );
        return Err(UsbBotError::Bot);
    };
    let dci_out = ep_out * 2;
    let dci_in = ep_in * 2 + 1;
    let hi = core::cmp::max(dci_out, dci_in);
    zero_page(hw, mem.inctx);
    let mut ic = [0u8; 4096];
    put_u32(&mut ic, 4, 1 | (1 << dci_out) | (1 << dci_in));
    put_u32(
        &mut ic,
        cs,
        (u32::from(speed) << 20) | (u32::from(hi) << 27),
    );
    put_u32(&mut ic, cs + 4, u32::from(port) << 24);
    put_u32(&mut ic, cs * 2 + 4, (4u32 << 3) | (3 << 1) | (mps0 << 16));
    put_u64(&mut ic, cs * 2 + 8, mem.ep0 | 1);
    let out_off = cs * (1 + dci_out as usize);
    put_u32(
        &mut ic,
        out_off + 4,
        (2u32 << 3) | (3 << 1) | (u32::from(mps_out) << 16),
    );
    put_u64(&mut ic, out_off + 8, mem.bulk_out | 1);
    let in_off = cs * (1 + dci_in as usize);
    put_u32(
        &mut ic,
        in_off + 4,
        (6u32 << 3) | (3 << 1) | (u32::from(mps_in) << 16),
    );
    put_u64(&mut ic, in_off + 8, mem.bulk_in | 1);
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
    )?;
    control_nodata(hw, caps, &mut ep0, ev, slot, setup_set_config(cfg_val))?;
    let mut live = LiveXhci {
        mmio,
        caps: *caps,
        ev: EventRing {
            base: ev.base,
            deq: ev.deq,
            cycle: ev.cycle,
        },
        bulk_out: Ring::new(mem.bulk_out),
        bulk_in: Ring::new(mem.bulk_in),
        slot,
        dci_out,
        dci_in,
        bounce: mem.bounce,
        lba: 512,
        tag: 10,
    };
    match usb_bot_bring_up(&mut live, min_bytes) {
        Ok((bytes, lba)) => {
            if lun_is_esp_cruzer(bytes) {
                let _ = cmd(
                    hw,
                    caps,
                    cmd_ring,
                    ev,
                    0,
                    trb_ctrl(0, TRB_DISABLE_SLOT, u32::from(slot) << 24),
                );
                return Err(UsbBotError::Cruzer);
            }
            live.lba = lba;
            live.tag = next_bot_tag();
            store_usb_bot_ready(bytes, lba);
            Ok(live)
        }
        Err(e) => {
            let _ = cmd(
                hw,
                caps,
                cmd_ring,
                ev,
                0,
                trb_ctrl(0, TRB_DISABLE_SLOT, u32::from(slot) << 24),
            );
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
    let mut last = UsbBotError::Enum;
    let ports = core::cmp::min(caps.max_ports, 16);
    for port in 1..=ports {
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
            Err(UsbBotError::Cruzer) => last = UsbBotError::Cruzer,
            Err(UsbBotError::TooSmall) => {
                if last != UsbBotError::Cruzer {
                    last = UsbBotError::TooSmall;
                }
            }
            Err(e) => {
                if last != UsbBotError::Cruzer && last != UsbBotError::TooSmall {
                    last = e;
                }
            }
        }
    }
    Err(last)
}

#[repr(C, align(4096))]
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
static mut SCRATCH0: Page = Page([0; 4096]);

#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
static mut LIVE: Option<LiveXhci> = None;
#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
static LIVE_LOCK: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);

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
                    if r.is_ok() && !write {
                        buf.copy_from_slice(&slice[..buf.len()]);
                    }
                    r.is_ok()
                }
            }
            None => false,
        }
    };
    LIVE_LOCK.store(false, core::sync::atomic::Ordering::Release);
    ok
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
    }
}
