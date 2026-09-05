//! RayNu-F event and timer services (UEFI 2.10 §7.1) plus TPL and `Stall`.
//!
//! Pillar: [Z] · Proven Core: **outside** (ADR-016)
//!
//! **Owned timer tick.** RayNu-F's clock is host-side: a [`TimeSource`] in
//! 100 ns units (TSC-derived in the hypervisor, manual in tests). Timer events
//! are armed with absolute deadlines and `WaitForEvent` / `CheckEvent` /
//! `Stall` are serviced in the hypervisor by comparing against that clock —
//! the guest firmware phase needs **no IDT, no PIT/LAPIC emulation, no
//! interrupt delivery**. We signal the wait; there is nothing foreign to poke.
//!
//! `EFI_EVENT` handles are tagged opaque values, never guest pointers, so a
//! stray dereference faults loudly instead of aliasing our state.
//!
//! Not implemented in this slice (honest): notify-function dispatch into the
//! guest (`EVT_NOTIFY_WAIT` / `EVT_NOTIFY_SIGNAL` callbacks) and event groups.

/// Host-side firmware clock, 100 ns units.
pub trait TimeSource {
    fn now_100ns(&self) -> u64;
}

/// Event slots available.
pub const EVENT_SLOTS: usize = 64;
/// Tag for event handles; low bits carry the slot.
pub const EVENT_HANDLE_TAG: u64 = 0x5246_0000_0000_1000;

/// `EFI_EVENT` type bits.
pub const EVT_TIMER: u32 = 0x8000_0000;
pub const EVT_RUNTIME: u32 = 0x4000_0000;
pub const EVT_NOTIFY_WAIT: u32 = 0x0000_0100;
pub const EVT_NOTIFY_SIGNAL: u32 = 0x0000_0200;
pub const EVT_SIGNAL_EXIT_BOOT_SERVICES: u32 = 0x0000_0201;

/// `EFI_TIMER_DELAY`.
pub const TIMER_CANCEL: u32 = 0;
pub const TIMER_PERIODIC: u32 = 1;
pub const TIMER_RELATIVE: u32 = 2;

/// Task priority levels.
pub const TPL_APPLICATION: u64 = 4;
pub const TPL_CALLBACK: u64 = 8;
pub const TPL_NOTIFY: u64 = 16;
pub const TPL_HIGH_LEVEL: u64 = 31;

pub const EFI_SUCCESS: u64 = 0;
pub const EFI_INVALID_PARAMETER: u64 = 0x8000_0000_0000_0002;
pub const EFI_UNSUPPORTED: u64 = 0x8000_0000_0000_0003;
pub const EFI_NOT_READY: u64 = 0x8000_0000_0000_0006;
pub const EFI_OUT_OF_RESOURCES: u64 = 0x8000_0000_0000_0009;
pub const EFI_TIMEOUT: u64 = 0x8000_0000_0000_0012;

/// Longest a blocking `WaitForEvent` / `Stall` may spin host-side (10 s).
/// A guest waiting longer than this on a firmware clock is stuck, not patient.
pub const WAIT_CAP_100NS: u64 = 10 * 10_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Slot {
    used: bool,
    event_type: u32,
    notify_tpl: u64,
    notify_fn: u64,
    notify_ctx: u64,
    signaled: bool,
    timer_kind: u32,
    /// Absolute deadline (100 ns) when `timer_kind != TIMER_CANCEL`.
    deadline: u64,
    period: u64,
}

const EMPTY: Slot = Slot {
    used: false,
    event_type: 0,
    notify_tpl: 0,
    notify_fn: 0,
    notify_ctx: 0,
    signaled: false,
    timer_kind: TIMER_CANCEL,
    deadline: 0,
    period: 0,
};

/// Why `WaitForEvent` returned — for host markers, not guest-visible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaitOutcome {
    /// An event was already signaled on entry.
    Immediate(usize),
    /// A timer event fired while we waited on the firmware clock.
    TimerFired(usize),
    /// Nothing in the list could ever signal (no armed timer) → `EFI_TIMEOUT`.
    Stuck,
    /// Bad arguments / TPL.
    Error(u64),
}

/// Event table + TPL + monotonic counter.
#[derive(Clone)]
pub struct Events {
    slots: [Slot; EVENT_SLOTS],
    tpl: u64,
    monotonic: u64,
    /// Timer events that have fired (host bookkeeping).
    pub timers_fired: u32,
}

/// Slot 0 is `ConIn->WaitForKey` (see `tables::CONIN_WAIT_KEY_EVENT`).
pub const WAIT_KEY_SLOT: usize = 0;

