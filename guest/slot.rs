//! Guest slots: which product guest the CPU is serving, and per-guest state.
//!
//! Pillar: [Z] · Outside the Proven Core (ADR-022).
//!
//! Every product guest is one instance of the same machine. The exit path,
//! the devices and the firmware are written once. Their state lives in
//! [`PerGuest`] / [`PerGuestCell`] banks with one entry per slot, and the
//! entry in use is the slot named by [`current`]. A second guest is not a
//! second code path; it is a second entry in every bank.
//!
//! INVARIANTS:
//!   - `current() < MAX_GUESTS` always.
//!   - Only the guest scheduler ([`crate::vmx::guest_sched`]) and
//!     [`with_slot`] change the current slot.
//!   - State reached through a bank belongs to exactly one slot. No bank
//!     entry is shared between two guests.
//!
//! One physical CPU runs the guests today, and the host never preempts
//! itself, so one current-slot word is enough. Multi-vCPU (ADR-021 B7) makes
//! the word per physical CPU; the banks do not change.

use core::cell::UnsafeCell;
use core::ops::Deref;
#[cfg(not(test))]
use core::sync::atomic::{AtomicUsize, Ordering};

/// Product guests that can exist at once. Each slot costs one copy of the
/// per-guest state banks; the machine itself (RAM, EPT, VMCS) is allocated
/// only when a slot is built.
pub const MAX_GUESTS: usize = 4;

#[cfg(not(test))]
static CURRENT: AtomicUsize = AtomicUsize::new(0);

// Host tests run on many threads. A thread-local keeps one test's slot
// selection out of another's.
#[cfg(test)]
std::thread_local! {
    static CURRENT: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
}

/// Slot whose state the banks resolve to.
#[inline(always)]
pub fn current() -> usize {
    #[cfg(not(test))]
    {
        CURRENT.load(Ordering::Relaxed)
    }
    #[cfg(test)]
    {
        CURRENT.with(|c| c.get())
    }
}

/// Select `slot`. Returns the slot that was current.
///
/// INVARIANTS:
///   - `slot < MAX_GUESTS` (an out-of-range slot is refused and the current
///     slot is left alone).
///   - Caller is the guest scheduler at a guest switch, or [`with_slot`].
#[inline]
pub fn set_current(slot: usize) -> usize {
    let prev = current();
    if slot >= MAX_GUESTS {
        debug_assert!(slot < MAX_GUESTS, "guest slot out of range");
        return prev;
    }
    #[cfg(not(test))]
    CURRENT.store(slot, Ordering::Relaxed);
    #[cfg(test)]
    CURRENT.with(|c| c.set(slot));
    prev
}

/// Run `f` with `slot` selected, then restore the slot that was current.
///
/// Host-side code (the management plane, the uplink receive path) uses this
/// to reach a guest that is not the one that took the VM exit.
#[inline]
pub fn with_slot<R>(slot: usize, f: impl FnOnce() -> R) -> R {
    let prev = set_current(slot);
    let r = f();
    set_current(prev);
    r
}

/// One `T` per guest slot. Dereferences to the current slot's `T`.
#[repr(transparent)]
pub struct PerGuest<T>([T; MAX_GUESTS]);

impl<T> PerGuest<T> {
    pub const fn new(bank: [T; MAX_GUESTS]) -> Self {
        Self(bank)
    }

    /// The entry for `slot`, whichever slot is current.
    #[inline]
    pub fn of(&self, slot: usize) -> &T {
        &self.0[slot % MAX_GUESTS]
    }
}

impl<T> Deref for PerGuest<T> {
    type Target = T;
    #[inline(always)]
    fn deref(&self) -> &T {
        &self.0[current()]
    }
}

/// One mutable `T` per guest slot, for state the exit path owns outright
/// (the former `static mut` items).
///
/// INVARIANTS:
///   - An entry is touched only while its guest is not executing (VMX root,
///     one CPU), so access is exclusive without a lock.
#[repr(transparent)]
pub struct PerGuestCell<T>(UnsafeCell<[T; MAX_GUESTS]>);

