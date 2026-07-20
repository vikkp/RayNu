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
    let created = dispatch_rest(
        &mut t,
        RestRequest {
            method: RestMethod::Post,
            path: "/vms/9",
            auth_token: None,
        },
    );
    assert_eq!(created.status, 201);
    let started = dispatch_rest(
        &mut t,
        RestRequest {
            method: RestMethod::Post,
            path: "/vms/9/start",
            auth_token: Some("stub"),
        },
    );
    assert_eq!(started.status, 200);
    assert_eq!(t.get(9).map(|r| r.state), Some(VmLifecycle::Running));
}

#[test]
fn auth_stub_documents_gap() {
    assert!(auth_allows(None));
    assert!(auth_allows(Some("anything")));
    assert!(AUTH_GAP_NOTE.contains("M6"));
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
