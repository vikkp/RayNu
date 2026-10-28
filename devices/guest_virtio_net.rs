//! Guest virtio-net for an accepted Start (outside Proven Core).
//!
//! Pillar: [Z] [D]
//! Proven Core: **outside** (ADR-002 / ADR-013 Appendix C).
//! VERIFICATION: L1 host tests. Host/CI never print an iron marker.
//!
//! PCI `00:05.0`, virtio-net `1AF4:1041`, only while a Start carries a
//! saved NIC. The MAC in device config is that guest MAC. TX frames are
//! handed to the function-1 egress path. This is not the M4 vSwitch.

use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};

use crate::devices::guest_platform::{pci_bdf, pci_cfg_offset};
use crate::mgmt::guest_nic_start::armed_start;

pub const GUEST_NET_PCI_DEV: u8 = 5;
pub const GUEST_NET_PCI_FN: u8 = 0;
pub const GUEST_NET_VENDOR: u16 = 0x1AF4;
pub const GUEST_NET_DEVICE: u16 = 0x1041;
pub const GUEST_NET_BAR: u32 = 0xFE00_3000;
const BAR_MASK: u32 = 0xFFFF_F000;
/// Ring size for both queues. Linux `check_sq_full_and_disable` stops the
/// TX queue whenever `num_free < 2 + MAX_SKB_FRAGS` (19) and only
/// `virtnet_poll_tx` / `skb_xmit_done` wake it, again at `>= 19` free.
/// Iron `c9f4451a`: with 8 entries the queue stopped on the first frame
/// and could never wake — `NETDEV WATCHDOG: ... transmit queue 0 timed
/// out` every 5 s while IRQ 11, the EOIs and the PIT were all healthy
/// (`take11=21 eoi_s=21 take0=149`). 256 is what QEMU exposes.
const QSIZE: u16 = 256;
/// Longest descriptor chain walked per buffer: header + linear + 17 frags
/// fits, and a malformed `next` loop still ends.
const CHAIN_MAX: usize = 32;
const OFF_NOTIFY: u16 = 0x300;
const OFF_DEVICE: u16 = 0x200;
const VIRTIO_F_VERSION_1: u64 = 1 << 32;
const VIRTIO_NET_F_MAC: u64 = 1 << 5;
/// Device config carries a `status` word; the driver reads it at probe and
/// on every config-change interrupt (`virtnet_config_changed_work`) and
/// calls `netif_carrier_on/off`. ADR-021 D6: the bit follows the `:39`
/// wire once RayNu-V drives that port, so a dark cable is `NO-CARRIER`
/// in the guest instead of frames that go nowhere (iron `d7e2e24f`).
const VIRTIO_NET_F_STATUS: u64 = 1 << 16;
const FEATURES: u64 = VIRTIO_F_VERSION_1 | VIRTIO_NET_F_MAC | VIRTIO_NET_F_STATUS;
const VIRTIO_NET_S_LINK_UP: u16 = 1;
/// Device config layout (virtio 1.x `virtio_net_config`): MAC at 0,
/// `status` at 6. Our config length is 12 (`cfg_dword(0x80)`).
const DEVCFG_STATUS_OFF: u16 = 6;
/// ISR bit 1: configuration change (bit 0 is a queue interrupt).
const ISR_CONFIG: u8 = 0x2;
const STATUS_DRIVER_OK: u8 = 4;
/// Linux 7 `virtnet_probe`: `VIRTIO_F_VERSION_1` selects
/// `sizeof(virtio_net_hdr_mrg_rxbuf)` even when `MRG_RXBUF` is off.
/// Iron `272c6db6` stripped 10. Those two header bytes became the
/// destination MAC, the gateway never answered, and Stop sampled
/// `poll_idle` (`pause` then `sub $1,%eax`, linked `0xffffffff8271c5e8`).
const VNET_HDR_LEN: usize = 12;

#[derive(Clone, Copy)]
struct Queue {
    enable: u16,
    desc: u64,
    avail: u64,
    used: u64,
    last_avail: u16,
    used_idx: u16,
}

struct Net {
    bar: u32,
    sizing: bool,
    cmd: u16,
    feat_sel: u32,
    drv_sel: u32,
    drv_feat: u64,
    status: u8,
    isr: u8,
    /// Common cfg `config_generation` (0x15); bumps on each carrier change.
    cfg_gen: u8,
    qsel: u16,
    q: [Queue; 2],
    notify: bool,
    logged: bool,
}

static mut NET: Net = Net {
    bar: GUEST_NET_BAR,
    sizing: false,
    cmd: 0,
    feat_sel: 0,
    drv_sel: 0,
    drv_feat: 0,
    status: 0,
    isr: 0,
    cfg_gen: 0,
    qsel: 0,
    q: [Queue {
        enable: 0,
        desc: 0,
        avail: 0,
        used: 0,
        last_avail: 0,
        used_idx: 0,
    }; 2],
    notify: false,
    logged: false,
};

static LOGGED_TX: AtomicBool = AtomicBool::new(false);
/// 1 when a TX used-ring update is waiting for the next VM exit.
///
/// Iron `03906000`: `enp0s5` printed `NETDEV WATCHDOG` on virtqueue 1.
/// The completion IRQ was armed only for a preemption-timer exit, and the
/// guest was leaving on serial I/O, so the driver never reaped the packet.
static TX_IRQ_ARM: AtomicU8 = AtomicU8::new(0);
/// 1 when a config-change interrupt (carrier flip) waits for the next exit.
static CFG_IRQ_ARM: AtomicU8 = AtomicU8::new(0);
static LOGGED_KICK_IDLE: AtomicBool = AtomicBool::new(false);
static LOGGED_RX: AtomicBool = AtomicBool::new(false);
static LOGGED_RX_IDLE: AtomicBool = AtomicBool::new(false);
static TX_DONE_N: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);
static TX_SENT_N: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);
static TX_HELD_N: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);
static RX_POSTED_N: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);

/// Frames parsed, frames posted to RX, frames the NIC accepted, frames held.
pub fn net_counts() -> (u32, u32, u32, u32) {
    (
        TX_DONE_N.load(Ordering::Acquire),
        RX_POSTED_N.load(Ordering::Acquire),
        TX_SENT_N.load(Ordering::Acquire),
        TX_HELD_N.load(Ordering::Acquire),
    )
}

/// virtio-net ISR byte the guest has not read yet, and whether a TX/RX
/// completion is still waiting for its raise on the next exit.
/// M8.9 post-TX trace.
pub fn irq_state() -> (u8, bool) {
    (
        with_net(|n| n.isr),
        TX_IRQ_ARM.load(Ordering::Acquire) == 1,
    )
}

