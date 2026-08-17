//! Dedicated management arena + restartable `MgmtFatal` (ADR-013 Phase E).
//!
//! Pillar: [Z] [V-architecture]
//! Proven Core: **outside** — this heap must never alias the frame allocator.
//!
//! On `MgmtFatal`: [`MgmtArena::reset`], drop in-flight sockets, listen again.
//! HV-wide `panic = abort` is unchanged. No `catch_unwind`.

/// Fixed mgmt heap (TCP/HTTP scratch). Distinct from `memory::FrameAllocator`.
pub const MGMT_ARENA_BYTES: usize = 64 * 1024;

/// Induced-fatal count used by the Phase E observable check.
pub const MGMT_FATAL_INJECT_N: u32 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MgmtFatal {
    Device,
    Bind,
    ArenaExhausted,
    /// Host/QEMU injected fatal (Phase E harness).
    Induced,
}

/// Bump arena. `reset` drops all allocations (listen restart).
pub struct MgmtArena {
    buf: [u8; MGMT_ARENA_BYTES],
    bump: usize,
    generation: u32,
}

impl MgmtArena {
    pub const fn new() -> Self {
        Self {
            buf: [0; MGMT_ARENA_BYTES],
            bump: 0,
            generation: 0,
        }
    }

    pub fn generation(&self) -> u32 {
        self.generation
    }

    pub fn used(&self) -> usize {
        self.bump
    }

    /// Zero and rewind. In-flight TCP/HTTP state is lost (acceptable for mgmt).
    pub fn reset(&mut self) {
        self.buf.fill(0);
        self.bump = 0;
        self.generation = self.generation.wrapping_add(1);
    }

    /// Allocate `n` bytes with `align` (power of two).
    pub fn alloc_bytes(&mut self, n: usize, align: usize) -> Result<&mut [u8], MgmtFatal> {
        if n == 0 || align == 0 || !align.is_power_of_two() {
            return Err(MgmtFatal::ArenaExhausted);
        }
        let mask = align - 1;
        let start = (self.bump + mask) & !mask;
        let end = start.checked_add(n).ok_or(MgmtFatal::ArenaExhausted)?;
        if end > MGMT_ARENA_BYTES {
            return Err(MgmtFatal::ArenaExhausted);
        }
        self.bump = end;
        Ok(&mut self.buf[start..end])
    }
}

/// Simulate N mgmt-loop fatals against `arena` without touching `alloc`.
///
/// Observable check (ADR-013 Appendix A): Proven Core `allocated_count` is
/// unchanged.
pub fn inject_mgmt_fatals(arena: &mut MgmtArena, n: u32) -> Result<(), MgmtFatal> {
    for _ in 0..n {
        let _ = arena.alloc_bytes(256, 16)?;
        let _ = MgmtFatal::Induced;
        arena.reset();
    }
    Ok(())
}

pub fn prop_arena_reset_rewinds() -> bool {
    let mut a = MgmtArena::new();
    if a.alloc_bytes(128, 8).is_err() {
        return false;
    }
    let g = a.generation();
    a.reset();
    a.generation() == g.wrapping_add(1) && a.used() == 0
}

#[cfg(test)]
#[path = "mgmt_arena_test.rs"]
mod mgmt_arena_test;
