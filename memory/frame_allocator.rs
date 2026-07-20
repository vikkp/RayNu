//! Physical frame allocator (Proven Core).
//!
//! Pillar: [V] · Proven Core · VERIFICATION: L2 (spec M2.6) + L1 runtime
//! Double-alloc / UAF are critical isolation failures (ADR-002).
//!
//! Bitmap-backed pool over a contiguous HPA range. Boot straps this from the
//! post-EBS bump region; the bump itself stays outside the Proven Core.

/// COM1 marker when the allocator self-test passes (M2.3 gate).
pub const M2_ALLOC_OK_MARKER: &str = "RAYNU-V-M2-ALLOC-OK";

/// Host-physical frame number (4K frames). Absolute: `phys >> 12`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PhysFrame(pub u64);

impl PhysFrame {
    pub const fn from_phys(phys: u64) -> Self {
        Self(phys >> 12)
    }

    pub const fn to_phys(self) -> u64 {
        self.0 << 12
    }
}

/// Set when [`run_allocator_selftest`] succeeds.
static mut ALLOC_SELFTEST_OK: bool = false;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AllocError {
    /// Pool exhausted.
    Oom,
    /// Frame outside pool or bad argument.
    InvalidFrame,
    /// Free of a frame that was not allocated (UAF / double-free).
    DoubleFree,
    /// Bootstrap / bitmap geometry failed.
    Bootstrap,
}

/// Bitmap-backed physical frame allocator.
///
/// INVARIANTS:
///   - A frame is in at most one of {free, allocated}
///   - `allocate` returns a frame that was free and is allocated after return
///   - `free` returns a frame that was allocated and is free after return
///   - Bitmap bit i tracks `base_frame + i`
///
/// VERIFICATION: L1 — see frame_allocator_spec.rs
pub struct FrameAllocator {
    base_frame: u64,
    capacity: u64,
    bitmap_phys: u64,
    bitmap_words: usize,
    allocated_count: u64,
    scan_hint: u64,
}

impl FrameAllocator {
    /// Pages of bitmap storage required for `capacity` frames (1 bit/frame).
    pub fn bitmap_pages_needed(capacity: u64) -> u64 {
        if capacity == 0 {
            return 0;
        }
        let bytes = (capacity + 7) / 8;
        (bytes + 4095) / 4096
    }

    /// Create an allocator over `[base_phys, base_phys + capacity*4K)`.
    ///
    /// `bitmap_phys` must provide at least `ceil(capacity/64)` qwords (boot
    /// typically hands full pages via [`bitmap_pages_needed`]).
    ///
    /// SAFETY: bitmap storage is exclusively owned and identity-mapped writable
    /// for at least `ceil(capacity/64)*8` bytes.
    /// KANI-TARGET: capacity>0; bitmap spans `ceil(capacity/64)` qwords.
    pub unsafe fn new(base_phys: u64, capacity: u64, bitmap_phys: u64) -> Result<Self, AllocError> {
        if capacity == 0 || (base_phys & 0xfff) != 0 {
            return Err(AllocError::Bootstrap);
        }
        let words = ((capacity + 63) / 64) as usize;
        if words == 0 {
            return Err(AllocError::Bootstrap);
        }
        core::ptr::write_bytes(bitmap_phys as *mut u8, 0, words * 8);
        Ok(Self {
            base_frame: base_phys >> 12,
            capacity,
            bitmap_phys,
            bitmap_words: words,
            allocated_count: 0,
            scan_hint: 0,
        })
    }

    pub fn capacity(&self) -> u64 {
        self.capacity
    }

    pub fn allocated_count(&self) -> u64 {
        self.allocated_count
    }

    pub fn base_phys(&self) -> u64 {
        self.base_frame << 12
    }

    fn bit_ptr(&self, index: u64) -> (*mut u64, u64) {
        let word = (index / 64) as usize;
        let bit = index % 64;
        let ptr = (self.bitmap_phys as *mut u64).wrapping_add(word);
        (ptr, bit)
    }

    fn test_bit(&self, index: u64) -> bool {
        if index >= self.capacity {
            return false;
        }
        let (ptr, bit) = self.bit_ptr(index);
        // SAFETY: index in range; bitmap owned by self.
        // KANI-TARGET: index < capacity; ptr within bitmap_words.
        unsafe { (*ptr >> bit) & 1 == 1 }
    }

    fn set_bit(&mut self, index: u64, on: bool) {
        let (ptr, bit) = self.bit_ptr(index);
        // SAFETY: index in range; bitmap owned by self.
        // KANI-TARGET: index < capacity; ptr within bitmap_words.
        unsafe {
            if on {
                *ptr |= 1u64 << bit;
            } else {
                *ptr &= !(1u64 << bit);
            }
        }
    }

    fn contains(&self, frame: PhysFrame) -> bool {
        frame.0 >= self.base_frame && frame.0 < self.base_frame + self.capacity
    }