fn with_net<R>(f: impl FnOnce(&mut Net) -> R) -> R {
    // SAFETY: BSP guest-exit path and single-threaded host tests.
    // KANI-TARGET: virtio-net PCI config bytes (outside Proven Core).
    unsafe { f(&mut *core::ptr::addr_of_mut!(NET)) }
}

pub fn pci_write_addr(addr: u32) {
    let _ = addr;
}

pub fn pci_selects(addr: u32) -> bool {
    if addr & 0x8000_0000 == 0 || armed_start().is_none() {
        return false;
    }
    #[cfg(feature = "uefi-bin")]
    if crate::devices::guest_virtio_blk::pei_host_bridge_did() {
        return false;
    }
    let (bus, dev, fun, _) = pci_bdf(addr);
    bus == 0 && dev == GUEST_NET_PCI_DEV && fun == GUEST_NET_PCI_FN
}

pub fn pci_read_data(port: u16, size: u8) -> u32 {
    let addr = crate::devices::ide_cdrom::pci_read_addr();
    if !pci_selects(addr) {
        return 0xFFFF_FFFF;
    }
    note_visible();
    let off = pci_cfg_offset(addr, port);
    shift(cfg_dword(off & 0xFC), off, size)
}

pub fn pci_write_data(port: u16, size: u8, val: u32) {
    let addr = crate::devices::ide_cdrom::pci_read_addr();
    if !pci_selects(addr) {
        return;
    }
    let off = pci_cfg_offset(addr, port);
    with_net(|n| {
        if off == 0x04 {
            n.cmd = val as u16;
        } else if off == 0x10 {
            if val == 0xFFFF_FFFF {
                n.sizing = true;
            } else {
                n.sizing = false;
                let next = val & BAR_MASK;
                n.bar = if next == 0 { GUEST_NET_BAR } else { next };
            }
        }
        let _ = size;
    });
}

fn cfg_dword(off: u8) -> u32 {
    with_net(|n| match off {
        0x00 => u32::from(GUEST_NET_VENDOR) | (u32::from(GUEST_NET_DEVICE) << 16),
        0x04 => u32::from(n.cmd) | 0x0010_0000,
        0x08 => 0x0200_0001,
        0x0C => 0,
        0x10 => {
            if n.sizing {
                BAR_MASK
            } else {
                n.bar
            }
        }
        0x2C => u32::from(GUEST_NET_VENDOR) | (1u32 << 16),
        0x34 => 0x40,
        // Pin 0 leaves pci_dev->irq at 0. virtio-pci then skips INTx and
        // virtio_net probe returns -22. virtio-blk, which probed on this
        // boot, uses the same line with pin INTA. Iron 33c9b141.
        0x3C => u32::from(crate::devices::guest_irq::VIRTIO_PIC_IRQ) | 0x0000_0100,
        0x40 => 0x0110_5009,
        0x44 => 0,
        0x48 => 0,
        0x4C => 0x38,
        0x50 => 0x0214_6409,
        0x54 => 0,
        0x58 => u32::from(OFF_NOTIFY),
        0x5C => 4,
        0x60 => 0,
        0x64 => 0x0310_7409,
        0x68 => 0,
        0x6C => 0x100,
        0x70 => 4,
        0x74 => 0x0410_0009,
        0x78 => 0,
        0x7C => u32::from(OFF_DEVICE),
        0x80 => 12,
        _ => 0,
    })
}

fn shift(dword: u32, off: u8, size: u8) -> u32 {
    let shifted = dword >> ((off & 3) * 8);
    match size {
        1 => shifted & 0xff,
        2 => shifted & 0xffff,
        _ => shifted,
    }
}

pub fn covers(gpa: u64) -> bool {
    if armed_start().is_none() {
        return false;
    }
    let bar = with_net(|n| u64::from(n.bar & BAR_MASK));
    gpa >= bar && gpa < bar + 0x1000
}

pub fn bar_gpa() -> Option<u64> {
    if armed_start().is_none() {
        None
    } else {
        Some(with_net(|n| u64::from(n.bar & BAR_MASK)))
    }
}

pub fn mmio_read(gpa: u64, size: u8) -> u64 {
    let bar = with_net(|n| u64::from(n.bar & BAR_MASK));
    let off = (gpa.wrapping_sub(bar)) as u16;
    if off < 0x38 {
        return common_read(off, size);
    }
    if off == 0x100 {
        return with_net(|n| {
            let v = u64::from(n.isr);
            n.isr = 0;
            // Level INTx stays asserted until this ack. Leaving IRQ 11
            // pending made every inject take the PIC and starved the
            // LAPIC timer (iron `9cdf496c`, `sleep 1` then no `exit:`).
            if v != 0 {
                crate::devices::guest_irq::lower_virtio();
            }
            v
        });
    }
    if (OFF_DEVICE..OFF_DEVICE + 6).contains(&off) {
        let mac = armed_start().map(|n| n.mac).unwrap_or([0; 6]);
        let i = (off - OFF_DEVICE) as usize;
        return u64::from(mac[i]);
    }
    if off == OFF_DEVICE + DEVCFG_STATUS_OFF {
        return u64::from(link_status_word()) & if size == 1 { 0xff } else { 0xffff };
    }
    if off == OFF_DEVICE + DEVCFG_STATUS_OFF + 1 {
        return u64::from(link_status_word() >> 8);
    }
    0
}

/// `virtio_net_config.status`: `VIRTIO_NET_S_LINK_UP` while the guest LAN
/// port has carrier (or is not yet driven, see `host_link::guest_carrier`).
pub fn link_status_word() -> u16 {
    if crate::mgmt::host_link::guest_carrier() {
        VIRTIO_NET_S_LINK_UP
    } else {
        0
    }
}

/// The `:39` link flipped while a Start is armed: mark a config change so
/// the driver re-reads `status` and moves the carrier. The interrupt is
/// raised on the next VM exit like a TX completion.
pub fn note_carrier_change() {
    if armed_start().is_none() {
        return;
    }
    with_net(|n| {
        n.cfg_gen = n.cfg_gen.wrapping_add(1);
        if n.status & STATUS_DRIVER_OK != 0 {
            n.isr |= ISR_CONFIG;
            CFG_IRQ_ARM.store(1, Ordering::Release);
        }
    });
}

pub fn mmio_write(gpa: u64, size: u8, val: u64) {
    let bar = with_net(|n| u64::from(n.bar & BAR_MASK));
    let off = (gpa.wrapping_sub(bar)) as u16;
    let n = match size {
        1 => 1u16,
        2 => 2,
        4 => 4,
        8 => 8,
        _ => 4,
    };
    let mut i = 0u16;
    while i < n {
        write_byte(off.wrapping_add(i), (val >> (8 * i)) as u8);
        i += 1;
    }
}

