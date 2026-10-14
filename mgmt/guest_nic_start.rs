//! M8.9.4: the saved NIC is applied on the next Start (outside Proven Core).
//!
//! Pillar: [Z] [D]
//! Proven Core: **outside** (ADR-002 / ADR-013 Appendix C).
//! VERIFICATION: L1 host tests. Host/CI never print an iron marker.
//!
//! Create leaves the guest with no NIC. A Start that is accepted copies
//! the saved MAC and address into the start plan. A guest that is already
//! on is not hot-plugged. This module does not program a virtio device
//! and does not send a frame on `:39`.

use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicU8, Ordering};

use crate::mgmt::guest_nic::{panel_snapshot, NicRecord, EGRESS_PHY_MAC, SPA_MAC};

/// NIC the next accepted Start will carry. Not a hot-plug.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StartNic {
    pub spare_off: u64,
    pub mac: [u8; 6],
    pub ipv4: [u8; 4],
    pub prefix: u8,
    pub gateway: [u8; 4],
    pub dns: [u8; 4],
    /// Station MAC of the egress port. The guest MAC is `mac`.
    pub egress_phy: [u8; 6],
}

/// Saved record for `spare_off`, when this Start is a fresh boot.
///
/// `guest_already_on` is the hot-plug case and returns none.
pub fn plan(spare_off: u64, guest_already_on: bool) -> Option<StartNic> {
    if guest_already_on {
        return None;
    }
    let rec = panel_snapshot().get(spare_off)?;
    from_record(rec)
}

fn from_record(rec: NicRecord) -> Option<StartNic> {
    if rec.mac == SPA_MAC || rec.mac == EGRESS_PHY_MAC || rec.mac[0] & 1 != 0 {
        return None;
    }
    Some(StartNic {
        spare_off: rec.spare_off,
        mac: rec.mac,
        ipv4: rec.ipv4,
        prefix: rec.prefix,
        gateway: rec.gateway,
        dns: rec.dns,
        egress_phy: EGRESS_PHY_MAC,
    })
}

static SEEN: AtomicBool = AtomicBool::new(false);
static OFF: AtomicU64 = AtomicU64::new(0);
static MAC: AtomicU64 = AtomicU64::new(0);
static IP: AtomicU32 = AtomicU32::new(0);
static PX: AtomicU8 = AtomicU8::new(0);
static GW: AtomicU32 = AtomicU32::new(0);
static DNS: AtomicU32 = AtomicU32::new(0);

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

/// Call only after a Start is accepted. A missing record clears the plan
/// so this boot does not keep a previous guest's NIC.
pub fn on_accepted_start(spare_off: u64) {
    match plan(spare_off, false) {
        Some(nic) => {
            OFF.store(nic.spare_off, Ordering::Relaxed);
            MAC.store(pack_mac(nic.mac), Ordering::Relaxed);
            IP.store(u32::from_be_bytes(nic.ipv4), Ordering::Relaxed);
            PX.store(nic.prefix, Ordering::Relaxed);
            GW.store(u32::from_be_bytes(nic.gateway), Ordering::Relaxed);
            DNS.store(u32::from_be_bytes(nic.dns), Ordering::Relaxed);
            SEEN.store(true, Ordering::Release);
            crate::mgmt::guest_addr::hold_token(&nic);
        }
        None => clear(),
    }
}

fn clear() {
    SEEN.store(false, Ordering::Release);
    crate::mgmt::guest_addr::clear_token();
}

/// NIC carried by the last accepted Start.
#[cfg(test)]
pub fn armed_start() -> Option<StartNic> {
    if !SEEN.load(Ordering::Acquire) {
        return None;
    }
    Some(StartNic {
        spare_off: OFF.load(Ordering::Relaxed),
        mac: unpack_mac(MAC.load(Ordering::Relaxed)),
        ipv4: IP.load(Ordering::Relaxed).to_be_bytes(),
        prefix: PX.load(Ordering::Relaxed),
        gateway: GW.load(Ordering::Relaxed).to_be_bytes(),
        dns: DNS.load(Ordering::Relaxed).to_be_bytes(),
        egress_phy: EGRESS_PHY_MAC,
    })
}

#[cfg(test)]
pub fn clear_start_for_test() {
    clear();
}

#[cfg(test)]
mod guest_nic_start_test {
    use super::*;
    use crate::mgmt::guest_nic::{clear_panel_for_test, panel_save, NicDraft, NicMode};

    fn draft(off: u64) -> NicDraft {
        NicDraft {
            spare_off: off,
            mode: NicMode::Manual,
            prefix: 24,
            ipv4: [10, 99, 99, 40],
            gateway: [10, 99, 99, 1],
            dns: [1, 1, 1, 1],
        }
    }

    #[test]
    fn next_start_carries_the_saved_nic() {
        clear_panel_for_test();
        clear_start_for_test();
        let off = 8u64 * 1024 * 1024 * 1024 + 10u64 * 1024 * 1024 * 1024;
        assert!(plan(off, false).is_none());
        let saved = panel_save(0, draft(off)).unwrap();
        let nic = plan(off, false).unwrap();
        assert_eq!(nic.mac, saved.mac);
        assert_ne!(nic.mac, SPA_MAC);
        assert_ne!(nic.mac, EGRESS_PHY_MAC);
        assert_eq!(nic.ipv4, [10, 99, 99, 40]);
        assert_eq!(nic.prefix, 24);
        assert_eq!(nic.egress_phy, EGRESS_PHY_MAC);
        assert!(plan(off, true).is_none());
        on_accepted_start(off);
        assert_eq!(armed_start().unwrap().mac, saved.mac);
        assert!(crate::mgmt::guest_addr::held_len() > 0);
        on_accepted_start(0);
        assert!(armed_start().is_none());
        assert_eq!(crate::mgmt::guest_addr::held_len(), 0);
        clear_panel_for_test();
        clear_start_for_test();
        let src = include_str!("guest_nic_start.rs");
        let dev = ["virtio", "_net"].concat();
        let tx = ["transmit", "_frame"].concat();
        assert!(!src.contains(&dev));
        assert!(!src.contains(&tx));
    }
}
