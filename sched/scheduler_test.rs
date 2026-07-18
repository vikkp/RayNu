use super::*;

#[test]
fn picks_highest_credit() {
    let mut s = CreditScheduler::new();
    s.register_vcpu(10).unwrap();
    s.register_vcpu(20).unwrap();
    assert_eq!(s.pick_next().unwrap(), 1);
}
