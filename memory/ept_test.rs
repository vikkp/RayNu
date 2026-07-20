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
    assert!(run_ownership_selftest(0x2000, 0x3000, 0x4000).is_ok());
    assert!(ownership_selftest_ok());
}

#[test]
fn marker_stable() {
    assert_eq!(M2_OWN_OK_MARKER, "RAYNU-V-M2-OWN-OK");
    assert_eq!(M2_BRINGUP_GUEST_ID, 1);
}

#[test]
fn precise_range_claim() {
    assert!(claim_precise_identity_ranges().is_ok());
    assert!(precise_ranges_ok());
    let ranges = core::ptr::addr_of!(PRECISE_RANGES);
    // SAFETY: single-threaded test; ranges filled by claim above.
    unsafe {
        assert!((*ranges).contains_gpa(M2_BRINGUP_GUEST_ID, 0));
        assert!((*ranges).contains_gpa(
            M2_BRINGUP_GUEST_ID,
            crate::guest::linux_boot::GUEST_RAM_BYTES - 0x1000
        ));
        assert!(!(*ranges).contains_gpa(
            M2_BRINGUP_GUEST_ID,
            crate::arch::apic::DEFAULT_APIC_PHYS
        ));
    }
}

#[test]
fn precise_range_claim_with_g1_hole() {
    use super::{claim_precise_with_guest1_hole, M4_GUEST1_ID};
    use crate::memory::ept_hw::TWO_MIB;
    let guest_ram = crate::guest::linux_boot::GUEST_RAM_BYTES;
    let g1 = guest_ram; // 256 MiB — above e820, inside precise window
    assert!(claim_precise_with_guest1_hole(g1, TWO_MIB).is_ok());
    let ranges = core::ptr::addr_of!(PRECISE_RANGES);
    // SAFETY: single-threaded test.
    unsafe {
        assert!((*ranges).contains_gpa(M2_BRINGUP_GUEST_ID, 0));
        assert!((*ranges).contains_gpa(M2_BRINGUP_GUEST_ID, guest_ram - 0x1000));
        assert!((*ranges).contains_gpa(M4_GUEST1_ID, g1));
        assert!(!(*ranges).contains_gpa(M2_BRINGUP_GUEST_ID, g1));
        assert!(!(*ranges).contains_gpa(M4_GUEST1_ID, 0));
    }
}

/// Bounded ADR-004 check: two guests cannot own the same HPA.
///
/// Concrete GPAs keep CBMC inside the Kani `MAP_CAP=8` unwind budget.
#[cfg(kani)]
#[kani::proof]
#[kani::unwind(16)]
fn kani_no_double_map_same_hpa() {
    let mut ept = EptMap::new();
    let frame = PhysFrame(3);
    assert!(ept
        .map(1, 0x1000, frame, EptPermissions::READ_WRITE)
        .is_ok());
    assert_eq!(
        ept.map(2, 0x2000, frame, EptPermissions::READ_WRITE),
        Err(EptError::AlreadyOwned)
    );
    assert_eq!(ept.owner_of(frame), Some(1));
    assert!(ept.check_invariants());
}
