//! Saved vCPU count for a Guests row (ADR-021 B7, ADR-023).
//!
//! Pillar: [Z] · [A] · Outside the Proven Core.
//!
//! The sector is one LBA below the host certificate, spare class only,
//! never LBA 0. A missing row starts with 1 vCPU. The count is used at
//! the next Start. A running guest is unchanged.
//!
//! `cpu_max` is what the page offers. B7 reports [`MAX_VCPUS`]. B21 will
//! report `min(MAX_VCPUS, host cores online)`.

use crate::guest::slot::{self, PerGuest};
use crate::mgmt::megaraid::{classify_ld_bytes, LdClass};
use crate::per_guest;
use core::sync::atomic::{AtomicU8, Ordering};

pub const MAGIC: &[u8; 8] = b"RAYNUCPU";
const VERSION: u8 = 1;
const HEADER: usize = 16;
const REC: usize = 16;
pub const MAX_ROWS: usize = 8;
const CHECK_OFF: usize = 508;

/// Count the next Start of the current slot should boot.
static ARMED: PerGuest<AtomicU8> = per_guest!(AtomicU8::new(1));

struct Bank {
    rows: [(u64, u8); MAX_ROWS],
    n: u8,
    dirty: [u8; 512],
    dirty_on: bool,
    gen: u32,
}

impl Bank {
    const fn empty() -> Self {
        Self {
            rows: [(0, 0); MAX_ROWS],
            n: 0,
            dirty: [0; 512],
            dirty_on: false,
            gen: 0,
        }
    }
}

struct Cell(core::cell::UnsafeCell<Bank>);
// SAFETY: the Guests route and the PERC flush take turns on the one CPU
// that runs the management plane. Host tests serialize on `LOCK`.
// KANI-TARGET: guest vCPU-count sector (outside Proven Core).
unsafe impl Sync for Cell {}

static BANK: Cell = Cell(core::cell::UnsafeCell::new(Bank::empty()));
static LOCK: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);

fn with<R>(f: impl FnOnce(&mut Bank) -> R) -> R {
    while LOCK.swap(true, Ordering::Acquire) {
        core::hint::spin_loop();
    }
    // SAFETY: lock held.
    // KANI-TARGET: guest vCPU-count sector lock (outside Proven Core).
    let out = unsafe { f(&mut *BANK.0.get()) };
    LOCK.store(false, Ordering::Release);
    out
}

fn checksum(bytes: &[u8]) -> u32 {
    bytes.iter().fold(0u32, |a, b| a.wrapping_add(u32::from(*b)))
}

/// LBA of the vCPU-count sector. One below the certificate. Spare only.
pub fn cpu_lba(spare_bytes: u64) -> Option<u64> {
    if classify_ld_bytes(spare_bytes) != LdClass::Spare {
        return None;
    }
    let cert = crate::mgmt::host_cert::cert_lba(spare_bytes)?;
    let lba = cert.checked_sub(1)?;
    if lba == 0 {
        None
    } else {
        Some(lba)
    }
}

/// Choices the Guests page offers. One core online, and `onecore.txt`,
/// keep the B7 cap. More than one core online narrows it to that many.
pub fn cpu_max() -> u8 {
    crate::arch::host_smp::cpu_cap(
        slot::MAX_VCPUS as u8,
        crate::arch::host_smp::cores_online(),
        crate::boot::raynu_f_flag::host_onecore(),
    )
}

/// Saved count is above [`cpu_max`]. `(saved, cores online)`.
pub fn start_refusal(spare_off: u64) -> Option<(u8, u8)> {
    let saved = count(spare_off);
    let cap = cpu_max();
    if saved > cap {
        Some((saved, cap))
    } else {
        None
    }
}

/// Encode `rows` (`spare_off`, `n`). `false` when a count is out of range.
pub fn encode(rows: &[(u64, u8)], buf: &mut [u8; 512]) -> bool {
    if rows.len() > MAX_ROWS {
        return false;
    }
    buf.fill(0);
    buf[..8].copy_from_slice(MAGIC);
    buf[8] = VERSION;
    buf[9] = rows.len() as u8;
    for (i, (off, n)) in rows.iter().enumerate() {
        if !(1..=slot::MAX_VCPUS as u8).contains(n) {
            return false;
        }
        let at = HEADER + i * REC;
        buf[at..at + 8].copy_from_slice(&off.to_le_bytes());
        buf[at + 8] = *n;
    }
    let sum = checksum(&buf[..CHECK_OFF]);
    buf[CHECK_OFF..CHECK_OFF + 4].copy_from_slice(&sum.to_le_bytes());
    true
}

