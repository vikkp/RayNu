//! M7.1 host verification gate (network HTTP mgmt plane).
//!
//! Pillar: [Z] [A]
//! Proven Core: outside (companion to `mgmt/http` — ADR-009).
//!
//! Checks HTTP codec, Bearer auth wire, SPA serve, listen stub honesty,
//! host TCP proof, runbook, and smoke/CI wiring.

use super::http::{prop_http_mgmt_package, HTTP_GAP_NOTE, HTTP_LAB_NOTE, M7_HTTP_OK_MARKER};
use super::http_listen::prop_listen_surface;

/// Host / CI marker when the M7.1 HTTP gate passes.
pub const M7_HTTP_GATE_MARKER: &str = M7_HTTP_OK_MARKER;

/// True when http module exposes codec props, closed GAP, lab note, marker.
pub fn http_surface_present() -> bool {
    let s = include_str!("http.rs");
    s.contains("fn parse_http_request(")
        && s.contains("fn handle_http_request(")
        && s.contains("fn extract_bearer_token(")
        && s.contains("fn prop_http_mgmt_package(")
        && s.contains("Authorization")
        && s.contains(M7_HTTP_OK_MARKER)
        && s.contains(HTTP_GAP_NOTE)
        && s.contains(HTTP_LAB_NOTE)
        && HTTP_GAP_NOTE.contains("CLOSED M7.1")
}

/// True when listen stub + host TcpListener proof exist.
pub fn http_listen_present() -> bool {
    let s = include_str!("http_listen.rs");
    s.contains("fn listen_mgmt_http_uefi(")
        && s.contains("UnsupportedOnFirmware")
        && s.contains("serve_one_connection_host")
        && s.contains("TcpListener")
        && prop_listen_surface()
}

/// True when runbook + smoke script exist with required phrases.
pub fn http_scripts_present() -> bool {
    let smoke = include_str!("../tools/m7-http-smoke.sh");
    let runbook = include_str!("../docs/runbooks/mgmt_http.md");
    smoke.contains(M7_HTTP_OK_MARKER)
        && smoke.contains("m7_1_http_gate_passes")
        && smoke.contains("prop_http_mgmt_package")
        && smoke.contains("host_tcp_serves_spa")
        && runbook.contains("RAYNU-V-M7-HTTP-OK")
        && runbook.contains("plaintext HTTP")
        && runbook.contains("hostfwd")
        && runbook.contains("Authorization: Bearer")
}

/// Full M7.1 artifact + package gate.
pub fn run_m7_http_gate() -> bool {
    http_surface_present()
        && http_listen_present()
        && http_scripts_present()
        && prop_http_mgmt_package()
        && HTTP_LAB_NOTE.contains("plaintext HTTP")
}

#[cfg(test)]
#[path = "m7_http_gate_test.rs"]
mod m7_http_gate_test;
