//! M8.1 TLS on the mgmt listen (outside Proven Core).
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** (ADR-002 / ADR-018). Do not touch VMX/EPT.
//! VERIFICATION: L1 host tests (rustls TLS 1.2 client ↔ freestanding server).
//!
//! Firmware coexist `:8443` speaks **TLS 1.2** (`Tls12Listen`, ECDHE-RSA-AES128-GCM).
//! rustls/ring stay a **dev-dependency** (ADR-003) — they cannot join `uefi-bin`.
//! Plaintext remains a lab fallback. Iron close is still `curl --cacert` on the
//! R640 native HTTPS window **before RayNu-F** ([`M8_TLS_OK_MARKER`]). Host/CI
//! never print that marker.

use core::sync::atomic::{AtomicBool, Ordering};

/// Iron COM2 / operator LAN close: HTTPS on native BCM5720 before RayNu-F.
/// Host/CI/nested must **never** print this.
pub const M8_TLS_OK_MARKER: &str = "RAYNU-V-M8-TLS-OK";

/// Host/CI: rustls server served SPA or Bearer REST. Not iron. Not nested.
pub const M8_TLS_HOST_OK_MARKER: &str = "RAYNU-V-M8-TLS-HOST-OK";

/// Honesty: host TLS ≠ iron HTTPS. Iron COM2 closed 2026-09-19 (`928d6224`).
pub const TLS_HOST_RESIDUAL_NOTE: &str =
    "residual: rustls host handshake is not iron RAYNU-V-M8-TLS-OK; iron HTTPS closed on COM2 928d6224 (lab millicert TLS 1.2); plaintext HTTP remains a lab fallback; PRE-EBS SNP does not count; nested QEMU ≠ R640; rustls stays a dev-dependency (ADR-003); do not print iron TLS-OK from host/CI";

/// Firmware listen: TLS 1.2 in-tree. Iron HTTPS closed on COM2 (`928d6224`).
pub const TLS_FIRMWARE_PLAINTEXT_NOTE: &str =
    "firmware coexist :8443 is TLS 1.2 ECDHE-RSA-AES128-GCM (plaintext HTTP remains a lab fallback; iron HTTPS closed on COM2 928d6224)";

/// How the mgmt listen is encrypted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TlsMode {
    /// Lab fallback: HTTP/1.1 on smoltcp TCP (Everest close).
    PlaintextLab,
    /// Host/`cfg(test)` rustls around the same HTTP codec.
    HostReady,
    /// Firmware coexist: freestanding TLS 1.2 (not iron TLS-OK).
    FirmwareTls12,
}

/// Product firmware listen today. Host rustls tests use [`TlsMode::HostReady`].
pub const FIRMWARE_TLS_MODE: TlsMode = TlsMode::FirmwareTls12;

static TLS_OK_PRINTED: AtomicBool = AtomicBool::new(false);

/// True when firmware still serves identity HTTP (lab fallback).
pub fn firmware_listen_is_plaintext() -> bool {
    matches!(FIRMWARE_TLS_MODE, TlsMode::PlaintextLab)
}

/// True when the EFI binary wraps coexist in TLS 1.2.
pub fn firmware_listen_is_tls12() -> bool {
    matches!(FIRMWARE_TLS_MODE, TlsMode::FirmwareTls12)
        && TLS_FIRMWARE_PLAINTEXT_NOTE.contains("TLS 1.2")
        && TLS_FIRMWARE_PLAINTEXT_NOTE.contains("plaintext HTTP remains a lab fallback")
}

/// Host/CI must never print the iron TLS marker.
pub fn host_never_prints_iron_tls_ok() -> bool {
    M8_TLS_OK_MARKER == "RAYNU-V-M8-TLS-OK"
        && M8_TLS_HOST_OK_MARKER == "RAYNU-V-M8-TLS-HOST-OK"
        && M8_TLS_HOST_OK_MARKER != M8_TLS_OK_MARKER
        && M8_TLS_OK_MARKER != "RAYNU-V-M7-HOST-NIC-HTTP-OK"
        && M8_TLS_OK_MARKER != crate::mgmt::disk_persist::M8_DISK_PERSIST_OK_MARKER
}

/// Iron COM2: print [`M8_TLS_OK_MARKER`] once after a TLS HTTP exchange on
/// BCM5720 coexist. Host/CI never `println!` the iron string.
pub fn maybe_print_iron_tls_ok(https_ok: bool, bcm5720: bool) -> bool {
    if !(https_ok && bcm5720) {
        return false;
    }
    if TLS_OK_PRINTED.swap(true, Ordering::AcqRel) {
        return false;
    }
    #[cfg(feature = "uefi-bin")]
    {
        crate::boot::serial::write_line(M8_TLS_OK_MARKER);
    }
    true
}

/// Host tests / handoff reset of the one-shot iron print latch.
pub fn tls_ok_clear_printed() {
    TLS_OK_PRINTED.store(false, Ordering::Release);
}

/// Package surface: firmware TLS 1.2, rustls not in uefi-bin, iron unprinted.
pub fn prop_tls_host_package() -> bool {
    let cargo = include_str!("../Cargo.toml");
    let plan = include_str!("../docs/m8_plan.md");
    let uefi_feat = cargo
        .lines()
        .find(|l| l.contains("uefi-bin = ["))
        .unwrap_or("");
    firmware_listen_is_tls12()
        && !firmware_listen_is_plaintext()
        && host_never_prints_iron_tls_ok()
        && TLS_HOST_RESIDUAL_NOTE.contains("plaintext HTTP remains a lab fallback")
        && cargo.contains("[dev-dependencies]")
        && cargo.contains("rustls")
        && cargo.contains("rcgen")
        && cargo.contains("aes-gcm")
        && cargo.contains("p256")
        && !uefi_feat.contains("rustls")
        && plan.contains("M8.1")
        && plan.contains("TLS")
        && plan.contains("Plaintext remains a lab fallback")
        && crate::mgmt::http::HTTP_LAB_NOTE.contains("plaintext HTTP remains a lab fallback")
        && crate::mgmt::http::HTTP_LAB_NOTE.contains("M8.1")
}

#[cfg(test)]
#[path = "tls_test.rs"]
mod tls_test;