fn write_byte(off: u16, val: u8) {
    with_net(|n| {
        if off < 0x38 {
            write_common(n, off, val);
        } else if off == OFF_NOTIFY || off == OFF_NOTIFY + 1 {
            n.notify = true;
        }
    });
}

fn write_common(n: &mut Net, off: u16, val: u8) {
    let shift = (off & 3) * 8;
    let q = (n.qsel as usize).min(1);
    match off {
        0x00..=0x03 => n.feat_sel = (n.feat_sel & !(0xff << shift)) | (u32::from(val) << shift),
        0x08..=0x0B => n.drv_sel = (n.drv_sel & !(0xff << shift)) | (u32::from(val) << shift),
        0x0C..=0x0F => {
            let bit = 8 * (off - 0x0C);
            n.drv_feat = (n.drv_feat & !(0xffu64 << bit)) | (u64::from(val) << bit);
        }
        0x14 => {
            n.status = val;
            if val == 0 {
                n.isr = 0;
                n.notify = false;
                n.qsel = 0;
                n.q = [Queue {
                    enable: 0,
                    desc: 0,
                    avail: 0,
                    used: 0,
                    last_avail: 0,
                    used_idx: 0,
                }; 2];
            } else if val & STATUS_DRIVER_OK != 0 {
                n.logged = true;
                note_tx_queue(n);
            }
        }
        0x16 => n.qsel = u16::from(val),
        0x17 => n.qsel = (n.qsel & 0xff) | (u16::from(val) << 8),
        0x1C => n.q[q].enable = u16::from(val),
        0x1D => n.q[q].enable = (n.q[q].enable & 0xff) | (u16::from(val) << 8),
        0x20..=0x27 => {
            let bit = 8 * (off - 0x20);
            n.q[q].desc = (n.q[q].desc & !(0xffu64 << bit)) | (u64::from(val) << bit);
        }
        0x28..=0x2F => {
            let bit = 8 * (off - 0x28);
            n.q[q].avail = (n.q[q].avail & !(0xffu64 << bit)) | (u64::from(val) << bit);
        }
        0x30..=0x37 => {
            let bit = 8 * (off - 0x30);
            n.q[q].used = (n.q[q].used & !(0xffu64 << bit)) | (u64::from(val) << bit);
        }
        _ => {}
    }
}

fn common_read(off: u16, size: u8) -> u64 {
    let n = match size {
        1 => 1u16,
        2 => 2,
        4 => 4,
        8 => 8,
        _ => 4,
    };
    let mut val = 0u64;
    let mut i = 0u16;
    while i < n {
        val |= u64::from(common_byte(off.wrapping_add(i))) << (8 * i);
        i += 1;
    }
    val
}

fn common_byte(off: u16) -> u8 {
    with_net(|n| {
        let q = (n.qsel as usize).min(1);
        let feat = if n.feat_sel == 0 {
            FEATURES as u32
        } else if n.feat_sel == 1 {
            (FEATURES >> 32) as u32
        } else {
            0
        };
        match off {
            0x00..=0x03 => (n.feat_sel >> (8 * (off - 0x00))) as u8,
            0x04..=0x07 => (feat >> (8 * (off - 0x04))) as u8,
            0x08..=0x0B => (n.drv_sel >> (8 * (off - 0x08))) as u8,
            0x0C..=0x0F => (n.drv_feat >> (8 * (off - 0x0C))) as u8,
            0x10 | 0x11 => 0xFF,
            0x12 => 2,
            0x13 => 0,
            0x14 => n.status,
            0x15 => n.cfg_gen,
            0x16 => n.qsel as u8,
            0x17 => (n.qsel >> 8) as u8,
            0x18 => QSIZE as u8,
            0x19 => (QSIZE >> 8) as u8,
            0x1A | 0x1B => 0xFF,
            0x1C => n.q[q].enable as u8,
            0x1D => (n.q[q].enable >> 8) as u8,
            0x20..=0x27 => (n.q[q].desc >> (8 * (off - 0x20))) as u8,
            0x28..=0x2F => (n.q[q].avail >> (8 * (off - 0x28))) as u8,
            0x30..=0x37 => (n.q[q].used >> (8 * (off - 0x30))) as u8,
            _ => 0,
        }
    })
}

fn note_tx_queue(n: &Net) {
    #[cfg(feature = "uefi-bin")]
    {
        static ONCE: AtomicBool = AtomicBool::new(false);
        if ONCE.swap(true, Ordering::AcqRel) {
            return;
        }
        let q = &n.q[1];
        crate::boot::serial::write_str("boot: M8.9 virtio-net q1 en=");
        crate::boot::serial::write_str(if q.enable != 0 { "1" } else { "0" });
        crate::boot::serial::write_str(" desc=0x");
        write_hex_u64(q.desc);
        crate::boot::serial::write_str(" avail=0x");
        write_hex_u64(q.avail);
        crate::boot::serial::write_str(" used=0x");
        write_hex_u64(q.used);
        crate::boot::serial::write_line(" (not ISO-INSTALL-OK)");
    }
    #[cfg(not(feature = "uefi-bin"))]
    {
        let _ = n;
    }
}

#[cfg(feature = "uefi-bin")]
fn write_hex_u64(mut n: u64) {
    const H: &[u8; 16] = b"0123456789abcdef";
    let mut buf = [b'0'; 16];
    for i in (0..16).rev() {
        buf[i] = H[(n & 0xf) as usize];
        n >>= 4;
    }
    for b in buf {
        crate::boot::serial::write_byte(b);
    }
}

fn note_visible() {
    with_net(|n| {
        if n.logged {
            return;
        }
        n.logged = true;
        #[cfg(feature = "uefi-bin")]
        {
            if let Some(nic) = armed_start() {
                crate::boot::serial::write_str("boot: M8.9 virtio-net mac=");
                for (i, b) in nic.mac.iter().enumerate() {
                    if i > 0 {
                        crate::boot::serial::write_byte(b':');
                    }
                    write_hex(*b);
                }
                crate::boot::serial::write_str(" ip=");
                let mut tok = [0u8; 96];
                let n = crate::mgmt::guest_addr::copy_token(&mut tok);
                for &b in &tok[..n] {
                    crate::boot::serial::write_byte(b);
                }
                crate::boot::serial::write_line(" (not the SPA MAC)");
            }
        }
    });
}

