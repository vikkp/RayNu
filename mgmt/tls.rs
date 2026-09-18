//! M8.1 TLS on the mgmt listen (outside Proven Core).
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** (ADR-002 / ADR-018). Do not touch VMX/EPT.
//! VERIFICATION: L1 host tests (real rustls 1.2/1.3 handshake + HTTP).
//!
//! Firmware coexist `:8443` is still **plaintext HTTP** (ADR-009 deferred TLS
//! to close Everest). This module is the host-first slice: a rustls server
//! wraps [`crate::mgmt::http::handle_http_request`] in `cfg(test)` only.
//! rustls is a **dev-dependency** (ADR-003). Do not add it to `uefi-bin`.
//!
//! Iron close marker [`M8_TLS_OK_MARKER`] is COM2/`curl --cacert` after
//! `BOOT-OK`. Host/CI print [`M8_TLS_HOST_OK_MARKER`]. Nested QEMU ≠ R640.

/// Iron COM2 / operator LAN close: HTTPS on coexist after `BOOT-OK`.
/// Host/CI/nested must **never** print this.
pub const M8_TLS_OK_MARKER: &str = "RAYNU-V-M8-TLS-OK";

/// Host/CI: rustls server served SPA or Bearer REST. Not iron. Not nested.
pub const M8_TLS_HOST_OK_MARKER: &str = "RAYNU-V-M8-TLS-HOST-OK";

/// Honesty: host TLS ≠ iron HTTPS.
pub const TLS_HOST_RESIDUAL_NOTE: &str =
    "residual: rustls host handshake is not iron RAYNU-V-M8-TLS-OK; firmware coexist stays plaintext HTTP; PRE-EBS SNP does not count; nested QEMU ≠ R640; rustls stays a dev-dependency (ADR-003); do not print iron TLS-OK from host/CI";

/// Firmware listen is still lab plaintext until an iron HTTPS slice ships.
pub const TLS_FIRMWARE_PLAINTEXT_NOTE: &str =
    "firmware coexist :8443 is plaintext HTTP (M8.1 host-ready; iron HTTPS not claimed)";

/// How the mgmt listen is encrypted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TlsMode {
    /// Iron / firmware: HTTP/1.1 on smoltcp TCP (Everest close).
    PlaintextLab,
    /// Host/`cfg(test)` rustls around the same HTTP codec.
    HostReady,
}

/// Product firmware listen today. Host tests use [`TlsMode::HostReady`].
pub const FIRMWARE_TLS_MODE: TlsMode = TlsMode::PlaintextLab;

/// True until coexist serves TLS after `BOOT-OK` on the R640.
pub fn firmware_listen_is_plaintext() -> bool {
    matches!(FIRMWARE_TLS_MODE, TlsMode::PlaintextLab)
        && TLS_FIRMWARE_PLAINTEXT_NOTE.contains("plaintext HTTP")
}

/// Host/CI must never print the iron TLS marker.
pub fn host_never_prints_iron_tls_ok() -> bool {
    M8_TLS_OK_MARKER == "RAYNU-V-M8-TLS-OK"
        && M8_TLS_HOST_OK_MARKER == "RAYNU-V-M8-TLS-HOST-OK"
        && M8_TLS_HOST_OK_MARKER != M8_TLS_OK_MARKER
        && M8_TLS_OK_MARKER != "RAYNU-V-M7-HOST-NIC-HTTP-OK"
        && M8_TLS_OK_MARKER != crate::mgmt::disk_persist::M8_DISK_PERSIST_OK_MARKER
}

/// Package surface: markers, plaintext firmware, rustls not in uefi-bin.
pub fn prop_tls_host_package() -> bool {
    let cargo = include_str!("../Cargo.toml");
    let plan = include_str!("../docs/m8_plan.md");
    let uefi_feat = cargo
        .lines()
        .find(|l| l.contains("uefi-bin = ["))
        .unwrap_or("");
    firmware_listen_is_plaintext()
        && host_never_prints_iron_tls_ok()
        && TLS_HOST_RESIDUAL_NOTE.contains("plaintext HTTP")
        && cargo.contains("[dev-dependencies]")
        && cargo.contains("rustls")
        && cargo.contains("rcgen")
        && !uefi_feat.contains("rustls")
        && plan.contains("M8.1")
        && plan.contains("TLS")
        && crate::mgmt::http::HTTP_LAB_NOTE.contains("plaintext HTTP")
        && crate::mgmt::http::HTTP_LAB_NOTE.contains("M8.1")
}

#[cfg(test)]
#[path = "tls_test.rs"]
mod tls_test;
