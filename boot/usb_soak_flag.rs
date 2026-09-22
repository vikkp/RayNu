//! ESP flag for the post-EBS USB BOT soak (M8 Phase 1 bench).
//!
//! Pillar: [Z]
//! Proven Core: **outside** (ADR-002)
//! VERIFICATION: L1 (host tests on the pure helpers)
//!
//! Presence of `\EFI\RayNu\usbsoak.txt` on the loaded-image volume asks
//! the hypervisor to run [`crate::mgmt::xhci::xhci_usb_soak`] right after
//! `usb I/O ready` + the GPT peek, instead of attaching virtio and
//! launching RayNu-F. The soak reads single sectors with idle gaps of
//! 0/5/30/120 s and prints ok/fail per gap plus the xHCI timeout dump on
//! every miss. One flash, hundreds of data points, no guest at risk.
//!
//! Never prints `RAYNU-V-M8-DISK-PERSIST-OK` or `RAYNU-V-M7-ISO-INSTALL-OK`.

use core::sync::atomic::{AtomicBool, Ordering};

static REQUESTED: AtomicBool = AtomicBool::new(false);

/// Flag path (namespaced like `raynuf.txt` / `paperverbose.txt`).
pub const USB_SOAK_FLAG_PATH: &str = "\\EFI\\RayNu\\usbsoak.txt";
/// Serial line when the flag is found pre-EBS.
pub const USB_SOAK_REQUESTED_MARKER: &str =
    "boot: USB soak requested (EFI/RayNu/usbsoak.txt; no guest this boot; not ISO-INSTALL-OK)";

/// Whether the operator asked for the USB soak.
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
    let Ok(p) = CString16::try_from(USB_SOAK_FLAG_PATH) else {
        return;
    };
    // Presence alone is the signal; content may be empty.
    if fs.read(p.as_ref()).is_ok() {
        REQUESTED.store(true, Ordering::Release);
        crate::boot::serial::write_line(USB_SOAK_REQUESTED_MARKER);
    }
}

#[cfg(not(target_os = "uefi"))]
pub fn probe() {}

/// Host gate: flag wiring is in place and the soak never reuses iron markers.
pub fn prop_usb_soak_wired() -> bool {
    let xhci = include_str!("../mgmt/xhci.rs");
    let lun = include_str!("../mgmt/durable_lun.rs");
    let esp = include_str!("esp_assets.rs");
    USB_SOAK_FLAG_PATH.ends_with("usbsoak.txt")
        && esp.contains("usb_soak_flag::probe()")
        && lun.contains("usb_soak_flag::requested()")
        && lun.contains("xhci_usb_soak()")
        && xhci.contains("pub fn xhci_usb_soak() -> !")
        && xhci.contains("USBSOAK gap_s=")
        && xhci.contains("RAYNU-V-USBSOAK-DONE")
        && !xhci.contains("println!(\"RAYNU-V-M8-DISK-PERSIST-OK")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usb_soak_flag_is_wired() {
        assert!(prop_usb_soak_wired());
    }

    #[test]
    fn usb_soak_flag_default_off() {
        force_for_test(false);
        assert!(!requested());
        force_for_test(true);
        assert!(requested());
        force_for_test(false);
    }
}
