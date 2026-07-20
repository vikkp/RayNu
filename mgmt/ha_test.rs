use super::*;

#[test]
fn ha_failover_restart() {
    assert!(prop_ha_failover_restart());
}

#[test]
fn security_harden_checklist() {
    assert!(prop_security_harden_checklist());
}

#[test]
fn failover_rest_requires_auth() {
    let mut pair = HaPair::new();
    let r = dispatch_ha_rest(
        &mut pair,
        RestRequest {
            method: RestMethod::Post,
            path: "/ha/failover",
            auth_token: None,
        },
    );
    assert_eq!(r.status, 401);
    assert_eq!(pair.active, HaRole::Primary);
}

#[test]
fn gap_closed_and_marker() {
    assert!(HA_GAP_NOTE.contains("CLOSED M6.6"));
    assert_eq!(M6_HA_OK_MARKER, "RAYNU-V-M6-HA-OK");
}
