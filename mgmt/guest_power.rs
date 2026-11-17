//! B4 clean power.
//!
//! Pillar: [Z]
//! Proven Core: **outside**
//! VERIFICATION: L1 (host tests)
//!
//! A shutdown or reboot ask raises PM1_STS bit 8. When the guest has enabled
//! that button, the exit loop raises SCI (IRQ 9) and lowers it when the
//! guest clears the status bit. The guest shuts down and writes `_S5`
//! (`SLP_TYP` 5, `SLP_EN`). The
//! exit loop then stops the guest or boots it again. Stop stays a separate
//! path and this module does not call it.

use crate::guest::slot::PerGuest;
use crate::per_guest;
use core::sync::atomic::{AtomicU8, Ordering};

/// ACPI PM1 status, power button.
pub const PM1_STS_PWRBTN: u16 = 1 << 8;
/// ACPI PM1 enable, power button.
pub const PM1_EN_PWRBTN: u16 = 1 << 8;
/// SCI line in the FADT (`SCI_INT` = 9).
pub const SCI_GSI: u8 = 9;
/// `_S5` sleep type written into `PM1_CNT` bits 10–12.
pub const SLP_TYP_S5: u16 = 5;

const NONE: u8 = 0;
const SHUTDOWN: u8 = 1;
const REBOOT: u8 = 2;
const OPEN: u8 = 0;
const SEEN: u8 = 1;
const DONE: u8 = 2;

static ASK: PerGuest<AtomicU8> = per_guest!(AtomicU8::new(NONE));
static PHASE: PerGuest<AtomicU8> = per_guest!(AtomicU8::new(OPEN));
static SCI: PerGuest<AtomicU8> = per_guest!(AtomicU8::new(0));

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PowerAsk {
    Shutdown,
    Reboot,
}

pub fn request(ask: PowerAsk) {
    ASK.store(code(ask), Ordering::Release);
    PHASE.store(OPEN, Ordering::Release);
    SCI.store(0, Ordering::Release);
    #[cfg(feature = "uefi-bin")]
    match ask {
        PowerAsk::Shutdown => crate::boot::serial::write_line("boot: B4 power shutdown"),
        PowerAsk::Reboot => crate::boot::serial::write_line("boot: B4 power reboot"),
    }
}

pub fn pending() -> Option<PowerAsk> {
    decode(ASK.load(Ordering::Acquire))
}

/// Status word a guest read should see. The stored PM1 copy stays unchanged.
pub fn apply_status(sts: u16) -> u16 {
    if pending().is_some() && PHASE.load(Ordering::Acquire) == OPEN {
        sts | PM1_STS_PWRBTN
    } else {
        sts
    }
}

/// `cleared` is the bit mask the guest wrote as 1 (write-1-to-clear).
/// The ask stays remembered until [`note_s5`] or [`clear`].
pub fn note_cleared(cleared: u16) {
    if pending().is_none() || (cleared & PM1_STS_PWRBTN) == 0 {
        return;
    }
    if PHASE.compare_exchange(OPEN, SEEN, Ordering::AcqRel, Ordering::Acquire).is_ok() {
        #[cfg(feature = "uefi-bin")]
        crate::boot::serial::write_line("boot: B4 power ack");
    }
}

/// True once per open ask, after the guest enables the power button.
pub fn arm_sci(pm1_en: u16) -> bool {
    if PHASE.load(Ordering::Acquire) != OPEN || pending().is_none() {
        return false;
    }
    if pm1_en & PM1_EN_PWRBTN == 0 {
        return false;
    }
    SCI.compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire).is_ok()
}

/// True once when the SCI line that [`arm_sci`] raised must go low: the
/// guest cleared the power-button status, or turned the button's enable
/// off. The caller lowers IRQ 9.
///
/// SCI is a level. On an I/O APIC a level pin that stays high is delivered
/// again after every EOI. Nested two-guest run: the ask raised IRQ 9 and
/// nothing lowered it, and the guest printed `soft lockup ... irq/9-acpi`.
/// B4 lived on the 8259 (`62165436`), where one raise is one interrupt.
pub fn drop_sci(pm1_en: u16) -> bool {
    if SCI.load(Ordering::Acquire) != 1 {
        return false;
    }
    let open = pending().is_some() && PHASE.load(Ordering::Acquire) == OPEN;
    if open && pm1_en & PM1_EN_PWRBTN != 0 {
        return false;
    }
    // Status still set and only the enable went off: the line rises again
    // when the guest turns the enable back on.
    let next = if open { 0 } else { 2 };
    SCI.compare_exchange(1, next, Ordering::AcqRel, Ordering::Acquire)
        .is_ok()
}

