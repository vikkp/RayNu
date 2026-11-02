//! ADR-021 B5. Which installed guests start at the next hypervisor boot.
//!
//! Pillar: [Z]
//! Proven Core: **outside** (ADR-002)
//!
//! The sector sits one LBA before the audit image. An absent or bad sector
//! leaves the SPA wait as it is. A saved list starts the first entry after
//! `delay_secs`. Later entries stay in the sector for when more than one
//! guest runs. Reinstall and a new tail install are not entries.
//!
//! Guests **Boot next** stages the sector. The next hypervisor boot reads it.

use crate::mgmt::megaraid::{classify_ld_bytes, LdClass};
use crate::mgmt::perc_boot_choice::{CHOICE_BOOT, CHOICE_TAIL_BOOT};

pub const MAGIC: &[u8; 8] = b"RAYNUAUT";
const VERSION: u8 = 1;
const HEADER: usize = 16;
const REC: usize = 24;
pub const MAX_ENTRIES: usize = 8;
const CHECK_OFF: usize = 508;

/// One installed disk that may start without a Guests click.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AutoEntry {
    pub spare_off: u64,
    pub disk_bytes: u64,
    /// [`CHOICE_BOOT`] or [`CHOICE_TAIL_BOOT`].
    pub kind: u8,
}

/// Saved order. `count == 0` is the SPA wait.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AutoPolicy {
    pub delay_secs: u16,
    pub count: u8,
    pub entries: [Option<AutoEntry>; MAX_ENTRIES],
}

impl AutoPolicy {
    pub const fn empty() -> Self {
        Self {
            delay_secs: 0,
            count: 0,
            entries: [None; MAX_ENTRIES],
        }
    }
}

/// What this boot should do while the SPA choice is still pending.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BootStep {
    /// No saved list. Keep waiting for Guests.
    SpaWait,
    /// A list is saved and the delay has not elapsed.
    Hold { delay_secs: u16 },
    /// Start this entry. Only the first entry is started on this boot.
    Start(AutoEntry),
}

/// LBA of the autostart sector. One before the audit image. `None` for
/// anything that is not the spare, and never LBA 0.
pub fn autostart_lba(spare_bytes: u64) -> Option<u64> {
    if classify_ld_bytes(spare_bytes) != LdClass::Spare {
        return None;
    }
    let audit = crate::mgmt::megaraid::audit_image_lba(spare_bytes)?;
    let lba = audit.checked_sub(1)?;
    if lba == 0 {
        None
    } else {
        Some(lba)
    }
}

/// Write `policy` into `buf`. `false` when an entry is not an installed boot.
pub fn encode(policy: &AutoPolicy, buf: &mut [u8; 512]) -> bool {
    if policy.count as usize > MAX_ENTRIES {
        return false;
    }
    buf.fill(0);
    buf[..8].copy_from_slice(MAGIC);
    buf[8] = VERSION;
    buf[9] = policy.count;
    buf[10..12].copy_from_slice(&policy.delay_secs.to_le_bytes());
    for i in 0..policy.count as usize {
        let Some(e) = policy.entries[i] else {
            return false;
        };
        if e.kind != CHOICE_BOOT && e.kind != CHOICE_TAIL_BOOT {
            return false;
        }
        if e.disk_bytes == 0 || e.disk_bytes % 512 != 0 || e.spare_off % 512 != 0 {
            return false;
        }
        let at = HEADER + i * REC;
        buf[at..at + 8].copy_from_slice(&e.spare_off.to_le_bytes());
        buf[at + 8..at + 16].copy_from_slice(&e.disk_bytes.to_le_bytes());
        buf[at + 16] = e.kind;
    }
    let sum = checksum(&buf[..CHECK_OFF]);
    buf[CHECK_OFF..CHECK_OFF + 4].copy_from_slice(&sum.to_le_bytes());
    true
}

