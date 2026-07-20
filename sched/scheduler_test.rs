use super::*;

#[test]
fn picks_highest_credit() {
    let mut s = CreditScheduler::new();
    s.register_vcpu(10).unwrap();
    s.register_vcpu(20).unwrap();
    assert_eq!(s.pick_next().unwrap(), 1);
}

#[test]
fn consume_quantum_switches_partner() {
    let mut s = CreditScheduler::new();
    let a = s.register_vcpu(DEFAULT_CREDIT).unwrap();
    let b = s.register_vcpu(DEFAULT_CREDIT).unwrap();
    assert_eq!(a, 0);
    assert_eq!(b, 1);
    // Equal credits → index 0 wins.
    assert_eq!(s.pick_next().unwrap(), 0);
    s.consume_quantum(0);
    assert_eq!(s.credit(0), Some(0));
    assert_eq!(s.credit(1), Some(DEFAULT_CREDIT));
    assert_eq!(s.pick_next_fair(Some(0)).unwrap(), 1);
    s.consume_quantum(1);
    // Both zero → replenish → either runnable; fair prefers not-1 → 0.
    assert_eq!(s.pick_next_fair(Some(1)).unwrap(), 0);
}

#[test]
fn markers_stable() {
    assert_eq!(M4_SCHED_OK_MARKER, "RAYNU-V-M4-SCHED-OK");
    assert_eq!(M4_SLICE_G0_MARKER, "RAYNU-V-M4-SLICE-G0");
    assert_eq!(M4_SLICE_G1_MARKER, "RAYNU-V-M4-SLICE-G1");
}
