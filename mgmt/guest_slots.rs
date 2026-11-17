//! Which guest is in which slot (outside Proven Core).
//!
//! Pillar: [Z] · Proven Core: **outside** (ADR-002, ADR-022)
//!
//! Every product guest is the same machine in its own slot
//! ([`crate::guest::slot`]). This module is the management plane's view of
//! the slots: which disk a slot is running, which slot a Start may take,
//! and which slot a Stop names. It holds no state of its own. The choice,
//! the stop latch and the disk span are the per-slot banks in
//! [`super::perc_boot_choice`].
//!
//! INVARIANTS:
//!   - One disk is in at most one slot. A Start of a disk that is already
//!     in a slot answers `running` and changes nothing.
//!   - A Start takes the lowest free slot, so the first guest is slot 0.
//!   - A slot is free when its guest is [`VmcsGuest::None`] (no choice, or
//!     a landed Stop).

use super::perc_boot_choice::{
    armed_tail, guest_stopped, perc_spa_choice, vmcs_guest, PercSpaChoice, VmcsGuest,
};
use crate::guest::slot::{with_slot, MAX_GUESTS};

/// One slot as the Guests list sees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SlotView {
    pub choice: PercSpaChoice,
    /// Stop landed, or the guest powered itself off.
    pub stopped: bool,
    /// Spare offset of the tail disk this slot holds, when it holds one.
    pub tail_off: Option<u64>,
    /// This slot's guest was started from the library ISO.
    pub cd: bool,
}

impl SlotView {
    /// A slot nobody has started.
    pub const EMPTY: SlotView = SlotView {
        choice: PercSpaChoice::Pending,
        stopped: false,
        tail_off: None,
        cd: false,
    };

    /// The slot holds the 8 GiB window (running, installing, or stopped).
    pub fn holds_window(&self) -> bool {
        matches!(
            self.choice,
            PercSpaChoice::BootInstalled | PercSpaChoice::CleanReinstall
        )
    }

    /// The slot holds the tail disk at `off`.
    pub fn holds_tail(&self, off: u64) -> bool {
        self.tail_off == Some(off)
    }

    /// A guest is in this slot's VMCS.
    pub fn on(&self) -> bool {
        !self.stopped && !matches!(self.choice, PercSpaChoice::Pending)
    }
}

/// Read one slot.
pub fn view(slot: usize) -> SlotView {
    with_slot(slot, || SlotView {
        choice: perc_spa_choice(),
        stopped: guest_stopped(),
        tail_off: armed_tail().map(|(off, _)| off),
        cd: crate::mgmt::iso_library::present_as_cd(),
    })
}

/// Read every slot.
pub fn views() -> [SlotView; MAX_GUESTS] {
    let mut out = [SlotView::EMPTY; MAX_GUESTS];
    for (slot, v) in out.iter_mut().enumerate() {
        *v = view(slot);
    }
    out
}

/// Slot that is running `want`.
pub fn slot_running(want: VmcsGuest) -> Option<usize> {
    if want == VmcsGuest::None {
        return None;
    }
    (0..MAX_GUESTS).find(|&s| with_slot(s, vmcs_guest) == want)
}

/// Lowest slot with no guest in it.
pub fn free_slot() -> Option<usize> {
    (0..MAX_GUESTS).find(|&s| with_slot(s, vmcs_guest) == VmcsGuest::None)
}

/// Slots with a guest in them.
pub fn running_count() -> usize {
    (0..MAX_GUESTS)
        .filter(|&s| with_slot(s, vmcs_guest) != VmcsGuest::None)
        .count()
}

/// True when any slot has a guest. Deferred host writes wait for false.
pub fn any_guest_on() -> bool {
    running_count() != 0
}

/// Outcome of naming "the running guest" without a row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Only {
    /// No guest is on.
    None,
    /// Exactly one guest is on, in this slot.
    Slot(usize),
    /// More than one guest is on. The caller must name a row.
    Many,
}