#[cfg(feature = "uefi-bin")]
fn write_hex(b: u8) {
    const H: &[u8; 16] = b"0123456789abcdef";
    crate::boot::serial::write_byte(H[(b >> 4) as usize]);
    crate::boot::serial::write_byte(H[(b & 0xf) as usize]);
}

/// Pull published TX descriptor chains (queue 1) and hand each Ethernet
/// frame to egress.
///
/// A notify with no new avail idx is not a buffer. Queue 0 (RX) and queue 1
/// share one notify flag. Iron `e79f7e83`: bringing `enp0s5` up kicked RX,
/// the next connect never printed `M8.9`, and the shell stopped taking
/// Ctrl+C until SPA Stop. Completing a TX slot the guest had not published
/// left the real packet unfinished.
pub fn drain(translate: impl Fn(u64) -> Option<u64>) {
    if armed_start().is_none() {
        return;
    }
    for _ in 0..usize::from(QSIZE) {
        if !drain_one(&translate) {
            break;
        }
    }
}

fn drain_one(translate: &impl Fn(u64) -> Option<u64>) -> bool {
    let (enabled, desc, avail) = with_net(|n| {
        let q = &n.q[1];
        (q.enable != 0, q.desc, q.avail)
    });
    if !enabled || desc == 0 || avail == 0 {
        with_net(|n| {
            if n.notify && !LOGGED_KICK_IDLE.swap(true, Ordering::AcqRel) {
                #[cfg(feature = "uefi-bin")]
                {
                    crate::boot::serial::write_str("boot: M8.9 virtio-net tx q1 idle en=");
                    crate::boot::serial::write_str(if enabled { "1" } else { "0" });
                    crate::boot::serial::write_str(" desc=0x");
                    write_hex_u64(desc);
                    crate::boot::serial::write_str(" avail=0x");
                    write_hex_u64(avail);
                    crate::boot::serial::write_line(" (not ISO-INSTALL-OK)");
                }
            }
            n.notify = false;
        });
        return false;
    }
    let Some(avail_hpa) = translate(avail) else {
        return false;
    };
    // SAFETY: translate returned a mapped guest page for this avail ring.
    // KANI-TARGET: virtio avail idx (outside Proven Core).
    let idx = unsafe { core::ptr::read_unaligned((avail_hpa as *const u16).add(1)) };
    let last = with_net(|n| n.q[1].last_avail);
    if idx == last {
        with_net(|n| n.notify = false);
        return false;
    }
    let slot = last % QSIZE;
    let Some(ent_hpa) = translate(avail.wrapping_add(4 + u64::from(slot) * 2)) else {
        return false;
    };
    let head = unsafe { core::ptr::read_unaligned(ent_hpa as *const u16) };
    let mut frame = [0u8; VNET_HDR_LEN + 1514];
    let mut flen = 0usize;
    let mut unmapped = false;
    let mut di = head % QSIZE;
    for _ in 0..CHAIN_MAX {
        let Some(d_hpa) = translate(desc.wrapping_add(u64::from(di) * 16)) else {
            unmapped = true;
            break;
        };
        let addr = unsafe { core::ptr::read_unaligned(d_hpa as *const u64) };
        let len = unsafe { core::ptr::read_unaligned((d_hpa as *const u32).add(2)) } as usize;
        let flags = unsafe { core::ptr::read_unaligned((d_hpa as *const u16).add(6)) };
        let next = unsafe { core::ptr::read_unaligned((d_hpa as *const u16).add(7)) };
        if flags & 2 == 0 && len > 0 {
            let Some(hpa) = translate(addr) else {
                unmapped = true;
                break;
            };
            let take = len.min(frame.len().saturating_sub(flen));
            unsafe {
                core::ptr::copy_nonoverlapping(hpa as *const u8, frame.as_mut_ptr().add(flen), take);
            }
            flen += take;
        }
        if flags & 1 == 0 {
            break;
        }
        di = next % QSIZE;
    }
    let (used_base, used_slot, used_idx) = with_net(|n| {
        let q = &mut n.q[1];
        let slot = q.used_idx % QSIZE;
        q.used_idx = q.used_idx.wrapping_add(1);
        q.last_avail = last.wrapping_add(1);
        n.notify = false;
        n.isr = 1;
        (q.used, slot, q.used_idx)
    });
    if used_base != 0 {
        if let Some(hpa) = translate(used_base.wrapping_add(4 + u64::from(used_slot) * 8)) {
            // SAFETY: translate returned the used-ring element for this slot.
            // KANI-TARGET: virtio-net used ring id/len (outside Proven Core).
            unsafe {
                core::ptr::write_unaligned(hpa as *mut u32, u32::from(head));
                core::ptr::write_unaligned((hpa as *mut u32).add(1), flen as u32);
            }
        }
        if let Some(hpa) = translate(used_base.wrapping_add(2)) {
            // SAFETY: translate returned the used.idx halfword.
            // KANI-TARGET: virtio-net used idx (outside Proven Core).
            unsafe {
                core::ptr::write_unaligned(hpa as *mut u16, used_idx);
            }
        }
    }
    // Deliver on the next VM exit, after this kick has returned.
    TX_IRQ_ARM.store(1, Ordering::Release);
    if unmapped {
        #[cfg(feature = "uefi-bin")]
        if !LOGGED_TX.swap(true, Ordering::AcqRel) {
            crate::boot::serial::write_line(
                "boot: M8.9 virtio-net tx unmapped (slot completed; not ISO-INSTALL-OK)",
            );
        }
        return true;
    }
    let Some(eth) = ethernet_from_virtio(&frame[..flen]) else {
        return true;
    };
    TX_DONE_N.fetch_add(1, Ordering::AcqRel);
    #[cfg(feature = "uefi-bin")]
    {
        static DONE: AtomicBool = AtomicBool::new(false);
        if !DONE.swap(true, Ordering::AcqRel) {
            crate::boot::serial::write_line(
                "boot: M8.9 virtio-net tx done (not ISO-INSTALL-OK)",
            );
        }
    }
    let result = crate::mgmt::bcm5720_mmio::egress_send(eth);
    match result {
        crate::mgmt::bcm5720_mmio::EgressSend::Sent(_) => {
            TX_SENT_N.fetch_add(1, Ordering::AcqRel);
        }
        crate::mgmt::bcm5720_mmio::EgressSend::Held => {
            TX_HELD_N.fetch_add(1, Ordering::AcqRel);
        }
    }
    if !LOGGED_TX.swap(true, Ordering::AcqRel) {
        #[cfg(feature = "uefi-bin")]
        {
            match result {
                crate::mgmt::bcm5720_mmio::EgressSend::Sent(n) => {
                    crate::boot::serial::write_str("boot: M8.9 guest frame leaves :39 len=");
                    crate::boot::serial::write_line("sent");
                    let _ = n;
                }
                crate::mgmt::bcm5720_mmio::EgressSend::Held => {
                    crate::boot::serial::write_line(
                        "boot: M8.9 egress tx held (phylock stays with iDRAC; SPA port stays)",
                    );
                }
            }
        }
        #[cfg(not(feature = "uefi-bin"))]
        {
            let _ = result;
        }
    }
    true
}

