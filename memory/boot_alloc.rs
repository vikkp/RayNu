//! Bootstrap Proven Core [`FrameAllocator`] from the early bump pool.
//!
//! Pillar: [V] [Z]
//! Proven Core: **allocator inside**; this glue is thin boot wiring.

use crate::boot::mem::FrameBump;
use crate::memory::frame_allocator::{AllocError, FrameAllocator};

/// Carve bitmap pages from `bump`, then wrap the remainder as a [`FrameAllocator`].
pub fn bootstrap_from_bump(bump: &mut FrameBump) -> Result<FrameAllocator, AllocError> {
    let rem = bump.remaining_pages();
    if rem < 2 {
        return Err(AllocError::Bootstrap);
    }

    let mut bitmap_pages = 1u64;
    let mut pool_pages = 0u64;
    while bitmap_pages < rem {
        let candidate = rem - bitmap_pages;
        if FrameAllocator::bitmap_pages_needed(candidate) <= bitmap_pages {
            pool_pages = candidate;
            break;
        }
        bitmap_pages += 1;
    }
    if pool_pages == 0 {
        return Err(AllocError::Bootstrap);
    }

    let Some(bitmap) = bump.alloc_frame() else {
        return Err(AllocError::Bootstrap);
    };
    for _ in 1..bitmap_pages {
        if bump.alloc_frame().is_none() {
            return Err(AllocError::Bootstrap);
        }
    }

    let Some((base, got)) = bump.take_remaining() else {
        return Err(AllocError::Bootstrap);
    };
    if got != pool_pages {
        return Err(AllocError::Bootstrap);
    }

    // SAFETY: bitmap and pool frames exclusively owned from bump; identity-mapped.
    unsafe { FrameAllocator::new(base, pool_pages, bitmap.0) }
}
