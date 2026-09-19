use super::{
    coexist_millis_from_tsc, http_accept_should_idle_abort, prop_http_accept_idle_abort,
    COEXIST_TSC_HZ_FALLBACK, HOST_NIC_DHCP_MS, HOST_NIC_HTTP_IDLE_MS, HOST_NIC_LISTEN_MS,
    PRE_RAYNUF_HTTPS_MS,
};

#[test]
fn http_idle_abort_at_limit_only() {
    assert_eq!(HOST_NIC_HTTP_IDLE_MS, 15_000);
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

#[test]
fn coexist_millis_from_tsc_2g1_one_ms() {
    assert_eq!(COEXIST_TSC_HZ_FALLBACK, 2_100_000_000);
    assert_eq!(coexist_millis_from_tsc(0, 2_100_000, 2_100_000_000), 1);
    assert_eq!(
        coexist_millis_from_tsc(10_000, 10_000 + 2_100_000 * 50, 2_100_000_000),
        50
    );
}

#[test]
fn coexist_millis_from_tsc_hz_zero_uses_fallback() {
    assert_eq!(coexist_millis_from_tsc(0, 2_100_000, 0), 1);
    assert_eq!(coexist_millis_from_tsc(0, 2_100_000, 999), 1);
}

#[test]
fn native_dhcp_budget_is_independent_of_browser_idle() {
    assert_eq!(HOST_NIC_DHCP_MS, 12_000);
    assert_eq!(HOST_NIC_HTTP_IDLE_MS, 15_000);
}

#[test]
fn pre_raynuf_https_window_is_longer_than_qemu_listen() {
    assert_eq!(HOST_NIC_LISTEN_MS, 20_000);
    assert_eq!(PRE_RAYNUF_HTTPS_MS, 45_000);
    assert!(PRE_RAYNUF_HTTPS_MS > HOST_NIC_LISTEN_MS);
}

#[test]
fn coexist_millis_from_tsc_wraps() {
    let start = u64::MAX - 2_099_999;
    assert_eq!(coexist_millis_from_tsc(start, 0, 2_100_000_000), 1);
}
