//! M8.9.3 DHCP preview for one guest MAC (outside Proven Core).
//!
//! Pillar: [Z] [D]
//! Proven Core: **outside** (ADR-002 / ADR-013 Appendix C).
//! VERIFICATION: L1 host tests. Host/CI never print an iron marker.
//!
//! Get from DHCP builds a discover for that guest's MAC and, when an offer
//! is in hand, returns the address fields. It does not Save and it does not
//! write the spare sector. The discover is handed to function 1
//! (`01:00.1` / `:39`). Function 0 stays the SPA. The APE phylock stays
//! with iDRAC. Rings on that port are not programmed.

use crate::mgmt::guest_nic::mint_mac;

#[cfg(test)]
use crate::mgmt::guest_nic::{EGRESS_PHY_MAC, SPA_MAC};

#[cfg(test)]
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering};

/// Address fields from one DHCPOFFER. Not a saved NIC record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DhcpFields {
    pub ipv4: [u8; 4],
    pub prefix: u8,
    pub gateway: [u8; 4],
    pub dns: [u8; 4],
}

/// A discover frame ready to send. `len` is the used prefix.
pub struct DiscoverFrame {
    pub bytes: [u8; 512],
    pub len: usize,
}

/// MAC the discover will use. Saved record wins. Otherwise the minted guest MAC.
pub fn discover_mac(saved: Option<[u8; 6]>, spare_off: u64) -> [u8; 6] {
    saved.unwrap_or_else(|| mint_mac(spare_off))
}

/// Ethernet/IPv4/UDP DHCPDISCOVER. `chaddr` is `mac`. Broadcast, client port 68.
pub fn discover_frame(mac: &[u8; 6], xid: u32) -> DiscoverFrame {
    let mut b = [0u8; 512];
    b[0..6].fill(0xff);
    b[6..12].copy_from_slice(mac);
    b[12] = 0x08;
    b[13] = 0x00;
    let ip = 14;
    b[ip] = 0x45;
    b[ip + 1] = 0;
    b[ip + 8] = 64;
    b[ip + 9] = 17;
    b[ip + 16..ip + 20].fill(0xff);
    let udp = ip + 20;
    b[udp] = 0;
    b[udp + 1] = 68;
    b[udp + 2] = 0;
    b[udp + 3] = 67;
    let boot = udp + 8;
    b[boot] = 1;
    b[boot + 1] = 1;
    b[boot + 2] = 6;
    b[boot + 4..boot + 8].copy_from_slice(&xid.to_be_bytes());
    b[boot + 10] = 0x80;
    b[boot + 28..boot + 34].copy_from_slice(mac);
    let cookie = boot + 236;
    b[cookie..cookie + 4].copy_from_slice(&[0x63, 0x82, 0x53, 0x63]);
    let mut o = cookie + 4;
    b[o] = 53;
    b[o + 1] = 1;
    b[o + 2] = 1;
    o += 3;
    b[o] = 55;
    b[o + 1] = 3;
    b[o + 2] = 1;
    b[o + 3] = 3;
    b[o + 4] = 6;
    o += 5;
    b[o] = 255;
    o += 1;
    let total = o;
    let ip_len = (total - ip) as u16;
    b[ip + 2..ip + 4].copy_from_slice(&ip_len.to_be_bytes());
    let sum = ip_checksum(&b[ip..ip + 20]);
    b[ip + 10..ip + 12].copy_from_slice(&sum.to_be_bytes());
    let udp_len = (total - udp) as u16;
    b[udp + 4..udp + 6].copy_from_slice(&udp_len.to_be_bytes());
    DiscoverFrame {
        bytes: b,
        len: total,
    }
}

