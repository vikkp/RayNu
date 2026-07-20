use super::*;

#[test]
fn m5_1_api_gate_passes() {
    assert!(
        mgmt_api_surface_present(),
        "mgmt/api must expose CLI+REST dispatch + marker + auth GAP"
    );
    assert!(
        vmtable_list_present(),
        "VmTable::list and mod api must be wired"
    );
    assert!(
        api_scripts_present(),
        "tools/m5-api-smoke.sh missing or incomplete"
    );
    assert!(
        prop_cli_verbs_parse(),
        "CLI verb parse failed"
    );
    assert!(
        prop_cli_rest_roundtrip(),
        "CLI+REST round-trip failed"
    );
    assert!(run_m5_api_gate(), "M5.1 API gate failed");
    assert_eq!(M5_API_OK_MARKER, "RAYNU-V-M5-API-OK");
    println!("{M5_API_OK_MARKER}");
}
