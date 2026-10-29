//! B4 clean power. A shutdown or reboot ask raises the ACPI power-button
//! status bit (PM1_STS bit 8) on the next guest read. The guest acks by
//! writing 1 to that bit. The Guests Stop path stays separate: this module
//! does not take that path.
//!
//! SCI and the Guests button are a later slice. A guest that only looks at
//! the status bit can see the ask. Ubuntu's button driver waits for SCI, so
//! this slice does not power the guest off.

use core::sync::atomic::{AtomicU8, Ordering};

/// ACPI PM1 status, power button.
pub const PM1_STS_PWRBTN: u16 = 1 << 8;

const NONE: u8 = 0;
const SHUTDOWN: u8 = 1;
const REBOOT: u8 = 2;
const OPEN: u8 = 0;
const ACKED: u8 = 1;

static ASK: AtomicU8 = AtomicU8::new(NONE);
static PHASE: AtomicU8 = AtomicU8::new(OPEN);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PowerAsk {
    Shutdown,
    Reboot,
}

pub fn request(ask: PowerAsk) {
    ASK.store(code(ask), Ordering::Release);
    PHASE.store(OPEN, Ordering::Release);
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
pub fn note_cleared(cleared: u16) {
    if pending().is_none() || (cleared & PM1_STS_PWRBTN) == 0 {
        return;
    }
    if PHASE.swap(ACKED, Ordering::AcqRel) == OPEN {
        ASK.store(NONE, Ordering::Release);
        #[cfg(feature = "uefi-bin")]
        crate::boot::serial::write_line("boot: B4 power ack");
    }
}

pub fn acked() -> bool {
    PHASE.load(Ordering::Acquire) == ACKED && pending().is_none()
}

pub fn clear() {
    #[cfg(test)]
    let _g = test_lock();
    ASK.store(NONE, Ordering::Release);
    PHASE.store(OPEN, Ordering::Release);
}

/// Hold this across a multi-step ask so a parallel `clear` waits.
/// Do not call [`clear`] or [`crate::devices::guest_platform::reset`] while
/// the guard is held.
#[cfg(test)]
pub fn test_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
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
        assert_eq!(apply_status(0x1), PM1_STS_PWRBTN | 0x1);
        note_cleared(0x1);
        assert_eq!(pending(), Some(PowerAsk::Shutdown));
        note_cleared(PM1_STS_PWRBTN);
        assert!(pending().is_none());
        assert!(acked());
        assert_eq!(apply_status(0), 0);
        request(PowerAsk::Reboot);
        assert_eq!(pending(), Some(PowerAsk::Reboot));
        assert!(!acked());
        assert_eq!(apply_status(0), PM1_STS_PWRBTN);
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