/// `None` when the magic, version, checksum, or an entry is wrong.
pub fn decode(buf: &[u8]) -> Option<AutoPolicy> {
    if buf.len() < 512 || &buf[..8] != MAGIC || buf[8] != VERSION {
        return None;
    }
    let sum = u32::from_le_bytes(buf[CHECK_OFF..CHECK_OFF + 4].try_into().ok()?);
    if sum != checksum(&buf[..CHECK_OFF]) {
        return None;
    }
    let count = buf[9];
    if count as usize > MAX_ENTRIES {
        return None;
    }
    let delay_secs = u16::from_le_bytes(buf[10..12].try_into().ok()?);
    let mut entries = [None; MAX_ENTRIES];
    for i in 0..count as usize {
        let at = HEADER + i * REC;
        let spare_off = u64::from_le_bytes(buf[at..at + 8].try_into().ok()?);
        let disk_bytes = u64::from_le_bytes(buf[at + 8..at + 16].try_into().ok()?);
        let kind = buf[at + 16];
        if kind != CHOICE_BOOT && kind != CHOICE_TAIL_BOOT {
            return None;
        }
        if disk_bytes == 0 || disk_bytes % 512 != 0 || spare_off % 512 != 0 {
            return None;
        }
        entries[i] = Some(AutoEntry {
            spare_off,
            disk_bytes,
            kind,
        });
    }
    Some(AutoPolicy {
        delay_secs,
        count,
        entries,
    })
}

/// First entry after `delay_secs`. `elapsed_secs == 0` and `delay_secs == 0`
/// starts immediately. An empty policy is the SPA wait.
pub fn step(policy: &AutoPolicy, elapsed_secs: u32) -> BootStep {
    if policy.count == 0 {
        return BootStep::SpaWait;
    }
    let Some(entry) = policy.entries[0] else {
        return BootStep::SpaWait;
    };
    if u32::from(policy.delay_secs) > elapsed_secs {
        BootStep::Hold {
            delay_secs: policy.delay_secs,
        }
    } else {
        BootStep::Start(entry)
    }
}

fn checksum(bytes: &[u8]) -> u32 {
    bytes
        .iter()
        .fold(0u32, |a, b| a.wrapping_add(u32::from(*b)))
}

struct Saved(core::cell::UnsafeCell<AutoPolicy>);
// SAFETY: exclusive access is `SAVED_LOCK`.
// KANI-TARGET: host encode/decode, not this cell.
unsafe impl Sync for Saved {}
static SAVED: Saved = Saved(core::cell::UnsafeCell::new(AutoPolicy::empty()));
static SAVED_LOCK: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);
static TAKEN: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);

fn with_saved<R>(f: impl FnOnce(&mut AutoPolicy) -> R) -> R {
    while SAVED_LOCK.swap(true, core::sync::atomic::Ordering::Acquire) {
        core::hint::spin_loop();
    }
    // SAFETY: lock held.
    let out = unsafe { f(&mut *SAVED.0.get()) };
    SAVED_LOCK.store(false, core::sync::atomic::Ordering::Release);
    out
}

/// Install a decoded sector. A bad image clears the list. This boot's
/// start decision uses that list. Edits from the page stage a sector for
/// the next boot and do not start a guest that is already waiting.
pub fn restore(buf: &[u8]) -> bool {
    match decode(buf) {
        Some(p) => {
            with_saved(|s| *s = p);
            with_shown(|s| *s = p);
            true
        }
        None => {
            with_saved(|s| *s = AutoPolicy::empty());
            with_shown(|s| *s = AutoPolicy::empty());
            false
        }
    }
}

struct Shown(core::cell::UnsafeCell<AutoPolicy>);
// SAFETY: exclusive access is `SHOWN_LOCK`.
// KANI-TARGET: host encode of the staged list, not this cell.
unsafe impl Sync for Shown {}
static SHOWN: Shown = Shown(core::cell::UnsafeCell::new(AutoPolicy::empty()));
static SHOWN_LOCK: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);

