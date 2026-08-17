//! Post-EBS SNP HTTP scaffold / package gate (outside Proven Core).
//!
//! Pillar: [Z] [D]
//! Proven Core: **outside** (ADR-012 residual)
//!
//! Host/CI proves wiring. Does **not** print
//! `RAYNU-V-M7-POST-EBS-HTTP-OK` (firmware runtime only).

use super::http_listen::{
    M7_POST_EBS_HTTP_OK_MARKER, M7_POST_EBS_HTTP_SCAFFOLD_MARKER, M7_UEFI_HTTP_OK_MARKER,
};

/// Host / CI marker when the post-EBS HTTP scaffold package passes.
pub const M7_POST_EBS_HTTP_GATE_MARKER: &str = M7_POST_EBS_HTTP_SCAFFOLD_MARKER;

/// True when post-EBS SNP park + probe + idle + PRE-EBS fallback exist.
pub fn post_ebs_http_surface_present() -> bool {
    let listen = include_str!("http_listen.rs");
    let main = include_str!("../src/main.rs");
    let snp = include_str!("snp_listen_uefi.rs");
    let launch = include_str!("../vmx/launch.rs");
    listen.contains("fn run_pre_ebs_mgmt_listen(")
        && listen.contains("fn run_post_ebs_mgmt_listen(")
        && listen.contains("fn run_post_ebs_http_idle(")
        && listen.contains(M7_POST_EBS_HTTP_OK_MARKER)
        && listen.contains(M7_POST_EBS_HTTP_SCAFFOLD_MARKER)
        && listen.contains(M7_UEFI_HTTP_OK_MARKER)
        && main.contains("run_pre_ebs_mgmt_listen")
        && main.contains("run_post_ebs_mgmt_listen")
        && main.contains("leave_firmware")
        && snp.contains("park_snp_http")
        && snp.contains("not polling SNP yet")
        && snp.contains("uefi_snp_post_ebs_idle")
        && snp.contains("POST-EBS SNP idle")
        && snp.contains("tsc_delay_ms")
        && snp.contains("firmware SNP dead after EBS")
        && snp.contains("PRE-EBS SNP window")
        && launch.contains("run_post_ebs_http_idle")
        && !snp.contains("Tcp4Protocol")
        && idle_skips_firmware_snp_poll(snp)
}

/// Idle after VMXOFF must print WARN and return — never `iface.poll` (iron hang + RSOD).
fn idle_skips_firmware_snp_poll(snp: &str) -> bool {
    let Some(start) = snp.find("pub fn uefi_snp_post_ebs_idle()") else {
        return false;
    };
    let rest = &snp[start..];
    let end = rest[1..].find("\nfn ").map(|i| i + 1).unwrap_or(rest.len());
    let idle = &rest[..end];
    idle.contains("POST-EBS SNP idle")
        && idle.contains("firmware SNP dead after EBS")
        && !idle.contains("iface.poll")
        && !idle.contains("CURL NOW (post-EBS)")
}

/// True when runbook + smoke name post-EBS markers and SNP residual.
pub fn post_ebs_http_scripts_present() -> bool {
    let smoke = include_str!("../tools/m7-post-ebs-http-smoke.sh");
    let runbook = include_str!("../docs/runbooks/mgmt_http.md");
    smoke.contains(M7_POST_EBS_HTTP_SCAFFOLD_MARKER)
        && smoke.contains("m7_post_ebs_http_scaffold_passes")
        && smoke.contains(M7_POST_EBS_HTTP_OK_MARKER)
        && smoke.contains("never print iron")
        && runbook.contains("POST-EBS")
        && runbook.contains("SNP")
        && runbook.contains(M7_POST_EBS_HTTP_OK_MARKER)
        && runbook.contains("do not chase")
        && runbook.contains("firmware SNP dead")
}

pub fn prop_post_ebs_http_scaffold_package() -> bool {
    post_ebs_http_surface_present() && post_ebs_http_scripts_present()
}

pub fn run_m7_post_ebs_http_scaffold_gate() -> bool {
    prop_post_ebs_http_scaffold_package()
}

#[cfg(test)]
#[path = "m7_post_ebs_http_gate_test.rs"]
mod m7_post_ebs_http_gate_test;
