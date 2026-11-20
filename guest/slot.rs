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
//! itself, so one current-slot word is enough. B7 adds a vCPU index beside
//! the slot (ADR-023); B21 makes both per physical CPU.

use core::cell::UnsafeCell;
use core::ops::Deref;
use core::sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering};

/// Product guests that can exist at once. Each slot costs one copy of the
/// per-guest state banks; the machine itself (RAM, EPT, VMCS) is allocated
/// only when a slot is built.
pub const MAX_GUESTS: usize = 4;
/// Virtual CPUs one guest slot can show. They share one host core (ADR-023).
pub const MAX_VCPUS: usize = 4;
/// Schedulable units: one per (slot, vcpu). `unit = slot * MAX_VCPUS + vcpu`.
pub const MAX_UNITS: usize = MAX_GUESTS * MAX_VCPUS;

/// Index of the schedulable unit `(slot, vcpu)`.
#[inline]
pub const fn unit(slot: usize, vcpu: usize) -> usize {
    slot * MAX_VCPUS + vcpu
}

/// Slot half of a unit index.
#[inline]
pub const fn unit_slot(u: usize) -> usize {
    u / MAX_VCPUS
}

/// vCPU half of a unit index.
#[inline]
pub const fn unit_vcpu(u: usize) -> usize {
    u % MAX_VCPUS
}

#[cfg(not(test))]
static CURRENT: AtomicUsize = AtomicUsize::new(0);

// Host tests run on many threads. A thread-local keeps one test's slot
// selection out of another's.
#[cfg(test)]
std::thread_local! {
    static CURRENT: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
}

/// vCPU the per-vCPU banks name, one word per slot. One CPU runs guests, so
/// the word is enough; B21 makes it per physical CPU (ADR-023).
#[cfg(not(test))]
static CUR_VCPU: [AtomicUsize; MAX_GUESTS] = [const { AtomicUsize::new(0) }; MAX_GUESTS];
/// Configured vCPU count of each slot. Starts at 1 (the N=1 identity).
#[cfg(not(test))]
static VCPUS: [AtomicU8; MAX_GUESTS] = [const { AtomicU8::new(1) }; MAX_GUESTS];

// Host tests run on many threads. Thread-locals keep one test's vCPU
// selection and configured count out of another's, the same way `CURRENT`
// does. `VCPU_UP` stays process-wide: it is the run table, like `ON`.
#[cfg(test)]
std::thread_local! {
    static CUR_VCPU: [core::cell::Cell<usize>; MAX_GUESTS] =
        [const { core::cell::Cell::new(0) }; MAX_GUESTS];
    static VCPUS: [core::cell::Cell<u8>; MAX_GUESTS] =
        [const { core::cell::Cell::new(1) }; MAX_GUESTS];
}

fn load_cur_vcpu(slot: usize) -> usize {
    #[cfg(not(test))]
    {
        CUR_VCPU[slot].load(Ordering::Relaxed)
    }
    #[cfg(test)]
    {
        CUR_VCPU.with(|c| c[slot].get())
    }
}

fn store_cur_vcpu(slot: usize, v: usize) {
    #[cfg(not(test))]
    CUR_VCPU[slot].store(v, Ordering::Relaxed);
    #[cfg(test)]
    CUR_VCPU.with(|c| c[slot].set(v));
}

fn load_vcpus_raw(slot: usize) -> u8 {
    #[cfg(not(test))]
    {
        VCPUS[slot].load(Ordering::Relaxed)
    }
    #[cfg(test)]
    {
        VCPUS.with(|c| c[slot].get())
    }
}

fn store_vcpus_raw(slot: usize, n: u8) {
    #[cfg(not(test))]
    VCPUS[slot].store(n, Ordering::Relaxed);
    #[cfg(test)]
    VCPUS.with(|c| c[slot].set(n));
}

/// vCPU of the current slot. The banks dereference to this entry.
#[inline(always)]
pub fn current_vcpu() -> usize {
    let v = load_cur_vcpu(current());
    if v < MAX_VCPUS {
        v
    } else {
        0
    }
}

