//! Pre-EBS ESP asset staging for guest kernels (M3.7 / M3.10).
//!
//! Pillar: [Z]
//! Proven Core: **outside** (ADR-002)
//!
//! ExitBootServices tears down file protocols — read `\EFI\BOOT\BZIMAGE`
//! and `\EFI\BOOT\INITRD` into static buffers before handoff.

/// Max bzImage staged from ESP (tinyconfig ~1–2 MiB).
pub const BZIMAGE_CAP: usize = 2 * 1024 * 1024;
/// Max initrd staged from ESP (static `/init` cpio is tiny).
pub const INITRD_CAP: usize = 256 * 1024;

static mut BZIMAGE_BUF: [u8; BZIMAGE_CAP] = [0; BZIMAGE_CAP];
static mut BZIMAGE_LEN: usize = 0;
static mut INITRD_BUF: [u8; INITRD_CAP] = [0; INITRD_CAP];
static mut INITRD_LEN: usize = 0;

/// Bytes staged by [`probe_bzimage`] (empty if none).
pub fn bzimage_bytes() -> Option<&'static [u8]> {
    // SAFETY: single-threaded boot; written only in probe before consumers run.
    unsafe {
        let len = BZIMAGE_LEN;
        if len == 0 || len > BZIMAGE_CAP {
            None
        } else {
            Some(&BZIMAGE_BUF[..len])
        }
    }
}

/// Store raw bzImage bytes into the static stage (tests / embedded fallback).
pub fn stage_bzimage(bytes: &[u8]) -> Result<(), ()> {
    if bytes.is_empty() || bytes.len() > BZIMAGE_CAP {
        return Err(());
    }
    // SAFETY: boot / test single-threaded.
    unsafe {
        BZIMAGE_BUF[..bytes.len()].copy_from_slice(bytes);
        BZIMAGE_LEN = bytes.len();
    }
    Ok(())
}

/// Bytes staged by [`probe_initrd`] / [`stage_initrd`] (empty if none).
pub fn initrd_bytes() -> Option<&'static [u8]> {
    // SAFETY: single-threaded boot; written only in probe before consumers run.
    unsafe {
        let len = INITRD_LEN;
        if len == 0 || len > INITRD_CAP {
            None
        } else {
            Some(&INITRD_BUF[..len])
        }
    }
}

/// Store raw initrd bytes into the static stage.
pub fn stage_initrd(bytes: &[u8]) -> Result<(), ()> {
    if bytes.is_empty() || bytes.len() > INITRD_CAP {
        return Err(());
    }
    // SAFETY: boot / test single-threaded.
    unsafe {
        INITRD_BUF[..bytes.len()].copy_from_slice(bytes);
        INITRD_LEN = bytes.len();
    }
    Ok(())
}

/// Clear staged bytes (host tests).
pub fn clear_staged() {
    unsafe {
        BZIMAGE_LEN = 0;
        INITRD_LEN = 0;
    }
}

/// Probe the loaded image's ESP for `\EFI\BOOT\BZIMAGE` (+ INITRD) (UEFI only).
///
/// Must run **before** [`crate::boot::handoff::leave_firmware`].
#[cfg(target_os = "uefi")]
pub fn probe_bzimage() {
    use uefi::boot;
    use uefi::fs::FileSystem;
    use uefi::CString16;

    let image = boot::image_handle();
    let Ok(sfs) = boot::get_image_file_system(image) else {
        return;
    };
    let mut fs = FileSystem::new(sfs);
    if let Ok(path) = CString16::try_from("\\EFI\\BOOT\\BZIMAGE") {
        if let Ok(data) = fs.read(path.as_ref()) {
            let _ = stage_bzimage(&data);
        }
    }
    if let Ok(path) = CString16::try_from("\\EFI\\BOOT\\INITRD") {
        if let Ok(data) = fs.read(path.as_ref()) {
            let _ = stage_initrd(&data);
        }
    }
}

#[cfg(not(target_os = "uefi"))]
pub fn probe_bzimage() {}
