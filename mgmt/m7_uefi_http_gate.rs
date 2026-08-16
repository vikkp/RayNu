//! M7.6 UEFI HTTP listen scaffold / package gate (outside Proven Core).
//!
//! Pillar: [Z] [D]
//! Proven Core: **outside** (ADR-012)
//!
//! Host/CI proves wiring + GAP closed. Does **not** print
//! `RAYNU-V-M7-UEFI-HTTP-OK` (firmware runtime only).

use super::http_listen::{
    prop_listen_surface, M7_UEFI_HTTP_OK_MARKER, M7_UEFI_HTTP_SCAFFOLD_MARKER, UEFI_HTTP_GAP_NOTE,
};

/// Host / CI marker when the M7.6 scaffold package passes.
pub const M7_UEFI_HTTP_GATE_MARKER: &str = M7_UEFI_HTTP_SCAFFOLD_MARKER;

/// True when ADR-012 markers + PRE-EBS entry + Tcp4/SNP residual modules exist.
pub fn uefi_http_surface_present() -> bool {
    let listen = include_str!("http_listen.rs");
    let main = include_str!("../src/main.rs");
    let tcp4 = include_str!("tcp4_uefi.rs");
    let snp = include_str!("snp_listen_uefi.rs");
    let probe = include_str!("net_probe_uefi.rs");
    listen.contains("fn run_pre_ebs_mgmt_listen(")
        && listen.contains("fn listen_mgmt_http_uefi(")
        && listen.contains(M7_UEFI_HTTP_OK_MARKER)
        && listen.contains(M7_UEFI_HTTP_SCAFFOLD_MARKER)
        && listen.contains(UEFI_HTTP_GAP_NOTE)
        && UEFI_HTTP_GAP_NOTE.contains("CLOSED M7.6")
        && listen.contains("falling back to SNP residual")
        && main.contains("run_pre_ebs_mgmt_listen")
        && tcp4.contains("Tcp4Protocol")
        && tcp4.contains("create_tcp4_child")
        && tcp4.contains("65530bc7")
        && snp.contains("uefi_snp_listen")
        && snp.contains("PRE-EBS SNP window")
        && snp.contains("CURL NOW")
        && listen.contains("SNP_POST_BIND_LISTEN_MS")
        && probe.contains("connect_network_stack_bindings")
        && probe.contains("NII_GUID")
        && probe.contains("extra-after")
        && probe.contains("NetworkPkg DXEs not dispatched")
}

/// True when runbook + smoke name M7.6 markers and PRE-EBS constraint.
pub fn uefi_http_scripts_present() -> bool {
    let smoke = include_str!("../tools/m7-uefi-http-smoke.sh");
    let runbook = include_str!("../docs/runbooks/mgmt_http.md");
    smoke.contains(M7_UEFI_HTTP_SCAFFOLD_MARKER)
        && smoke.contains("m7_6_uefi_http_scaffold_passes")
        && smoke.contains(M7_UEFI_HTTP_OK_MARKER)
        && smoke.contains("never print iron")
        && runbook.contains("ADR-012")
        && runbook.contains("PRE-EBS")
        && runbook.contains(M7_UEFI_HTTP_OK_MARKER)
        && runbook.contains("hostfwd")
        && runbook.contains("R640 Tcp4 absent")
        && runbook.contains("2026-08-16-uefi-tcp4-absent-root-cause.md")
}

/// Full M7.6 scaffold package prop.
pub fn prop_uefi_http_scaffold_package() -> bool {
    prop_listen_surface() && uefi_http_surface_present() && uefi_http_scripts_present()
}

pub fn run_m7_uefi_http_scaffold_gate() -> bool {
    prop_uefi_http_scaffold_package()
}

#[cfg(test)]
#[path = "m7_uefi_http_gate_test.rs"]
mod m7_uefi_http_gate_test;
