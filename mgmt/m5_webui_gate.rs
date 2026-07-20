//! M5.2 host verification gate (embedded Web UI).
//!
//! Pillar: [Z] [A]
//! Proven Core: outside (companion to `mgmt/webui` — not a boot path).
//!
//! Checks PE `.aswebui` embed, lazy load + SPA wires M5.1 list/start/stop,
//! and the host smoke script is present.

use super::webui::{
    prop_webui_list_start_stop, webui_html_wires_api, webui_present, M5_WEBUI_OK_MARKER,
    SECTION_WEBUI, WEBUI_ZSTD_GAP_NOTE,
};

/// True when webui module embeds SPA in `.aswebui` with lazy load.
pub fn webui_embed_present() -> bool {
    let s = include_str!("webui.rs");
    s.contains("link_section = \".aswebui\"")
        && s.contains("include_bytes!(\"../assets/webui.html\")")
        && s.contains("fn load_webui(")
        && s.contains("fn dispatch_webui_action(")
        && s.contains(M5_WEBUI_OK_MARKER)
        && s.contains(WEBUI_ZSTD_GAP_NOTE)
        && s.contains(SECTION_WEBUI)
}

/// True when SPA HTML is present and wired to M5.1 routes.
pub fn webui_asset_and_api_wired() -> bool {
    webui_present() && webui_html_wires_api()
}

/// True when the M5.2 smoke script is present.
pub fn webui_scripts_present() -> bool {
    let smoke = include_str!("../tools/m5-webui-smoke.sh");
    smoke.contains(M5_WEBUI_OK_MARKER)
        && smoke.contains("m5_2_webui_gate_passes")
        && smoke.contains("webui_list_start_stop")
}

/// Full M5.2 artifact + list/start/stop gate.
pub fn run_m5_webui_gate() -> bool {
    webui_embed_present()
        && webui_asset_and_api_wired()
        && webui_scripts_present()
        && prop_webui_list_start_stop()
}

#[cfg(test)]
#[path = "m5_webui_gate_test.rs"]
mod m5_webui_gate_test;
