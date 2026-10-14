//! M8.9.6 close check (outside Proven Core).
//!
//! Pillar: [Z] [D]
//! Proven Core: **outside** (ADR-002 / ADR-013 Appendix C).
//! VERIFICATION: L1 host tests. Host/CI never print an iron marker.
//!
//! The close is a guest frame leaving `:39` and a login to the saved
//! address from another machine. Nothing in this boot latches either
//! fact, so [`live_close`] stays open. Scores do not move here.

use crate::mgmt::guest_addr::ip_token;
use crate::mgmt::guest_nic::{EGRESS_PHY_MAC, SPA_MAC};
use crate::mgmt::guest_nic_start::StartNic;

/// Whether M8.9.6's two facts are both in hand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Close {
    /// Guest MAC left `:39`, and another machine logged into the saved address.
    Met,
    /// One of those facts is missing, or the frame used the SPA port.
    Open,
}

/// One observed Ethernet frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SeenFrame {
    pub src_mac: [u8; 6],
    pub phy_mac: [u8; 6],
    pub phylock: bool,
}

/// One observed login to a guest address.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SeenLogin {
    pub address: [u8; 4],
    pub other_machine: bool,
}

/// Judge a frame and a login against the saved NIC.
///
/// The frame source is the guest MAC. The PHY is `:39`. The phylock stays
/// with iDRAC. The login address is the saved IP, from another machine.
/// The kernel `ip=` token has to name that same address.
pub fn judge(nic: &StartNic, frame: Option<SeenFrame>, login: Option<SeenLogin>) -> Close {
    let (Some(frame), Some(login)) = (frame, login) else {
        return Close::Open;
    };
    if frame.phylock || frame.phy_mac != EGRESS_PHY_MAC {
        return Close::Open;
    }
    if frame.src_mac != nic.mac
        || frame.src_mac == SPA_MAC
        || frame.src_mac == EGRESS_PHY_MAC
        || frame.src_mac[0] & 1 != 0
    {
        return Close::Open;
    }
    if !login.other_machine || login.address != nic.ipv4 {
        return Close::Open;
    }
    let Some(tok) = ip_token(nic) else {
        return Close::Open;
    };
    if !client_matches(tok.as_str(), nic.ipv4) {
        return Close::Open;
    }
    Close::Met
}

/// This boot has no frame and no login latched.
pub fn live_close() -> Close {
    Close::Open
}

fn client_matches(token: &str, ip: [u8; 4]) -> bool {
    let Some(rest) = token.strip_prefix("ip=") else {
        return false;
    };
    let Some((addr, _)) = rest.split_once("::") else {
        return false;
    };
    let mut dotted = [0u8; 15];
    let mut n = 0usize;
    for (i, oct) in ip.iter().enumerate() {
        if i > 0 {
            if n >= dotted.len() {
                return false;
            }
            dotted[n] = b'.';
            n += 1;
        }
        let mut tmp = [0u8; 3];
        let s: &[u8] = if *oct >= 100 {
            tmp[0] = b'0' + oct / 100;
            tmp[1] = b'0' + (oct / 10) % 10;
            tmp[2] = b'0' + oct % 10;
            &tmp[..3]
        } else if *oct >= 10 {
            tmp[0] = b'0' + oct / 10;
            tmp[1] = b'0' + oct % 10;
            &tmp[..2]
        } else {
            tmp[0] = b'0' + oct;
            &tmp[..1]
        };
        if n + s.len() > dotted.len() {
            return false;
        }
        dotted[n..n + s.len()].copy_from_slice(s);
        n += s.len();
    }
    let Ok(text) = core::str::from_utf8(&dotted[..n]) else {
        return false;
    };
    addr == text
}

#[cfg(test)]
mod guest_close_test {
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

    fn frame(mac: [u8; 6]) -> SeenFrame {
        SeenFrame {
            src_mac: mac,
            phy_mac: EGRESS_PHY_MAC,
            phylock: false,
        }
    }

    fn login() -> SeenLogin {
        SeenLogin {
            address: [10, 99, 99, 40],
            other_machine: true,
        }
    }

    #[test]
    fn close_needs_the_frame_and_the_login() {
        let nic = nic();
        assert_eq!(judge(&nic, Some(frame(nic.mac)), Some(login())), Close::Met);
        assert_eq!(judge(&nic, None, Some(login())), Close::Open);
        assert_eq!(judge(&nic, Some(frame(nic.mac)), None), Close::Open);
        let mut spa = frame(nic.mac);
        spa.phy_mac = SPA_MAC;
        assert_eq!(judge(&nic, Some(spa), Some(login())), Close::Open);
        assert_eq!(
            judge(&nic, Some(frame(SPA_MAC)), Some(login())),
            Close::Open
        );
        let mut locked = frame(nic.mac);
        locked.phylock = true;
        assert_eq!(judge(&nic, Some(locked), Some(login())), Close::Open);
        let mut here = login();
        here.other_machine = false;
        assert_eq!(judge(&nic, Some(frame(nic.mac)), Some(here)), Close::Open);
        let mut other = login();
        other.address = [10, 99, 99, 41];
        assert_eq!(judge(&nic, Some(frame(nic.mac)), Some(other)), Close::Open);
        assert_eq!(live_close(), Close::Open);
        let src = include_str!("guest_close.rs");
        let com = ["serial", "::write"].concat();
        let out = ["print", "ln!"].concat();
        assert!(!src.contains(&com));
        assert!(!src.contains(&out));
    }
}
