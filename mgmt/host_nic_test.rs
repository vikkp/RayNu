use super::{
    coexist_keepalive_idle, coexist_millis_from_tsc, http_accept_idle_limit_ms,
    http_accept_should_idle_abort, http_reply_should_give_up, listen_reclaim_slot,
    prop_http_accept_idle_abort, spare_listen_slot, ListenSlot, COEXIST_KEEPALIVE_IDLE_MS,
    COEXIST_LISTEN_BURST, COEXIST_LISTEN_RESERVE, COEXIST_TCP_SLOTS, COEXIST_TSC_HZ_FALLBACK,
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
fn a_closed_port_reclaims_one_slot() {
    let fresh = ListenSlot {
        listening: false,
        active: true,
        kept: true,
        age_ms: 100,
    };
    let young = ListenSlot {
        listening: false,
        active: true,
        kept: false,
        age_ms: 50,
    };
    let ancient = ListenSlot {
        listening: false,
        active: true,
        kept: false,
        age_ms: HOST_NIC_HTTP_IDLE_MS,
    };
    let spare = ListenSlot {
        listening: true,
        active: false,
        kept: false,
        age_ms: 0,
    };
    assert_eq!(COEXIST_LISTEN_RESERVE, 2);
    assert_eq!(COEXIST_LISTEN_BURST, 6);
    let mut full = [fresh; 10];
    full[3].age_ms = 5_000;
    assert_eq!(
        listen_reclaim_slot(&full, COEXIST_LISTEN_BURST, HOST_NIC_HTTP_IDLE_MS),
        Some(3)
    );
    let mut with_young = [fresh; 10];
    with_young[1].age_ms = 9_000;
    with_young[7] = young;
    assert_eq!(
        listen_reclaim_slot(&with_young, COEXIST_LISTEN_BURST, HOST_NIC_HTTP_IDLE_MS),
        Some(1)
    );
    let mut one_listen = [fresh; 10];
    one_listen[0] = spare;
    one_listen[4] = young;
    assert_eq!(
        listen_reclaim_slot(&one_listen, COEXIST_LISTEN_RESERVE, HOST_NIC_HTTP_IDLE_MS),
        None
    );
    let mut one_and_ancient = [fresh; 10];
    one_and_ancient[0] = spare;
    one_and_ancient[4] = ancient;
    assert_eq!(
        listen_reclaim_slot(
            &one_and_ancient,
            COEXIST_LISTEN_RESERVE,
            HOST_NIC_HTTP_IDLE_MS
        ),
        Some(4)
    );
    let mut two = [fresh; 10];
    two[0] = spare;
    two[1] = spare;
    assert_eq!(
        listen_reclaim_slot(&two, COEXIST_LISTEN_RESERVE, HOST_NIC_HTTP_IDLE_MS),
        None
    );
    // A reload with nothing listening frees kept sessions up to the burst
    // and does not take the handshake that was just accepted.
    let mut reload = [fresh; 10];
    reload[9] = young;
    let mut listening = 0usize;
    for _ in 0..COEXIST_TCP_SLOTS {
        let Some(i) = listen_reclaim_slot(&reload, COEXIST_LISTEN_BURST, HOST_NIC_HTTP_IDLE_MS)
        else {
            break;
        };
        assert_ne!(i, 9);
        reload[i] = spare;
        listening += 1;
    }
    assert_eq!(listening, COEXIST_LISTEN_BURST);
    assert!(!reload[9].listening);
    assert!(!reload[9].kept);
}

#[test]
fn spare_listener_waits_until_the_page_is_kept() {
    let idle = ListenSlot {
        listening: false,
        active: false,
        kept: false,
        age_ms: 0,
    };
    let handshake = ListenSlot {
        listening: false,
        active: true,
        kept: false,
        age_ms: 100,
    };
    let kept = ListenSlot {
        listening: false,
        active: true,
        kept: true,
        age_ms: 100,
    };
    let listening = ListenSlot {
        listening: true,
        active: false,
        kept: false,
        age_ms: 0,
    };
    let mut first = [idle; 4];
    first[0] = handshake;
    assert_eq!(spare_listen_slot(&first), None);
    let mut page = [idle; 4];
    page[0] = kept;
    assert_eq!(spare_listen_slot(&page), Some(1));
    page[1] = listening;
    assert_eq!(spare_listen_slot(&page), None);
    let mut full = [handshake; 4];
    full[0] = kept;
    assert_eq!(spare_listen_slot(&full), None);
}

#[test]
fn a_reply_that_never_queues_gives_the_slot_back() {
    assert!(!http_reply_should_give_up(
        false,
        true,
        true,
        HOST_NIC_HTTP_IDLE_MS - 1,
        HOST_NIC_HTTP_IDLE_MS
    ));
    assert!(http_reply_should_give_up(
        false,
        true,
        true,
        HOST_NIC_HTTP_IDLE_MS,
        HOST_NIC_HTTP_IDLE_MS
    ));
    assert!(!http_reply_should_give_up(
        true,
        true,
        true,
        HOST_NIC_HTTP_IDLE_MS,
        HOST_NIC_HTTP_IDLE_MS
    ));
    assert!(!http_reply_should_give_up(
        false,
        true,
        false,
        HOST_NIC_HTTP_IDLE_MS,
        HOST_NIC_HTTP_IDLE_MS
    ));
    assert!(!http_accept_should_idle_abort(
        true,
        true,
        HOST_NIC_HTTP_IDLE_MS,
        HOST_NIC_HTTP_IDLE_MS
    ));
}

#[test]
fn handshake_idle_matches_the_header_budget() {
    assert_eq!(HOST_NIC_HTTP_HS_IDLE_MS, 15_000);
    assert_eq!(HOST_NIC_HTTP_HS_IDLE_MS, HOST_NIC_HTTP_IDLE_MS);
    assert_eq!(http_accept_idle_limit_ms(true), HOST_NIC_HTTP_IDLE_MS);
    assert_eq!(http_accept_idle_limit_ms(false), HOST_NIC_HTTP_IDLE_MS);
    assert_eq!(
        super::http_accept_idle_limit_for(true, true),
        HOST_NIC_HTTP_IDLE_MS
    );
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
fn ten_slots_leave_listeners_after_an_eight_accept_burst() {
    assert_eq!(COEXIST_TCP_SLOTS, 10);
    assert!(COEXIST_TCP_SLOTS > 8);
    assert_eq!(COEXIST_KEEPALIVE_IDLE_MS, 600_000);
    let listen = include_str!("host_nic_listen.rs");
    assert!(listen.contains("COEXIST_TCP_SLOTS"));
    assert!(listen.contains("COEXIST_CLOSE_FORCE_MS"));
    assert!(listen.contains("stay in LISTEN"));
    assert!(listen.contains("filled all six slots"));
    assert!(listen.contains("FIN, not RST"));
    assert!(!listen.contains("coexist_reclaim_slot"));
}
