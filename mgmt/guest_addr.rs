//! M8.9.5: the saved address for the installed Ubuntu (outside Proven Core).
//!
//! Pillar: [Z] [D]
//! Proven Core: **outside** (ADR-002 / ADR-013 Appendix C).
//! VERIFICATION: L1 host tests. Host/CI never print an iron marker.
//!
//! `cloud-init` is disabled on this disk. The address is a kernel `ip=`
//! token for that boot (Documentation/admin-guide/kernel-parameters.txt).
//! The guest disk is not written. The token is not appended at StartImage
//! yet: the virtio device is not programmed, and a line with no device
//! would sit on the installed command line unused.

use core::sync::atomic::{AtomicU8, Ordering};

use crate::mgmt::guest_nic::{EGRESS_PHY_MAC, SPA_MAC};
use crate::mgmt::guest_nic_start::StartNic;

/// Kernel `ip=` text. `len` is the used prefix of `bytes`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IpToken {
    pub bytes: [u8; 96],
    pub len: usize,
}

impl IpToken {
    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.bytes[..self.len]).unwrap_or("")
    }
}

static HELD: AtomicU8 = AtomicU8::new(0);

/// `ip=<client>::<gw>:<mask>:::off[:<dns>]`.
///
/// Empty gateway and empty device are adjacent colons. `off` means the
/// kernel does not start DHCP. A zero DNS is omitted. `0.0.0.0` and
/// `255.255.255.255` are not a client address.
pub fn ip_token(nic: &StartNic) -> Option<IpToken> {
    if nic.mac == SPA_MAC || nic.mac == EGRESS_PHY_MAC || nic.mac[0] & 1 != 0 {
        return None;
    }
    if nic.egress_phy != EGRESS_PHY_MAC {
        return None;
    }
    if nic.ipv4 == [0, 0, 0, 0] || nic.ipv4 == [255, 255, 255, 255] {
        return None;
    }
    let mask = mask_of(nic.prefix)?;
    let mut bytes = [0u8; 96];
    let mut n = 0usize;
    if !push(&mut bytes, &mut n, b"ip=") {
        return None;
    }
    if !push_ip(&mut bytes, &mut n, nic.ipv4) {
        return None;
    }
    if !push(&mut bytes, &mut n, b"::") {
        return None;
    }
    if nic.gateway != [0, 0, 0, 0] && !push_ip(&mut bytes, &mut n, nic.gateway) {
        return None;
    }
    if !push(&mut bytes, &mut n, b":") {
        return None;
    }
    if !push_ip(&mut bytes, &mut n, mask) {
        return None;
    }
    if !push(&mut bytes, &mut n, b":::off") {
        return None;
    }
    if nic.dns != [0, 0, 0, 0] {
        if !push(&mut bytes, &mut n, b":") {
            return None;
        }
        if !push_ip(&mut bytes, &mut n, nic.dns) {
            return None;
        }
    }
    Some(IpToken { bytes, len: n })
}

/// Remember that this Start has a token. The bytes are rebuilt from the plan.
pub fn hold_token(nic: &StartNic) {
    let n = ip_token(nic).map(|t| t.len).unwrap_or(0);
    HELD.store(n as u8, Ordering::Release);
}

pub fn clear_token() {
    HELD.store(0, Ordering::Release);
}

/// Length of the token held for the last accepted Start. Zero means none.
pub fn held_len() -> usize {
    HELD.load(Ordering::Acquire) as usize
}

fn mask_of(prefix: u8) -> Option<[u8; 4]> {
    if prefix == 0 || prefix > 32 {
        return None;
    }
    let v = u32::MAX << (32 - prefix);
    Some(v.to_be_bytes())
}

fn push(buf: &mut [u8], n: &mut usize, s: &[u8]) -> bool {
    if *n + s.len() > buf.len() {
        return false;
    }
    buf[*n..*n + s.len()].copy_from_slice(s);
    *n += s.len();
    true
}

fn push_ip(buf: &mut [u8], n: &mut usize, ip: [u8; 4]) -> bool {
    for (i, oct) in ip.iter().enumerate() {
        if i > 0 && !push(buf, n, b".") {
            return false;
        }
        if !push_u8(buf, n, *oct) {
            return false;
        }
    }
    true
}

fn push_u8(buf: &mut [u8], n: &mut usize, v: u8) -> bool {
    let mut tmp = [0u8; 3];
    let s: &[u8] = if v >= 100 {
        tmp[0] = b'0' + v / 100;
        tmp[1] = b'0' + (v / 10) % 10;
        tmp[2] = b'0' + v % 10;
        &tmp[..3]
    } else if v >= 10 {
        tmp[0] = b'0' + v / 10;
        tmp[1] = b'0' + v % 10;
        &tmp[..2]
    } else {
        tmp[0] = b'0' + v;
        &tmp[..1]
    };
    push(buf, n, s)
}

#[cfg(test)]
mod guest_addr_test {
    use super::*;
    use crate::mgmt::guest_nic::mint_mac;

    fn nic() -> StartNic {
        StartNic {
            spare_off: 0,
            mac: mint_mac(0),
            ipv4: [10, 99, 99, 40],
            prefix: 24,
            gateway: [10, 99, 99, 1],
            dns: [1, 1, 1, 1],
            egress_phy: EGRESS_PHY_MAC,
        }
    }

    #[test]
    fn token_is_the_saved_address() {
        let tok = ip_token(&nic()).unwrap();
        assert_eq!(
            tok.as_str(),
            "ip=10.99.99.40::10.99.99.1:255.255.255.0:::off:1.1.1.1"
        );
        let mut no_dns = nic();
        no_dns.dns = [0, 0, 0, 0];
        assert_eq!(
            ip_token(&no_dns).unwrap().as_str(),
            "ip=10.99.99.40::10.99.99.1:255.255.255.0:::off"
        );
        let mut no_gw = nic();
        no_gw.gateway = [0, 0, 0, 0];
        assert_eq!(
            ip_token(&no_gw).unwrap().as_str(),
            "ip=10.99.99.40:::255.255.255.0:::off:1.1.1.1"
        );
        let mut wide = nic();
        wide.prefix = 16;
        assert!(ip_token(&wide).unwrap().as_str().contains("255.255.0.0"));
        let mut bad = nic();
        bad.ipv4 = [0, 0, 0, 0];
        assert!(ip_token(&bad).is_none());
        bad = nic();
        bad.prefix = 0;
        assert!(ip_token(&bad).is_none());
        bad = nic();
        bad.mac = SPA_MAC;
        assert!(ip_token(&bad).is_none());
        hold_token(&nic());
        assert_eq!(held_len(), tok.len);
        clear_token();
        assert_eq!(held_len(), 0);
        let src = include_str!("guest_addr.rs");
        let disk = ["ext4", "_write"].concat();
        let grub = ["grub", ".cfg"].concat();
        assert!(!src.contains(&disk));
        assert!(!src.contains(&grub));
    }
}
