//! ADR-021 B6 slice clock for two product guests.
//!
//! The guest on the CPU is the primary. A seated guest is the companion.
//! A slice comes due on a fixed exit interval while a companion is seated.
//! Companion progress stays zero until the seated guest itself is entered.
//! A one-shot HLT on a second VMCS does not move that counter.
//!
//! Pillar: [Z]
//! Proven Core: **outside** (ADR-002 / ADR-021).
//! VERIFICATION: L1 (host tests).

use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// Linux exits between slice points.
pub const SLICE_EXITS: u32 = 8192;

static PRIMARY: AtomicU32 = AtomicU32::new(0);
static COMPANION: AtomicU32 = AtomicU32::new(0);
static MARKED: AtomicBool = AtomicBool::new(false);

pub fn clear_for_test() {
    PRIMARY.store(0, Ordering::Release);
    COMPANION.store(0, Ordering::Release);
    MARKED.store(false, Ordering::Release);
}

pub fn note_primary() {
    PRIMARY.fetch_add(1, Ordering::AcqRel);
}

pub fn note_companion() {
    COMPANION.fetch_add(1, Ordering::AcqRel);
}

pub fn primary_slices() -> u32 {
    PRIMARY.load(Ordering::Acquire)
}

pub fn companion_slices() -> u32 {
    COMPANION.load(Ordering::Acquire)
}

/// True once each guest has been entered at least once in this boot.
pub fn both_progressed() -> bool {
    primary_slices() > 0 && companion_slices() > 0
}

/// One-shot for the COM2 close line. Stays false until both have progressed.
pub fn take_close_line() -> bool {
    both_progressed()
        && MARKED
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
}

/// A companion slice is due when one is seated and the exit count hits the interval.
pub fn slice_due(exits: u32, parked: bool) -> bool {
    parked && exits != 0 && exits % SLICE_EXITS == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_guests_must_enter_before_the_close_line() {
        clear_for_test();
        assert!(!slice_due(0, true));
        assert!(!slice_due(SLICE_EXITS, false));
        assert!(slice_due(SLICE_EXITS, true));
        assert!(slice_due(SLICE_EXITS * 2, true));
        note_primary();
        assert!(!both_progressed());
        assert!(!take_close_line());
        note_companion();
        assert!(both_progressed());
        assert!(take_close_line());
        assert!(!take_close_line());
        clear_for_test();
    }
}
