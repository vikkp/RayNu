//! ADR-021 B6. Two product guests can be named at once.
//!
//! The RayNu-F VMCS still runs one of them. A second Start used to answer
//! `busy` and forget the request. It is now seated: recorded, not given the
//! CPU, and not allowed to retarget the running guest's disk or NIC.
//! Not lived until two product guests both make progress on the R640.
//!
//! Pillar: [Z]
//! Proven Core: **outside** (ADR-002 / ADR-021).
//! VERIFICATION: L1 (host tests).

use core::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};

/// Window guest plus one other, or two tail disks. A third is still refused.
pub const CAP: usize = 2;

const EMPTY: u8 = 0;
const WINDOW: u8 = 1;
const TAIL: u8 = 2;

static KIND: [AtomicU8; CAP] = [AtomicU8::new(EMPTY), AtomicU8::new(EMPTY)];
static OFF: [AtomicU64; CAP] = [AtomicU64::new(0), AtomicU64::new(0)];
static ON_CPU: [AtomicBool; CAP] = [AtomicBool::new(false), AtomicBool::new(false)];

/// One product guest the operator named.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Seat {
    Window,
    Tail(u64),
}

/// What a Start may do while another guest may already own the VMCS.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Admit {
    /// Nobody owns the VMCS. The caller may boot this guest.
    Free,
    /// This guest already owns the VMCS.
    Running,
    /// Recorded next to the guest that owns the VMCS.
    Seated,
    /// Both seats are already other guests.
    Full,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placed {
    pub seat: Seat,
    pub on_cpu: bool,
}

fn read(i: usize) -> Option<Placed> {
    let kind = KIND[i].load(Ordering::Acquire);
    let seat = match kind {
        WINDOW => Seat::Window,
        TAIL => Seat::Tail(OFF[i].load(Ordering::Acquire)),
        _ => return None,
    };
    Some(Placed {
        seat,
        on_cpu: ON_CPU[i].load(Ordering::Acquire),
    })
}

fn write(i: usize, seat: Seat, on_cpu: bool) {
    match seat {
        Seat::Window => {
            KIND[i].store(WINDOW, Ordering::Release);
            OFF[i].store(0, Ordering::Release);
        }
        Seat::Tail(off) => {
            OFF[i].store(off, Ordering::Release);
            KIND[i].store(TAIL, Ordering::Release);
        }
    }
    ON_CPU[i].store(on_cpu, Ordering::Release);
}

fn clear_slot(i: usize) {
    ON_CPU[i].store(false, Ordering::Release);
    KIND[i].store(EMPTY, Ordering::Release);
    OFF[i].store(0, Ordering::Release);
}

fn find(seat: Seat) -> Option<usize> {
    (0..CAP).find(|&i| read(i).is_some_and(|p| p.seat == seat))
}

fn empty_slot() -> Option<usize> {
    (0..CAP).find(|&i| KIND[i].load(Ordering::Acquire) == EMPTY)
}

/// Drop every seat. Tests and a full stop of the table.
pub fn clear_all() {
    for i in 0..CAP {
        clear_slot(i);
    }
}

/// The guest that just took the VMCS. Any other seat stays parked.
pub fn note_running(seat: Seat) {
    for i in 0..CAP {
        if read(i).is_some_and(|p| p.seat != seat) {
            ON_CPU[i].store(false, Ordering::Release);
        }
    }
    if let Some(i) = find(seat) {
        ON_CPU[i].store(true, Ordering::Release);
        return;
    }
    let Some(i) = empty_slot().or_else(|| {
        (0..CAP).find(|&i| !ON_CPU[i].load(Ordering::Acquire))
    }) else {
        return;
    };
    write(i, seat, true);
}

/// Remember `want` without giving it the CPU. `owner` is marked on the CPU.
pub fn seat_beside(owner: Seat, want: Seat) -> Admit {
    if owner == want {
        note_running(owner);
        return Admit::Running;
    }
    note_running(owner);
    if find(want).is_some() {
        return Admit::Seated;
    }
    let Some(i) = empty_slot() else {
        return Admit::Full;
    };
    write(i, want, false);
    Admit::Seated
}

/// The VMCS guest has stopped. Parked seats stay named.
pub fn clear_running() {
    for i in 0..CAP {
        if ON_CPU[i].load(Ordering::Acquire) {
            clear_slot(i);
        }
    }
}

pub fn placed() -> [Option<Placed>; CAP] {
    [read(0), read(1)]
}

pub fn has_parked() -> bool {
    (0..CAP).any(|i| read(i).is_some_and(|p| !p.on_cpu))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_second_guest_is_seated_beside_the_one_on_the_cpu() {
        clear_all();
        note_running(Seat::Tail(100));
        assert_eq!(
            seat_beside(Seat::Tail(100), Seat::Window),
            Admit::Seated
        );
        let seats = placed();
        assert!(seats.iter().flatten().any(|p| p.seat == Seat::Tail(100) && p.on_cpu));
        assert!(seats.iter().flatten().any(|p| p.seat == Seat::Window && !p.on_cpu));
        assert!(has_parked());
        assert_eq!(
            seat_beside(Seat::Tail(100), Seat::Window),
            Admit::Seated,
            "seating the same guest again does not take the CPU"
        );
        clear_running();
        let left = placed();
        assert!(left.iter().flatten().all(|p| p.seat == Seat::Window && !p.on_cpu));
        note_running(Seat::Window);
        assert!(placed().iter().flatten().any(|p| p.seat == Seat::Window && p.on_cpu));
        assert_eq!(
            seat_beside(Seat::Window, Seat::Tail(200)),
            Admit::Seated
        );
        assert_eq!(
            seat_beside(Seat::Window, Seat::Tail(300)),
            Admit::Full
        );
        clear_all();
    }
}