    fn index_of(&self, frame: PhysFrame) -> Option<u64> {
        if !self.contains(frame) {
            return None;
        }
        Some(frame.0 - self.base_frame)
    }

    /// Allocate a single physical frame.
    ///
    /// INVARIANTS:
    ///   - Returned frame was NOT previously allocated
    ///   - After return, frame IS in the allocated set
    ///
    /// VERIFICATION: L1
    pub fn allocate_frame(&mut self) -> Option<PhysFrame> {
        if self.allocated_count >= self.capacity {
            return None;
        }
        let start = self.scan_hint;
        for off in 0..self.capacity {
            let i = (start + off) % self.capacity;
            if !self.test_bit(i) {
                self.set_bit(i, true);
                self.allocated_count += 1;
                self.scan_hint = (i + 1) % self.capacity;
                let frame = PhysFrame(self.base_frame + i);
                debug_assert!(self.is_allocated(frame));
                return Some(frame);
            }
        }
        None
    }

    /// Allocate `n` contiguous frames; returns the base frame.
    ///
    /// Used by M3.7/M3.8 multi-page bzImage placement.
    pub fn allocate_contiguous(&mut self, n: u64) -> Option<PhysFrame> {
        self.allocate_contiguous_aligned(n, 1)
    }

    /// Allocate `n` contiguous frames with page-index alignment `align_pages`
    /// (must be a power of two). Used by M4.0 for a 2 MiB-aligned G1 slab.
    pub fn allocate_contiguous_aligned(&mut self, n: u64, align_pages: u64) -> Option<PhysFrame> {
        if n == 0 || align_pages == 0 || (align_pages & (align_pages - 1)) != 0 {
            return None;
        }
        if n == 1 && align_pages == 1 {
            return self.allocate_frame();
        }
        if n > self.capacity || self.allocated_count + n > self.capacity {
            return None;
        }
        let max_start = self.capacity - n;
        let mut start = 0u64;
        while start <= max_start {
            let abs = self.base_frame + start;
            if (abs & (align_pages - 1)) != 0 {
                start += 1;
                continue;
            }
            let mut free = true;
            for j in 0..n {
                if self.test_bit(start + j) {
                    free = false;
                    break;
                }
            }
            if !free {
                start += 1;
                continue;
            }
            for j in 0..n {
                self.set_bit(start + j, true);
            }
            self.allocated_count += n;
            self.scan_hint = (start + n) % self.capacity;
            return Some(PhysFrame(self.base_frame + start));
        }
        None
    }

    /// Free a previously allocated frame.
    ///
    /// INVARIANTS:
    ///   - Frame was allocated
    ///   - After Ok, frame is free
    ///
    /// VERIFICATION: L1
    pub fn free_frame(&mut self, frame: PhysFrame) -> Result<(), AllocError> {
        let Some(i) = self.index_of(frame) else {
            return Err(AllocError::InvalidFrame);
        };
        if !self.test_bit(i) {
            return Err(AllocError::DoubleFree);
        }
        self.set_bit(i, false);
        self.allocated_count -= 1;
        self.scan_hint = i;
        debug_assert!(!self.is_allocated(frame));
        Ok(())
    }

    pub fn is_allocated(&self, frame: PhysFrame) -> bool {
        match self.index_of(frame) {
            Some(i) => self.test_bit(i),
            None => false,
        }
    }
}

/// True after a successful [`run_allocator_selftest`] on this boot.
pub fn allocator_selftest_ok() -> bool {
    // SAFETY: written once on BSP before VMLAUNCH; read after VMEXIT.
    unsafe { ALLOC_SELFTEST_OK }
}

/// Alloc / free / double-free / reuse self-test (ADR-002).
///
/// Leaves the pool empty on success so the boot path can use it next.
pub fn run_allocator_selftest(alloc: &mut FrameAllocator) -> Result<(), AllocError> {
    let before = alloc.allocated_count();
    let f1 = alloc.allocate_frame().ok_or(AllocError::Oom)?;
    let f2 = alloc.allocate_frame().ok_or(AllocError::Oom)?;
    if f1 == f2 || !alloc.is_allocated(f1) || !alloc.is_allocated(f2) {
        return Err(AllocError::Bootstrap);
    }
    alloc.free_frame(f1)?;
    match alloc.free_frame(f1) {
        Err(AllocError::DoubleFree) => {}
        Ok(()) => return Err(AllocError::DoubleFree),
        Err(e) => return Err(e),
    }
    let f3 = alloc.allocate_frame().ok_or(AllocError::Oom)?;
    if f3 != f1 {
        return Err(AllocError::Bootstrap);
    }
    alloc.free_frame(f2)?;
    alloc.free_frame(f3)?;
    if alloc.allocated_count() != before {
        return Err(AllocError::Bootstrap);
    }
    // SAFETY: single-threaded boot path.
    unsafe {
        ALLOC_SELFTEST_OK = true;
    }
    Ok(())
}

#[cfg(test)]
#[path = "frame_allocator_test.rs"]
mod frame_allocator_test;
