//! M8.9.1 per-guest NIC record on RAYNU-SPARE (outside Proven Core).
//!
//! Pillar: [Z] [D]
//! Proven Core: **outside** (ADR-002 / ADR-013 Appendix C).
//! VERIFICATION: L1 host tests. Host/CI never print an iron marker.
//!
//! Create does not write a record. An empty or unrecognized sector means
//! the guest has no NIC, including the Ubuntu disk at 8 GiB + 10 GiB.
//! Save pins a minted MAC and an address. A later Save keeps that MAC.
//! Detach removes that guest only. This module does not write the sector
//! and does not attach a virtio device.

use core::sync::atomic::{AtomicU32, AtomicU64, AtomicU8, Ordering};

use crate::mgmt::megaraid::{classify_ld_bytes, LdClass, NIC_CATALOG_BYTES, SPARE_HIGH_RESERVE};

/// SPA LOM `01:00.0`. A guest MAC must not be this.
pub const SPA_MAC: [u8; 6] = [0xb0, 0x26, 0x28, 0x5c, 0x5a, 0x38];
/// This lab's guest-LAN PHY `01:00.1`. It does not choose the egress port
/// (ADR-021 D4: `guest_egress::discover` does, from the SPA's position);
/// it is kept so a minted guest MAC never collides with it, and as the
/// test fixture for that port.
pub const EGRESS_PHY_MAC: [u8; 6] = [0xb0, 0x26, 0x28, 0x5c, 0x5a, 0x39];

const MAGIC: &[u8; 8] = b"RAYNUNIC";
const VERSION: u8 = 1;
const SLOT_CAP: usize = 8;
const SLOT_BYTES: usize = 48;
const HEADER_BYTES: usize = 16;
const CHECK_OFF: usize = 508;

/// How the admin pinned the address. Get from DHCP and manual entry both Save.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum NicMode {
    DhcpPinned = 1,
    Manual = 2,
}

/// One saved NIC. Absence of a record is "no NIC".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NicRecord {
    pub spare_off: u64,
    pub mac: [u8; 6],
    pub mode: NicMode,
    pub prefix: u8,
    pub ipv4: [u8; 4],
    pub gateway: [u8; 4],
    pub dns: [u8; 4],
}

/// Fields the admin Saves. The MAC is not one of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NicDraft {
    pub spare_off: u64,
    pub mode: NicMode,
    pub prefix: u8,
    pub ipv4: [u8; 4],
    pub gateway: [u8; 4],
    pub dns: [u8; 4],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NicError {
    /// Not a spare VD, or the key is not a disk that starts below the catalog.
    BadDisk,
    /// Prefix or IPv4 cannot be pinned.
    BadAddress,
    /// Eight guests already have a record.
    Full,
}

/// Records in one catalog sector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NicCatalog {
    slots: [Option<NicRecord>; SLOT_CAP],
}

impl NicCatalog {
    pub const fn empty() -> Self {
        Self {
            slots: [None; SLOT_CAP],
        }
    }

    pub fn get(&self, spare_off: u64) -> Option<NicRecord> {
        self.slots
            .iter()
            .flatten()
            .find(|r| r.spare_off == spare_off)
            .copied()
    }

    /// Pin `draft` on `spare_off`. The first Save mints the MAC. A later Save
    /// keeps it and replaces the address fields.
    pub fn slot(&self, i: usize) -> Option<NicRecord> {
        self.slots.get(i).copied().flatten()
    }

    pub fn save(&mut self, spare_bytes: u64, draft: NicDraft) -> Result<NicRecord, NicError> {
        accept_draft(spare_bytes, &draft)?;
        if let Some(idx) = self.index_of(draft.spare_off) {
            let rec = self.slots[idx].as_mut().expect("index matches a record");
            rec.mode = draft.mode;
            rec.prefix = draft.prefix;
            rec.ipv4 = draft.ipv4;
            rec.gateway = draft.gateway;
            rec.dns = draft.dns;
            return Ok(*rec);
        }
        let idx = self
            .slots
            .iter()
            .position(|s| s.is_none())
            .ok_or(NicError::Full)?;
        let mac = mint_unique(draft.spare_off, self);
        let rec = NicRecord {
            spare_off: draft.spare_off,
            mac,
            mode: draft.mode,
            prefix: draft.prefix,
            ipv4: draft.ipv4,
            gateway: draft.gateway,
            dns: draft.dns,
        };
        self.slots[idx] = Some(rec);
        Ok(rec)
    }

