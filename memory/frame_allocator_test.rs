use super::*;

/// Build a tiny stack-backed allocator; bitmap storage must outlive `a`.
unsafe fn tiny_alloc(capacity: u64, words: &mut [u64; 64]) -> FrameAllocator {
    let base = 0x1000u64;
    FrameAllocator::new(base, capacity, words.as_mut_ptr() as u64).unwrap()
}

#[test]
fn allocate_then_free() {
    let mut words = [0u64; 64];
    let mut a = unsafe { tiny_alloc(4, &mut words) };
    let f = a.allocate_frame().unwrap();
    assert!(a.is_allocated(f));
    assert!(a.free_frame(f).is_ok());
    assert!(!a.is_allocated(f));
}

#[test]
fn no_double_alloc_same_frame() {
    let mut words = [0u64; 64];
    let mut a = unsafe { tiny_alloc(2, &mut words) };
    let f0 = a.allocate_frame().unwrap();
    let f1 = a.allocate_frame().unwrap();
    assert_ne!(f0, f1);
    assert!(a.allocate_frame().is_none());
}

#[test]
fn double_free_rejected() {
    let mut words = [0u64; 64];
    let mut a = unsafe { tiny_alloc(2, &mut words) };
    let f = a.allocate_frame().unwrap();
    a.free_frame(f).unwrap();
    assert_eq!(a.free_frame(f), Err(AllocError::DoubleFree));
}

#[test]
fn freelist_reuses_frame() {
    let mut words = [0u64; 64];
    let mut a = unsafe { tiny_alloc(2, &mut words) };
    let f0 = a.allocate_frame().unwrap();
    a.free_frame(f0).unwrap();
    let f1 = a.allocate_frame().unwrap();
    assert_eq!(f0, f1);
}

#[test]
fn selftest_ok() {
    let mut words = [0u64; 64];
    let mut a = unsafe { tiny_alloc(8, &mut words) };
    assert!(run_allocator_selftest(&mut a).is_ok());
    assert!(allocator_selftest_ok());
    assert_eq!(a.allocated_count(), 0);
}

#[test]
fn marker_and_geometry() {
    assert_eq!(M2_ALLOC_OK_MARKER, "RAYNU-V-M2-ALLOC-OK");
    assert_eq!(FrameAllocator::bitmap_pages_needed(1), 1);
    assert_eq!(FrameAllocator::bitmap_pages_needed(32768), 1);
    assert_eq!(FrameAllocator::bitmap_pages_needed(32769), 2);
    let f = PhysFrame::from_phys(0x1784000);
    assert_eq!(f.to_phys(), 0x1784000);
}

/// Bounded ADR-002 check: distinct alloc + double-free rejected.
#[cfg(kani)]
#[kani::proof]
#[kani::unwind(16)]
fn kani_alloc_no_alias_double_free_rejected() {
    let mut words = [0u64; 64];
    // SAFETY: stack bitmap; capacity 4 fits in words.
    let mut a = unsafe { tiny_alloc(4, &mut words) };
    let f0 = a.allocate_frame().unwrap();
    let f1 = a.allocate_frame().unwrap();
    assert_ne!(f0, f1);
    assert!(a.is_allocated(f0) && a.is_allocated(f1));
    a.free_frame(f0).unwrap();
    assert_eq!(a.free_frame(f0), Err(AllocError::DoubleFree));
    let f2 = a.allocate_frame().unwrap();
    assert_eq!(f2, f0);
}
