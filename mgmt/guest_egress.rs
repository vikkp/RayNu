//! Where a guest DHCP discover is allowed to leave (outside Proven Core).
//!
//! Pillar: [Z] [D]
//! Proven Core: **outside** (ADR-002 / ADR-013 Appendix C).
//! VERIFICATION: L1 host tests. Host/CI never print an iron marker.
//!
//! The port is BCM5720 function 1 (`01:00.1`), station MAC
//! `b0:26:28:5c:5a:39`. The discover's own source address is the guest
//! MAC. Function 0 is the SPA. The APE phylock stays with iDRAC.
//!
//! This module does not program rings and does not call the SPA NIC init.
//! That path owns the one DMA arena and the APE globals the SPA is
//! already using. A shared BAR with the SPA is refused.

use core::sync::atomic::{AtomicBool, AtomicU16, AtomicU64, AtomicU8, Ordering};

use crate::mgmt::guest_nic::{EGRESS_PHY_MAC, SPA_MAC};

/// PCI function the discover may leave on. `phylock` is always false.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EgressPort {
    pub bus: u8,
    pub dev: u8,
    pub func: u8,
    pub phy_mac: [u8; 6],
    pub phylock: bool,
}

/// Result of asking whether rings on that port may be programmed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RingArm {
    /// Function 1, its own BAR, phylock not taken, SPA left on function 0.
    Ready { bar: u64 },
    /// SPA port, shared BAR, missing BAR, or a phylock request.
    Hold,
}

/// This R640's egress identity. The live scan uses [`pick`].
pub fn iron_port() -> EgressPort {
    EgressPort {
        bus: 0x01,
        dev: 0x00,
        func: 1,
        phy_mac: EGRESS_PHY_MAC,
        phylock: false,
    }
}

/// Function 1 with the egress PHY MAC, and the phylock not taken.
pub fn port_ok(port: &EgressPort) -> bool {
    port.func == 1 && port.phy_mac == EGRESS_PHY_MAC && !port.phylock && port.phy_mac != SPA_MAC
}

/// Pick function 1 whose peeked MAC is the egress PHY. Function 0 is ignored.
pub fn pick(cands: &[(u8, u8, u8, [u8; 6])]) -> Option<EgressPort> {
    for &(bus, dev, func, mac) in cands {
        if func == 1 && mac == EGRESS_PHY_MAC {
            return Some(EgressPort {
                bus,
                dev,
                func,
                phy_mac: mac,
                phylock: false,
            });
        }
    }
    None
}

/// Rings may be programmed only on a legal port with its own non-zero BAR.
///
/// The SPA stays on function 0. The same BAR as the SPA is a hold, even
/// when the function numbers differ. A phylock request is a hold.
pub fn rings_ready(port: &EgressPort, egress_bar: u64, spa_func: u8, spa_bar: u64) -> RingArm {
    if !port_ok(port) || port.phylock {
        return RingArm::Hold;
    }
    if spa_func != 0 || spa_bar == 0 || egress_bar == 0 || egress_bar == spa_bar {
        return RingArm::Hold;
    }
    if port.func == spa_func {
        return RingArm::Hold;
    }
    RingArm::Ready { bar: egress_bar }
}

/// A discover that passed the port check. Not a transmit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Handoff {
    pub port: EgressPort,
    pub guest_mac: [u8; 6],
    pub len: usize,
}

const BOOT: usize = 14 + 20 + 8;
const CHADDR: usize = BOOT + 28;
const COOKIE: usize = BOOT + 236;

/// Accept a discover for `guest` on `port`. Refuses the SPA MAC, the PHY
/// MAC, a multicast MAC, function 0, and a phylock.
pub fn handoff(guest: [u8; 6], frame: &[u8], len: usize, port: EgressPort) -> Option<Handoff> {
    if guest == SPA_MAC || guest == EGRESS_PHY_MAC || guest[0] & 1 != 0 {
        return None;
    }
    if !port_ok(&port) {
        return None;
    }
    if len > frame.len() || len < COOKIE + 7 {
        return None;
    }
    let frame = &frame[..len];
    if frame[6..12] != guest || frame[CHADDR..CHADDR + 6] != guest {
        return None;
    }
    if frame[12] != 0x08 || frame[13] != 0x00 || frame[BOOT] != 1 {
        return None;
    }
    if frame[COOKIE..COOKIE + 4] != [0x63, 0x82, 0x53, 0x63] {
        return None;
    }
    if frame[COOKIE + 4] != 53 || frame[COOKIE + 6] != 1 {
        return None;
    }
    Some(Handoff {
        port,
        guest_mac: guest,
        len,
    })
}

static LAST_SEEN: AtomicBool = AtomicBool::new(false);
static LAST_FUNC: AtomicU8 = AtomicU8::new(0);
static LAST_PHYLOCK: AtomicU8 = AtomicU8::new(0);
static LAST_GUEST: AtomicU64 = AtomicU64::new(0);
static LAST_PHY: AtomicU64 = AtomicU64::new(0);
static LAST_LEN: AtomicU16 = AtomicU16::new(0);