/// `None` on bad magic, version, checksum, count, or a duplicate offset.
pub fn decode(buf: &[u8]) -> Option<[(u64, u8); MAX_ROWS]> {
    if buf.len() < 512 || &buf[..8] != MAGIC || buf[8] != VERSION {
        return None;
    }
    let sum = u32::from_le_bytes(buf[CHECK_OFF..CHECK_OFF + 4].try_into().ok()?);
    if sum != checksum(&buf[..CHECK_OFF]) {
        return None;
    }
    let count = buf[9] as usize;
    if count > MAX_ROWS {
        return None;
    }
    let mut rows = [(0u64, 0u8); MAX_ROWS];
    for i in 0..count {
        let at = HEADER + i * REC;
        let off = u64::from_le_bytes(buf[at..at + 8].try_into().ok()?);
        let n = buf[at + 8];
        if !(1..=slot::MAX_VCPUS as u8).contains(&n) {
            return None;
        }
        if rows[..i].iter().any(|(o, _)| *o == off) {
            return None;
        }
        rows[i] = (off, n);
    }
    Some(rows)
}

pub fn restore(sector: &[u8]) {
    let parsed = decode(sector);
    with(|b| {
        b.rows = [(0, 0); MAX_ROWS];
        b.n = 0;
        if let Some(rows) = parsed {
            let mut n = 0u8;
            for (off, count) in rows {
                if count == 0 {
                    break;
                }
                b.rows[n as usize] = (off, count);
                n += 1;
            }
            b.n = n;
        }
    });
}

/// Saved count for `spare_off`, or 1 when the row has none.
pub fn count(spare_off: u64) -> u8 {
    with(|b| {
        b.rows[..b.n as usize]
            .iter()
            .find(|(off, _)| *off == spare_off)
            .map(|(_, n)| *n)
            .unwrap_or(1)
    })
}

/// Rows whose saved count is greater than 1. The page defaults the rest to 1.
pub fn shown() -> [(u64, u8); MAX_ROWS] {
    with(|b| {
        let mut out = [(0u64, 0u8); MAX_ROWS];
        let mut w = 0usize;
        for (off, n) in b.rows[..b.n as usize].iter().copied() {
            if n > 1 && w < MAX_ROWS {
                out[w] = (off, n);
                w += 1;
            }
        }
        out
    })
}

/// Remember `n` for `spare_off`. Takes effect at the next Start.
pub fn save(spare_off: u64, n: u8) -> Result<(), ()> {
    if !(1..=cpu_max()).contains(&n) {
        return Err(());
    }
    with(|b| {
        let mut rows = [(0u64, 0u8); MAX_ROWS];
        let mut count = 0usize;
        let mut replaced = false;
        for (off, cur) in b.rows[..b.n as usize].iter().copied() {
            if off == spare_off {
                rows[count] = (off, n);
                replaced = true;
            } else {
                rows[count] = (off, cur);
            }
            count += 1;
        }
        if !replaced {
            if count >= MAX_ROWS {
                return Err(());
            }
            rows[count] = (spare_off, n);
            count += 1;
        }
        let mut sector = [0u8; 512];
        if !encode(&rows[..count], &mut sector) {
            return Err(());
        }
        b.rows = rows;
        b.n = count as u8;
        b.dirty = sector;
        b.dirty_on = true;
        b.gen = b.gen.wrapping_add(1);
        Ok(())
    })
}

pub fn copy_dirty() -> Option<(u32, [u8; 512])> {
    with(|b| {
        if b.dirty_on {
            Some((b.gen, b.dirty))
        } else {
            None
        }
    })
}

pub fn ack_dirty(gen: u32) {
    with(|b| {
        if b.dirty_on && b.gen == gen {
            b.dirty_on = false;
        }
    });
}

