use super::{
    http_accept_should_idle_abort, prop_http_accept_idle_abort, HOST_NIC_HTTP_IDLE_MS,
};

#[test]
fn http_idle_abort_at_limit_only() {
    assert_eq!(HOST_NIC_HTTP_IDLE_MS, 3000);
    assert!(prop_http_accept_idle_abort());
    assert!(!http_accept_should_idle_abort(
        true,
        false,
        HOST_NIC_HTTP_IDLE_MS - 1,
        HOST_NIC_HTTP_IDLE_MS
    ));
    assert!(http_accept_should_idle_abort(
        true,
        false,
        HOST_NIC_HTTP_IDLE_MS,
        HOST_NIC_HTTP_IDLE_MS
    ));
}
