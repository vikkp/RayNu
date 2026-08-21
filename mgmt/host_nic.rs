//! Host-owned mgmt NIC markers + lab flag (ADR-013 Phase C / M7.8).
//!
//! Pillar: [Z]
//! Proven Core: **outside**
//!
//! Iron marker [`M7_HOST_NIC_HTTP_OK_MARKER`] is **Phase D only** — never print
//! it from host tests or QEMU. QEMU GET `/` after EBS prints
//! [`M7_HOST_NIC_QEMU_MARKER`].

use core::sync::atomic::{AtomicBool, Ordering};

/// Host/CI scaffold (wiring). Never claims iron or QEMU GET /.
pub const M7_HOST_NIC_SCAFFOLD_MARKER: &str = "RAYNU-V-M7-HOST-NIC-SCAFFOLD-OK";

/// Firmware/QEMU: native e1000 served ≥1 HTTP exchange **after** EBS.
pub const M7_HOST_NIC_QEMU_MARKER: &str = "RAYNU-V-M7-HOST-NIC-QEMU-OK";

/// Iron Phase D (R640 census NIC after BOOT-OK). **Do not print from Phase C.**
pub const M7_HOST_NIC_HTTP_OK_MARKER: &str = "RAYNU-V-M7-HOST-NIC-HTTP-OK";

/// QEMU user-net station address (first NIC).
pub const QEMU_USERNET_IPV4: [u8; 4] = [10, 0, 2, 15];
/// QEMU user-net default gateway / SLIRP.
pub const QEMU_USERNET_GW: [u8; 4] = [10, 0, 2, 2];
pub const QEMU_USERNET_PREFIX: u8 = 24;

/// Post-EBS listen window on the native NIC (ms). Then guest path continues.
pub const HOST_NIC_LISTEN_MS: u64 = 20_000;

/// Max HTTP exchanges in the Phase C window.
pub const HOST_NIC_MAX_EXCHANGES: u32 = 8;

/// Coexist has **one** TCP listen slot. Iron 2026-08-21: a SPA/browser
/// half-open ESTABLISHED (COM2 `TCP accept` with no `HTTP exchange ok`)
/// held the slot so `curl` SYN timed out. Abort and re-listen after this
/// many coexist-tick milliseconds without complete HTTP headers.
pub const HOST_NIC_HTTP_IDLE_MS: i64 = 3000;

/// Whether a single-socket HTTP accept should RST and re-listen.
///
/// INVARIANTS:
/// - `true` only when an accept was announced, headers are incomplete,
///   and `elapsed_since_accept_ms >= limit_ms` with `limit_ms > 0`
/// - Complete headers never abort (the exchange/close path owns that socket)
pub fn http_accept_should_idle_abort(
    announced: bool,
    headers_done: bool,
    elapsed_since_accept_ms: i64,
    limit_ms: i64,
) -> bool {
    announced && !headers_done && limit_ms > 0 && elapsed_since_accept_ms >= limit_ms
}

/// Host/CI: idle abort fires at the limit and never on complete headers.
pub fn prop_http_accept_idle_abort() -> bool {
    !http_accept_should_idle_abort(true, false, 0, HOST_NIC_HTTP_IDLE_MS)
        && !http_accept_should_idle_abort(true, false, HOST_NIC_HTTP_IDLE_MS - 1, HOST_NIC_HTTP_IDLE_MS)
        && http_accept_should_idle_abort(true, false, HOST_NIC_HTTP_IDLE_MS, HOST_NIC_HTTP_IDLE_MS)
        && !http_accept_should_idle_abort(true, true, HOST_NIC_HTTP_IDLE_MS, HOST_NIC_HTTP_IDLE_MS)
        && !http_accept_should_idle_abort(false, false, HOST_NIC_HTTP_IDLE_MS, HOST_NIC_HTTP_IDLE_MS)
        && !http_accept_should_idle_abort(true, false, 5000, 0)
}

/// JUSTIFICATION: lab ESP flag; BSP-only; set PRE-EBS, read after EBS.
static HOST_NIC_LAB: AtomicBool = AtomicBool::new(false);

pub fn host_nic_lab_armed() -> bool {
    HOST_NIC_LAB.load(Ordering::Acquire)
}

pub fn arm_host_nic_lab() {
    HOST_NIC_LAB.store(true, Ordering::Release);
}

pub const HOST_NIC_LAB_ARM_NOTE: &str = "boot: ADR-013 Phase C lab hostnic.txt armed (QEMU e1000)";

/// Probe ESP `EFI/RayNu/hostnic.txt` (must run before ExitBootServices).
#[cfg(target_os = "uefi")]
pub fn probe_host_nic_lab_flag() {
    use crate::boot::serial;
    use uefi::boot;
    use uefi::fs::FileSystem;

    let image = boot::image_handle();
    let Ok(sfs) = boot::get_image_file_system(image) else {
        return;
    };
    let mut fs = FileSystem::new(sfs);
    if flag_present(&mut fs, "\\EFI\\RayNu\\hostnic.txt")
        || flag_present(&mut fs, "\\hostnic.txt")
    {
        arm_host_nic_lab();
        serial::write_line(HOST_NIC_LAB_ARM_NOTE);
    }
}

#[cfg(not(target_os = "uefi"))]
pub fn probe_host_nic_lab_flag() {}

#[cfg(target_os = "uefi")]
fn flag_present(fs: &mut uefi::fs::FileSystem, path: &str) -> bool {
    use uefi::CString16;
    let Ok(p) = CString16::try_from(path) else {
        return false;
    };
    fs.read(p.as_ref()).is_ok()
}

/// Skip the long PRE-EBS SNP/Tcp4 window when QEMU e1000 is the post-EBS path.
pub fn should_skip_pre_ebs_firmware_listen() -> bool {
    crate::mgmt::e1000_mmio::qemu_e1000_present()
}

#[cfg(test)]
#[path = "host_nic_test.rs"]
mod host_nic_test;
