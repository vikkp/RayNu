//! Pre-EBS retain of ESP `\\EFI\\RayNu\\OVMF.fd` (ADR-014 presence rule).
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** (ADR-002 / ADR-003 / ADR-014)
//! VERIFICATION: L1 (runtime assert + unit tests)
//!
//! ExitBootServices tears down file protocols. Real EDK2 bytes are copied
//! into a static retained buffer **before** handoff. That is ADR-003
//! split-mode, not a PE embed.
//!
//! [`bytes_present`] is true only when the retained image passes the
//! accept rule (size, `_FVH`, nonempty). Heap fixtures used by Stages
//! 5–35 fail that rule. Presence is not a VMLAUNCH.

use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use crate::audit::AuditEvent;
use crate::audit_log;

/// ADR-003 uncompressed cap. Same as [`crate::mgmt::guest_fw::GUEST_FW_MAX_UNCOMPRESSED`].
pub const OVMF_ESP_CAP: usize = 4 * 1024 * 1024;
/// Minimum real EDK2 size. Mock / size-floor fixtures stay below this.
pub const MIN_REAL_OVMF_BYTES: usize = 1024 * 1024;
/// Nonzero-byte floor. Zero-padded `_FVH` fixtures fail this.
pub const MIN_REAL_OVMF_NONEMPTY: usize = 64 * 1024;
/// `_FVH` offset in `EFI_FIRMWARE_VOLUME_HEADER` (PI Vol 3).
const FVH_SIG_OFF: usize = 0x28;
const FVH_SIG: [u8; 4] = *b"_FVH";

/// QEMU / serial marker when real ESP `OVMF.fd` bytes are retained.
pub const M7_E5_LIVE_BYTES_PRESENT_OK_MARKER: &str = "RAYNU-V-M7-E5-LIVE-BYTES-PRESENT-OK";

/// Honest residual after retain. Not VMLAUNCH. Not Everest E5.
pub const E5_OVMF_RETAIN_RESIDUAL_NOTE: &str =
    "residual: real ESP OVMF.fd bytes retained; guest_uefi_live_esp_bytes_present is true; private guest-UEFI VMCS + EPT are not allocated; VMLAUNCH insn not issued; attach_cdrom_uefi stays UnsupportedOnFirmware; no guest UEFI VMLAUNCH";

static mut OVMF_BUF: [u8; OVMF_ESP_CAP] = [0; OVMF_ESP_CAP];
static OVMF_LEN: AtomicU32 = AtomicU32::new(0);
static OVMF_PRESENT: AtomicBool = AtomicBool::new(false);

/// True when the retained buffer holds accepted ESP `OVMF.fd` bytes.
///
/// INVARIANTS:
/// - False until [`retain_ovmf_bytes`] accepts an image
/// - False for mock / size-floor / zero-padded alias fixtures
/// - Does not mean a private VMCS exists
/// - Does not issue VMLAUNCH
pub fn bytes_present() -> bool {
    OVMF_PRESENT.load(Ordering::Acquire)
}

/// Retained ESP `OVMF.fd` bytes when [`bytes_present`] is true.
pub fn retained_bytes() -> Option<&'static [u8]> {
    if !bytes_present() {
        return None;
    }
    let len = OVMF_LEN.load(Ordering::Acquire) as usize;
    if len < MIN_REAL_OVMF_BYTES || len > OVMF_ESP_CAP {
        return None;
    }
    // SAFETY: single-threaded boot / host `--test-threads=1`; written only
    // in retain/clear. Length is stored after the copy.
    // KANI-TARGET: retain/clear exclusive writer; present+len bound the slice.
    unsafe { Some(&OVMF_BUF[..len]) }
}

/// Accept rule for real ESP `OVMF.fd` bytes (not a Stage 5–35 fixture).
///
/// INVARIANTS:
/// - `MIN_REAL_OVMF_BYTES <= len <= OVMF_ESP_CAP`
/// - `_FVH` at offset `0x28`
/// - At least [`MIN_REAL_OVMF_NONEMPTY`] nonzero bytes
pub fn accept_real_ovmf_bytes(bytes: &[u8]) -> bool {
    if bytes.len() < MIN_REAL_OVMF_BYTES || bytes.len() > OVMF_ESP_CAP {
        return false;
    }
    let sig_end = FVH_SIG_OFF.saturating_add(4);
    let Some(sig) = bytes.get(FVH_SIG_OFF..sig_end) else {
        return false;
    };
    if sig != FVH_SIG {
        return false;
    }
    let nonempty = bytes.iter().filter(|b| **b != 0).count();
    nonempty >= MIN_REAL_OVMF_NONEMPTY
}

