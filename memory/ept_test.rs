use super::*;
use crate::memory::PhysFrame;

#[test]
fn exclusive_ownership() {
    let mut ept = EptMap::new();
    ept.map(1, 0x1000, PhysFrame(3), EptPermissions::READ_WRITE)
        .unwrap();
    assert_eq!(ept.owner_of(PhysFrame(3)), Some(1));
    assert_eq!(
        ept.map(2, 0x2000, PhysFrame(3), EptPermissions::READ_WRITE),
        Err(EptError::AlreadyOwned)
    );
    assert!(ept.check_invariants());
}

#[test]
fn unmap_releases_frame() {
    let mut ept = EptMap::new();
    ept.map(1, 0x1000, PhysFrame(3), EptPermissions::READ_WRITE)
        .unwrap();
    assert_eq!(ept.unmap(1, 0x1000).unwrap(), PhysFrame(3));
    ept.map(2, 0x2000, PhysFrame(3), EptPermissions::READ_WRITE)
        .unwrap();
    assert_eq!(ept.len(), 1);
}

#[test]
fn rejects_guest_zero() {
    let mut ept = EptMap::new();
    assert_eq!(
        ept.map(0, 0x1000, PhysFrame(1), EptPermissions::READ_WRITE),
        Err(EptError::InvalidGuest)
    );
}

#[test]
fn from_phys_roundtrip() {
    let f = PhysFrame::from_phys(0x1784000);
    assert_eq!(f.0, 0x1784);
    assert_eq!(f.to_phys(), 0x1784000);
}

#[test]
fn ownership_selftest_happy_path() {
    assert!(run_ownership_selftest(0x2000, 0x3000).is_ok());
    assert!(ownership_selftest_ok());
}

#[test]
fn marker_stable() {
    assert_eq!(M2_OWN_OK_MARKER, "RAYNU-V-M2-OWN-OK");
    assert_eq!(M2_BRINGUP_GUEST_ID, 1);
}
