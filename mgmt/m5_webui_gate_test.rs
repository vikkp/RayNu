use super::*;

#[test]
fn m5_2_webui_gate_passes() {
    assert!(
        webui_embed_present(),
        "mgmt/webui must PE-link SPA + lazy load + marker"
    );
    assert!(
        webui_asset_and_api_wired(),
        "assets/webui.html must wire list/start/stop to /vms"
    );
    assert!(
        webui_scripts_present(),
        "tools/m5-webui-smoke.sh missing or incomplete"
    );
    assert!(
        prop_webui_list_start_stop(),
        "Web UI list/start/stop round-trip failed"
    );
    assert!(run_m5_webui_gate(), "M5.2 Web UI gate failed");
    assert_eq!(M5_WEBUI_OK_MARKER, "RAYNU-V-M5-WEBUI-OK");
    println!("{M5_WEBUI_OK_MARKER}");
}