/// Select vCPU `v` of `slot` without changing which slot is current.
pub fn set_slot_vcpu(slot: usize, v: usize) {
    if slot < MAX_GUESTS && v < MAX_VCPUS {
        store_cur_vcpu(slot, v);
    }
}

/// vCPU last stored for `slot`, not necessarily the slot that is current.
pub fn slot_vcpu(slot: usize) -> usize {
    let v = load_cur_vcpu(slot);
    if v < MAX_VCPUS {
        v
    } else {
        0
    }
}

/// Select vCPU `v` for the current slot. Returns the vCPU that was current.
///
/// An out-of-range vCPU is refused and the current vCPU is left alone.
#[inline]
pub fn set_current_vcpu(v: usize) -> usize {
    let slot = current();
    let prev = current_vcpu();
    if v >= MAX_VCPUS {
        debug_assert!(v < MAX_VCPUS, "guest vCPU out of range");
        return prev;
    }
    store_cur_vcpu(slot, v);
    prev
}

/// Run `f` with `slot` and `vcpu` selected, then restore both words.
#[inline]
pub fn with_vcpu<R>(slot: usize, vcpu: usize, f: impl FnOnce() -> R) -> R {
    let prev_slot = set_current(slot);
    let prev_vcpu = set_current_vcpu(vcpu);
    let r = f();
    set_current_vcpu(prev_vcpu);
    set_current(prev_slot);
    r
}

/// Configured vCPU count of the current slot, in `1..=MAX_VCPUS`.
#[inline]
pub fn vcpus() -> usize {
    vcpus_of(current())
}

/// Configured vCPU count of `slot`. Out of range reads as 1.
#[inline]
pub fn vcpus_of(slot: usize) -> usize {
    if slot >= MAX_GUESTS {
        return 1;
    }
    let n = load_vcpus_raw(slot) as usize;
    n.clamp(1, MAX_VCPUS)
}