    /// Remove one guest. Other guests stay. `false` when that guest had no NIC.
    pub fn detach(&mut self, spare_off: u64) -> bool {
        let Some(idx) = self.index_of(spare_off) else {
            return false;
        };
        self.slots[idx] = None;
        true
    }

    fn index_of(&self, spare_off: u64) -> Option<usize> {
        self.slots
            .iter()
            .position(|s| s.is_some_and(|r| r.spare_off == spare_off))
    }
}

fn accept_draft(spare_bytes: u64, draft: &NicDraft) -> Result<(), NicError> {
    if spare_bytes == 0 {
        if draft.spare_off % 512 != 0 {
            return Err(NicError::BadDisk);
        }
    } else {
        let Some(limit) = catalog_off(spare_bytes) else {
            return Err(NicError::BadDisk);
        };
        if draft.spare_off % 512 != 0 || draft.spare_off >= limit {
            return Err(NicError::BadDisk);
        }
    }
    if draft.prefix == 0 || draft.prefix > 32 {
        return Err(NicError::BadAddress);
    }
    if draft.ipv4 == [0, 0, 0, 0] || draft.ipv4 == [255, 255, 255, 255] {
        return Err(NicError::BadAddress);
    }
    Ok(())
}

/// Spare offset of the catalog sector. `None` when `spare_bytes` is not RAYNU-SPARE.
pub fn catalog_off(spare_bytes: u64) -> Option<u64> {
    if classify_ld_bytes(spare_bytes) != LdClass::Spare || spare_bytes % 512 != 0 {
        return None;
    }
    let off = spare_bytes.checked_sub(SPARE_HIGH_RESERVE)?;
    if off % 512 != 0 {
        return None;
    }
    Some(off)
}

/// Locally administered unicast MAC. Not the SPA MAC and not the egress PHY.
pub fn mint_mac(spare_off: u64) -> [u8; 6] {
    let mut x = spare_off ^ 0x524E_5638_4D38_39;
    x = x.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    [
        0x02,
        0x52,
        0x4E,
        (x >> 8) as u8,
        (x >> 16) as u8,
        (x >> 24) as u8,
    ]
}

fn mint_unique(spare_off: u64, catalog: &NicCatalog) -> [u8; 6] {
    for n in 0..16u64 {
        let mac = mint_mac(spare_off.wrapping_add(n.wrapping_mul(512)));
        if mac == SPA_MAC || mac == EGRESS_PHY_MAC {
            continue;
        }
        let taken = catalog.slots.iter().flatten().any(|r| r.mac == mac);
        if !taken {
            return mac;
        }
    }
    [0x02, 0x52, 0x4E, 0x01, 0x00, spare_off as u8]
}

/// Bytes of one catalog. A zeroed sector and a sector that fails the magic
/// or the checksum both decode as no NICs.
pub fn decode(sector: &[u8]) -> NicCatalog {
    if sector.len() < NIC_CATALOG_BYTES as usize || &sector[..8] != MAGIC {
        return NicCatalog::empty();
    }
    if sector[8] != VERSION {
        return NicCatalog::empty();
    }
    if checksum(&sector[..CHECK_OFF])
        != u32::from_le_bytes(
            sector[CHECK_OFF..CHECK_OFF + 4]
                .try_into()
                .unwrap_or([0; 4]),
        )
    {
        return NicCatalog::empty();
    }
    let mut out = NicCatalog::empty();
    let mut seen = [false; SLOT_CAP];
    for i in 0..SLOT_CAP {
        let start = HEADER_BYTES + i * SLOT_BYTES;
        let slot = &sector[start..start + SLOT_BYTES];
        if slot[0] == 0 {
            continue;
        }
        if slot[0] != 1 {
            return NicCatalog::empty();
        }
        let Some(mode) = mode_from(slot[1]) else {
            return NicCatalog::empty();
        };
        let prefix = slot[2];
        if prefix == 0 || prefix > 32 {
            return NicCatalog::empty();
        }
        let mut mac = [0u8; 6];
        mac.copy_from_slice(&slot[4..10]);
        if mac == SPA_MAC || mac == EGRESS_PHY_MAC || mac[0] & 0x01 != 0 {
            return NicCatalog::empty();
        }
        let spare_off = u64::from_le_bytes(slot[12..20].try_into().unwrap_or([0; 8]));
        if seen.iter().any(|&s| s) && out.get(spare_off).is_some() {
            return NicCatalog::empty();
        }
        let mut ipv4 = [0u8; 4];
        let mut gateway = [0u8; 4];
        let mut dns = [0u8; 4];
        ipv4.copy_from_slice(&slot[20..24]);
        gateway.copy_from_slice(&slot[24..28]);
        dns.copy_from_slice(&slot[28..32]);
        if ipv4 == [0, 0, 0, 0] || ipv4 == [255, 255, 255, 255] {
            return NicCatalog::empty();
        }
        out.slots[i] = Some(NicRecord {
            spare_off,
            mac,
            mode,
            prefix,
            ipv4,
            gateway,
            dns,
        });
        seen[i] = true;
    }
    out
}

