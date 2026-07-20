use super::*;

#[test]
fn cli_verbs_parse() {
    assert!(prop_cli_verbs_parse());
}

#[test]
fn cli_rest_roundtrip() {
    assert!(prop_cli_rest_roundtrip());
}

#[test]
fn rest_create_start_via_routes() {
    let mut t = VmTable::new();
    let tok = Some(BRINGUP_AUTH_TOKEN);
    let created = dispatch_rest(
        &mut t,
        RestRequest {
            method: RestMethod::Post,
            path: "/vms/9",
            auth_token: tok,
        },
    );
    assert_eq!(created.status, 201);
    let started = dispatch_rest(
        &mut t,
        RestRequest {
            method: RestMethod::Post,
            path: "/vms/9/start",
            auth_token: tok,
        },
    );
    assert_eq!(started.status, 200);
    assert_eq!(t.get(9).map(|r| r.state), Some(VmLifecycle::Running));
}

#[test]
fn auth_deny_allow() {
    assert!(prop_auth_deny_allow());
    assert!(!auth_allows(None));
    assert!(!auth_allows(Some("anything")));
    assert!(auth_allows(Some(BRINGUP_AUTH_TOKEN)));
    assert!(AUTH_GAP_NOTE.contains("CLOSED M6.4"));
    assert_eq!(M6_AUTH_OK_MARKER, "RAYNU-V-M6-AUTH-OK");
}

#[test]
fn list_fills_active_only() {
    let mut t = VmTable::new();
    assert!(t.create(1).is_ok());
    assert!(t.create(2).is_ok());
    assert!(t.start(2).is_ok());
    assert!(t.stop(2).is_ok());
    assert!(t.destroy(2).is_ok());
    let mut buf = [None; MGMT_GUEST_CAP];
    let n = t.list(&mut buf);
    assert_eq!(n, 1);
    assert_eq!(buf[0].map(|r| r.guest_id), Some(1));
}
