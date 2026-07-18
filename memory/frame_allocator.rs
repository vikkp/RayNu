//! Physical frame allocator.
//!
//! Pillar: [V] · Proven Core · VERIFICATION: L0
//! Double-alloc / UAF are critical isolation failures (ADR-002).

/// Host-physical frame number (4K frames).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PhysFrame(pub u64);

/// Bitmap-backed stub allocator (software model for host tests).
///
/// INVARIANTS:
///   - A frame is in at most one of {free, allocated}
///   - `allocate` returns a frame that was free and is allocated after return
///   - `free` returns a frame that was allocated and is free after return
///
/// VERIFICATION: L0 — see frame_allocator_spec.rs
/// FALLBACK: L1 asserts planned; L3 targeted M2 (roadmap)
pub struct FrameAllocator {
    /// Bit i set => frame i allocated. Capacity fixed for scaffold tests.
    allocated: u64,
    capacity: u32,
}

impl FrameAllocator {
    /// Create an allocator with `capacity` frames (max 64 in this stub).
    pub fn new(capacity: u32) -> Self {
        assert!(capacity > 0 && capacity <= 64);
        Self {
            allocated: 0,
            capacity,
        }
    }

    /// Allocate a single physical frame.
    ///
    /// INVARIANTS:
    ///   - Returned frame was NOT previously allocated
    ///   - After return, frame IS in the allocated set
    ///   - No other frame's allocation status changed
    ///
    /// VERIFICATION: L0
    /// FALLBACK: L1 (runtime assert at entry + exit)
    pub fn allocate_frame(&mut self) -> Option<PhysFrame> {
        for i in 0..self.capacity {
            let bit = 1u64 << i;
            if self.allocated & bit == 0 {
                self.allocated |= bit;
                return Some(PhysFrame(i as u64));
            }
        }
        None
    }

    /// Free a previously allocated frame.
    ///
    /// INVARIANTS:
    ///   - Frame was allocated
    ///   - After return, frame is free
    ///
    /// VERIFICATION: L0
    pub fn free_frame(&mut self, frame: PhysFrame) -> bool {
        if frame.0 >= self.capacity as u64 {
            return false;
        }
        let bit = 1u64 << frame.0;
        if self.allocated & bit == 0 {
            return false;
        }
        self.allocated &= !bit;
        true
    }

    pub fn is_allocated(&self, frame: PhysFrame) -> bool {
        if frame.0 >= self.capacity as u64 {
            return false;
        }
        (self.allocated & (1u64 << frame.0)) != 0
    }
}

#[cfg(test)]
#[path = "frame_allocator_test.rs"]
mod frame_allocator_test;
