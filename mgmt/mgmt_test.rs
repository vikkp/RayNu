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
fn product_iso_defaults_and_rejects_bzimage() {
    let mut t = VmTable::new();
    assert!(t
        .create_with_spec(
            4,
            VmSpec {
                cpu: 2,
                ram_mib: 2048,
                disk_mib: 10240,
                iso_id: 1,
                image_type: None,
            },
        )
        .is_ok());
    let rec = t.get(4).expect("created");
    assert_eq!(rec.image_type, Some(GuestImageType::LinuxIso));
    assert!(rec.boot_spec().is_some());

    let mut t = VmTable::new();
    assert_eq!(
        t.create_with_spec(
            5,
            VmSpec {
                cpu: 1,
                ram_mib: 512,
                disk_mib: 1024,
                iso_id: 1,
                image_type: Some(GuestImageType::LinuxBzImage),
            },
        ),
        Err(LifecycleError::InvalidGuest)
    );
    assert!(t.create(6).is_ok());
    assert_eq!(t.get(6).and_then(|r| r.image_type), None);
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
    crate::audit::integrity::boot_ring_reset_for_test();
    let before = crate::audit::integrity::boot_ring_len_for_test();
    let mut t = VmTable::new();
    assert!(t.create(3).is_ok());
    assert!(t.start(3).is_ok());
    assert!(t.stop(3).is_ok());
    assert!(t.destroy(3).is_ok());
    let after = crate::audit::integrity::boot_ring_len_for_test();
    assert!(after >= before + 4, "expected ≥4 audit events from lifecycle");
}
