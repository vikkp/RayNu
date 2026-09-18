//! M8.2 host auth gate (outside Proven Core).
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** (ADR-018)
//!
//! Proves HostReady rejects `raynu-v-bringup`, firmware still uses the lab
//! latch when no ESP token is armed, and the iron marker is never printed
//! from host/CI. Nested QEMU is not this gate. Iron ESP-required default
//! after `BOOT-OK` is not this gate.

use crate::mgmt::auth::{
    firmware_auth_is_lab_bringup, host_never_prints_iron_auth_ok, prop_auth_host_package,
    AUTH_FIRMWARE_LAB_NOTE, AUTH_HOST_RESIDUAL_NOTE, M8_AUTH_HOST_OK_MARKER, M8_AUTH_OK_MARKER,
};

/// Host / CI marker when the M8.2 auth package passes.
pub const M8_AUTH_GATE_MARKER: &str = M8_AUTH_HOST_OK_MARKER;

/// True when plan, markers, lab firmware, and HostReady policy hold.
pub fn auth_surface_present() -> bool {
    let auth = include_str!("auth.rs");
    let plan = include_str!("../docs/m8_plan.md");
    let smoke = include_str!("../tools/m8-auth-smoke.sh");
    let runbook = include_str!("../docs/runbooks/mgmt_http.md");
    let forbidden = concat!("println!(", "\"RAYNU-V-M8-AUTH-OK\")");
    auth.contains("enum AuthMode")
        && auth.contains("LabBringUp")
        && auth.contains("HostReady")
        && auth.contains("fn firmware_auth_is_lab_bringup(")
        && auth.contains("fn host_never_prints_iron_auth_ok(")
        && auth.contains("fn prop_auth_host_package(")
        && auth.contains("fn auth_allows_for(")
        && auth.contains(M8_AUTH_OK_MARKER)
        && auth.contains(M8_AUTH_HOST_OK_MARKER)
        && !auth.contains(forbidden)
        && !include_str!("http.rs").contains(forbidden)
        && !include_str!("api.rs").contains(forbidden)
        && include_str!("http.rs").contains("if !auth_allows(parsed.auth_token)")
        && AUTH_HOST_RESIDUAL_NOTE.contains("not iron")
        && AUTH_FIRMWARE_LAB_NOTE.contains("BRINGUP_AUTH_TOKEN")
        && plan.contains("M8.2")
        && plan.contains("Product default is not")
        && smoke.contains(M8_AUTH_HOST_OK_MARKER)
        && smoke.contains("m8_auth_host_gate_passes")
        && runbook.contains("M8.2")
        && runbook.contains("not product default")
}

/// Full M8.2 host artifact gate (not iron operator default).
pub fn run_m8_auth_host_gate() -> bool {
    auth_surface_present()
        && prop_auth_host_package()
        && firmware_auth_is_lab_bringup()
        && host_never_prints_iron_auth_ok()
        && M8_AUTH_OK_MARKER == "RAYNU-V-M8-AUTH-OK"
        && M8_AUTH_GATE_MARKER == "RAYNU-V-M8-AUTH-HOST-OK"
}

#[cfg(test)]
#[path = "m8_auth_gate_test.rs"]
mod m8_auth_gate_test;