/// Raise IRQ 11 for a TX completion whose kick has already returned.
///
/// Call at VM-exit entry. The notify exit arms the flag after this has
/// already run, so the raise happens on the following exit.
pub fn raise_deferred_tx_irq(now: u64) {
    let _ = now;
    let tx = TX_IRQ_ARM.swap(0, Ordering::AcqRel) == 1;
    let cfg = CFG_IRQ_ARM.swap(0, Ordering::AcqRel) == 1;
    if tx || cfg {
        crate::devices::guest_irq::raise_virtio();
    }
}

fn ethernet_from_virtio(buf: &[u8]) -> Option<&[u8]> {
    let eth = buf.get(VNET_HDR_LEN..)?;
    if eth.len() < 14 {
        None
    } else {
        Some(eth)
    }
}

/// Pull frames the `:39` port has DMA'd and post them on virtio queue 0.
///
/// Call from the Linux VM-exit path. `poll_idle` does not touch the BAR,
/// so a drain that runs only on virtio MMIO never sees the SYN-ACK.
pub fn poll_rx(translate: impl Fn(u64) -> Option<u64>) {
    if armed_start().is_none() {
        return;
    }
    for _ in 0..4 {
        let mut buf = [0u8; 1514];
        let Some(n) = crate::mgmt::bcm5720_mmio::egress_recv(&mut buf) else {
            break;
        };
        if n < 14 {
            continue;
        }
        if !post_rx(&buf[..n], &translate) {
            #[cfg(feature = "uefi-bin")]
            if !LOGGED_RX_IDLE.swap(true, Ordering::AcqRel) {
                crate::boot::serial::write_line(
                    "boot: M8.9 virtio-net rx idle (not ISO-INSTALL-OK)",
                );
            }
            break;
        }
        #[cfg(feature = "uefi-bin")]
        if !LOGGED_RX.swap(true, Ordering::AcqRel) {
            crate::boot::serial::write_line("boot: M8.9 virtio-net rx (not ISO-INSTALL-OK)");
        }
    }
}

/// Write one Ethernet frame into the next guest RX buffer.
///
/// The 12-byte header is zeros (`num_buffers` stays 0; `MRG_RXBUF` is off).
/// Returns false when queue 0 has no published buffer. `poll_rx` has already
/// taken the frame off the NIC, so a false return drops it.
fn post_rx(eth: &[u8], translate: &impl Fn(u64) -> Option<u64>) -> bool {
    if eth.len() < 14 || eth.len() > 1514 {
        return false;
    }
    let (enabled, desc, avail) = with_net(|n| {
        let q = &n.q[0];
        (q.enable != 0, q.desc, q.avail)
    });
    if !enabled || desc == 0 || avail == 0 {
        return false;
    }
    let Some(avail_hpa) = translate(avail) else {
        return false;
    };
    // SAFETY: translate returned the avail ring page. idx is the u16 at +2.
    // KANI-TARGET: virtio avail idx (outside Proven Core).
    let idx = unsafe { core::ptr::read_unaligned((avail_hpa as *const u16).add(1)) };
    let last = with_net(|n| n.q[0].last_avail);
    if idx == last {
        return false;
    }
    let slot = last % QSIZE;
    let Some(ent_hpa) = translate(avail.wrapping_add(4 + u64::from(slot) * 2)) else {
        return false;
    };
    // SAFETY: translate returned this avail ring entry.
    // KANI-TARGET: virtio avail ring id (outside Proven Core).
    let head = unsafe { core::ptr::read_unaligned(ent_hpa as *const u16) };
    let mut pkt = [0u8; VNET_HDR_LEN + 1514];
    let total = VNET_HDR_LEN + eth.len();
    pkt[VNET_HDR_LEN..total].copy_from_slice(eth);
    let mut off = 0usize;
    let mut di = head % QSIZE;
    for _ in 0..CHAIN_MAX {
        if off == total {
            break;
        }
        let Some(d_hpa) = translate(desc.wrapping_add(u64::from(di) * 16)) else {
            return false;
        };
        // SAFETY: translate returned this descriptor.
        // KANI-TARGET: virtio-net RX descriptor (outside Proven Core).
        let (addr, len, flags, next) = unsafe {
            let addr = core::ptr::read_unaligned(d_hpa as *const u64);
            let len = core::ptr::read_unaligned((d_hpa as *const u32).add(2)) as usize;
            let flags = core::ptr::read_unaligned((d_hpa as *const u16).add(6));
            let next = core::ptr::read_unaligned((d_hpa as *const u16).add(7));
            (addr, len, flags, next)
        };
        if flags & 2 != 0 && len > 0 {
            let Some(hpa) = translate(addr) else {
                return false;
            };
            let take = len.min(total - off);
            // SAFETY: translate returned the guest RX buffer. `take` stays
            // inside that descriptor's length and inside `pkt`.
            // KANI-TARGET: virtio-net RX payload copy (outside Proven Core).
            unsafe {
                core::ptr::copy_nonoverlapping(pkt.as_ptr().add(off), hpa as *mut u8, take);
            }
            off += take;
        }
        if flags & 1 == 0 {
            break;
        }
        di = next % QSIZE;
    }
    if off != total {
        return false;
    }
    let (used_base, used_slot, used_idx) = with_net(|n| {
        let q = &mut n.q[0];
        let slot = q.used_idx % QSIZE;
        q.used_idx = q.used_idx.wrapping_add(1);
        q.last_avail = last.wrapping_add(1);
        n.isr = 1;
        (q.used, slot, q.used_idx)
    });
    if used_base != 0 {
        if let Some(hpa) = translate(used_base.wrapping_add(4 + u64::from(used_slot) * 8)) {
            // SAFETY: translate returned the used-ring element for this slot.
            // KANI-TARGET: virtio-net used ring id/len (outside Proven Core).
            unsafe {
                core::ptr::write_unaligned(hpa as *mut u32, u32::from(head));
                core::ptr::write_unaligned((hpa as *mut u32).add(1), total as u32);
            }
        }
        if let Some(hpa) = translate(used_base.wrapping_add(2)) {
            // SAFETY: translate returned the used.idx halfword.
            // KANI-TARGET: virtio-net used idx (outside Proven Core).
            unsafe {
                core::ptr::write_unaligned(hpa as *mut u16, used_idx);
            }
        }
    }
    TX_IRQ_ARM.store(1, Ordering::Release);
    RX_POSTED_N.fetch_add(1, Ordering::AcqRel);
    true
}

