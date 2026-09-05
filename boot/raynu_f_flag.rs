//! RayNu-F launch opt-in (ADR-016, F2b).
//!
//! Presence of `\EFI\RayNu\raynuf.txt` on the loaded-image volume asks the
//! hypervisor to launch the RayNu-F test application on the private
//! guest-firmware VMCS **after** the retained-OVMF leg stops (where it would
//! otherwise fall through to E4). Absent the flag the boot path is unchanged.
//! The QEMU harness stages the flag with `RAYNU_F=1`; iron only when asked.
//!
//! Pillar: [Z] · Proven Core: **outside** (ADR-002)

use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

static REQUESTED: AtomicBool = AtomicBool::new(false);
/// TSC ticks per second, calibrated pre-EBS against a UEFI `Stall`. 0 = none.
static TSC_HZ: AtomicU64 = AtomicU64::new(0);

/// Calibration window (microseconds) for the pre-EBS `Stall`.
pub const TSC_CALIB_STALL_US: u64 = 10_000;
/// Reject calibrations outside [200 MHz, 10 GHz] — a bad firmware Stall.
pub const TSC_HZ_MIN: u64 = 200_000_000;
pub const TSC_HZ_MAX: u64 = 10_000_000_000;

/// RayNu-F's clock rate (TSC Hz), or 0 if never calibrated.
#[inline]
pub fn tsc_hz() -> u64 {
    TSC_HZ.load(Ordering::Acquire)
}

/// Derive Hz from a TSC delta over `TSC_CALIB_STALL_US`; `None` if implausible.
pub fn tsc_hz_from_delta(delta: u64) -> Option<u64> {
    let hz = delta.saturating_mul(1_000_000 / TSC_CALIB_STALL_US);
    if (TSC_HZ_MIN..=TSC_HZ_MAX).contains(&hz) {
        Some(hz)
    } else {
        None
    }
}

/// Host tests only.
#[cfg(test)]
pub fn force_tsc_hz_for_test(hz: u64) {
    TSC_HZ.store(hz, Ordering::Release);
}

/// Decimal formatter for pre-EBS serial (no `alloc`).
pub fn fmt_dec(mut v: u64, buf: &mut [u8; 20]) -> &str {
    let mut i = buf.len();
    if v == 0 {
        i -= 1;
        buf[i] = b'0';
    }
    while v > 0 {
        i -= 1;
        buf[i] = b'0' + (v % 10) as u8;
        v /= 10;
    }
    // SAFETY-free: digits are ASCII.
    core::str::from_utf8(&buf[i..]).unwrap_or("?")
}

/// Flag path (namespaced like `paperverbose.txt`, ADR-011).
pub const RAYNU_F_FLAG_PATH: &str = "\\EFI\\RayNu\\raynuf.txt";
/// Serial line when the flag is found pre-EBS.
pub const RAYNU_F_REQUESTED_MARKER: &str =
    "boot: RayNu-F launch requested (EFI/RayNu/raynuf.txt; ADR-016 F2b; not ISO-INSTALL-OK)";

/// Whether the operator asked for the RayNu-F F2 launch.
#[inline]
pub fn requested() -> bool {
    REQUESTED.load(Ordering::Acquire)
}

/// Host tests only.
#[cfg(test)]
pub fn force_for_test(on: bool) {
    REQUESTED.store(on, Ordering::Release);
}

/// Probe the loaded-image volume. Must run **before** ExitBootServices.
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
    let Ok(p) = CString16::try_from(RAYNU_F_FLAG_PATH) else {
        return;
    };
    // Presence alone is the signal; content may be empty.
    if fs.read(p.as_ref()).is_ok() {
        REQUESTED.store(true, Ordering::Release);
        crate::boot::serial::write_line(RAYNU_F_REQUESTED_MARKER);
        // Owned firmware clock: calibrate TSC against the platform's Stall
        // while boot services are still alive.
        let t0 = crate::arch::cpu::rdtsc();
        boot::stall(TSC_CALIB_STALL_US as usize);
        let t1 = crate::arch::cpu::rdtsc();
        match tsc_hz_from_delta(t1.wrapping_sub(t0)) {
            Some(hz) => {
                TSC_HZ.store(hz, Ordering::Release);
                let mut buf = [0u8; 20];
                let s = fmt_dec(hz, &mut buf);
                crate::boot::serial::write_str("boot: RayNu-F tsc_hz=");
                crate::boot::serial::write_str(s);
                crate::boot::serial::write_line(" (pre-EBS Stall calibration)");
            }
            None => {
                crate::boot::serial::write_line(
                    "boot: RayNu-F WARN tsc calibration implausible; clock falls back to 1 GHz",
                );
            }
        }
    }
}

#[cfg(not(target_os = "uefi"))]
pub fn probe() {}
