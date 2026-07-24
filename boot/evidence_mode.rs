//! Evidence / verbose mode triggered by ESP flag file (ADR-011).
//!
//! Pillar: [V] [A] [Z] [D]
//! Proven Core: **outside** for detection and serial formatting (ADR-002).
//! The activation audit event is recorded via the Proven Core integrity path.
//!
//! Presence of `paperverbose.txt` (volume root) or `\\EFI\\RayNu\\paperverbose.txt`
//! on the image load volume activates evidence mode **before** ExitBootServices.
//! Matching is case-insensitive on FAT. The file may be empty.
//!
//! Evidence mode:
//! - raises structured serial output in the living-paper evidence-block shape
//! - records `AuditEvent::EvidenceModeActivated`
//! - never claims L2/L3 (EFI can only report runtime-enforced / self-test results)
//!
//! Default path (flag absent) is unchanged.

use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};

/// CI / serial marker when evidence mode is active.
pub const EVIDENCE_MODE_ON_MARKER: &str = "RAYNU-V-EVIDENCE-MODE-ON";

/// Bundle delimiters for easy scrape from iDRAC / QEMU serial logs.
pub const EVIDENCE_BUNDLE_BEGIN: &str = "=== RAYNU-V EVIDENCE BUNDLE BEGIN ===";
pub const EVIDENCE_BUNDLE_END: &str = "=== RAYNU-V EVIDENCE BUNDLE END ===";

/// Source tag stored in the activation audit event.
pub const SOURCE_NONE: u8 = 0;
pub const SOURCE_ROOT: u8 = 1; // \\paperverbose.txt
pub const SOURCE_EFI_RAYNU: u8 = 2; // \\EFI\\RayNu\\paperverbose.txt

static ACTIVE: AtomicBool = AtomicBool::new(false);
static SOURCE: AtomicU8 = AtomicU8::new(SOURCE_NONE);

/// Whether evidence mode was activated for this boot.
#[inline]
pub fn is_active() -> bool {
    ACTIVE.load(Ordering::Acquire)
}

/// Which flag path activated the mode (`SOURCE_*`).
#[inline]
pub fn source() -> u8 {
    SOURCE.load(Ordering::Acquire)
}

/// Force-activate for host unit tests (no UEFI filesystem).
#[cfg(test)]
pub fn force_active_for_test(src: u8) {
    SOURCE.store(src, Ordering::Release);
    ACTIVE.store(true, Ordering::Release);
}

/// Clear mode state (host tests).
#[cfg(test)]
pub fn clear_for_test() {
    ACTIVE.store(false, Ordering::Release);
    SOURCE.store(SOURCE_NONE, Ordering::Release);
}

/// Probe the loaded image volume for the ADR-011 flag file.
///
/// Must run **before** [`crate::boot::handoff::leave_firmware`].
/// On non-UEFI builds this is a no-op (tests use [`force_active_for_test`]).
#[cfg(target_os = "uefi")]
pub fn probe() {
    use uefi::boot;
    use uefi::fs::FileSystem;
    use uefi::CString16;

    let image = boot::image_handle();
    let Ok(sfs) = boot::get_image_file_system(image) else {
        return;
    };
    let mut fs = FileSystem::new(sfs);

    // Prefer conventional path, then volume root (ADR-011 check order inverted
    // only for preference of the namespaced location; either activates).
    if flag_present(&mut fs, "\\EFI\\RayNu\\paperverbose.txt") {
        activate(SOURCE_EFI_RAYNU);
        return;
    }
    if flag_present(&mut fs, "\\paperverbose.txt") {
        activate(SOURCE_ROOT);
    }
}

#[cfg(not(target_os = "uefi"))]
pub fn probe() {}

#[cfg(target_os = "uefi")]
fn flag_present(fs: &mut uefi::fs::FileSystem, path: &str) -> bool {
    use uefi::CString16;
    let Ok(p) = CString16::try_from(path) else {
        return false;
    };
    // Presence alone is the signal; content may be empty.
    fs.read(p.as_ref()).is_ok()
}

fn activate(src: u8) {
    SOURCE.store(src, Ordering::Release);
    ACTIVE.store(true, Ordering::Release);

    crate::audit::integrity::record_event(crate::audit::AuditEvent::EvidenceModeActivated {
        source: src,
    });

    crate::boot::serial::write_line(EVIDENCE_MODE_ON_MARKER);
    crate::boot::serial::write_str("boot: evidence mode ON source=");
    match src {
        SOURCE_EFI_RAYNU => crate::boot::serial::write_line("EFI/RayNu/paperverbose.txt"),
        SOURCE_ROOT => crate::boot::serial::write_line("paperverbose.txt (volume root)"),
        _ => crate::boot::serial::write_line("(unknown)"),
    }
}

/// Emit a structured Evidence Bundle header on COM1 (living-paper friendly).
///
/// Call once after activation (and optionally again at gate checkpoints).
/// Maturity claims never exceed L1 from the EFI itself (ADR-011).
pub fn emit_bundle_header(milestone_label: &str) {
    if !is_active() {
        return;
    }

    use crate::boot::serial;

    serial::write_line(EVIDENCE_BUNDLE_BEGIN);
    serial::write_line("**Evidence (runtime, ADR-011 paperverbose.txt)**");
    serial::write_str("- Mode: active source=");
    write_u8_dec(source());
    serial::write_byte(b'\n');
    serial::write_str("- Milestone context: ");
    serial::write_line(milestone_label);
    serial::write_line("- Maturity level claimed: L1 (runtime-enforced / self-test)");
    serial::write_line("- Artifact type: serial log | audit-ring event | gate checklist");
    serial::write_line("- Observation: evidence mode activated; formal Verus/Kani transcripts remain offline");
    serial::write_line("- Note: L2/L3 claims require host-side Verus/Kani artifacts (ADR-001/008)");
    serial::write_line(EVIDENCE_BUNDLE_END);
}

/// Extra diagnostic line used throughout boot when evidence mode is on.
pub fn verbose_line(msg: &str) {
    if is_active() {
        crate::boot::serial::write_str("EVID: ");
        crate::boot::serial::write_line(msg);
    }
}

fn write_u8_dec(n: u8) {
    if n >= 10 {
        crate::boot::serial::write_byte(b'0' + (n / 10));
    }
    crate::boot::serial::write_byte(b'0' + (n % 10));
}

#[cfg(test)]
#[path = "evidence_mode_test.rs"]
mod evidence_mode_test;
