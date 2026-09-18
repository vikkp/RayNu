//! M8.1 TLS host gate (outside Proven Core).
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** (ADR-018)
//!
//! Proves the host rustls package exists, firmware listen is still plaintext,
//! and the iron marker is minted but never printed from host/CI. Does **not**
//! print `RAYNU-V-M8-TLS-OK`. Nested QEMU is not this gate. Iron `curl --cacert`
//! after `BOOT-OK` is not this gate.

use crate::mgmt::tls::{
    firmware_listen_is_plaintext, host_never_prints_iron_tls_ok, prop_tls_host_package,
    M8_TLS_HOST_OK_MARKER, M8_TLS_OK_MARKER, TLS_FIRMWARE_PLAINTEXT_NOTE, TLS_HOST_RESIDUAL_NOTE,
};

/// Host / CI marker when the M8.1 TLS package passes.
pub const M8_TLS_GATE_MARKER: &str = M8_TLS_HOST_OK_MARKER;

/// True when plan, markers, plaintext firmware, and rustls-as-dev-dep hold.
pub fn tls_surface_present() -> bool {
    let tls = include_str!("tls.rs");
    let plan = include_str!("../docs/m8_plan.md");
    let cargo = include_str!("../Cargo.toml");
    let smoke = include_str!("../tools/m8-tls-smoke.sh");
    let runbook = include_str!("../docs/runbooks/mgmt_http.md");
    let html = include_str!("../assets/webui.html");
    let forbidden = concat!("println!(", "\"RAYNU-V-M8-TLS-OK\")");
    tls.contains("enum TlsMode")
        && tls.contains("PlaintextLab")
        && tls.contains("HostReady")
        && tls.contains("fn firmware_listen_is_plaintext(")
        && tls.contains("fn host_never_prints_iron_tls_ok(")
        && tls.contains("fn prop_tls_host_package(")
        && tls.contains(M8_TLS_OK_MARKER)
        && tls.contains(M8_TLS_HOST_OK_MARKER)
        && !tls.contains(forbidden)
        && !include_str!("http.rs").contains(forbidden)
        && !include_str!("http_listen.rs").contains(forbidden)
        && TLS_HOST_RESIDUAL_NOTE.contains("not iron")
        && TLS_FIRMWARE_PLAINTEXT_NOTE.contains("plaintext HTTP")
        && cargo.contains("rustls")
        && cargo.contains("[dev-dependencies]")
        && !cargo.contains("dep:rustls")
        && plan.contains("M8.1")
        && plan.contains("Plaintext remains a lab fallback")
        && smoke.contains(M8_TLS_HOST_OK_MARKER)
        && smoke.contains("m8_tls_host_gate_passes")
        && runbook.contains("M8.1")
        && runbook.contains("plaintext HTTP")
        && html.contains("data-go=\"overview\"")
        && html.contains("d-host")
        && html.contains("data-raynu-phase-b")
        && crate::mgmt::webui::spa_operator_surface_present()
}

/// Full M8.1 host artifact gate (not iron HTTPS).
pub fn run_m8_tls_host_gate() -> bool {
    tls_surface_present()
        && prop_tls_host_package()
        && firmware_listen_is_plaintext()
        && host_never_prints_iron_tls_ok()
        && M8_TLS_OK_MARKER == "RAYNU-V-M8-TLS-OK"
        && M8_TLS_GATE_MARKER == "RAYNU-V-M8-TLS-HOST-OK"
}

#[cfg(test)]
#[path = "m8_tls_gate_test.rs"]
mod m8_tls_gate_test;