#[cfg(test)]
mod guest_virtio_net_test {
    use super::*;
    use crate::mgmt::guest_nic::{clear_panel_for_test, panel_save, NicDraft, NicMode};
    use crate::mgmt::guest_nic_start::{clear_start_for_test, on_accepted_start};
    use std::sync::{Mutex, MutexGuard};

    fn lock() -> MutexGuard<'static, ()> {
        static LOCK: Mutex<()> = Mutex::new(());
        LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    #[test]
    fn nic_appears_only_after_an_accepted_start() {
        let _guard = lock();
        clear_panel_for_test();
        clear_start_for_test();
        let addr = 0x8000_0000 | (u32::from(GUEST_NET_PCI_DEV) << 11);
        crate::devices::ide_cdrom::pci_write_addr(addr);
        assert!(!pci_selects(addr));
        assert_eq!(pci_read_data(0xCFC, 4), 0xFFFF_FFFF);
        let off = 8u64 * 1024 * 1024 * 1024;
        panel_save(
            0,
            NicDraft {
                spare_off: off,
                mode: NicMode::Manual,
                prefix: 24,
                ipv4: [10, 99, 99, 40],
                gateway: [10, 99, 99, 1],
                dns: [1, 1, 1, 1],
            },
        )
        .unwrap();
        on_accepted_start(off);
        assert!(pci_selects(addr));
        let id = pci_read_data(0xCFC, 4);
        assert_eq!(id as u16, GUEST_NET_VENDOR);
        assert_eq!((id >> 16) as u16, GUEST_NET_DEVICE);
        let mac = armed_start().unwrap().mac;
        let b0 = mmio_read(u64::from(GUEST_NET_BAR) + u64::from(OFF_DEVICE), 1);
        assert_eq!(b0 as u8, mac[0]);
        clear_panel_for_test();
        clear_start_for_test();
    }

    #[test]
    fn net_mmio_resolves_so_an_ept_hit_does_not_stop() {
        let _guard = lock();
        clear_panel_for_test();
        clear_start_for_test();
        let gpa = u64::from(GUEST_NET_BAR);
        assert!(
            crate::devices::guest_virtio_blk::mmio_bar_base_for_gpa(gpa).is_none(),
            "no NIC until Start"
        );
        let off = 8u64 * 1024 * 1024 * 1024;
        panel_save(
            0,
            NicDraft {
                spare_off: off,
                mode: NicMode::Manual,
                prefix: 24,
                ipv4: [10, 99, 99, 40],
                gateway: [10, 99, 99, 1],
                dns: [1, 1, 1, 1],
            },
        )
        .unwrap();
        on_accepted_start(off);
        assert_eq!(
            crate::devices::guest_virtio_blk::mmio_bar_base_for_gpa(gpa),
            Some(gpa)
        );
        assert!(crate::devices::guest_virtio_blk::is_virtio_bar_gpa(gpa));
        assert!(crate::devices::guest_virtio_blk::is_virtio_bar_2m_gpa(gpa));
        let mac = armed_start().unwrap().mac;
        assert_eq!(
            crate::devices::guest_virtio_blk::mmio_read_at(gpa + u64::from(OFF_DEVICE), 1) as u8,
            mac[0]
        );
        assert!(crate::devices::guest_virtio_blk::mmio_bar_base_for_gpa(gpa + 0x1000).is_none());
        let src = include_str!("../vmx/guest_uefi.rs");
        let handler = src
            .split("fn handle_virtio_bar_ept")
            .nth(1)
            .expect("ept handler");
        assert!(handler.contains("mmio_bar_base_for_gpa"));
        assert!(handler
            .contains("boot: M8.9 virtio-net MMIO (guest keeps running; not ISO-INSTALL-OK)"));
        clear_panel_for_test();
        clear_start_for_test();
    }

    #[test]
    fn interrupt_pin_is_inta_so_virtio_net_can_probe() {
        let _guard = lock();
        clear_panel_for_test();
        clear_start_for_test();
        let off = 8u64 * 1024 * 1024 * 1024;
        panel_save(
            0,
            NicDraft {
                spare_off: off,
                mode: NicMode::Manual,
                prefix: 24,
                ipv4: [10, 99, 99, 40],
                gateway: [10, 99, 99, 1],
                dns: [1, 1, 1, 1],
            },
        )
        .unwrap();
        on_accepted_start(off);
        let addr = 0x8000_0000 | (u32::from(GUEST_NET_PCI_DEV) << 11);
        crate::devices::ide_cdrom::pci_write_addr(addr | 0x3C);
        let irq = pci_read_data(0xCFC, 4);
        assert_eq!(
            irq as u8,
            crate::devices::guest_irq::VIRTIO_PIC_IRQ,
            "interrupt line"
        );
        assert_eq!(
            (irq >> 8) as u8,
            1,
            "INTA; pin 0 makes virtio_net probe return -22"
        );
        clear_panel_for_test();
        clear_start_for_test();
    }