impl Events {
    pub const fn new() -> Self {
        let mut slots = [EMPTY; EVENT_SLOTS];
        slots[WAIT_KEY_SLOT] = Slot {
            used: true,
            event_type: EVT_NOTIFY_WAIT,
            notify_tpl: TPL_CALLBACK,
            notify_fn: 0,
            notify_ctx: 0,
            signaled: false,
            timer_kind: TIMER_CANCEL,
            deadline: 0,
            period: 0,
        };
        Events {
            slots,
            tpl: TPL_APPLICATION,
            monotonic: 0,
            timers_fired: 0,
        }
    }

    /// Host console input arrived: signal `WaitForKey`.
    pub fn signal_wait_key(&mut self) {
        self.slots[WAIT_KEY_SLOT].signaled = true;
    }

    pub const fn handle_for(slot: usize) -> u64 {
        EVENT_HANDLE_TAG | slot as u64
    }

    fn slot_of(&self, handle: u64) -> Option<usize> {
        if handle & !0xFFF != EVENT_HANDLE_TAG {
            return None;
        }
        let s = (handle & 0xFFF) as usize;
        if s < EVENT_SLOTS && self.slots[s].used {
            Some(s)
        } else {
            None
        }
    }

    pub fn tpl(&self) -> u64 {
        self.tpl
    }

    /// `RaiseTPL` — returns the previous TPL.
    pub fn raise_tpl(&mut self, new: u64) -> u64 {
        let old = self.tpl;
        if new > self.tpl && new <= TPL_HIGH_LEVEL {
            self.tpl = new;
        }
        old
    }

    /// `RestoreTPL`.
    pub fn restore_tpl(&mut self, old: u64) {
        if old <= TPL_HIGH_LEVEL {
            self.tpl = old;
        }
    }

    /// `GetNextMonotonicCount`.
    pub fn next_monotonic(&mut self) -> u64 {
        self.monotonic = self.monotonic.wrapping_add(1);
        self.monotonic
    }

    /// `CreateEvent` / `CreateEventEx` (group ignored). Returns `(status, handle)`.
    pub fn create(&mut self, event_type: u32, notify_tpl: u64, notify_fn: u64, notify_ctx: u64) -> (u64, u64) {
        let needs_notify = event_type & (EVT_NOTIFY_WAIT | EVT_NOTIFY_SIGNAL) != 0;
        if needs_notify && (notify_fn == 0 || notify_tpl == 0 || notify_tpl > TPL_HIGH_LEVEL) {
            return (EFI_INVALID_PARAMETER, 0);
        }
        if event_type & (EVT_NOTIFY_WAIT | EVT_NOTIFY_SIGNAL) == (EVT_NOTIFY_WAIT | EVT_NOTIFY_SIGNAL) {
            return (EFI_INVALID_PARAMETER, 0);
        }
        for (i, s) in self.slots.iter_mut().enumerate() {
            if !s.used {
                *s = Slot {
                    used: true,
                    event_type,
                    notify_tpl,
                    notify_fn,
                    notify_ctx,
                    ..EMPTY
                };
                return (EFI_SUCCESS, Self::handle_for(i));
            }
        }
        (EFI_OUT_OF_RESOURCES, 0)
    }

    /// `CloseEvent`.
    pub fn close(&mut self, handle: u64) -> u64 {
        match self.slot_of(handle) {
            Some(s) => {
                self.slots[s] = EMPTY;
                EFI_SUCCESS
            }
            None => EFI_INVALID_PARAMETER,
        }
    }

    /// `SignalEvent`.
    pub fn signal(&mut self, handle: u64) -> u64 {
        match self.slot_of(handle) {
            Some(s) => {
                self.slots[s].signaled = true;
                EFI_SUCCESS
            }
            None => EFI_INVALID_PARAMETER,
        }
    }

    /// `SetTimer(Event, Type, TriggerTime)`; `trigger` in 100 ns.
    pub fn set_timer(&mut self, handle: u64, kind: u32, trigger: u64, now: u64) -> u64 {
        let Some(s) = self.slot_of(handle) else {
            return EFI_INVALID_PARAMETER;
        };
        if self.slots[s].event_type & EVT_TIMER == 0 {
            return EFI_INVALID_PARAMETER;
        }
        match kind {
            TIMER_CANCEL => {
                self.slots[s].timer_kind = TIMER_CANCEL;
                self.slots[s].period = 0;
                EFI_SUCCESS
            }
            TIMER_RELATIVE => {
                self.slots[s].timer_kind = TIMER_RELATIVE;
                self.slots[s].deadline = now.saturating_add(trigger);
                self.slots[s].period = 0;
                EFI_SUCCESS
            }
            TIMER_PERIODIC => {
                if trigger == 0 {
                    return EFI_INVALID_PARAMETER;
                }
                self.slots[s].timer_kind = TIMER_PERIODIC;
                self.slots[s].deadline = now.saturating_add(trigger);
                self.slots[s].period = trigger;
                EFI_SUCCESS
            }
            _ => EFI_INVALID_PARAMETER,
        }
    }