/// Set the current slot's vCPU count. Clamped to `1..=MAX_VCPUS`.
pub fn set_vcpus(n: usize) {
    store_vcpus_raw(current(), n.clamp(1, MAX_VCPUS) as u8);
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

/// A guest is in this slot's VMCS (launched and not yet left).
static ON: [AtomicBool; MAX_GUESTS] = [const { AtomicBool::new(false) }; MAX_GUESTS];
/// Bit v set means vCPU v of this slot has been started (INIT/SIPI, or the
/// BSP at `set_on`). An off slot's mask is 0.
static VCPU_UP: [AtomicU8; MAX_GUESTS] = [const { AtomicU8::new(0) }; MAX_GUESTS];
/// A device event is waiting. Bit v is vCPU v. The scheduler takes the bit;
/// a halted vCPU becomes runnable. `wake` sets every bit: on one core any
/// of the slot's vCPUs may be the one that is halted.
static WAKE: [AtomicU8; MAX_GUESTS] = [const { AtomicU8::new(0) }; MAX_GUESTS];
/// Slot whose guest has COM2 and the management plane's untargeted routes.
static CONSOLE: AtomicUsize = AtomicUsize::new(0);

/// Mark `slot` on (its guest is launched) or off (it left the VMCS).
pub fn set_on(slot: usize, on: bool) {
    if slot < MAX_GUESTS {
        ON[slot].store(on, Ordering::Release);
        // An off slot's current vCPU is 0. Turning a slot on starts the BSP
        // only; APs come up later through `set_vcpu_up`.
        VCPU_UP[slot].store(if on { 1 } else { 0 }, Ordering::Release);
        store_cur_vcpu(slot, 0);
    }
}

/// True while a guest is launched in `slot`.
pub fn is_on(slot: usize) -> bool {
    slot < MAX_GUESTS && ON[slot].load(Ordering::Acquire)
}

/// Guests launched right now.
pub fn on_count() -> usize {
    ON.iter().filter(|s| s.load(Ordering::Acquire)).count()
}

/// More than one guest is launched. The console and the uplink split their
/// traffic by slot only then; one guest keeps the single-guest paths.
pub fn shared() -> bool {
    on_count() > 1
}

/// Lowest slot that is on, other than `not`.
pub fn first_on_except(not: usize) -> Option<usize> {
    (0..MAX_GUESTS).find(|&s| s != not && is_on(s))
}

/// A host-side event for `slot`'s guest arrived.
/// Bit v of `slot`'s up-mask is set.
pub fn vcpu_up(slot: usize, v: usize) -> bool {
    slot < MAX_GUESTS && v < MAX_VCPUS && up_mask(slot) & (1u8 << v) != 0
}

/// Mark vCPU `v` of `slot` started or stopped. The BSP bit is also set by
/// [`set_on`].
pub fn set_vcpu_up(slot: usize, v: usize, on: bool) {
    if slot >= MAX_GUESTS || v >= MAX_VCPUS {
        return;
    }
    let bit = 1u8 << v;
    if on {
        VCPU_UP[slot].fetch_or(bit, Ordering::Release);
    } else {
        VCPU_UP[slot].fetch_and(!bit, Ordering::Release);
    }
}

/// Started-vCPU bitmask of `slot`. Zero when the slot is out of range.
pub fn up_mask(slot: usize) -> u8 {
    if slot < MAX_GUESTS {
        VCPU_UP[slot].load(Ordering::Acquire)
    } else {
        0
    }
}

/// Started vCPUs across slots that are on.
pub fn units_on() -> usize {
    (0..MAX_GUESTS)
        .filter(|&s| is_on(s))
        .map(|s| up_mask(s).count_ones() as usize)
        .sum()
}

/// More than one vCPU is started (one guest with APs, or several guests).
/// The scheduler shares the one host core only then.
pub fn cpu_shared() -> bool {
    units_on() > 1
}

/// Clear every AP bit of `slot`. vCPU 0 is left as it was.
pub fn clear_aps(slot: usize) {
    if slot < MAX_GUESTS {
        VCPU_UP[slot].fetch_and(1, Ordering::Release);
    }
}

pub fn wake(slot: usize) {
    if slot < MAX_GUESTS {
        WAKE[slot].fetch_or(0xFF, Ordering::Release);
        for v in 0..MAX_VCPUS {
            if vcpu_up(slot, v) {
                crate::arch::host_smp::poke_unit(unit(slot, v));
            }
        }
    }
}

/// A host-side event for one vCPU of `slot` arrived.
pub fn wake_vcpu(slot: usize, v: usize) {
    if slot < MAX_GUESTS && v < MAX_VCPUS {
        WAKE[slot].fetch_or(1u8 << v, Ordering::Release);
        crate::arch::host_smp::poke_unit(unit(slot, v));
    }
}

/// One-shot read of vCPU `v`'s wake bit.
pub fn take_wake_vcpu(slot: usize, v: usize) -> bool {
    if slot >= MAX_GUESTS || v >= MAX_VCPUS {
        return false;
    }
    let bit = 1u8 << v;
    WAKE[slot].fetch_and(!bit, Ordering::AcqRel) & bit != 0
}

/// One-shot read of vCPU 0's wake bit. [`wake`] sets every bit; this takes
/// the BSP's, which is the only vCPU a one-CPU guest has.
pub fn take_wake(slot: usize) -> bool {
    take_wake_vcpu(slot, 0)
}

/// Slot that owns COM2 input and unprefixed COM2 output.
pub fn console_owner() -> usize {
    CONSOLE.load(Ordering::Acquire) % MAX_GUESTS
}

/// Hand COM2 to `slot`. Out-of-range is refused.
pub fn set_console_owner(slot: usize) {
    if slot < MAX_GUESTS {
        CONSOLE.store(slot, Ordering::Release);
    }
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
///   - With more than one host core, the writer is the core that holds the
///     monitor lock (ADR-024).
#[repr(transparent)]
pub struct PerGuestCell<T>(UnsafeCell<[T; MAX_GUESTS]>);

// SAFETY: every access is from VMX root on the one CPU that runs guests, and
// an entry is only reached while its guest is stopped at a VM exit. There is
// no concurrent access to share. With more than one host core, the writer is
// the core that holds the monitor lock (ADR-024).
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

/// One `T` per (slot, vCPU). Dereferences to the current slot's current vCPU.
#[repr(transparent)]
pub struct PerVcpu<T>([[T; MAX_VCPUS]; MAX_GUESTS]);

impl<T> PerVcpu<T> {
    pub const fn new(bank: [[T; MAX_VCPUS]; MAX_GUESTS]) -> Self {
        Self(bank)
    }

    /// The entry for `(slot, vcpu)`, whichever slot is current.
    #[inline]
    pub fn at(&self, slot: usize, vcpu: usize) -> &T {
        &self.0[slot % MAX_GUESTS][vcpu % MAX_VCPUS]
    }
}

impl<T> Deref for PerVcpu<T> {
    type Target = T;
    #[inline(always)]
    fn deref(&self) -> &T {
        &self.0[current()][current_vcpu()]
    }
}

/// One mutable `T` per (slot, vCPU), for state the exit path owns outright.
///
/// INVARIANTS:
///   - An entry is touched only while its vCPU is not executing (VMX root,
///     one CPU), or through the one remote-post seam (ADR-023 D12).
///   - With more than one host core, the writer is the core that holds the
///     monitor lock (ADR-024).
#[repr(transparent)]
pub struct PerVcpuCell<T>(UnsafeCell<[[T; MAX_VCPUS]; MAX_GUESTS]>);

// SAFETY: every access is from VMX root on the one CPU that runs guests, and
// an entry is only reached while its guest is stopped at a VM exit. There is
// no concurrent access to share. With more than one host core, the writer is
// the core that holds the monitor lock (ADR-024).
// KANI-TARGET: per-guest cell access (outside Proven Core).
unsafe impl<T> Sync for PerVcpuCell<T> {}

impl<T> PerVcpuCell<T> {
    pub const fn new(bank: [[T; MAX_VCPUS]; MAX_GUESTS]) -> Self {
        Self(UnsafeCell::new(bank))
    }

    /// Pointer to the current slot's current vCPU.
    #[inline(always)]
    pub fn p(&self) -> *mut T {
        self.p_at(current(), current_vcpu())
    }

    /// Pointer to `(slot, vcpu)`'s entry.
    #[inline(always)]
    pub fn p_at(&self, slot: usize, vcpu: usize) -> *mut T {
        // SAFETY: index is reduced into the bank; the pointer stays inside
        // the array this cell owns. `[[T; MAX_VCPUS]; MAX_GUESTS]` is
        // contiguous, vCPU index innermost.
        // KANI-TARGET: per-guest cell index (outside Proven Core).
        unsafe {
            (self.0.get() as *mut T).add((slot % MAX_GUESTS) * MAX_VCPUS + (vcpu % MAX_VCPUS))
        }
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
        $crate::guest::slot::PerGuestCell::new([const { $init }; $crate::guest::slot::MAX_GUESTS])
    };
}

/// `per_vcpu!(init)` builds a [`PerVcpu`] bank with every (slot, vCPU) set
/// to the constant `init`.
#[macro_export]
macro_rules! per_vcpu {
    ($init:expr) => {
        $crate::guest::slot::PerVcpu::new(
            [const { [const { $init }; $crate::guest::slot::MAX_VCPUS] };
                $crate::guest::slot::MAX_GUESTS],
        )
    };
}

/// `per_vcpu_cell!(init)` builds a [`PerVcpuCell`] bank with every
/// (slot, vCPU) set to the constant `init`.
#[macro_export]
macro_rules! per_vcpu_cell {
    ($init:expr) => {
        $crate::guest::slot::PerVcpuCell::new(
            [const { [const { $init }; $crate::guest::slot::MAX_VCPUS] };
                $crate::guest::slot::MAX_GUESTS],
        )
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::sync::atomic::AtomicU32;

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
    fn the_run_table_counts_launched_guests() {
        // Slot 3 is not used by any other host test.
        assert!(!is_on(3));
        let before = on_count();
        set_on(3, true);
        assert!(is_on(3));
        assert_eq!(on_count(), before + 1);
        assert_eq!(first_on_except(0).is_some(), true);
        wake(3);
        assert!(take_wake(3));
        assert!(!take_wake(3));
        set_on(3, false);
        assert_eq!(on_count(), before);
        set_on(MAX_GUESTS, true);
        assert!(!is_on(MAX_GUESTS));
        set_console_owner(MAX_GUESTS);
        assert!(console_owner() < MAX_GUESTS);
    }

    #[test]
    fn the_vcpu_bank_follows_the_current_vcpu() {
        static N: PerVcpu<AtomicU32> = per_vcpu!(AtomicU32::new(0));
        static C: PerVcpuCell<u32> = per_vcpu_cell!(0);
        set_current_vcpu(0);
        N.store(1, Ordering::Relaxed);
        // SAFETY: this static is private to the test.
        unsafe {
            *C.p() = 10;
        }
        let prev = set_current_vcpu(2);
        assert_eq!(prev, 0);
        N.store(3, Ordering::Relaxed);
        unsafe {
            *C.p() = 30;
        }
        assert_eq!(N.at(current(), 0).load(Ordering::Relaxed), 1);
        assert_eq!(N.at(current(), 2).load(Ordering::Relaxed), 3);
        assert_eq!(unsafe { *C.p_at(current(), 0) }, 10);
        assert_eq!(unsafe { *C.p() }, 30);
        assert_ne!(C.p_at(0, 0), C.p_at(0, 1));
        set_current_vcpu(0);
    }

    #[test]
    fn with_vcpu_restores_both_words() {
        assert_eq!(current(), 0);
        assert_eq!(current_vcpu(), 0);
        let inner = with_vcpu(2, 3, || {
            let seen = (current(), current_vcpu());
            let nested = with_vcpu(1, 1, || (current(), current_vcpu()));
            (seen, nested, current(), current_vcpu())
        });
        assert_eq!(inner, ((2, 3), (1, 1), 2, 3));
        assert_eq!(current(), 0);
        assert_eq!(current_vcpu(), 0);
    }

    #[test]
    fn set_on_false_zeroes_the_mask_and_the_current_vcpu() {
        let s = 3;
        set_on(s, true);
        set_vcpu_up(s, 1, true);
        with_slot(s, || {
            set_current_vcpu(2);
        });
        assert_eq!(up_mask(s), 0b11);
        assert_eq!(load_cur_vcpu(s), 2);
        set_on(s, false);
        assert_eq!(up_mask(s), 0);
        assert_eq!(load_cur_vcpu(s), 0);
        assert!(!vcpu_up(s, 0));
    }

    #[test]
    fn units_on_counts_started_vcpus_of_slots_that_are_on() {
        let s = 3;
        set_on(s, false);
        let before = units_on();
        set_on(s, true);
        set_vcpu_up(s, 1, true);
        assert_eq!(up_mask(s), 0b11);
        assert_eq!(units_on(), before + 2);
        assert!(cpu_shared());
        set_on(s, false);
        assert_eq!(units_on(), before);
        assert_eq!(cpu_shared(), before > 1);
    }

    #[test]
    fn wake_reaches_every_vcpu_and_take_wake_is_vcpu_0() {
        let s = 3;
        // Drain anything an earlier test left.
        for v in 0..MAX_VCPUS {
            let _ = take_wake_vcpu(s, v);
        }
        wake(s);
        assert!(take_wake(s));
        assert!(!take_wake(s));
        for v in 1..MAX_VCPUS {
            assert!(take_wake_vcpu(s, v), "vCPU {v}");
            assert!(!take_wake_vcpu(s, v));
        }
        wake_vcpu(s, 2);
        assert!(!take_wake_vcpu(s, 0));
        assert!(take_wake_vcpu(s, 2));
    }

    #[test]
    fn unit_round_trips_and_the_count_clamps() {
        for s in 0..MAX_GUESTS {
            for v in 0..MAX_VCPUS {
                let u = unit(s, v);
                assert_eq!(unit_slot(u), s);
                assert_eq!(unit_vcpu(u), v);
            }
        }
        assert_eq!(MAX_UNITS, 16);
        set_vcpus(0);
        assert_eq!(vcpus(), 1);
        set_vcpus(4);
        assert_eq!(vcpus(), 4);
        set_vcpus(9);
        assert_eq!(vcpus(), 4);
        set_vcpus(1);
        assert_eq!(vcpus(), 1);
        assert_eq!(vcpus_of(MAX_GUESTS), 1);
        let r = std::panic::catch_unwind(|| set_current_vcpu(MAX_VCPUS));
        let _ = r;
        assert_eq!(current_vcpu(), 0);
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
