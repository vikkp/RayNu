use super::*;

#[test]
fn allocate_then_free() {
    let mut a = FrameAllocator::new(4);
    let f = a.allocate_frame().unwrap();
    assert!(a.is_allocated(f));
    assert!(a.free_frame(f));
    assert!(!a.is_allocated(f));
}

#[test]
fn no_double_alloc_same_frame() {
    let mut a = FrameAllocator::new(2);
    let f0 = a.allocate_frame().unwrap();
    let f1 = a.allocate_frame().unwrap();
    assert_ne!(f0, f1);
    assert!(a.allocate_frame().is_none());
}
