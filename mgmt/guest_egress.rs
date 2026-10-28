//! Where a guest DHCP discover is allowed to leave (outside Proven Core).
//!
//! Pillar: [Z] [D]
//! Proven Core: **outside** (ADR-002 / ADR-013 Appendix C).
//! VERIFICATION: L1 host tests. Host/CI never print an iron marker.
//!
//! The port is the other function of the SPA's BCM5720 (this lab:
//! `01:00.1`, station MAC `b0:26:28:5c:5a:39`), found by [`discover`] from
//! the SPA's own PCI position and recorded with [`note_discovered`]. No MAC
//! constant names it (ADR-021 D4). The discover's own source address is
//! the guest MAC. The SPA's function is never the egress. The APE phylock
//! stays with iDRAC.
//!
//! This module does not program rings and does not call the SPA NIC init.
//! That path owns the one DMA arena and the APE globals the SPA is
//! already using. A shared BAR with the SPA is refused.

use core::sync::atomic::{AtomicBool, AtomicU16, AtomicU32, AtomicU64, AtomicU8, Ordering};

#[cfg(test)]
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

/// This lab's egress identity, as a test fixture. Iron never calls this;
/// the live port comes from [`discovered`].
#[cfg(test)]
pub fn iron_port() -> EgressPort {
    EgressPort {
        bus: 0x01,
        dev: 0x00,
        func: 1,
        phy_mac: EGRESS_PHY_MAC,
        phylock: false,
    }
}

/// A unicast, non-zero station MAC and the phylock not taken. Which
/// function it is on is [`rings_ready`]'s question (never the SPA's).
pub fn port_ok(port: &EgressPort) -> bool {
    port.phy_mac != [0; 6] && port.phy_mac[0] & 1 == 0 && !port.phylock
}

static DISC_SEEN: AtomicBool = AtomicBool::new(false);
static DISC_PCI: AtomicU32 = AtomicU32::new(0);
static DISC_MAC: AtomicU64 = AtomicU64::new(0);

/// Record the port [`discover`] found. Called once from the SPA NIC init,
/// where every BCM5720 function has just been peeked.
pub fn note_discovered(port: &EgressPort) {
    DISC_PCI.store(
        u32::from(port.bus) << 16 | u32::from(port.dev) << 8 | u32::from(port.func),
        Ordering::Relaxed,
    );
    DISC_MAC.store(pack_mac(port.phy_mac), Ordering::Relaxed);
    DISC_SEEN.store(true, Ordering::Release);
}

/// The discovered guest LAN port. None until the SPA NIC init has run, and
/// none on a box whose SPA BCM5720 has no other function. Host tests see
/// none unless they record one.
pub fn discovered() -> Option<EgressPort> {
    if !DISC_SEEN.load(Ordering::Acquire) {
        return None;
    }
    let pci = DISC_PCI.load(Ordering::Relaxed);
    Some(EgressPort {
        bus: (pci >> 16) as u8,
        dev: (pci >> 8) as u8,
        func: pci as u8,
        phy_mac: unpack_mac(DISC_MAC.load(Ordering::Relaxed)),
        phylock: false,
    })
}

/// Station MAC of the discovered port, all-zero when there is none.
pub fn discovered_phy_mac() -> [u8; 6] {
    discovered().map_or([0; 6], |p| p.phy_mac)
}

