use super::{
    coexist_keepalive_idle, coexist_millis_from_tsc, coexist_reclaim_slot, http_accept_idle_limit_ms,
    http_accept_should_idle_abort, prop_http_accept_idle_abort, CoexistSlotIdle,
    COEXIST_KEEPALIVE_IDLE_MS, COEXIST_RECLAIM_IDLE_MS, COEXIST_TCP_SLOTS, COEXIST_TSC_HZ_FALLBACK,
    HOST_NIC_DHCP_MS, HOST_NIC_HTTP_HS_IDLE_MS, HOST_NIC_HTTP_IDLE_MS, HOST_NIC_LISTEN_MS,
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
fn handshake_idle_reclaim_is_faster_than_header_idle() {
    assert_eq!(HOST_NIC_HTTP_HS_IDLE_MS, 2_000);
    assert_eq!(http_accept_idle_limit_ms(true), HOST_NIC_HTTP_HS_IDLE_MS);
    assert_eq!(http_accept_idle_limit_ms(false), HOST_NIC_HTTP_IDLE_MS);
    assert!(http_accept_should_idle_abort(
        true,
        false,
        HOST_NIC_HTTP_HS_IDLE_MS,
        http_accept_idle_limit_ms(true)
    ));
    assert!(!http_accept_should_idle_abort(
        true,
        false,
        HOST_NIC_HTTP_HS_IDLE_MS - 1,
        http_accept_idle_limit_ms(true)
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
fn coexist_keepalive_idle_is_ten_minutes() {
    assert_eq!(COEXIST_KEEPALIVE_IDLE_MS, 600_000);
    assert!(!coexist_keepalive_idle(
        30_000,
        0,
        COEXIST_KEEPALIVE_IDLE_MS
    ));
    assert!(!coexist_keepalive_idle(
        599_999,
        0,
        COEXIST_KEEPALIVE_IDLE_MS
    ));
    assert!(coexist_keepalive_idle(
        600_000,
        0,
        COEXIST_KEEPALIVE_IDLE_MS
    ));
    assert!(!coexist_keepalive_idle(
        600_000,
        1,
        COEXIST_KEEPALIVE_IDLE_MS
    ));
    assert!(coexist_keepalive_idle(
        600_001,
        1,
        COEXIST_KEEPALIVE_IDLE_MS
    ));
    assert!(!coexist_keepalive_idle(600_000, 0, 0));
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

#[test]
fn a_listening_slot_blocks_reclaim_and_the_oldest_idle_held_slot_is_the_one() {
    assert_eq!(COEXIST_TCP_SLOTS, 6);
    let listen = include_str!("host_nic_listen.rs");
    assert!(listen.contains("COEXIST_TCP_SLOTS"));
    assert!(listen.contains("COEXIST_CLOSE_FORCE_MS"));
    assert!(listen.contains("coexist_reclaim_slot"));
    let young = CoexistSlotIdle {
        listening: false,
        held: true,
        idle_ms: COEXIST_RECLAIM_IDLE_MS - 1,
    };
    let old = CoexistSlotIdle {
        listening: false,
        held: true,
        idle_ms: COEXIST_RECLAIM_IDLE_MS + 50,
    };
    let older = CoexistSlotIdle {
        listening: false,
        held: true,
        idle_ms: COEXIST_RECLAIM_IDLE_MS + 500,
    };
    let open = CoexistSlotIdle {
        listening: true,
        held: false,
        idle_ms: 0,
    };
    assert_eq!(coexist_reclaim_slot(&[young]), None);
    assert_eq!(coexist_reclaim_slot(&[young, old]), Some(1));
    assert_eq!(coexist_reclaim_slot(&[older, old, open]), None);
    assert_eq!(coexist_reclaim_slot(&[old, older, young]), Some(1));
}
