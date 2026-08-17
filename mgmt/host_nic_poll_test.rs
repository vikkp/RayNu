use super::{bounded_poll, prop_bounded_poll_respects_budget, HOST_NIC_POLL_BUDGET};

#[test]
fn bounded_poll_stops_when_idle() {
    let mut n = 0u32;
    let r = bounded_poll(HOST_NIC_POLL_BUDGET, || {
        n += 1;
        n < 3
    });
    assert_eq!(r.iterations, 3);
    assert!(!r.exhausted_budget);
}

#[test]
fn bounded_poll_budget_package() {
    assert!(prop_bounded_poll_respects_budget());
    assert_eq!(HOST_NIC_POLL_BUDGET, 32);
}
