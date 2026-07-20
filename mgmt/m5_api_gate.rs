//! M5.1 host verification gate (CLI + REST control plane).
//!
//! Pillar: [Z] [A]
//! Proven Core: outside (companion to `mgmt/` — not a boot path).
//!
//! Checks that CLI/REST dispatch exists over `VmTable`, auth GAP is documented,
//! and the CLI+REST round-trip property holds.

use super::api::{
    prop_cli_rest_roundtrip, prop_cli_verbs_parse, AUTH_GAP_NOTE, M5_API_OK_MARKER,
};

/// True when CLI + REST surfaces are present in `mgmt/api.rs`.
pub fn mgmt_api_surface_present() -> bool {
    let s = include_str!("api.rs");
    s.contains("fn parse_cli(")
        && s.contains("fn dispatch_cli(")
        && s.contains("fn dispatch_rest(")
        && s.contains("enum CliCommand")
        && s.contains("struct RestRequest")
        && s.contains("Create")
        && s.contains("Start")
        && s.contains("Stop")
        && s.contains("Destroy")
        && s.contains("List")
        && s.contains(M5_API_OK_MARKER)
        && s.contains(AUTH_GAP_NOTE)
}

/// True when `VmTable::list` exists for the list/GET verbs.
pub fn vmtable_list_present() -> bool {
    let s = include_str!("mod.rs");
    s.contains("fn list(") && s.contains("pub mod api")
}

/// True when the M5.1 smoke script is present.
pub fn api_scripts_present() -> bool {
    let smoke = include_str!("../tools/m5-api-smoke.sh");
    smoke.contains(M5_API_OK_MARKER)
        && smoke.contains("m5_1_api_gate_passes")
        && smoke.contains("cli_rest_roundtrip")
}

/// Full M5.1 artifact + round-trip gate.
pub fn run_m5_api_gate() -> bool {
    mgmt_api_surface_present()
        && vmtable_list_present()
        && api_scripts_present()
        && prop_cli_verbs_parse()
        && prop_cli_rest_roundtrip()
}

#[cfg(test)]
#[path = "m5_api_gate_test.rs"]
mod m5_api_gate_test;