// SAFETY: every access is from VMX root on the one CPU that runs guests, and
// an entry is only reached while its guest is stopped at a VM exit. There is
// no concurrent access to share.
// KANI-TARGET: per-guest cell access (outside Proven Core).
unsafe impl<T> Sync for PerGuestCell<T> {}

impl<T> PerGuestCell<T> {
    pub const fn new(bank: [T; MAX_GUESTS]) -> Self {
        Self(UnsafeCell::new(bank))
    }

    /// Pointer to the current slot's entry.
    #[inline(always)]
    pub fn p(&self) -> *mut T {
        self.p_of(current())
    }

    /// Pointer to `slot`'s entry.
    #[inline(always)]
    pub fn p_of(&self, slot: usize) -> *mut T {
        // SAFETY: index is reduced into the bank; the pointer stays inside
        // the array this cell owns.
        // KANI-TARGET: per-guest cell index (outside Proven Core).
        unsafe { (self.0.get() as *mut T).add(slot % MAX_GUESTS) }
    }
}

/// `per_guest!(init)` builds a [`PerGuest`] bank with every slot set to the
/// constant `init`.
#[macro_export]
macro_rules! per_guest {
    ($init:expr) => {
        $crate::guest::slot::PerGuest::new([const { $init }; $crate::guest::slot::MAX_GUESTS])
    };
}

/// `per_guest_cell!(init)` builds a [`PerGuestCell`] bank with every slot set
/// to the constant `init`.
#[macro_export]
macro_rules! per_guest_cell {
    ($init:expr) => {
        $crate::guest::slot::PerGuestCell::new(
            [const { $init }; $crate::guest::slot::MAX_GUESTS],
        )
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::sync::atomic::{AtomicU32, Ordering};

    static COUNT: PerGuest<AtomicU32> = per_guest!(AtomicU32::new(7));
    static CELL: PerGuestCell<u64> = per_guest_cell!(1);

    #[test]
    fn every_slot_starts_from_the_same_constant() {
        for slot in 0..MAX_GUESTS {
            assert_eq!(COUNT.of(slot).load(Ordering::Relaxed) % 7, 0);
            // SAFETY: test-only read of a slot no other test writes below 1.
            assert!(unsafe { *CELL.p_of(slot) } >= 1);
        }
    }

    #[test]
    fn a_write_in_one_slot_is_not_seen_in_another() {
        static N: PerGuest<AtomicU32> = per_guest!(AtomicU32::new(0));
        with_slot(1, || N.store(11, Ordering::Relaxed));
        with_slot(2, || N.store(22, Ordering::Relaxed));
        assert_eq!(N.load(Ordering::Relaxed), 0);
        assert_eq!(N.of(1).load(Ordering::Relaxed), 11);
        assert_eq!(with_slot(2, || N.load(Ordering::Relaxed)), 22);
        assert_eq!(N.of(3).load(Ordering::Relaxed), 0);
    }

    #[test]
    fn with_slot_restores_the_slot_that_was_current() {
        assert_eq!(current(), 0);
        let inner = with_slot(3, || {
            let seen = current();
            let nested = with_slot(1, current);
            (seen, nested, current())
        });
        assert_eq!(inner, (3, 1, 3));
        assert_eq!(current(), 0);
    }

    #[test]
    fn an_out_of_range_slot_leaves_the_current_slot_alone() {
        assert_eq!(current(), 0);
        let r = std::panic::catch_unwind(|| set_current(MAX_GUESTS));
        // Debug builds assert; release builds refuse. Neither moves the slot.
        let _ = r;
        assert_eq!(current(), 0);
    }

    #[test]
    fn a_cell_entry_is_per_slot() {
        static C: PerGuestCell<[u8; 4]> = per_guest_cell!([0; 4]);
        // SAFETY: this static is private to the test.
        unsafe {
            with_slot(1, || (*C.p())[0] = 9);
            assert_eq!((*C.p())[0], 0);
            assert_eq!((*C.p_of(1))[0], 9);
            assert_ne!(C.p_of(0), C.p_of(1));
        }
    }
}
