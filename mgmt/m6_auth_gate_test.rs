use super::{
    audit_auth_events_present, auth_scripts_present, mgmt_auth_surface_present, run_m6_auth_gate,
    M6_AUTH_GATE_MARKER,
};
use crate::mgmt::api::prop_auth_deny_allow;

#[test]
fn m6_4_auth_gate_passes() {
    assert_eq!(M6_AUTH_GATE_MARKER, "RAYNU-V-M6-AUTH-OK");
    assert!(mgmt_auth_surface_present(), "mgmt/api must embed M6.4 auth");
    assert!(
        audit_auth_events_present(),
        "audit must carry AuthAllowed/AuthDenied"
    );
    assert!(auth_scripts_present(), "m6-auth-smoke.sh must be present");
    assert!(prop_auth_deny_allow(), "deny/allow prop must hold");
    assert!(run_m6_auth_gate());
    println!("RAYNU-V-M6-AUTH-OK");
}