/// The count Start should use, recorded when the Start is accepted.
pub fn on_accepted_start(spare_off: u64) {
    ARMED.store(count(spare_off), Ordering::Release);
}

/// vCPUs for the current slot's next launch. 1 until a Start is accepted.
pub fn armed() -> u8 {
    match ARMED.load(Ordering::Acquire) {
        n if (1..=slot::MAX_VCPUS as u8).contains(&n) => n,
        _ => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mgmt::megaraid::IRON_LD1_BYTES;

    #[test]
    fn encode_round_trip_and_the_reject_cases() {
        let rows = [(100u64, 2u8), (200, 4)];
        let mut buf = [0u8; 512];
        assert!(encode(&rows, &mut buf));
        let got = decode(&buf).unwrap();
        assert_eq!(got[0], (100, 2));
        assert_eq!(got[1], (200, 4));
        assert_eq!(got[2], (0, 0));
        buf[8] = 9;
        assert!(decode(&buf).is_none(), "version");
        assert!(encode(&rows, &mut buf));
        buf[0] = b'X';
        assert!(decode(&buf).is_none(), "magic");
        assert!(encode(&rows, &mut buf));
        buf[CHECK_OFF] ^= 1;
        assert!(decode(&buf).is_none(), "checksum");
        assert!(encode(&[(1, 0)], &mut buf) == false, "n 0");
        assert!(encode(&[(1, 5)], &mut buf) == false, "n 5");
        let mut dup = [0u8; 512];
        assert!(encode(&[(7, 2), (8, 2)], &mut dup));
        dup[HEADER + REC..HEADER + REC + 8].copy_from_slice(&7u64.to_le_bytes());
        let sum = checksum(&dup[..CHECK_OFF]);
        dup[CHECK_OFF..CHECK_OFF + 4].copy_from_slice(&sum.to_le_bytes());
        assert!(decode(&dup).is_none(), "duplicate offset");
    }

    #[test]
    fn cpu_lba_sits_under_the_certificate_and_not_on_another_reserved_lba() {
        let lba = cpu_lba(IRON_LD1_BYTES).unwrap();
        let cert = crate::mgmt::host_cert::cert_lba(IRON_LD1_BYTES).unwrap();
        assert_eq!(lba, cert - 1);
        assert_ne!(lba, 0);
        assert_ne!(lba, crate::mgmt::operator::operator_lba(IRON_LD1_BYTES).unwrap());
        assert_ne!(
            lba,
            crate::mgmt::guest_autostart::autostart_lba(IRON_LD1_BYTES).unwrap()
        );
        assert_ne!(lba, crate::mgmt::megaraid::audit_image_lba(IRON_LD1_BYTES).unwrap());
        assert_ne!(lba, crate::mgmt::host_addr::addr_lba(IRON_LD1_BYTES).unwrap());
        assert!(cpu_lba(0).is_none());
    }

    #[test]
    fn save_is_what_the_next_start_arms() {
        restore(&[0u8; 512]);
        assert_eq!(count(42), 1);
        assert!(save(42, 3).is_ok());
        assert_eq!(count(42), 3);
        let shown = shown();
        assert_eq!(shown[0], (42, 3));
        on_accepted_start(42);
        assert_eq!(armed(), 3);
        assert!(save(42, 1).is_ok());
        assert!(super::shown().iter().all(|(off, _)| *off != 42));
        on_accepted_start(42);
        assert_eq!(armed(), 1);
        assert!(save(42, 0).is_err());
        assert!(save(42, 9).is_err());
    }

    #[test]
    fn save_and_start_refuse_more_vcpus_than_cores_online() {
        restore(&[0u8; 512]);
        assert!(save(99, 4).is_ok());
        crate::arch::host_smp::set_cores_online_for_test(2);
        assert_eq!(cpu_max(), 2);
        assert!(save(99, 3).is_err());
        assert_eq!(start_refusal(99), Some((4, 2)));
        assert!(save(99, 2).is_ok());
        assert_eq!(start_refusal(99), None);
        crate::arch::host_smp::set_cores_online_for_test(1);
        restore(&[0u8; 512]);
    }
}
