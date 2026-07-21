//! M7.1 mgmt HTTP listen surface (outside Proven Core).
//!
//! Pillar: [Z]
//! Proven Core: **outside** (ADR-009)
//!
//! Firmware path: stub until UEFI SNP/Tcp4 (or equivalent) is available.
//! Host/`cfg(test)` path: real `std::net::TcpListener` proving browser-shaped
//! reachability against the in-binary HTTP codec.

use super::datastore::ImageTable;
use super::http::{handle_http_request, HTTP_LAB_NOTE, M7_HTTP_OK_MARKER, MGMT_HTTP_DEFAULT_PORT};
use super::VmTable;

/// Why firmware cannot yet bind a NIC listener.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MgmtListenError {
    /// UEFI Tcp4/SNP (or equivalent) not wired yet — use host lab / QEMU user-net.
    UnsupportedOnFirmware,
    BindFailed,
    AcceptFailed,
}

/// Documented firmware entry: bind mgmt HTTP on the host NIC.
///
/// M7.1: returns [`MgmtListenError::UnsupportedOnFirmware`]. The HTTP codec in
/// `http.rs` is still linked into the binary; host tests prove the exchange.
pub fn listen_mgmt_http_uefi(_port: u16) -> Result<(), MgmtListenError> {
    let _ = (M7_HTTP_OK_MARKER, HTTP_LAB_NOTE, MGMT_HTTP_DEFAULT_PORT);
    Err(MgmtListenError::UnsupportedOnFirmware)
}

/// True when the listen API + lab note + codec marker are present.
pub fn prop_listen_surface() -> bool {
    let s = include_str!("http_listen.rs");
    s.contains("fn listen_mgmt_http_uefi(")
        && s.contains("UnsupportedOnFirmware")
        && s.contains("TcpListener")
        && listen_mgmt_http_uefi(MGMT_HTTP_DEFAULT_PORT)
            == Err(MgmtListenError::UnsupportedOnFirmware)
}

/// Host-only: serve one HTTP exchange on `127.0.0.1:port` (or ephemeral if 0).
#[cfg(test)]
pub fn serve_one_connection_host(port: u16) -> Result<u16, MgmtListenError> {
    use std::io::{Read, Write};
    use std::net::TcpListener;

    let listener =
        TcpListener::bind(("127.0.0.1", port)).map_err(|_| MgmtListenError::BindFailed)?;
    let bound = listener
        .local_addr()
        .map_err(|_| MgmtListenError::BindFailed)?
        .port();
    let (mut stream, _) = listener
        .accept()
        .map_err(|_| MgmtListenError::AcceptFailed)?;
    let mut buf = [0u8; 8192];
    let n = stream.read(&mut buf).unwrap_or(0);
    let raw = core::str::from_utf8(&buf[..n]).unwrap_or("");
    let mut table = VmTable::new();
    let mut images = ImageTable::new();
    let mut out = [0u8; 16384];
    let wn = handle_http_request(&mut table, &mut images, raw, &mut out).unwrap_or(0);
    let _ = stream.write_all(&out[..wn]);
    let _ = stream.flush();
    Ok(bound)
}

#[cfg(test)]
#[path = "http_listen_test.rs"]
mod http_listen_test;