/// Copy accepted bytes into the retained buffer and set presence.
///
/// INVARIANTS:
/// - Rejected images leave presence false and the buffer cleared
/// - Does not write live EPT and does not issue VMLAUNCH
pub fn retain_ovmf_bytes(bytes: &[u8]) -> Result<usize, ()> {
    if !accept_real_ovmf_bytes(bytes) {
        clear_retained();
        return Err(());
    }
    // SAFETY: single-threaded boot / host `--test-threads=1`.
    // KANI-TARGET: accept_real_ovmf_bytes already bound len to OVMF_ESP_CAP.
    unsafe {
        OVMF_BUF[..bytes.len()].copy_from_slice(bytes);
    }
    OVMF_LEN.store(bytes.len() as u32, Ordering::Release);
    OVMF_PRESENT.store(true, Ordering::Release);
    audit_log!(AuditEvent::OvmfLiveEspBytesRetained {
        bytes_len: bytes.len() as u64,
    });
    debug_assert!(bytes_present());
    Ok(bytes.len())
}

/// Clear the retained buffer (host tests / failed accept).
pub fn clear_retained() {
    OVMF_PRESENT.store(false, Ordering::Release);
    OVMF_LEN.store(0, Ordering::Release);
}

/// Patch OVMF host-bridge switch immediates: i440FX DID `0x1237` → virtio `0x1042`.
///
/// Debian/QEMU 4M `OVMF.fd` compiles the switch as `cmp bx, imm16`
/// (`66 81 fb 37 12` then `66 81 fb c0 29` for Q35). Blind `37 12`
/// replace also hits LZMA payload in FV1 (~20 coincidences) and would
/// corrupt PEIM decompress. Only the `cmp r16, imm16` encoding is
/// rewritten. Hardware DID at `00:00.0` stays virtio (not two-phase DID).
/// Does **not** rewrite the retain buffer.
///
/// INVARIANTS:
/// - Hardware DID at `00:00.0` stays `0x1042`
/// - Retain buffer is not this slice
/// - Lone `37 12` in compressed FVs is left alone
/// - Returns the number of `cmp r16, 0x1237` sites replaced
pub fn remap_i440fx_did_imm(buf: &mut [u8]) -> u32 {
    // `66 81 /7 iw` with ModRM 11_111_r/m: cmp r16, imm16.
    const I440FX: [u8; 2] = [0x37, 0x12];
    const VIRTIO: [u8; 2] = [0x42, 0x10];
    let mut n = 0u32;
    let mut i = 0usize;
    while i + 5 <= buf.len() {
        let modrm = buf[i + 2];
        if buf[i] == 0x66
            && buf[i + 1] == 0x81
            && (modrm & 0xF8) == 0xF8
            && buf[i + 3] == I440FX[0]
            && buf[i + 4] == I440FX[1]
        {
            buf[i + 3] = VIRTIO[0];
            buf[i + 4] = VIRTIO[1];
            n = n.saturating_add(1);
            i += 5;
        } else {
            i += 1;
        }
    }
    n
}

/// Probe ESP `\\EFI\\RayNu\\OVMF.fd` before ExitBootServices.
///
/// Missing file is silent (iron Cruzer may not have one). Accepted
/// bytes print [`M7_E5_LIVE_BYTES_PRESENT_OK_MARKER`].
#[cfg(target_os = "uefi")]
pub fn probe_ovmf_esp() {
    use crate::boot::serial;
    use uefi::boot;
    use uefi::fs::FileSystem;
    use uefi::CString16;

    let image = boot::image_handle();
    let Ok(sfs) = boot::get_image_file_system(image) else {
        return;
    };
    let mut fs = FileSystem::new(sfs);
    let Ok(path) = CString16::try_from("\\EFI\\RayNu\\OVMF.fd") else {
        return;
    };
    let Ok(data) = fs.read(path.as_ref()) else {
        return;
    };
    match retain_ovmf_bytes(&data) {
        Ok(n) => {
            serial::write_line(M7_E5_LIVE_BYTES_PRESENT_OK_MARKER);
            serial::write_str("boot: ESP OVMF.fd retained bytes=");
            write_dec(n as u64);
            serial::write_byte(b'\n');
            let _ = E5_OVMF_RETAIN_RESIDUAL_NOTE;
        }
        Err(()) => {
            serial::write_line("boot: ESP OVMF.fd present but rejected (not a real EDK2 image)");
        }
    }
}

#[cfg(not(target_os = "uefi"))]
pub fn probe_ovmf_esp() {}

#[cfg(target_os = "uefi")]
fn write_dec(mut n: u64) {
    use crate::boot::serial;
    if n == 0 {
        serial::write_byte(b'0');
        return;
    }
    let mut buf = [0u8; 20];
    let mut i = 20;
    while n > 0 && i > 0 {
        i -= 1;
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
    }
    for &b in &buf[i..] {
        serial::write_byte(b);
    }
}

#[cfg(test)]
#[path = "ovmf_esp_test.rs"]
mod ovmf_esp_test;
