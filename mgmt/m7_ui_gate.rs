//! M7.4 Ops Web UI MVP (outside Proven Core).
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** (ADR-009)
//!
//! Create-VM fields (CPU/RAM/disk/ISO) + media list/deploy wiring in the
//! embedded SPA. Console/TLS/firmware NIC remain residual.

use super::api::{dispatch_rest, prop_cli_rest_roundtrip, RestMethod, RestRequest, BRINGUP_AUTH_TOKEN};
use super::webui::{prop_webui_list_start_stop, webui_html_wires_api, webui_present};
use super::{VmSpec, VmTable};

/// Host / CI marker when the M7.4 Ops UI gate passes.
pub const M7_UI_OK_MARKER: &str = "RAYNU-V-M7-UI-OK";

/// Network create-VM + ISO UI GAP closed in M7.4.
pub const UI_GAP_NOTE: &str = "GAP(CLOSED M7.4): Network create-VM + ISO UI";

/// Honest residual note (console / TLS / firmware NIC).
pub const UI_RESIDUAL_NOTE: &str =
    "residual: console/serial UI, TLS, firmware NIC listen (lab HTTP + host smoke)";

/// True when create-with-spec REST path works.
pub fn prop_create_vm_spec() -> bool {
    let mut t = VmTable::new();
    let tok = Some(BRINGUP_AUTH_TOKEN);
    let created = dispatch_rest(
        &mut t,
        RestRequest {
            method: RestMethod::Post,
            path: "/vms/4/spec/2/2048/10240/1",
            auth_token: tok,
        },
    );
    if created.status != 201 {
        return false;
    }
    match t.get(4) {
        Some(r) => {
            r.cpu == 2 && r.ram_mib == 2048 && r.disk_mib == 10240 && r.iso_id == 1
        }
        None => false,
    }
}

/// True when SPA + create-spec + media phrases are present.
pub fn ui_surface_present() -> bool {
    let s = include_str!("../assets/webui.html");
    s.contains("data-raynu-m7-ui")
        && s.contains("/spec/")
        && s.contains("/images")
        && s.contains("/iso/")
        && s.contains("createVm")
        && s.contains("listImages")
        && s.contains("Console / serial log UI deferred")
        && webui_present()
        && webui_html_wires_api()
}

/// True when runbook + smoke exist.
pub fn ui_scripts_present() -> bool {
    let smoke = include_str!("../tools/m7-ui-smoke.sh");
    let runbook = include_str!("../docs/runbooks/ops_ui.md");
    smoke.contains(M7_UI_OK_MARKER)
        && smoke.contains("m7_4_ui_gate_passes")
        && smoke.contains("prop_create_vm_spec")
        && runbook.contains("RAYNU-V-M7-UI-OK")
        && runbook.contains("/spec/")
        && runbook.contains("Console")
        && runbook.contains("TLS")
}

/// Full M7.4 package prop.
pub fn prop_ops_ui_package() -> bool {
    let _ = (UI_GAP_NOTE, UI_RESIDUAL_NOTE, M7_UI_OK_MARKER);
    ui_surface_present()
        && prop_create_vm_spec()
        && prop_webui_list_start_stop()
        && prop_cli_rest_roundtrip()
        && UI_GAP_NOTE.contains("CLOSED M7.4")
        && M7_UI_OK_MARKER == "RAYNU-V-M7-UI-OK"
        && UI_RESIDUAL_NOTE.contains("console")
}

/// Full M7.4 artifact + package gate.
pub fn run_m7_ui_gate() -> bool {
    ui_surface_present() && ui_scripts_present() && prop_ops_ui_package()
}

#[cfg(test)]
#[path = "m7_ui_gate_test.rs"]
mod m7_ui_gate_test;