/// Parse one DHCPOFFER. `buf` may be a full Ethernet frame or a BOOTP message.
pub fn parse_offer(buf: &[u8]) -> Option<DhcpFields> {
    let boot = bootp_start(buf)?;
    let msg = &buf[boot..];
    if msg.len() < 240 || msg[0] != 2 || msg[2] != 6 {
        return None;
    }
    let mut ipv4 = [0u8; 4];
    ipv4.copy_from_slice(&msg[16..20]);
    if ipv4 == [0, 0, 0, 0] {
        return None;
    }
    if msg.len() < 240 || msg[236..240] != [0x63, 0x82, 0x53, 0x63] {
        return None;
    }
    let opts = &msg[240..];
    let mut i = 0;
    let mut mask = None;
    let mut gw = [0u8; 4];
    let mut dns = [0u8; 4];
    let mut is_offer = false;
    while i < opts.len() {
        let code = opts[i];
        if code == 255 {
            break;
        }
        if code == 0 {
            i += 1;
            continue;
        }
        if i + 1 >= opts.len() {
            return None;
        }
        let len = opts[i + 1] as usize;
        let v = i + 2;
        if v + len > opts.len() {
            return None;
        }
        match code {
            53 if len == 1 && opts[v] == 2 => is_offer = true,
            1 if len == 4 => {
                mask = Some(prefix_of([opts[v], opts[v + 1], opts[v + 2], opts[v + 3]]))
            }
            3 if len >= 4 => gw.copy_from_slice(&opts[v..v + 4]),
            6 if len >= 4 => dns.copy_from_slice(&opts[v..v + 4]),
            _ => {}
        }
        i = v + len;
    }
    let prefix = mask??;
    if !is_offer || prefix == 0 || prefix > 32 {
        return None;
    }
    Some(DhcpFields {
        ipv4,
        prefix,
        gateway: gw,
        dns,
    })
}

/// Offer for `mac`, if one is already in hand. Does not Save.
///
/// The discover is handed to function 1. A shared BAR with the SPA, or a
/// phylock, does not arm rings. With no reply in hand this returns none.
pub fn preview_offer(mac: [u8; 6]) -> Option<DhcpFields> {
    let frame = discover_frame(&mac, 1);
    let port = crate::mgmt::guest_egress::iron_port();
    let Some(hand) = crate::mgmt::guest_egress::handoff(mac, &frame.bytes, frame.len, port) else {
        return None;
    };
    crate::mgmt::guest_egress::note_handoff(&hand);
    // This call site has no function-1 BAR. Ready here would mean a zero
    // BAR was accepted, so the form stays empty.
    if !matches!(
        crate::mgmt::guest_egress::rings_ready(&hand.port, 0, 0, 0),
        crate::mgmt::guest_egress::RingArm::Hold
    ) {
        return None;
    }
    injected_offer()
}

fn bootp_start(buf: &[u8]) -> Option<usize> {
    if buf.len() >= 240 && buf[0] == 2 {
        return Some(0);
    }
    let cookie = [0x63, 0x82, 0x53, 0x63];
    buf.windows(4).position(|w| w == cookie).and_then(
        |c| {
            if c >= 236 {
                Some(c - 236)
            } else {
                None
            }
        },
    )
}

fn prefix_of(mask: [u8; 4]) -> Option<u8> {
    let v = u32::from_be_bytes(mask);
    if v == 0 {
        return Some(0);
    }
    let host = !v;
    if host & host.wrapping_add(1) != 0 {
        return None;
    }
    Some(v.count_ones() as u8)
}

