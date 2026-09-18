//! Coexist TCP → mgmt HTTP session (outside Proven Core).
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** (ADR-002 / ADR-018). Do not touch VMX/EPT.
//! VERIFICATION: L1 host tests (plaintext session + rustls session, same API).
//!
//! Firmware coexist used to treat TCP bytes as HTTP. This module is the wrap
//! point: [`PlaintextListen`] accumulates TCP and yields a complete HTTP
//! request. Host tests implement the same feed/take/wrap flow with rustls.
//!
//! rustls/ring **cannot** join `uefi-bin` today: ring's C build needs
//! `<assert.h>` on `x86_64-unknown-uefi`. Firmware coexist uses the
//! freestanding [`crate::mgmt::tls12::Tls12Listen`] (TLS 1.2). This module
//! keeps the plaintext session API as a lab fallback. Iron
//! `RAYNU-V-M8-TLS-OK` is not this module.

/// Coexist RX accumulator size (must match `host_nic_listen` scratch).
pub const COEXIST_RX_ACC_N: usize = 8192;
/// HTTP response / TLS ciphertext drain size (must match coexist HTTP out).
pub const COEXIST_HTTP_OUT_N: usize = 16384;

/// Host/CI: rustls session used the coexist feed/take/wrap API. Not iron.
pub const M8_TLS_FW_HOST_OK_MARKER: &str = "RAYNU-V-M8-TLS-FW-HOST-OK";

/// Honesty: wrap is wired; firmware session is still plaintext.
pub const TLS_FW_WRAP_NOTE: &str =
    "firmware coexist TCP is wrapped by Tls12Listen; rustls/ring cannot join uefi-bin (ring C needs assert.h on x86_64-unknown-uefi); PlaintextListen remains a lab fallback; host rustls proves the same feed/take/wrap API; CURL NOW is https:// on the TLS EFI; nested QEMU ≠ R640; do not print RAYNU-V-M8-TLS-OK from host/CI";

/// COM2 / operator: still curl HTTP, not HTTPS.
pub const TLS_FW_CURL_NOTE: &str =
    "CURL NOW is https:// on the TLS 1.2 EFI; plaintext remains a lab fallback until a freestanding backend was missing";

/// True when buf holds a complete HTTP/1.1 header block.
pub fn headers_complete(buf: &[u8]) -> bool {
    buf.len() >= 4 && buf.windows(4).any(|w| w == b"\r\n\r\n")
}

/// Identity wrap: HTTP bytes are the TCP payload (lab plaintext).
pub fn wrap_plaintext_http(http: &[u8], tcp_out: &mut [u8]) -> usize {
    let n = http.len().min(tcp_out.len());
    if n == 0 {
        return 0;
    }
    tcp_out[..n].copy_from_slice(&http[..n]);
    n
}

/// Firmware coexist TCP session. Plaintext until a UEFI TLS backend exists.
pub struct PlaintextListen {
    rx: [u8; COEXIST_RX_ACC_N],
    rx_len: usize,
}

impl PlaintextListen {
    pub const fn new() -> Self {
        Self {
            rx: [0; COEXIST_RX_ACC_N],
            rx_len: 0,
        }
    }

    pub fn reset(&mut self) {
        self.rx_len = 0;
    }

    /// Append one TCP chunk. Returns bytes stored.
    pub fn feed_tcp(&mut self, chunk: &[u8]) -> usize {
        if chunk.is_empty() {
            return 0;
        }
        let copy = chunk.len().min(self.rx.len().saturating_sub(self.rx_len));
        self.rx[self.rx_len..self.rx_len + copy].copy_from_slice(&chunk[..copy]);
        self.rx_len += copy;
        copy
    }

    /// Complete HTTP request bytes, if headers have ended.
    pub fn take_http(&self) -> Option<&[u8]> {
        if headers_complete(&self.rx[..self.rx_len]) {
            Some(&self.rx[..self.rx_len])
        } else {
            None
        }
    }

    pub fn rx_len(&self) -> usize {
        self.rx_len
    }
}

/// Package: wrap point, plaintext firmware, rustls still not in uefi-bin.
pub fn prop_tls_fw_wrap_package() -> bool {
    let cargo = include_str!("../Cargo.toml");
    let listen = include_str!("host_nic_listen.rs");
    let uefi_feat = cargo
        .lines()
        .find(|l| l.contains("uefi-bin = ["))
        .unwrap_or("");
    TLS_FW_WRAP_NOTE.contains("Tls12Listen")
        && TLS_FW_WRAP_NOTE.contains("assert.h")
        && TLS_FW_CURL_NOTE.contains("https://")
        && headers_complete(b"GET / HTTP/1.1\r\n\r\n")
        && !headers_complete(b"GET / HTTP/1.1\r\n")
        && listen.contains("Tls12Listen")
        && listen.contains("feed_tcp")
        && listen.contains("take_http")
        && listen.contains("drain_tcp")
        && listen.contains("wrap_http")
        && listen.contains("https://")
        && !uefi_feat.contains("rustls")
        && cargo.contains("[dev-dependencies]")
        && cargo.contains("rustls")
        && crate::mgmt::tls::firmware_listen_is_tls12()
}

#[cfg(test)]
#[path = "tls_coexist_test.rs"]
mod tls_coexist_test;