    /// Fire any timer whose deadline has passed. Returns how many fired.
    pub fn poll_timers(&mut self, now: u64) -> u32 {
        let mut fired = 0;
        for s in self.slots.iter_mut() {
            if !s.used || s.timer_kind == TIMER_CANCEL {
                continue;
            }
            if now >= s.deadline {
                s.signaled = true;
                fired += 1;
                if s.timer_kind == TIMER_PERIODIC {
                    // Catch up without drifting; a long host stall does not
                    // queue a burst.
                    let missed = (now - s.deadline) / s.period + 1;
                    s.deadline = s.deadline.saturating_add(missed * s.period);
                } else {
                    s.timer_kind = TIMER_CANCEL;
                }
            }
        }
        self.timers_fired = self.timers_fired.saturating_add(fired);
        fired
    }

    /// `CheckEvent`. `input` reports whether host console input is pending
    /// (drives `WaitForKey`).
    pub fn check(&mut self, handle: u64, now: u64, input: bool) -> u64 {
        let Some(s) = self.slot_of(handle) else {
            return EFI_INVALID_PARAMETER;
        };
        if self.slots[s].event_type & EVT_NOTIFY_SIGNAL != 0 {
            return EFI_INVALID_PARAMETER;
        }
        if input {
            self.signal_wait_key();
        }
        self.poll_timers(now);
        if self.slots[s].signaled {
            self.slots[s].signaled = false;
            EFI_SUCCESS
        } else {
            EFI_NOT_READY
        }
    }

    /// Earliest armed deadline among `handles`, if any.
    fn earliest_deadline(&self, handles: &[u64]) -> Option<u64> {
        let mut best: Option<u64> = None;
        for &h in handles {
            if let Some(s) = self.slot_of(h) {
                let sl = &self.slots[s];
                if sl.timer_kind != TIMER_CANCEL {
                    best = Some(best.map_or(sl.deadline, |b| b.min(sl.deadline)));
                }
            }
        }
        best
    }

    /// `WaitForEvent` over already-read handles. Blocks on the firmware clock
    /// until one signals or the cap elapses; `input()` is polled for
    /// `WaitForKey`. Returns `(status, index, outcome)`.
    pub fn wait(
        &mut self,
        handles: &[u64],
        clock: &dyn TimeSource,
        input: &dyn Fn() -> bool,
    ) -> (u64, u64, WaitOutcome) {
        if self.tpl != TPL_APPLICATION {
            return (EFI_UNSUPPORTED, 0, WaitOutcome::Error(EFI_UNSUPPORTED));
        }
        if handles.is_empty() {
            return (EFI_INVALID_PARAMETER, 0, WaitOutcome::Error(EFI_INVALID_PARAMETER));
        }
        for &h in handles {
            match self.slot_of(h) {
                None => return (EFI_INVALID_PARAMETER, 0, WaitOutcome::Error(EFI_INVALID_PARAMETER)),
                Some(s) if self.slots[s].event_type & EVT_NOTIFY_SIGNAL != 0 => {
                    return (EFI_INVALID_PARAMETER, 0, WaitOutcome::Error(EFI_INVALID_PARAMETER))
                }
                _ => {}
            }
        }
        let start = clock.now_100ns();
        let mut first_pass = true;
        let waits_for_key = handles.contains(&Self::handle_for(WAIT_KEY_SLOT));
        loop {
            let now = clock.now_100ns();
            if waits_for_key && input() {
                self.signal_wait_key();
            }
            let fired = self.poll_timers(now);
            for (i, &h) in handles.iter().enumerate() {
                let s = self.slot_of(h).unwrap_or(0);
                if self.slots[s].signaled {
                    self.slots[s].signaled = false;
                    let outcome = if first_pass && fired == 0 {
                        WaitOutcome::Immediate(i)
                    } else {
                        WaitOutcome::TimerFired(i)
                    };
                    return (EFI_SUCCESS, i as u64, outcome);
                }
            }
            first_pass = false;
            if self.earliest_deadline(handles).is_none() && !waits_for_key {
                // Nothing here can ever signal without a notify path we do
                // not implement yet. Do not hang the host.
                return (EFI_TIMEOUT, 0, WaitOutcome::Stuck);
            }
            if now.saturating_sub(start) > WAIT_CAP_100NS {
                return (EFI_TIMEOUT, 0, WaitOutcome::Stuck);
            }
            core::hint::spin_loop();
        }
    }
}

/// `Stall(Microseconds)`: spin on the firmware clock, capped.
pub fn stall(clock: &dyn TimeSource, microseconds: u64) -> u64 {
    let want = microseconds.saturating_mul(10).min(WAIT_CAP_100NS);
    let start = clock.now_100ns();
    while clock.now_100ns().saturating_sub(start) < want {
        core::hint::spin_loop();
    }
    EFI_SUCCESS
}
