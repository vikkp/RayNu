//! ADR-021 H2. A host panic dumps to COM2, then reboots.
//!
//! Pillar: [Z] [D]
//! Proven Core: **outside** (ADR-002)
//!
//! COM2 is the iDRAC SOL console (`console com2`). The dump is that
//! line. The reboot is `ResetSystem` COLD after `VMXOFF`. Saved guests
//! start on the next RayNu boot (ADR-021 B5). On this lab F11 is a
//! one-shot, so a cold reset returns to Ubuntu until the operator boots
//! the stick again.
//!
//! Host/CI never call `ResetSystem`.

use core::sync::atomic::{AtomicBool, Ordering};

pub const PANIC_PREFIX: &str = "boot: H2 panic ";
pub const REBOOT_LINE: &str =
    "boot: H2 reboot (saved guests start on the next RayNu boot)";
pub const VMXOFF_FAIL: &str = "boot: H2 reboot skipped (VMXOFF failed)";

static REQUESTED: AtomicBool = AtomicBool::new(false);

/// Overview asked for the same dump a panic would print, then a reboot.
pub fn note_request() {
    REQUESTED.store(true, Ordering::Release);
}

pub fn requested() -> bool {
    REQUESTED.load(Ordering::Acquire)
}

pub fn clear_request() {
    REQUESTED.store(false, Ordering::Release);
}

/// `boot: H2 panic ` plus a single-line reason. Newlines become spaces.
pub fn format_panic_line(reason: &str, buf: &mut [u8]) -> usize {
    let prefix = PANIC_PREFIX.as_bytes();
    let n = prefix.len().min(buf.len());
    buf[..n].copy_from_slice(&prefix[..n]);
    let mut at = n;
    for b in reason.bytes() {
        if at >= buf.len() {
            break;
        }
        buf[at] = if b == b'\n' || b == b'\r' { b' ' } else { b };
        at += 1;
    }
    at
}

/// Firmware panic and the Overview button both end here.
///
/// Returns only when `VMXOFF` fails. A successful reset does not return.
#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
pub fn reboot_after_dump(reason: &str) -> bool {
    use crate::boot::serial;
    let mut line = [0u8; 180];
    let n = format_panic_line(reason, &mut line);
    if let Ok(text) = core::str::from_utf8(&line[..n]) {
        serial::write_line_nowait(text);
    } else {
        serial::write_line_nowait(PANIC_PREFIX);
    }
    serial::write_line_nowait(REBOOT_LINE);
    // SAFETY: a panic or an authenticated SPA click. VMXOFF in VMX root
    // leaves VMX operation. ResetSystem with VMX still on can hang the
    // R640, so a failed VMXOFF does not reset.
    // KANI-TARGET: host tests cover the line, not this reset.
    match unsafe { crate::vmx::hardware::vmxoff() } {
        Ok(()) => {
            uefi::runtime::reset(
                uefi::runtime::ResetType::COLD,
                uefi::Status::ABORTED,
                None,
            );
        }
        Err(_) => {
            serial::write_line_nowait(VMXOFF_FAIL);
            false
        }
    }
}

#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
pub fn firmware_panic(info: &core::panic::PanicInfo) -> ! {
    use core::fmt::Write;
    let mut reason = [0u8; 120];
    let mut w = LineBuf {
        buf: &mut reason,
        n: 0,
    };
    let _ = write!(&mut w, "{info}");
    let n = w.n;
    drop(w);
    let text = core::str::from_utf8(&reason[..n]).unwrap_or("panic");
    if reboot_after_dump(text) {
        // reset does not return
    }
    loop {
        core::hint::spin_loop();
    }
}

#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
struct LineBuf<'a> {
    buf: &'a mut [u8],
    n: usize,
}

#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
impl core::fmt::Write for LineBuf<'_> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        for b in s.bytes() {
            if self.n >= self.buf.len() {
                break;
            }
            self.buf[self.n] = b;
            self.n += 1;
        }
        Ok(())
    }
}

/// Host package: the dump line, the button, and the cold reset are in
/// tree. Host tests do not execute `ResetSystem`.
pub fn prop_host_panic() -> bool {
    let http = include_str!("http.rs");
    let listen = include_str!("host_nic_listen.rs");
    let html = include_str!("../assets/webui.html");
    let main = include_str!("../src/main.rs");
    http.contains("/host/panic")
        && listen.contains("reboot_after_dump")
        && listen.contains("ResetType::COLD")
        && html.contains("id=\"btn-panic\"")
        && html.contains("/host/panic")
        && main.contains("firmware_panic")
        && main.contains("fn raynu_panic")
}

#[cfg(test)]
mod host_panic_test {
    use super::*;

    #[test]
    fn the_dump_is_one_com2_line_and_the_page_posts_it() {
        let mut buf = [0u8; 64];
        let n = format_panic_line("spa\nline", &mut buf);
        let text = core::str::from_utf8(&buf[..n]).unwrap();
        assert_eq!(text, "boot: H2 panic spa line");
        assert!(prop_host_panic());
        note_request();
        assert!(requested());
        clear_request();
        assert!(!requested());
    }
}