struct Dirty(core::cell::UnsafeCell<[u8; 512]>);
// SAFETY: exclusive access is `SHOWN_LOCK`, same as [`SHOWN`].
// KANI-TARGET: host sector round-trip, not this cell.
unsafe impl Sync for Dirty {}
static DIRTY: Dirty = Dirty(core::cell::UnsafeCell::new([0u8; 512]));
static DIRTY_ON: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);
static DIRTY_GEN: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);

fn with_shown<R>(f: impl FnOnce(&mut AutoPolicy) -> R) -> R {
    while SHOWN_LOCK.swap(true, core::sync::atomic::Ordering::Acquire) {
        core::hint::spin_loop();
    }
    // SAFETY: lock held.
    let out = unsafe { f(&mut *SHOWN.0.get()) };
    SHOWN_LOCK.store(false, core::sync::atomic::Ordering::Release);
    out
}

fn stage_locked(policy: &AutoPolicy) -> bool {
    let mut buf = [0u8; 512];
    if !encode(policy, &mut buf) {
        return false;
    }
    // SAFETY: `SHOWN_LOCK` is held by the caller.
    unsafe {
        *DIRTY.0.get() = buf;
    }
    DIRTY_GEN.fetch_add(1, core::sync::atomic::Ordering::Release);
    DIRTY_ON.store(true, core::sync::atomic::Ordering::Release);
    true
}

/// The list the page shows. Edits land here. This boot still uses [`restore`].
pub fn shown() -> AutoPolicy {
    with_shown(|s| *s)
}

/// Put `entry` on the saved list for the next boot. A repeat updates that
/// row. `delay_secs` is how long the next boot waits before the first entry.
pub fn remember(entry: AutoEntry, delay_secs: u16) -> bool {
    if entry.kind != CHOICE_BOOT && entry.kind != CHOICE_TAIL_BOOT {
        return false;
    }
    with_shown(|s| {
        s.delay_secs = delay_secs;
        let mut found = false;
        for i in 0..s.count as usize {
            if let Some(e) = s.entries[i].as_mut() {
                if e.spare_off == entry.spare_off && e.kind == entry.kind {
                    *e = entry;
                    found = true;
                    break;
                }
            }
        }
        if !found {
            if s.count as usize >= MAX_ENTRIES {
                return false;
            }
            s.entries[s.count as usize] = Some(entry);
            s.count += 1;
        }
        stage_locked(s)
    })
}

/// Drop one disk from the next-boot list.
pub fn forget(spare_off: u64, kind: u8) -> bool {
    with_shown(|s| {
        let mut next = [None; MAX_ENTRIES];
        let mut n = 0u8;
        let mut dropped = false;
        for i in 0..s.count as usize {
            let Some(e) = s.entries[i] else {
                continue;
            };
            if e.spare_off == spare_off && e.kind == kind {
                dropped = true;
                continue;
            }
            next[n as usize] = Some(e);
            n += 1;
        }
        if !dropped {
            return false;
        }
        s.entries = next;
        s.count = n;
        stage_locked(s)
    })
}

/// Next boot waits for Guests again.
pub fn clear() -> bool {
    with_shown(|s| {
        *s = AutoPolicy::empty();
        stage_locked(s)
    })
}

/// Staged sector for one WRITE(16). `None` when the page has not edited it.
pub fn copy_dirty() -> Option<(u32, [u8; 512])> {
    if !DIRTY_ON.load(core::sync::atomic::Ordering::Acquire) {
        return None;
    }
    while SHOWN_LOCK.swap(true, core::sync::atomic::Ordering::Acquire) {
        core::hint::spin_loop();
    }
    let gen = DIRTY_GEN.load(core::sync::atomic::Ordering::Acquire);
    // SAFETY: lock held.
    let sector = unsafe { *DIRTY.0.get() };
    SHOWN_LOCK.store(false, core::sync::atomic::Ordering::Release);
    Some((gen, sector))
}

pub fn ack_dirty(gen: u32) {
    if DIRTY_GEN.load(core::sync::atomic::Ordering::Acquire) == gen {
        DIRTY_ON.store(false, core::sync::atomic::Ordering::Release);
    }
}