/// The one running guest, for the routes that do not name a row.
pub fn only_running() -> Only {
    let mut found = Only::None;
    for s in 0..MAX_GUESTS {
        if with_slot(s, vmcs_guest) == VmcsGuest::None {
            continue;
        }
        found = match found {
            Only::None => Only::Slot(s),
            _ => return Only::Many,
        };
    }
    found
}

/// Slot a row's Stop or power request names. `off` 0 is the 8 GiB window.
pub fn slot_of_row(off: u64) -> Option<usize> {
    let want = if off == 0 {
        VmcsGuest::Window
    } else {
        VmcsGuest::Tail(off)
    };
    slot_running(want)
}

#[cfg(test)]
mod guest_slots_test {
    use super::*;
    use crate::mgmt::perc_boot_choice::{
        apply_spa_choice, apply_tail_boot_choice, clear_perc_spa_choice_for_test,
        request_guest_stop, take_guest_stop, PercChoiceApply, CHOICE_BOOT,
    };

    const TEN: u64 = 10_240 * 1024 * 1024;

    fn spare() -> u64 {
        crate::mgmt::megaraid::IRON_LD1_BYTES
    }

    #[test]
    fn the_first_start_takes_slot_zero_and_the_second_takes_slot_one() {
        clear_perc_spa_choice_for_test();
        assert_eq!(free_slot(), Some(0));
        assert_eq!(only_running(), Only::None);
        assert!(!any_guest_on());
        assert_eq!(
            with_slot(0, || apply_spa_choice(true, CHOICE_BOOT)),
            PercChoiceApply::Accepted
        );
        assert_eq!(slot_running(VmcsGuest::Window), Some(0));
        assert_eq!(only_running(), Only::Slot(0));
        assert_eq!(free_slot(), Some(1));
        assert_eq!(
            with_slot(1, || apply_tail_boot_choice(true, spare(), TEN)),
            PercChoiceApply::Accepted
        );
        let off = crate::mgmt::megaraid::PERC_IMAGE_BYTES;
        assert_eq!(slot_running(VmcsGuest::Tail(off)), Some(1));
        assert_eq!(slot_of_row(off), Some(1));
        assert_eq!(slot_of_row(0), Some(0));
        assert_eq!(running_count(), 2);
        assert_eq!(only_running(), Only::Many);
        assert_eq!(free_slot(), Some(2));
        clear_perc_spa_choice_for_test();
    }

    #[test]
    fn a_stopped_slot_is_free_again_and_the_other_guest_stays() {
        clear_perc_spa_choice_for_test();
        with_slot(0, || apply_spa_choice(true, CHOICE_BOOT));
        with_slot(1, || apply_tail_boot_choice(true, spare(), TEN));
        assert!(with_slot(0, request_guest_stop));
        assert!(with_slot(0, take_guest_stop));
        assert_eq!(slot_running(VmcsGuest::Window), None);
        assert_eq!(only_running(), Only::Slot(1));
        assert_eq!(free_slot(), Some(0));
        let v = views();
        assert!(v[0].stopped && v[0].holds_window() && !v[0].on());
        assert!(v[1].on() && v[1].holds_tail(crate::mgmt::megaraid::PERC_IMAGE_BYTES));
        assert_eq!(v[2], SlotView::EMPTY);
        clear_perc_spa_choice_for_test();
    }

    #[test]
    fn a_slot_keeps_its_own_stop_latch() {
        clear_perc_spa_choice_for_test();
        with_slot(0, || apply_spa_choice(true, CHOICE_BOOT));
        with_slot(1, || apply_tail_boot_choice(true, spare(), TEN));
        assert!(with_slot(1, request_guest_stop));
        assert!(!with_slot(0, take_guest_stop), "slot 0 was not asked to stop");
        assert!(with_slot(1, take_guest_stop));
        assert!(with_slot(0, || !guest_stopped()));
        clear_perc_spa_choice_for_test();
    }
}