fn ip_checksum(hdr: &[u8]) -> u16 {
    let mut sum = 0u32;
    let mut i = 0;
    while i + 1 < hdr.len() {
        if i == 10 {
            i += 2;
            continue;
        }
        sum += u32::from(u16::from_be_bytes([hdr[i], hdr[i + 1]]));
        i += 2;
    }
    while sum > 0xffff {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    !(sum as u16)
}

#[cfg(test)]
static INJECT_ON: AtomicBool = AtomicBool::new(false);
#[cfg(test)]
static INJECT_IP: AtomicU32 = AtomicU32::new(0);
#[cfg(test)]
static INJECT_PX: AtomicU8 = AtomicU8::new(0);
#[cfg(test)]
static INJECT_GW: AtomicU32 = AtomicU32::new(0);
#[cfg(test)]
static INJECT_DNS: AtomicU32 = AtomicU32::new(0);

#[cfg(test)]
pub fn inject_offer_for_test(fields: Option<DhcpFields>) {
    match fields {
        None => INJECT_ON.store(false, Ordering::Release),
        Some(f) => {
            INJECT_IP.store(u32::from_be_bytes(f.ipv4), Ordering::Relaxed);
            INJECT_PX.store(f.prefix, Ordering::Relaxed);
            INJECT_GW.store(u32::from_be_bytes(f.gateway), Ordering::Relaxed);
            INJECT_DNS.store(u32::from_be_bytes(f.dns), Ordering::Relaxed);
            INJECT_ON.store(true, Ordering::Release);
        }
    }
}

fn injected_offer() -> Option<DhcpFields> {
    #[cfg(test)]
    {
        if INJECT_ON.load(Ordering::Acquire) {
            return Some(DhcpFields {
                ipv4: INJECT_IP.load(Ordering::Relaxed).to_be_bytes(),
                prefix: INJECT_PX.load(Ordering::Relaxed),
                gateway: INJECT_GW.load(Ordering::Relaxed).to_be_bytes(),
                dns: INJECT_DNS.load(Ordering::Relaxed).to_be_bytes(),
            });
        }
    }
    None
}

#[cfg(test)]
mod guest_dhcp_test {
    use super::*;

    #[test]
    fn discover_uses_the_guest_mac() {
        let mac = mint_mac(0);
        let frame = discover_frame(&mac, 0x11);
        assert_ne!(mac, SPA_MAC);
        assert_ne!(mac, EGRESS_PHY_MAC);
        assert_eq!(&frame.bytes[6..12], &mac);
        assert_eq!(&frame.bytes[14 + 20 + 8 + 28..14 + 20 + 8 + 34], &mac);
        assert!(parse_offer(&frame.bytes[..frame.len]).is_none());
        assert!(preview_offer(SPA_MAC).is_none());
        assert!(preview_offer(EGRESS_PHY_MAC).is_none());
    }

    #[test]
    fn offer_fills_address_fields() {
        let mut pkt = [0u8; 280];
        pkt[0] = 2;
        pkt[2] = 6;
        pkt[16..20].copy_from_slice(&[10, 99, 99, 50]);
        pkt[236..240].copy_from_slice(&[0x63, 0x82, 0x53, 0x63]);
        let opts = [
            53u8, 1, 2, 1, 4, 255, 255, 255, 0, 3, 4, 10, 99, 99, 1, 6, 4, 1, 1, 1, 1, 255,
        ];
        pkt[240..240 + opts.len()].copy_from_slice(&opts);
        let got = parse_offer(&pkt).unwrap();
        assert_eq!(got.ipv4, [10, 99, 99, 50]);
        assert_eq!(got.prefix, 24);
        assert_eq!(got.gateway, [10, 99, 99, 1]);
        assert_eq!(got.dns, [1, 1, 1, 1]);
        assert!(parse_offer(&[0u8; 16]).is_none());
    }

    #[test]
    fn preview_hands_the_discover_to_function_1() {
        let mac = mint_mac(0);
        inject_offer_for_test(Some(DhcpFields {
            ipv4: [10, 99, 99, 50],
            prefix: 24,
            gateway: [10, 99, 99, 1],
            dns: [1, 1, 1, 1],
        }));
        let got = preview_offer(mac).unwrap();
        assert_eq!(got.ipv4, [10, 99, 99, 50]);
        let (guest, phy, func, phylock, _) = crate::mgmt::guest_egress::last_handoff().unwrap();
        assert_eq!(guest, mac);
        assert_eq!(phy, EGRESS_PHY_MAC);
        assert_eq!(func, 1);
        assert!(!phylock);
        inject_offer_for_test(None);
    }
}