pub fn encode(catalog: &NicCatalog) -> [u8; 512] {
    let mut sector = [0u8; 512];
    sector[..8].copy_from_slice(MAGIC);
    sector[8] = VERSION;
    for (i, slot) in catalog.slots.iter().enumerate() {
        let Some(rec) = slot else {
            continue;
        };
        let start = HEADER_BYTES + i * SLOT_BYTES;
        sector[start] = 1;
        sector[start + 1] = rec.mode as u8;
        sector[start + 2] = rec.prefix;
        sector[start + 4..start + 10].copy_from_slice(&rec.mac);
        sector[start + 12..start + 20].copy_from_slice(&rec.spare_off.to_le_bytes());
        sector[start + 20..start + 24].copy_from_slice(&rec.ipv4);
        sector[start + 24..start + 28].copy_from_slice(&rec.gateway);
        sector[start + 28..start + 32].copy_from_slice(&rec.dns);
    }
    let sum = checksum(&sector[..CHECK_OFF]);
    sector[CHECK_OFF..CHECK_OFF + 4].copy_from_slice(&sum.to_le_bytes());
    sector
}

fn mode_from(b: u8) -> Option<NicMode> {
    match b {
        1 => Some(NicMode::DhcpPinned),
        2 => Some(NicMode::Manual),
        _ => None,
    }
}

fn checksum(bytes: &[u8]) -> u32 {
    let mut s = 0u32;
    for b in bytes {
        s = s.wrapping_add(u32::from(*b));
    }
    s
}

struct AtomSlot {
    live: AtomicU8,
    prefix: AtomicU8,
    spare: AtomicU64,
    mac: AtomicU64,
    ip: AtomicU32,
    gw: AtomicU32,
    dns: AtomicU32,
}

impl AtomSlot {
    const fn new() -> Self {
        Self {
            live: AtomicU8::new(0),
            prefix: AtomicU8::new(0),
            spare: AtomicU64::new(0),
            mac: AtomicU64::new(0),
            ip: AtomicU32::new(0),
            gw: AtomicU32::new(0),
            dns: AtomicU32::new(0),
        }
    }
}

static PANEL_SEQ: AtomicU32 = AtomicU32::new(0);
static PANEL: [AtomSlot; SLOT_CAP] = [
    AtomSlot::new(),
    AtomSlot::new(),
    AtomSlot::new(),
    AtomSlot::new(),
    AtomSlot::new(),
    AtomSlot::new(),
    AtomSlot::new(),
    AtomSlot::new(),
];

