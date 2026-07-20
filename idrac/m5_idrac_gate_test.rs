use super::*;

#[test]
fn m5_6_idrac_gate_passes() {
    assert!(
        idrac_surface_present(),
        "idrac must expose Tier-1 health + topology + marker"
    );
    assert!(
        idrac_assets_present(),
        "mock Redfish or topology assets incomplete"
    );
    assert!(
        idrac_scripts_present(),
        "m5-idrac-smoke.sh incomplete"
    );
    assert!(
        prop_tier1_health_and_topology(),
        "Tier-1 health + topology prop failed"
    );
    assert!(run_m5_idrac_gate(), "M5.6 idrac gate failed");
    assert_eq!(M5_IDRAC_OK_MARKER, "RAYNU-V-M5-IDRAC-OK");
    println!("{M5_IDRAC_OK_MARKER}");
}
