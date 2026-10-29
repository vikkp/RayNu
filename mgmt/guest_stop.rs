//! Last reason a guest stopped, for the SPA (ADR-021 B20).
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside**. The exit path records; HTTP reads.
//! One record, the latest stop. Not an iron marker.
//!
//! JUSTIFICATION (global state): one chassis, one running guest. The VM-exit
//! path writes the record and the management HTTP handler reads it. The
//! spinlock is the only access. Holders do not sleep.

use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, Ordering};

/// Why the guest left the CPU.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StopKind {
    OperatorStop,
    ExitCap,
    WallCap,
    TripleFault,
    EntryFail,
    Unhandled,
}

/// The fields COM2 already prints on `guest-UEFI stop n=`, plus a kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StopRecord {
    pub kind: StopKind,
    pub reason: u32,
    pub rip: u64,
    pub n: u64,
    pub cap: u64,
    pub tf: bool,
    pub entry_fail: bool,
    pub intr: u64,
}

impl StopKind {
    /// JSON token.
    pub fn name(self) -> &'static str {
        match self {
            StopKind::OperatorStop => "operator",
            StopKind::ExitCap => "exit-cap",
            StopKind::WallCap => "wall-cap",
            StopKind::TripleFault => "triple-fault",
            StopKind::EntryFail => "entry-fail",
            StopKind::Unhandled => "unhandled",
        }
    }

    /// One sentence an operator can read. No quotes: it is embedded in JSON.
    pub fn sentence(self) -> &'static str {
        match self {
            StopKind::OperatorStop => "Stopped from the Guests page.",
            StopKind::ExitCap => "Exit cap reached.",
            StopKind::WallCap => "Wall-clock cap reached.",
            StopKind::TripleFault => "Guest hit a triple fault.",
            StopKind::EntryFail => "VM-entry failed.",
            StopKind::Unhandled => "Guest stopped on an unhandled exit.",
        }
    }
}

/// Kind for the `guest-UEFI stop n=` line. Wall-cap is not visible there;
/// that one arrives through [`kind_from_raynu_f`].
pub fn kind_from_exit(tf: bool, entry_fail: bool, n: u64, cap: u64) -> StopKind {
    if tf {
        StopKind::TripleFault
    } else if entry_fail {
        StopKind::EntryFail
    } else if cap != 0 && n >= cap {
        StopKind::ExitCap
    } else {
        StopKind::Unhandled
    }
}

/// Kind for `boot: RayNu-F stop <why>`. Unknown whys stay unhandled.
pub fn kind_from_raynu_f(why: &str) -> StopKind {
    match why {
        "spa-stop" => StopKind::OperatorStop,
        "exit-cap" | "reset-cap" => StopKind::ExitCap,
        "wall-cap" => StopKind::WallCap,
        "triple-fault" => StopKind::TripleFault,
        "entry-fail" => StopKind::EntryFail,
        _ => StopKind::Unhandled,
    }
}

struct Slot(UnsafeCell<Option<StopRecord>>);

// SAFETY: exclusive access is enforced by `LOCK` in `with_slot`.
// KANI-TARGET: guest_stop note / last (host).
unsafe impl Sync for Slot {}

static SLOT: Slot = Slot(UnsafeCell::new(None));
static LOCK: AtomicBool = AtomicBool::new(false);

fn with_slot<R>(f: impl FnOnce(&mut Option<StopRecord>) -> R) -> R {
    while LOCK.swap(true, Ordering::Acquire) {
        core::hint::spin_loop();
    }
    // SAFETY: lock held; exclusive mutable access to the one record.
    let out = unsafe { f(&mut *SLOT.0.get()) };
    LOCK.store(false, Ordering::Release);
    out
}

/// Replace the last stop. The newer stop wins.
pub fn note(rec: StopRecord) {
    with_slot(|s| *s = Some(rec));
}

/// Record the `guest-UEFI stop n=` exit. Does not change what that line prints.
pub fn note_exit(reason: u32, rip: u64, n: u64, cap: u64, tf: bool, entry_fail: bool, intr: u64) {
    note(StopRecord {
        kind: kind_from_exit(tf, entry_fail, n, cap),
        reason,
        rip,
        n,
        cap,
        tf,
        entry_fail,
        intr,
    });
}

/// Record `boot: RayNu-F stop <why>`. Does not change what that line prints.
pub fn note_raynu_f(why: &str, reason: u32, rip: u64, n: u64, cap: u64, intr: u64) {
    let kind = kind_from_raynu_f(why);
    note(StopRecord {
        kind,
        reason,
        rip,
        n,
        cap,
        tf: kind == StopKind::TripleFault,
        entry_fail: kind == StopKind::EntryFail,
        intr,
    });
}

/// The SPA-stop idle. A reason already recorded by the exit path stays.
/// An idle with no record is an operator stop.
pub fn note_operator_if_empty() {
    with_slot(|s| {
        if s.is_none() {
            *s = Some(StopRecord {
                kind: StopKind::OperatorStop,
                reason: 0,
                rip: 0,
                n: 0,
                cap: 0,
                tf: false,
                entry_fail: false,
                intr: 0,
            });
        }
    });
}

/// The latest stop, if any guest has stopped since boot.
pub fn last() -> Option<StopRecord> {
    with_slot(|s| *s)
}

/// Host tests start from no record.
pub fn reset_for_test() {
    with_slot(|s| *s = None);
}

#[cfg(test)]
#[path = "guest_stop_test.rs"]
mod guest_stop_test;
