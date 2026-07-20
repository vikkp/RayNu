//! M6.4 host verification gate (REST auth).
//!
//! Pillar: [Z] [A]
//! Proven Core: outside (companion to `mgmt/api` — not a boot path).
//!
//! Checks that REST auth rejects missing/wrong tokens, allows the bring-up
//! mock token, audits deny/allow, embeds `RAYNU-V-M6-AUTH-OK`, and closes the
//! M5.1 auth stub GAP. Runtime verify is exercised by `tools/m6-auth-smoke.sh`.

use super::api::{
    prop_auth_deny_allow, AUTH_GAP_NOTE, AUTH_TOKEN_SOURCE_NOTE, BRINGUP_AUTH_TOKEN,
    M6_AUTH_OK_MARKER,
};

/// Host / CI marker when the M6.4 REST auth gate passes.
pub const M6_AUTH_GATE_MARKER: &str = M6_AUTH_OK_MARKER;

/// True when REST auth surface is present (real verify; closed GAP; marker).
pub fn mgmt_auth_surface_present() -> bool {
    let s = include_str!("api.rs");
    s.contains("fn auth_allows(")
        && s.contains("BRINGUP_AUTH_TOKEN")
        && s.contains("prop_auth_deny_allow")
        && s.contains("AuthDenied")
        && s.contains("AuthAllowed")
        && s.contains(M6_AUTH_OK_MARKER)
        && s.contains(AUTH_GAP_NOTE)
        && AUTH_GAP_NOTE.contains("CLOSED M6.4")
        && !s.contains("Auth stub: always allows")
}

/// True when audit AuthAllowed / AuthDenied variants exist.
pub fn audit_auth_events_present() -> bool {
    let s = include_str!("../audit/integrity.rs");
    s.contains("AuthAllowed")
        && s.contains("AuthDenied")
        && s.contains("method_tag")
}

/// True when the M6.4 smoke script is present.
pub fn auth_scripts_present() -> bool {
    let smoke = include_str!("../tools/m6-auth-smoke.sh");
    smoke.contains(M6_AUTH_OK_MARKER)
        && smoke.contains("m6_4_auth_gate_passes")
        && smoke.contains("prop_auth_deny_allow")
        && smoke.contains("BRINGUP_AUTH_TOKEN")
}

/// Full M6.4 artifact + deny/allow gate.
pub fn run_m6_auth_gate() -> bool {
    mgmt_auth_surface_present()
        && audit_auth_events_present()
        && auth_scripts_present()
        && prop_auth_deny_allow()
        && AUTH_TOKEN_SOURCE_NOTE.contains("BRINGUP_AUTH_TOKEN")
        && BRINGUP_AUTH_TOKEN == "raynu-v-bringup"
}

#[cfg(test)]
#[path = "m6_auth_gate_test.rs"]
mod m6_auth_gate_test;