/// One-shot. `None` while the delay is running, or when no list is saved.
pub fn take_due(elapsed_secs: u32) -> Option<AutoEntry> {
    if TAKEN.load(core::sync::atomic::Ordering::Acquire) {
        return None;
    }
    let step = with_saved(|s| step(s, elapsed_secs));
    match step {
        BootStep::Start(e) => {
            TAKEN.store(true, core::sync::atomic::Ordering::Release);
            Some(e)
        }
        BootStep::Hold { .. } | BootStep::SpaWait => None,
    }
}

pub fn saved_count() -> u8 {
    with_saved(|s| s.count)
}

#[cfg(test)]
pub fn test_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

#[cfg(test)]
mod autostart_test {
    use super::*;
    use crate::mgmt::megaraid::IRON_LD1_BYTES;

    fn tail() -> AutoPolicy {
        let mut p = AutoPolicy::empty();
        p.delay_secs = 30;
        p.count = 1;
        p.entries[0] = Some(AutoEntry {
            spare_off: 19_327_352_832,
            disk_bytes: 10 * 1024 * 1024 * 1024,
            kind: CHOICE_TAIL_BOOT,
        });
        p
    }

    #[test]
    fn empty_sector_keeps_the_spa_wait() {
        let mut buf = [0u8; 512];
        assert!(decode(&buf).is_none());
        assert!(encode(&AutoPolicy::empty(), &mut buf));
        let p = decode(&buf).unwrap();
        assert_eq!(step(&p, 0), BootStep::SpaWait);
        assert!(autostart_lba(IRON_LD1_BYTES).is_some());
        assert!(autostart_lba(400 * 1024 * 1024 * 1024).is_none());
        let audit = crate::mgmt::megaraid::audit_image_lba(IRON_LD1_BYTES).unwrap();
        assert_eq!(autostart_lba(IRON_LD1_BYTES), Some(audit - 1));
    }

    #[test]
    fn delay_holds_then_starts_the_first_entry() {
        let p = tail();
        let mut buf = [0u8; 512];
        assert!(encode(&p, &mut buf));
        buf[20] ^= 0xff;
        assert!(decode(&buf).is_none());
        buf[20] ^= 0xff;
        let back = decode(&buf).unwrap();
        assert_eq!(step(&back, 29), BootStep::Hold { delay_secs: 30 });
        match step(&back, 30) {
            BootStep::Start(e) => {
                assert_eq!(e.kind, CHOICE_TAIL_BOOT);
                assert_eq!(e.spare_off, 19_327_352_832);
            }
            other => panic!("expected start, got {other:?}"),
        }
        let mut bad = p;
        bad.entries[0].as_mut().unwrap().kind = 2;
        assert!(!encode(&bad, &mut buf));
    }

    #[test]
    fn zero_delay_starts_immediately() {
        let mut p = tail();
        p.delay_secs = 0;
        assert!(matches!(step(&p, 0), BootStep::Start(_)));
    }

    #[test]
    fn remember_stages_the_next_boot_and_does_not_start_this_one() {
        let _guard = super::test_lock();
        clear();
        restore(&[0u8; 512]);
        assert!(take_due(10_000).is_none());
        assert!(remember(
            AutoEntry {
                spare_off: 19_327_352_832,
                disk_bytes: 10 * 1024 * 1024 * 1024,
                kind: CHOICE_TAIL_BOOT,
            },
            0,
        ));
        assert!(
            take_due(10_000).is_none(),
            "this boot stays on the SPA wait"
        );
        let (gen, sector) = copy_dirty().unwrap();
        let p = decode(&sector).unwrap();
        assert_eq!(p.count, 1);
        assert_eq!(p.delay_secs, 0);
        ack_dirty(gen);
        assert!(copy_dirty().is_none());
        assert!(forget(19_327_352_832, CHOICE_TAIL_BOOT));
        let (_, cleared) = copy_dirty().unwrap();
        assert_eq!(decode(&cleared).unwrap().count, 0);
    }
}
