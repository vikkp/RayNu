use super::{
    inject_mgmt_fatals, prop_arena_reset_rewinds, MgmtArena, MgmtFatal, MGMT_FATAL_INJECT_N,
};
use crate::memory::frame_allocator::FrameAllocator;

/// SAFETY: `words` is exclusive stack storage for the bitmap; identity-mapped
/// host tests treat the pointer as the allocator's bitmap_phys.
/// KANI-TARGET: frame_allocator_test covers allocate/free; this only snapshots counts.
unsafe fn tiny_alloc(capacity: u64, words: &mut [u64; 64]) -> FrameAllocator {
    FrameAllocator::new(0x1000, capacity, words.as_mut_ptr() as u64).unwrap()
}

#[test]
fn arena_reset_package() {
    assert!(prop_arena_reset_rewinds());
}

#[test]
fn alloc_then_reset_frees_space() {
    let mut a = MgmtArena::new();
    assert!(a.alloc_bytes(32 * 1024, 16).is_ok());
    a.reset();
    assert!(a.alloc_bytes(32 * 1024, 16).is_ok());
}

#[test]
fn exhausted_is_mgmt_fatal() {
    let mut a = MgmtArena::new();
    assert_eq!(
        a.alloc_bytes(super::MGMT_ARENA_BYTES + 1, 1),
        Err(MgmtFatal::ArenaExhausted)
    );
}

/// Phase E: N induced fatals must not change Proven Core allocator accounting.
#[test]
fn induced_fatals_do_not_touch_frame_allocator() {
    let mut words = [0u64; 64];
    let mut fa = unsafe { tiny_alloc(8, &mut words) };
    let f = fa.allocate_frame().unwrap();
    let before = fa.allocated_count();
    let mut arena = MgmtArena::new();
    inject_mgmt_fatals(&mut arena, MGMT_FATAL_INJECT_N).unwrap();
    assert_eq!(fa.allocated_count(), before);
    assert!(fa.is_allocated(f));
    assert_eq!(arena.used(), 0);
}
