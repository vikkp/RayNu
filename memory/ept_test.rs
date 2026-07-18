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
}

#[test]
fn unmap_releases_frame() {
    let mut ept = EptMap::new();
    ept.map(1, 0x1000, PhysFrame(3), EptPermissions::READ_WRITE)
        .unwrap();
    assert_eq!(ept.unmap(1, 0x1000).unwrap(), PhysFrame(3));
    ept.map(2, 0x2000, PhysFrame(3), EptPermissions::READ_WRITE)
        .unwrap();
}
