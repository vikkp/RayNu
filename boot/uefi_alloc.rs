//! Dual global allocator: UEFI pool before EBS, BSS bump after.
//!
//! Pillar: [Z]
//! Proven Core: **outside**
//!
//! `uefi` `global_allocator` returns null / panics after ExitBootServices.
//! Firmware TLS 1.2 (`rsa` PKCS#8 + sign, `p256` ECDHE) allocates. After EBS
//! those calls use a leaky BSS bump so QEMU/iron HTTPS can complete without
//! boot services.
//!
//! Measured on the host with a counting allocator (same crates, same
//! sizes): one PKCS#8 RSA-2048 parse is ~100 KiB, one ECDHE-RSA handshake
//! is ~73 KiB, and the wrapped HTTP exchange is 0. Nothing is freed, so
//! the capacity is the number of handshakes one boot can serve. 512 KiB
//! was about six. Iron `0beeb871` parsed the key on every accept and
//! every re-listen and the chassis reset on the second connection
//! (`mgmt/tls12.rs` now parses once). `.bss` is not in the EFI file.

#![cfg(feature = "uefi-bin")]

use core::alloc::{GlobalAlloc, Layout};
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use uefi::allocator::Allocator as UefiPool;

/// Post-EBS heap. About fifty handshakes at ~73 KiB each.
pub const BUMP_N: usize = 4 * 1024 * 1024;
static mut BUMP: [u8; BUMP_N] = [0; BUMP_N];
static BUMP_OFF: AtomicUsize = AtomicUsize::new(0);
static POST_EBS: AtomicBool = AtomicBool::new(false);

/// Call once after [`crate::boot::handoff::leave_firmware`]. Further `alloc`
/// uses the BSS bump; UEFI `free_pool` is not invoked.
pub fn mark_post_ebs() {
    POST_EBS.store(true, Ordering::SeqCst);
}

/// Bytes handed out by the post-EBS bump so far. Never decreases. The H2
/// panic line prints it so an exhausted heap names itself on COM2.
pub fn bump_used() -> usize {
    BUMP_OFF.load(Ordering::SeqCst)
}

/// True once the bump is the allocator (boot services gone).
pub fn post_ebs() -> bool {
    POST_EBS.load(Ordering::SeqCst)
}

struct DualAlloc;

#[global_allocator]
static ALLOC: DualAlloc = DualAlloc;

unsafe impl GlobalAlloc for DualAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if POST_EBS.load(Ordering::SeqCst) {
            bump_alloc(layout)
        } else {
            UefiPool.alloc(layout)
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if !POST_EBS.load(Ordering::SeqCst) {
            UefiPool.dealloc(ptr, layout);
        }
    }
}

fn bump_alloc(layout: Layout) -> *mut u8 {
    let align = layout.align().max(1);
    let size = layout.size();
    let base = unsafe { core::ptr::addr_of_mut!(BUMP) as usize };
    let mut off = BUMP_OFF.load(Ordering::Relaxed);
    loop {
        let start = base.saturating_add(off);
        let mask = align - 1;
        let aligned = (start + mask) & !mask;
        if aligned < base {
            return core::ptr::null_mut();
        }
        let aligned_off = aligned - base;
        let next = match aligned_off.checked_add(size) {
            Some(n) if n <= BUMP_N => n,
            _ => return core::ptr::null_mut(),
        };
        match BUMP_OFF.compare_exchange_weak(off, next, Ordering::SeqCst, Ordering::Relaxed) {
            Ok(_) => return aligned as *mut u8,
            Err(cur) => off = cur,
        }
    }
}