    #[test]
    fn rx_kick_does_not_complete_a_tx_the_guest_has_not_published() {
        let _guard = lock();
        clear_panel_for_test();
        clear_start_for_test();
        let off = 8u64 * 1024 * 1024 * 1024;
        panel_save(
            0,
            NicDraft {
                spare_off: off,
                mode: NicMode::Manual,
                prefix: 24,
                ipv4: [10, 99, 99, 145],
                gateway: [10, 99, 99, 1],
                dns: [1, 1, 1, 1],
            },
        )
        .unwrap();
        on_accepted_start(off);
        let bar = u64::from(GUEST_NET_BAR);
        mmio_write(bar + 0x14, 1, 0);
        mmio_write(bar + 0x16, 2, 1);
        let mut desc = [0u8; 16];
        let mut avail = [0u8; 8];
        let used = [0u8; 16];
        let mut buf = [0u8; 32];
        buf[10..24].copy_from_slice(b"0123456789abcd");
        let desc_gpa = 0x10_0000u64;
        let avail_gpa = 0x20_0000u64;
        let used_gpa = 0x30_0000u64;
        let buf_gpa = 0x40_0000u64;
        desc[0..8].copy_from_slice(&buf_gpa.to_le_bytes());
        desc[8..12].copy_from_slice(&24u32.to_le_bytes());
        mmio_write(bar + 0x20, 8, desc_gpa);
        mmio_write(bar + 0x28, 8, avail_gpa);
        mmio_write(bar + 0x30, 8, used_gpa);
        mmio_write(bar + 0x1C, 2, 1);
        mmio_write(bar + u64::from(OFF_NOTIFY), 2, 0);
        let desc_ptr = desc.as_ptr() as u64;
        let avail_ptr = avail.as_ptr() as u64;
        let used_ptr = used.as_ptr() as u64;
        let buf_ptr = buf.as_ptr() as u64;
        let translate = move |gpa: u64| -> Option<u64> {
            if gpa == desc_gpa {
                Some(desc_ptr)
            } else if (avail_gpa..avail_gpa + 8).contains(&gpa) {
                Some(avail_ptr + (gpa - avail_gpa))
            } else if (used_gpa..used_gpa + 16).contains(&gpa) {
                Some(used_ptr + (gpa - used_gpa))
            } else if gpa == buf_gpa {
                Some(buf_ptr)
            } else {
                None
            }
        };
        drain(&translate);
        let used_idx = u16::from_le_bytes([used[2], used[3]]);
        assert_eq!(used_idx, 0, "RX kick must not invent a TX completion");
        avail[2] = 1;
        assert_eq!(avail[2], 1);
        drain(&translate);
        let used_idx = u16::from_le_bytes([used[2], used[3]]);
        assert_eq!(used_idx, 1);
        let id = u32::from_le_bytes([used[4], used[5], used[6], used[7]]);
        assert_eq!(id, 0);
        let src = include_str!("guest_virtio_net.rs");
        let one = src
            .split_once("fn drain_one")
            .expect("drain_one")
            .1
            .split_once("fn raise_deferred_tx_irq")
            .expect("raise")
            .0;
        assert!(!one.contains("raise_virtio()"));
        assert!(include_str!("../vmx/guest_uefi.rs").contains("raise_deferred_tx_irq(cpu::rdtsc())"));
        assert!(include_str!("../vmx/guest_uefi.rs").contains("poll_rx(guest_uefi_gpa_to_hpa)"));
        clear_panel_for_test();
        clear_start_for_test();
    }

    #[test]
    fn tx_queue_is_deep_enough_for_linux_to_wake_it() {
        // Iron `c9f4451a`: interrupts healthy (`idtv=1 reinj=1 eoi_s=21
        // take0=149`), yet `NETDEV WATCHDOG: transmit queue 0 timed out`
        // every 5 s. Linux stops the TX queue at `num_free < 2 +
        // MAX_SKB_FRAGS` and wakes it at `>= 2 + MAX_SKB_FRAGS`; an 8-entry
        // ring can never satisfy the wake.
        const MAX_SKB_FRAGS: u16 = 17;
        assert!(QSIZE >= 2 + MAX_SKB_FRAGS + 1, "queue can never wake below this");
        assert!(QSIZE.is_power_of_two(), "virtio 1.0 split ring");
        assert_eq!(QSIZE, 256);
        assert!(CHAIN_MAX >= usize::from(2 + MAX_SKB_FRAGS), "one skb per chain");
        let _guard = lock();
        clear_panel_for_test();
        clear_start_for_test();
        let off = 8u64 * 1024 * 1024 * 1024;
        panel_save(
            0,
            NicDraft {
                spare_off: off,
                mode: NicMode::Manual,
                prefix: 24,
                ipv4: [10, 99, 99, 145],
                gateway: [10, 99, 99, 1],
                dns: [1, 1, 1, 1],
            },
        )
        .unwrap();
        on_accepted_start(off);
        let bar = u64::from(GUEST_NET_BAR);
        for q in 0..2u64 {
            mmio_write(bar + 0x16, 2, q);
            assert_eq!(mmio_read(bar + 0x18, 2), u64::from(QSIZE), "queue_size for queue {q}");
        }
        clear_panel_for_test();
        clear_start_for_test();
    }

    #[test]
    fn version1_header_is_twelve_bytes() {
        let mut buf = [0u8; 12 + 14];
        buf[12] = 0x02;
        buf[13] = 0x52;
        let eth = ethernet_from_virtio(&buf).unwrap();
        assert_eq!(eth.len(), 14);
        assert_eq!(&eth[..2], &[0x02, 0x52]);
        assert!(ethernet_from_virtio(&buf[..25]).is_none());
        let drain = include_str!("guest_virtio_net.rs")
            .split_once("fn drain_one")
            .expect("drain_one")
            .1
            .split_once("fn raise_deferred_tx_irq")
            .expect("raise")
            .0;
        assert!(drain.contains("ethernet_from_virtio"));
        assert!(!drain.contains("[10.."));
        let arm = include_str!("../mgmt/bcm5720_mmio.rs")
            .split_once("unsafe fn arm_egress")
            .expect("arm")
            .1
            .split_once("pub fn mac_address")
            .expect("mac")
            .0;
        assert!(
            arm.contains("MAILBOX_RCV_STD_PROD_IDX"),
            "function 1 must post RX buffers or a reply is never DMA'd"
        );
    }

