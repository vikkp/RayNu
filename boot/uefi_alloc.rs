//! Dual global allocator: UEFI pool before EBS, freeing heap after.
//!
//! Pillar: [Z]
//! Proven Core: **outside**
//!
//! `uefi` `global_allocator` returns null / panics after ExitBootServices.
//! Firmware TLS 1.2 (`rsa` PKCS#8 + sign, `p256` ECDHE) allocates. After EBS
//! those calls use [`crate::boot::post_ebs_heap`]. `dealloc` returns the
//! block, so a handshake's temporaries (~73 KiB, measured) are reused by
//! the next one instead of walking a leaky bump. `.bss` is not in the EFI
//! file.

#![cfg(feature = "uefi-bin")]

use core::alloc::{GlobalAlloc, Layout};
use core::mem::MaybeUninit;
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use uefi::allocator::Allocator as UefiPool;

use crate::boot::post_ebs_heap::Heap;

/// Post-EBS heap. The handshake path frees what it drops, so this is
/// headroom for the live key plus a few in-flight handshakes, not a
/// budget of one handshake each.
pub const BUMP_N: usize = 4 * 1024 * 1024;

#[repr(align(16))]
struct HeapMem {
    bytes: [u8; BUMP_N],
}

static mut HEAP_MEM: HeapMem = HeapMem {
    bytes: [0; BUMP_N],
};
static mut HEAP: MaybeUninit<Heap> = MaybeUninit::uninit();
static HEAP_READY: AtomicBool = AtomicBool::new(false);
static LIVE: AtomicUsize = AtomicUsize::new(0);
static HIGH: AtomicUsize = AtomicUsize::new(0);
static POST_EBS: AtomicBool = AtomicBool::new(false);

/// Call once after [`crate::boot::handoff::leave_firmware`]. Further `alloc`
/// uses the freeing heap; UEFI `free_pool` is not invoked.
pub fn mark_post_ebs() {
    POST_EBS.store(true, Ordering::SeqCst);
}

/// Bytes currently held by the post-EBS heap (not the high water).
pub fn bump_used() -> usize {
    LIVE.load(Ordering::SeqCst)
}

/// Most bytes the post-EBS heap has held at once.
pub fn bump_high() -> usize {
    HIGH.load(Ordering::SeqCst)
}

/// True once the heap is the allocator (boot services gone).
pub fn post_ebs() -> bool {
    POST_EBS.load(Ordering::SeqCst)
}

fn heap() -> &'static mut Heap {
    // SAFETY: BSP-only. The buffer is `.bss` and outlives every alloc.
    unsafe {
        if !HEAP_READY.load(Ordering::Acquire) {
            let base = core::ptr::addr_of_mut!(HEAP_MEM.bytes) as *mut u8;
            HEAP.write(Heap::new(base, BUMP_N));
            HEAP_READY.store(true, Ordering::Release);
        }
        HEAP.assume_init_mut()
    }
}

fn note_usage(h: &Heap) {
    LIVE.store(h.live(), Ordering::SeqCst);
    HIGH.store(h.high(), Ordering::SeqCst);
}

struct DualAlloc;

#[global_allocator]
static ALLOC: DualAlloc = DualAlloc;

unsafe impl GlobalAlloc for DualAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if POST_EBS.load(Ordering::SeqCst) {
            let h = heap();
            let ptr = h.alloc(layout);
            note_usage(h);
            ptr
        } else {
            UefiPool.alloc(layout)
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if POST_EBS.load(Ordering::SeqCst) {
            let h = heap();
            h.dealloc(ptr);
            note_usage(h);
            let _ = layout;
        } else {
            UefiPool.dealloc(ptr, layout);
        }
    }
}