fn lock_panel() {
    loop {
        let s = PANEL_SEQ.load(Ordering::Acquire);
        if s & 1 == 0
            && PANEL_SEQ
                .compare_exchange(s, s.wrapping_add(1), Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
        {
            return;
        }
        core::hint::spin_loop();
    }
}

fn unlock_panel() {
    PANEL_SEQ.fetch_add(1, Ordering::Release);
}

fn slots_to_catalog() -> NicCatalog {
    let mut cat = NicCatalog::empty();
    for (i, slot) in PANEL.iter().enumerate() {
        let live = slot.live.load(Ordering::Relaxed);
        let Some(mode) = mode_from(live) else {
            continue;
        };
        let mac_u = slot.mac.load(Ordering::Relaxed);
        let mut mac = [0u8; 6];
        mac.copy_from_slice(&mac_u.to_le_bytes()[..6]);
        cat.slots[i] = Some(NicRecord {
            spare_off: slot.spare.load(Ordering::Relaxed),
            mac,
            mode,
            prefix: slot.prefix.load(Ordering::Relaxed),
            ipv4: slot.ip.load(Ordering::Relaxed).to_be_bytes(),
            gateway: slot.gw.load(Ordering::Relaxed).to_be_bytes(),
            dns: slot.dns.load(Ordering::Relaxed).to_be_bytes(),
        });
    }
    cat
}

fn catalog_to_slots(cat: &NicCatalog) {
    for (i, slot) in PANEL.iter().enumerate() {
        match cat.slots[i] {
            None => {
                slot.live.store(0, Ordering::Relaxed);
            }
            Some(rec) => {
                let mut mac_b = [0u8; 8];
                mac_b[..6].copy_from_slice(&rec.mac);
                slot.spare.store(rec.spare_off, Ordering::Relaxed);
                slot.mac.store(u64::from_le_bytes(mac_b), Ordering::Relaxed);
                slot.ip
                    .store(u32::from_be_bytes(rec.ipv4), Ordering::Relaxed);
                slot.gw
                    .store(u32::from_be_bytes(rec.gateway), Ordering::Relaxed);
                slot.dns
                    .store(u32::from_be_bytes(rec.dns), Ordering::Relaxed);
                slot.prefix.store(rec.prefix, Ordering::Relaxed);
                slot.live.store(rec.mode as u8, Ordering::Relaxed);
            }
        }
    }
}

/// The records the SPA last Saved. Same bytes a refresh reads.
pub fn panel_snapshot() -> NicCatalog {
    loop {
        let s = PANEL_SEQ.load(Ordering::Acquire);
        if s & 1 != 0 {
            core::hint::spin_loop();
            continue;
        }
        let cat = slots_to_catalog();
        if PANEL_SEQ.load(Ordering::Acquire) == s {
            return cat;
        }
    }
}

/// Pin one guest. Does not write the spare sector and does not attach virtio.
pub fn panel_save(spare_bytes: u64, draft: NicDraft) -> Result<NicRecord, NicError> {
    lock_panel();
    let mut cat = slots_to_catalog();
    let result = cat.save(spare_bytes, draft);
    if result.is_ok() {
        catalog_to_slots(&cat);
    }
    unlock_panel();
    result
}

/// Drop one guest's NIC. Other guests stay.
pub fn panel_detach(spare_off: u64) -> bool {
    lock_panel();
    let mut cat = slots_to_catalog();
    let removed = cat.detach(spare_off);
    if removed {
        catalog_to_slots(&cat);
    }
    unlock_panel();
    removed
}

/// Host tests only.
#[cfg(test)]
pub fn clear_panel_for_test() {
    lock_panel();
    catalog_to_slots(&NicCatalog::empty());
    unlock_panel();
}

#[cfg(test)]
mod guest_nic_test {
    use super::*;
    use crate::mgmt::iso_library::place_guest_after_occupied;
    use crate::mgmt::megaraid::{
        place_in_free_tail, IRON_LD1_BYTES, LAB_UBUNTU0_BYTES, PERC_IMAGE_BYTES,
    };

    fn draft(spare_off: u64, ip: [u8; 4]) -> NicDraft {
        NicDraft {
            spare_off,
            mode: NicMode::Manual,
            prefix: 24,
            ipv4: ip,
            gateway: [10, 99, 99, 1],
            dns: [10, 99, 99, 1],
        }
    }

    #[test]
    fn m891_empty_sector_is_no_nic_on_the_ubuntu_disk_and_the_window() {
        let spare = IRON_LD1_BYTES;
        let cat = catalog_off(spare).unwrap();
        assert_eq!(cat + NIC_CATALOG_BYTES + 512, spare);
        assert!(catalog_off(LAB_UBUNTU0_BYTES).is_none());
        let ubuntu = PERC_IMAGE_BYTES + 10 * 1024 * 1024 * 1024;
        let ubuntu_end = ubuntu + 10 * 1024 * 1024 * 1024;
        assert!(ubuntu_end < cat);
        assert!(PERC_IMAGE_BYTES < cat);
        let none = decode(&[0u8; 512]);
        assert!(none.get(0).is_none());
        assert!(none.get(ubuntu).is_none());
        let mut garbage = [0xA5u8; 512];
        garbage[..5].copy_from_slice(b"CD001");
        assert!(decode(&garbage).get(ubuntu).is_none());
        let mut catlog = NicCatalog::empty();
        assert!(catlog.save(spare, draft(ubuntu, [10, 99, 99, 40])).is_ok());
        assert!(catlog.get(0).is_none(), "the window stays without a NIC");
        let placed = place_guest_after_occupied(
            spare,
            PERC_IMAGE_BYTES,
            true,
            ubuntu_end,
            0,
            10 * 1024 * 1024 * 1024,
        )
        .unwrap();
        assert!(decode(&encode(&NicCatalog::empty()))
            .get(placed.spare_off)
            .is_none());
        assert!(
            place_in_free_tail(spare, PERC_IMAGE_BYTES, true, spare - PERC_IMAGE_BYTES).is_none()
        );
    }

    #[test]
    fn m891_save_keeps_the_mac_and_a_dropped_struct_reads_it_back() {
        let spare = IRON_LD1_BYTES;
        let ubuntu = PERC_IMAGE_BYTES + 10 * 1024 * 1024 * 1024;
        let mut cat = NicCatalog::empty();
        let first = cat.save(spare, draft(ubuntu, [10, 99, 99, 40])).unwrap();
        assert_eq!(first.mac[0] & 0x03, 0x02);
        assert_ne!(first.mac, SPA_MAC);
        assert_ne!(first.mac, EGRESS_PHY_MAC);
        let mut again = draft(ubuntu, [10, 99, 99, 41]);
        again.mode = NicMode::DhcpPinned;
        again.prefix = 22;
        again.gateway = [0, 0, 0, 0];
        again.dns = [0, 0, 0, 0];
        let second = cat.save(spare, again).unwrap();
        assert_eq!(second.mac, first.mac);
        assert_eq!(second.ipv4, [10, 99, 99, 41]);
        assert_eq!(second.mode, NicMode::DhcpPinned);
        let window = cat.save(spare, draft(0, [10, 99, 99, 50])).unwrap();
        assert_ne!(window.mac, first.mac);
        let bytes = encode(&cat);
        let restored = decode(&bytes);
        assert_eq!(restored.get(ubuntu).unwrap().mac, first.mac);
        assert_eq!(restored.get(ubuntu).unwrap().ipv4, [10, 99, 99, 41]);
        assert_eq!(restored.get(0).unwrap().ipv4, [10, 99, 99, 50]);
        assert!(cat.detach(ubuntu));
        assert!(cat.get(ubuntu).is_none());
        assert!(cat.get(0).is_some());
        assert!(!cat.detach(ubuntu));
        let mut flipped = bytes;
        flipped[20] ^= 0x01;
        assert!(decode(&flipped).get(ubuntu).is_none());
        let mut bad = draft(ubuntu, [0, 0, 0, 0]);
        assert_eq!(cat.save(spare, bad), Err(NicError::BadAddress));
        bad.ipv4 = [10, 1, 1, 1];
        bad.prefix = 0;
        assert_eq!(cat.save(spare, bad), Err(NicError::BadAddress));
        bad.prefix = 33;
        assert_eq!(cat.save(spare, bad), Err(NicError::BadAddress));
        bad.spare_off = catalog_off(spare).unwrap();
        bad.prefix = 24;
        bad.ipv4 = [10, 1, 1, 1];
        assert_eq!(cat.save(spare, bad), Err(NicError::BadDisk));
        assert_eq!(
            cat.save(LAB_UBUNTU0_BYTES, draft(0, [10, 1, 1, 1])),
            Err(NicError::BadDisk)
        );
    }

    #[test]
    fn m891_eight_records_fill_the_sector() {
        let spare = IRON_LD1_BYTES;
        let mut cat = NicCatalog::empty();
        for n in 0..8u64 {
            let off = n * 1024 * 1024 * 1024;
            cat.save(spare, draft(off, [10, 0, 0, n as u8 + 1]))
                .unwrap();
        }
        assert_eq!(
            cat.save(spare, draft(9 * 1024 * 1024 * 1024, [10, 0, 0, 9])),
            Err(NicError::Full)
        );
        let mut back = decode(&encode(&cat));
        assert_eq!(back.get(0).unwrap().ipv4, [10, 0, 0, 1]);
        assert_eq!(
            back.get(7 * 1024 * 1024 * 1024).unwrap().ipv4,
            [10, 0, 0, 8]
        );
        assert!(back.detach(0));
        assert!(back
            .save(spare, draft(9 * 1024 * 1024 * 1024, [10, 0, 0, 9]))
            .is_ok());
    }
}