fn pack_mac(mac: [u8; 6]) -> u64 {
    let mut v = 0u64;
    for (i, b) in mac.iter().enumerate() {
        v |= u64::from(*b) << (8 * i);
    }
    v
}

fn unpack_mac(v: u64) -> [u8; 6] {
    let mut mac = [0u8; 6];
    for (i, b) in mac.iter_mut().enumerate() {
        *b = (v >> (8 * i)) as u8;
    }
    mac
}

pub(crate) fn note_handoff(hand: &Handoff) {
    LAST_FUNC.store(hand.port.func, Ordering::Relaxed);
    LAST_PHYLOCK.store(u8::from(hand.port.phylock), Ordering::Relaxed);
    LAST_GUEST.store(pack_mac(hand.guest_mac), Ordering::Relaxed);
    LAST_PHY.store(pack_mac(hand.port.phy_mac), Ordering::Relaxed);
    LAST_LEN.store(hand.len as u16, Ordering::Relaxed);
    LAST_SEEN.store(true, Ordering::Release);
}

/// Last accepted discover. Tests run single-threaded.
#[cfg(test)]
pub fn last_handoff() -> Option<([u8; 6], [u8; 6], u8, bool, usize)> {
    if !LAST_SEEN.load(Ordering::Acquire) {
        return None;
    }
    Some((
        unpack_mac(LAST_GUEST.load(Ordering::Relaxed)),
        unpack_mac(LAST_PHY.load(Ordering::Relaxed)),
        LAST_FUNC.load(Ordering::Relaxed),
        LAST_PHYLOCK.load(Ordering::Relaxed) != 0,
        LAST_LEN.load(Ordering::Relaxed) as usize,
    ))
}

#[cfg(test)]
mod guest_egress_test {
    use super::*;
    use crate::mgmt::guest_dhcp::discover_frame;
    use crate::mgmt::guest_nic::mint_mac;

    #[test]
    fn pick_is_function_1_and_refuses_the_spa() {
        let spa = (1u8, 0u8, 0u8, SPA_MAC);
        let ape = (1u8, 0u8, 1u8, [0xb0, 0x26, 0x28, 0x5c, 0x5a, 0x3a]);
        let egress = (1u8, 0u8, 1u8, EGRESS_PHY_MAC);
        assert!(pick(&[spa]).is_none());
        assert!(pick(&[spa, ape]).is_none());
        let got = pick(&[spa, egress]).unwrap();
        assert_eq!(got.func, 1);
        assert_eq!(got.phy_mac, EGRESS_PHY_MAC);
        assert!(!got.phylock);
        assert_eq!(got.bus, 1);
        assert_eq!(got.dev, 0);
        let iron = iron_port();
        assert!(port_ok(&iron));
        assert!(!port_ok(&EgressPort {
            phylock: true,
            ..iron
        }));
        assert!(!port_ok(&EgressPort { func: 0, ..iron }));
    }

    #[test]
    fn rings_stay_off_the_spa_bar() {
        let port = iron_port();
        let spa_bar = 0xE000_0000u64;
        let egress_bar = 0xF000_0000u64;
        assert_eq!(
            rings_ready(&port, egress_bar, 0, spa_bar),
            RingArm::Ready { bar: egress_bar }
        );
        assert_eq!(rings_ready(&port, spa_bar, 0, spa_bar), RingArm::Hold);
        assert_eq!(rings_ready(&port, egress_bar, 0, 0), RingArm::Hold);
        assert_eq!(rings_ready(&port, 0, 0, spa_bar), RingArm::Hold);
        assert_eq!(rings_ready(&port, egress_bar, 1, spa_bar), RingArm::Hold);
        let locked = EgressPort {
            phylock: true,
            ..port
        };
        assert_eq!(rings_ready(&locked, egress_bar, 0, spa_bar), RingArm::Hold);
        let func0 = EgressPort { func: 0, ..port };
        assert_eq!(rings_ready(&func0, egress_bar, 0, spa_bar), RingArm::Hold);
    }

    #[test]
    fn discover_is_handed_to_function_1() {
        let mac = mint_mac(0);
        let frame = discover_frame(&mac, 1);
        let hand = handoff(mac, &frame.bytes, frame.len, iron_port()).unwrap();
        assert_eq!(hand.port.func, 1);
        assert!(!hand.port.phylock);
        assert_eq!(hand.guest_mac, mac);
        note_handoff(&hand);
        assert!(handoff(SPA_MAC, &frame.bytes, frame.len, iron_port()).is_none());
        let locked = EgressPort {
            phylock: true,
            ..iron_port()
        };
        assert!(handoff(mac, &frame.bytes, frame.len, locked).is_none());
        let (guest, phy, func, phylock, len) = last_handoff().unwrap();
        assert_eq!(guest, mac);
        assert_eq!(phy, EGRESS_PHY_MAC);
        assert_eq!(func, 1);
        assert!(!phylock);
        assert_eq!(len, frame.len);
        let src = include_str!("guest_egress.rs");
        let spa_init = ["init", "_bcm5720"].concat();
        let ape = ["ape", "_lock"].concat();
        let tx = ["transmit", "_frame"].concat();
        assert!(!src.contains(&spa_init));
        assert!(!src.contains(&ape));
        assert!(!src.contains(&tx));
    }
}
