//! RayNu-F launch opt-in (ADR-016, F2b).
//!
//! Presence of `\EFI\RayNu\raynuf.txt` on the loaded-image volume asks the
//! hypervisor to launch the RayNu-F test application on the private
//! guest-firmware VMCS **after** the retained-OVMF leg stops (where it would
//! otherwise fall through to E4). Absent the flag the boot path is unchanged.
//! The QEMU harness stages the flag with `RAYNU_F=1`; iron only when asked.
//!
//! Pillar: [Z] · Proven Core: **outside** (ADR-002)

use core::sync::atomic::{AtomicBool, Ordering};

static REQUESTED: AtomicBool = AtomicBool::new(false);

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
    }
}

#[cfg(not(target_os = "uefi"))]
pub fn probe() {}
