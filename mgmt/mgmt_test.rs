use super::*;

#[test]
fn starts_defined() {
    assert_eq!(initial_lifecycle(), VmLifecycle::Defined);
}

#[test]
fn create_start_stop_destroy_roundtrip() {
    assert!(prop_lifecycle_roundtrip());
}

#[test]
fn rejects_zero_guest_and_bad_transitions() {
    let mut t = VmTable::new();
    assert_eq!(t.create(0), Err(LifecycleError::InvalidGuest));
    assert!(t.create(1).is_ok());
    assert_eq!(t.create(1), Err(LifecycleError::BadState));
    assert_eq!(t.stop(1), Err(LifecycleError::BadState)); // Defined → stop illegal
    assert!(t.start(1).is_ok());
    assert_eq!(t.destroy(1), Err(LifecycleError::BadState)); // must stop first
    assert!(t.stop(1).is_ok());
    assert!(t.destroy(1).is_ok());
    assert_eq!(t.start(1), Err(LifecycleError::NotFound));
}

#[test]
fn restart_from_stopped() {
    let mut t = VmTable::new();
    assert!(t.create(2).is_ok());
    assert!(t.start(2).is_ok());
    assert!(t.stop(2).is_ok());
    assert!(t.start(2).is_ok());
    assert_eq!(t.get(2).map(|r| r.state), Some(VmLifecycle::Running));
}

#[test]
fn lifecycle_emits_audit_events() {
    let before = crate::audit::integrity::boot_ring_len_for_test();
    let mut t = VmTable::new();
    assert!(t.create(3).is_ok());
    assert!(t.start(3).is_ok());
    assert!(t.stop(3).is_ok());
    assert!(t.destroy(3).is_ok());
    let after = crate::audit::integrity::boot_ring_len_for_test();
    assert!(after >= before + 4, "expected ≥4 audit events from lifecycle");
}