/// ADR-021 D4: the other function of the SPA's BCM5720: same bus and device, a
/// different function, a unicast station MAC that is not the SPA's.
/// The pair partner (`func ^ 1`) wins over any further function.
///
/// INVARIANTS:
/// - Never returns the SPA's own function or MAC
/// - Never returns a multicast or all-zero MAC
/// - Ignores every candidate on another bus/device (an H840 slot card,
///   a second LOM) so a customer R640 with more ports still gets the
///   partner of the SPA port
pub fn discover(
    spa: (u8, u8, u8, [u8; 6]),
    cands: &[(u8, u8, u8, [u8; 6])],
) -> Option<EgressPort> {
    let (sbus, sdev, sfunc, smac) = spa;
    // Rank 0 is the pair partner; any other function ranks by its number.
    let rank = |func: u8| if func == sfunc ^ 1 { 0u16 } else { 1 + u16::from(func) };
    let mut best: Option<(u16, (u8, u8, u8, [u8; 6]))> = None;
    for &c in cands {
        let (bus, dev, func, mac) = c;
        if bus != sbus || dev != sdev || func == sfunc {
            continue;
        }
        if mac == smac || mac == [0; 6] || mac[0] & 1 != 0 {
            continue;
        }
        if best.map_or(true, |(r, _)| rank(func) < r) {
            best = Some((rank(func), c));
        }
    }
    let (_, (bus, dev, func, mac)) = best?;
    Some(EgressPort {
        bus,
        dev,
        func,
        phy_mac: mac,
        phylock: false,
    })
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

/// Accept a discover for `guest` on `port`. Refuses the port's own station
/// MAC, a multicast MAC, a bad port, and a phylock.
pub fn handoff(guest: [u8; 6], frame: &[u8], len: usize, port: EgressPort) -> Option<Handoff> {
    if guest == port.phy_mac || guest[0] & 1 != 0 {
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
    fn discover_finds_the_spa_partner_on_any_r640() {
        // ADR-021 D4. This lab: SPA on 01:00.0 `:38`, partner 01:00.1 `:39`.
        let spa = (1u8, 0u8, 0u8, SPA_MAC);
        let lab = (1u8, 0u8, 1u8, EGRESS_PHY_MAC);
        let got = discover(spa, &[spa, lab]).unwrap();
        assert_eq!(got, iron_port());
        assert!(port_ok(&got));
        // Another customer's R640: other MACs, same shape. Found, legal,
        // and rings may arm on it. No constant is consulted.
        let other_spa = (1u8, 0u8, 0u8, [0xd0, 0x94, 0x66, 0x12, 0x34, 0x56]);
        let other_lan = (1u8, 0u8, 1u8, [0xd0, 0x94, 0x66, 0x12, 0x34, 0x57]);
        let got = discover(other_spa, &[other_spa, other_lan]).unwrap();
        assert_eq!(got.phy_mac, other_lan.3);
        assert_eq!((got.bus, got.dev, got.func), (1, 0, 1));
        assert!(port_ok(&got));
        assert_eq!(
            rings_ready(&got, 0xF000_0000, 0, 0xE000_0000),
            RingArm::Ready { bar: 0xF000_0000 }
        );
        // SPA on function 1 (firmware picked the other port): egress is
        // function 0, never the SPA's own function. Rings still hold,
        // because the SPA is not on function 0.
        let spa1 = (1u8, 0u8, 1u8, EGRESS_PHY_MAC);
        let lan0 = (1u8, 0u8, 0u8, SPA_MAC);
        let got = discover(spa1, &[lan0, spa1]).unwrap();
        assert_eq!(got.func, 0);
        assert_eq!(got.phy_mac, SPA_MAC);
        assert_eq!(rings_ready(&got, 0xF000_0000, 1, 0xE000_0000), RingArm::Hold);
        // A slot card on another bus, a different device, a multicast or
        // zero station MAC, or the SPA MAC on a second function: ignored.
        let h840_bus = (0x3bu8, 0u8, 1u8, [0xd0, 0x94, 0x66, 0xaa, 0xbb, 0xcc]);
        let other_dev = (1u8, 1u8, 1u8, [0xd0, 0x94, 0x66, 0xaa, 0xbb, 0xcd]);
        let mcast = (1u8, 0u8, 1u8, [0x01, 0x00, 0x5e, 0, 0, 1]);
        let zero = (1u8, 0u8, 1u8, [0u8; 6]);
        let dup = (1u8, 0u8, 1u8, SPA_MAC);
        assert!(discover(spa, &[spa, h840_bus, other_dev, mcast, zero, dup]).is_none());
        assert!(discover(spa, &[spa]).is_none());
        assert!(discover(spa, &[]).is_none());
        // Quad-port LOM: the pair partner wins over functions 2 and 3.
        let f2 = (1u8, 0u8, 2u8, [0xb0, 0x26, 0x28, 0x5c, 0x5a, 0x3a]);
        let f3 = (1u8, 0u8, 3u8, [0xb0, 0x26, 0x28, 0x5c, 0x5a, 0x3b]);
        let got = discover(spa, &[spa, f3, f2, lab]).unwrap();
        assert_eq!(got.func, 1);
        let got = discover(spa, &[spa, f3, f2]).unwrap();
        assert_eq!(got.func, 2, "lowest other function when the partner is missing");
        // The SPA NIC init records the discover; the arm reads it back
        // and programs the port it names. Neither consults the constant.
        let src = include_str!("bcm5720_mmio.rs");
        let init = src
            .split_once(&["pub fn init", "_bcm5720("].concat())
            .expect("init")
            .1
            .split_once("pub fn peek_bcm5720_bmsr_pre_ebs")
            .expect("peek")
            .0;
        assert!(init.contains("guest_egress::discover("));
        assert!(init.contains("guest_egress::note_discovered("));
        assert!(init.contains("boot: M8.9 egress discovered pci="));
        let arm = src
            .split_once("unsafe fn arm_egress")
            .expect("arm")
            .1
            .split_once("pub fn mac_address")
            .expect("mac")
            .0;
        assert!(arm.contains("guest_egress::discovered()"));
        assert!(arm.contains("enable_mac(bar, port.phy_mac)"));
        let constant = ["EGRESS_", "PHY_MAC"].concat();
        assert!(!arm.contains(&constant), "the arm names no MAC constant");
        assert!(!src.contains(&["constant=", ":39"].concat()));
    }

    #[test]
    fn port_ok_refuses_a_phylock_and_a_bad_station_mac() {
        let iron = iron_port();
        assert!(port_ok(&iron));
        assert!(!port_ok(&EgressPort {
            phylock: true,
            ..iron
        }));
        assert!(!port_ok(&EgressPort {
            phy_mac: [0; 6],
            ..iron
        }));
        assert!(!port_ok(&EgressPort {
            phy_mac: [0x01, 0, 0x5e, 0, 0, 1],
            ..iron
        }));
        // Function 0 is a legal port shape; rings_ready is what keeps it
        // off the SPA's function.
        assert!(port_ok(&EgressPort { func: 0, ..iron }));
    }

    #[test]
    fn discovered_is_what_the_nic_init_recorded() {
        note_discovered(&iron_port());
        assert_eq!(discovered(), Some(iron_port()));
        assert_eq!(discovered_phy_mac(), EGRESS_PHY_MAC);
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
        let phy_frame = discover_frame(&EGRESS_PHY_MAC, 1);
        assert!(handoff(EGRESS_PHY_MAC, &phy_frame.bytes, phy_frame.len, iron_port()).is_none());
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