    #[test]
    fn carrier_follows_the_guest_lan_port_once_it_is_driven() {
        // ADR-021 D6. Iron `d7e2e24f`: the guest kept `UP` on a dark `:39`
        // because the device never offered `F_STATUS`. Now the status word
        // follows `host_link`, and a flip raises a config-change interrupt.
        use crate::mgmt::host_link::{observe, reset_for_test, LinkSample};
        let _guard = lock();
        clear_panel_for_test();
        clear_start_for_test();
        reset_for_test();
        crate::devices::guest_irq::reset();
        // Linux owns the VMCS; IOAPIC pin for the virtio GSI is unmasked.
        crate::devices::guest_irq::note_linux_dev_irq();
        crate::devices::guest_irq::ioapic_write(
            0,
            0x10 + 2 * u32::from(crate::devices::guest_irq::VIRTIO_GSI),
        );
        crate::devices::guest_irq::ioapic_write(0x10, 0x51);
        let off = 8u64 * 1024 * 1024 * 1024;
        panel_save(
            0,
            NicDraft {
                spare_off: off,
                mode: NicMode::Manual,
                prefix: 24,
                ipv4: [10, 99, 99, 145],
                gateway: [10, 99, 99, 1],
                dns: [1, 1, 1, 1],
            },
        )
        .unwrap();
        on_accepted_start(off);
        let bar = u64::from(GUEST_NET_BAR);
        mmio_write(bar, 4, 0);
        let feat0 = mmio_read(bar + 0x04, 4);
        assert_ne!(feat0 & (1 << 16), 0, "VIRTIO_NET_F_STATUS offered");
        assert_ne!(feat0 & (1 << 5), 0, "VIRTIO_NET_F_MAC still offered");
        let status = bar + u64::from(OFF_DEVICE) + u64::from(DEVCFG_STATUS_OFF);
        let gen = bar + 0x15;
        // Never sampled: optimistic, so the guest's first frame can arm `:39`.
        assert_eq!(mmio_read(status, 2), 1);
        // Sampled before the arm (firmware-held, `driven=false`): still up.
        let dark = LinkSample {
            up: false,
            mbps: 0,
            full_duplex: false,
            bmsr: 0x7949,
            driven: false,
        };
        assert!(observe(1, [0xb0, 0x26, 0x28, 0x5c, 0x5a, 0x39], dark).is_none());
        assert_eq!(mmio_read(status, 2), 1);
        let gen0 = mmio_read(gen, 1);
        // Driven and up: carrier.
        let lit = LinkSample {
            up: true,
            mbps: 1000,
            full_duplex: true,
            bmsr: 0x796d,
            driven: true,
        };
        let change = observe(1, [0; 6], lit).expect("down→up is a change");
        assert_eq!(change.mbps, 1000);
        assert_eq!(mmio_read(status, 2), 1);
        assert_eq!(mmio_read(status, 1), 1);
        assert_eq!(mmio_read(status + 1, 1), 0);
        // Driven and dark: NO-CARRIER. A flip before DRIVER_OK bumps the
        // generation but raises nothing (the driver is not listening yet).
        mmio_write(bar + 0x14, 1, 0);
        let dark_driven = LinkSample { driven: true, ..dark };
        assert!(observe(1, [0; 6], dark_driven).is_some());
        note_carrier_change();
        assert_eq!(mmio_read(status, 2), 0);
        assert_eq!(mmio_read(gen, 1), (gen0 + 1) & 0xff);
        assert_eq!(mmio_read(bar + 0x100, 1), 0, "no ISR before DRIVER_OK");
        raise_deferred_tx_irq(0);
        assert!(crate::devices::guest_irq::take_inject_vector().is_none());
        // After DRIVER_OK a flip is a config interrupt on the next exit.
        mmio_write(bar + 0x14, 1, u64::from(STATUS_DRIVER_OK));
        assert!(observe(1, [0; 6], lit).is_some());
        note_carrier_change();
        assert_eq!(mmio_read(status, 2), 1);
        assert_eq!(mmio_read(gen, 1), (gen0 + 2) & 0xff);
        raise_deferred_tx_irq(0);
        assert_eq!(
            crate::devices::guest_irq::take_inject_vector(),
            Some(0x51),
            "virtio interrupt for the config change"
        );
        assert_eq!(mmio_read(bar + 0x100, 1), u64::from(ISR_CONFIG));
        assert_eq!(mmio_read(bar + 0x100, 1), 0, "ISR clears on read");
        // The sampler calls in here when function 1 flips (`report_link`).
        let report = include_str!("../mgmt/bcm5720_mmio.rs")
            .split_once("fn report_link")
            .expect("report_link")
            .1
            .split_once("egress_say(\"boot: D5 link lom=\")")
            .expect("say")
            .0;
        assert!(report.contains("note_carrier_change()"));
        assert!(report.contains("c.func == 1"));
        reset_for_test();
        crate::devices::guest_irq::reset();
        clear_panel_for_test();
        clear_start_for_test();
    }

    #[test]
    fn posted_rx_keeps_a_twelve_byte_header() {
        let _guard = lock();
        clear_panel_for_test();
        clear_start_for_test();
        let off = 8u64 * 1024 * 1024 * 1024;
        panel_save(
            0,
            NicDraft {
                spare_off: off,
                mode: NicMode::Manual,
                prefix: 24,
                ipv4: [10, 99, 99, 145],
                gateway: [10, 99, 99, 1],
                dns: [1, 1, 1, 1],
            },
        )
        .unwrap();
        on_accepted_start(off);
        let bar = u64::from(GUEST_NET_BAR);
        mmio_write(bar + 0x14, 1, 0);
        mmio_write(bar + 0x16, 2, 0);
        let mut desc = [0u8; 16];
        let mut avail = [0u8; 8];
        let used = [0u8; 16];
        let mut buf = [0xABu8; 64];
        let desc_gpa = 0x10_0000u64;
        let avail_gpa = 0x20_0000u64;
        let used_gpa = 0x30_0000u64;
        let buf_gpa = 0x40_0000u64;
        desc[0..8].copy_from_slice(&buf_gpa.to_le_bytes());
        desc[8..12].copy_from_slice(&64u32.to_le_bytes());
        desc[12] = 2;
        mmio_write(bar + 0x20, 8, desc_gpa);
        mmio_write(bar + 0x28, 8, avail_gpa);
        mmio_write(bar + 0x30, 8, used_gpa);
        mmio_write(bar + 0x1C, 2, 1);
        let desc_ptr = desc.as_ptr() as u64;
        let avail_ptr = avail.as_ptr() as u64;
        let used_ptr = used.as_ptr() as u64;
        let buf_ptr = buf.as_mut_ptr() as u64;
        let translate = move |gpa: u64| -> Option<u64> {
            if gpa == desc_gpa {
                Some(desc_ptr)
            } else if (avail_gpa..avail_gpa + 8).contains(&gpa) {
                Some(avail_ptr + (gpa - avail_gpa))
            } else if (used_gpa..used_gpa + 16).contains(&gpa) {
                Some(used_ptr + (gpa - used_gpa))
            } else if gpa == buf_gpa {
                Some(buf_ptr)
            } else {
                None
            }
        };
        let mut eth = [0u8; 14];
        eth[0] = 0x02;
        eth[6] = 0xb0;
        assert!(!post_rx(&eth, &translate), "no buffer published yet");
        avail[2] = 1;
        assert_eq!(avail[2], 1);
        assert!(post_rx(&eth, &translate));
        assert_eq!(&buf[..12], &[0u8; 12], "virtio header stays zero");
        assert_eq!(&buf[12..26], &eth);
        assert_eq!(buf[26], 0xAB, "bytes past the frame stay");
        let used_idx = u16::from_le_bytes([used[2], used[3]]);
        assert_eq!(used_idx, 1);
        let len = u32::from_le_bytes([used[8], used[9], used[10], used[11]]);
        assert_eq!(len, 12 + 14);
        let isr = mmio_read(bar + 0x100, 1);
        assert_eq!(isr, 1);
        clear_panel_for_test();
        clear_start_for_test();
    }
}
