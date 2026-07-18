use super::*;

#[test]
fn new_region_not_launched() {
    let mut r = VmcsRegion::new(1);
    assert!(!r.is_launched());
    assert_eq!(r.handle().id, 1);
    r.mark_launched();
    assert!(r.is_launched());
}
