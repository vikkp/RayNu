use super::{
    coexist_millis_from_tsc, http_accept_should_idle_abort, prop_http_accept_idle_abort,
    COEXIST_TSC_HZ_FALLBACK, HOST_NIC_HTTP_IDLE_MS,
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
fn coexist_millis_from_tsc_wraps() {
    let start = u64::MAX - 2_099_999;
    assert_eq!(coexist_millis_from_tsc(start, 0, 2_100_000_000), 1);
}