/// Guest wrote `SLP_EN` with this `SLP_TYP`. Only type 5, and only while an
/// ask is open, finishes the button.
pub fn note_s5(slp_typ: u16) {
    if slp_typ != SLP_TYP_S5 || pending().is_none() {
        return;
    }
    let phase = PHASE.load(Ordering::Acquire);
    if phase != OPEN && phase != SEEN {
        return;
    }
    PHASE.store(DONE, Ordering::Release);
}

/// The exit loop consumes a finished S5. `Shutdown` stops. `Reboot` boots again.
pub fn take_s5() -> Option<PowerAsk> {
    if PHASE.load(Ordering::Acquire) != DONE {
        return None;
    }
    let ask = pending();
    ASK.store(NONE, Ordering::Release);
    PHASE.store(OPEN, Ordering::Release);
    SCI.store(0, Ordering::Release);
    ask
}

pub fn acked() -> bool {
    matches!(PHASE.load(Ordering::Acquire), SEEN | DONE) && pending().is_some()
}

fn code(ask: PowerAsk) -> u8 {
    match ask {
        PowerAsk::Shutdown => SHUTDOWN,
        PowerAsk::Reboot => REBOOT,
    }
}

fn decode(v: u8) -> Option<PowerAsk> {
    match v {
        SHUTDOWN => Some(PowerAsk::Shutdown),
        REBOOT => Some(PowerAsk::Reboot),
        _ => None,
    }
}

pub fn clear() {
    #[cfg(test)]
    let _g = test_lock();
    ASK.store(NONE, Ordering::Release);
    PHASE.store(OPEN, Ordering::Release);
    SCI.store(0, Ordering::Release);
}

/// Hold this across a multi-step ask so a parallel `clear` waits.
/// Do not call [`clear`] or [`crate::devices::guest_platform::reset`] while
/// the guard is held.
#[cfg(test)]
pub fn test_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

#[cfg(test)]
pub fn reset_for_test() {
    clear();
}

#[cfg(test)]
mod guest_power_test {
    use super::*;

    #[test]
    fn an_open_ask_sets_the_power_button_bit_until_the_guest_clears_it() {
        reset_for_test();
        let _g = test_lock();
        assert!(pending().is_none());
        assert_eq!(apply_status(0), 0);
        request(PowerAsk::Shutdown);
        assert_eq!(pending(), Some(PowerAsk::Shutdown));
        assert_eq!(apply_status(0), PM1_STS_PWRBTN);
        assert!(!arm_sci(0));
        assert!(arm_sci(PM1_EN_PWRBTN));
        assert!(!arm_sci(PM1_EN_PWRBTN));
        note_cleared(0x1);
        assert_eq!(pending(), Some(PowerAsk::Shutdown));
        assert_eq!(apply_status(0), PM1_STS_PWRBTN);
        note_cleared(PM1_STS_PWRBTN);
        assert_eq!(pending(), Some(PowerAsk::Shutdown));
        assert!(acked());
        assert_eq!(apply_status(0), 0);
        note_s5(0);
        assert!(take_s5().is_none());
        note_s5(SLP_TYP_S5);
        assert_eq!(take_s5(), Some(PowerAsk::Shutdown));
        assert!(pending().is_none());
        request(PowerAsk::Reboot);
        note_s5(SLP_TYP_S5);
        assert_eq!(take_s5(), Some(PowerAsk::Reboot));
        drop(_g);
        reset_for_test();
    }

    #[test]
    fn the_sci_line_goes_low_when_the_guest_clears_the_button() {
        reset_for_test();
        let _g = test_lock();
        assert!(!drop_sci(PM1_EN_PWRBTN), "nothing raised");
        request(PowerAsk::Shutdown);
        assert!(arm_sci(PM1_EN_PWRBTN));
        assert!(!drop_sci(PM1_EN_PWRBTN), "status set and enabled: stays high");
        note_cleared(PM1_STS_PWRBTN);
        assert!(drop_sci(PM1_EN_PWRBTN), "cleared: the line goes low");
        assert!(!drop_sci(PM1_EN_PWRBTN), "once");
        assert!(!arm_sci(PM1_EN_PWRBTN), "one ask is one interrupt");
        // The enable going off also lowers the line, and it rises again
        // when the enable comes back with the status still set.
        request(PowerAsk::Reboot);
        assert!(arm_sci(PM1_EN_PWRBTN));
        assert!(drop_sci(0));
        assert!(arm_sci(PM1_EN_PWRBTN));
        drop(_g);
        reset_for_test();
    }

    #[test]
    fn this_module_does_not_call_the_hard_stop() {
        let src = include_str!("guest_power.rs");
        let body = src.split_once("mod guest_power_test").expect("tests").0;
        assert!(!body.contains("guest_stop"));
        assert!(body.contains("boot: B4 power shutdown"));
        assert!(body.contains("boot: B4 power reboot"));
        assert!(body.contains("boot: B4 power ack"));
    }
}
