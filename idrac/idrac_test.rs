use super::*;

#[test]
fn default_is_tier1() {
    assert_eq!(default_tier(), IdracTier::Tier1);
    assert!(thermal_ok_stub());
}

#[test]
fn tier1_health_and_topology() {
    assert!(prop_tier1_health_and_topology());
    let h = read_tier1_health(MOCK_REDFISH).expect("mock redfish");
    assert!(h.all_ok());
    assert!(h.psu_count >= 2);
    let t = read_topology_mock().expect("mock topology");
    assert!(t.socket_count() >= 2);
    assert!(t.slit_count >= 2);
}

#[test]
fn parse_health_ok() {
    assert_eq!(parse_health("\"Health\": \"OK\""), HealthState::Ok);
    assert_eq!(parse_health("Warning"), HealthState::Warning);
}
