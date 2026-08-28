//! Physical frame bump pool for early HV use (post-UEFI handoff).
//!
//! Pillar: [Z]
//! Proven Core: **outside** for this bump allocator (the real frame allocator
//! in `memory/` is Proven Core — this is boot scaffolding only).
//!
//! VERIFICATION: N/A

/// 4 KiB page size (x86 / UEFI).
pub const PAGE_SIZE: u64 = 4096;

/// Physical address (host).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhysAddr(pub u64);

/// Inclusive bump allocator over one contiguous conventional-memory span.
///
/// INVARIANTS:
///   - `next` is always page-aligned and `next <= end`
///   - Frames returned were in `[base, end)` and advance `next` by PAGE_SIZE
#[derive(Debug, Clone, Copy)]
pub struct FrameBump {
    base: u64,
    next: u64,
    end: u64,
}

impl FrameBump {
    /// Create a bump pool over `[start, start + pages * PAGE_SIZE)`.
    ///
    /// `start` is rounded up to a page boundary; empty if no full page fits.
    pub const fn new(start: u64, pages: u64) -> Self {
        let aligned = (start + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);
        let end = start.saturating_add(pages.saturating_mul(PAGE_SIZE));
        if aligned >= end {
            Self {
                base: aligned,
                next: aligned,
                end: aligned,
            }
        } else {
            Self {
                base: aligned,
                next: aligned,
                end,
            }
        }
    }

    pub const fn base(&self) -> PhysAddr {
        PhysAddr(self.base)
    }

    pub const fn remaining_pages(&self) -> u64 {
        self.end.saturating_sub(self.next) / PAGE_SIZE
    }

    pub const fn capacity_pages(&self) -> u64 {
        self.end.saturating_sub(self.base) / PAGE_SIZE
    }

    /// Allocate one 4K frame. Returns physical address of the frame.
    pub fn alloc_frame(&mut self) -> Option<PhysAddr> {
        if self.next + PAGE_SIZE > self.end {
            return None;
        }
        let frame = self.next;
        self.next += PAGE_SIZE;
        Some(PhysAddr(frame))
    }

    /// Take the entire remaining contiguous span as `(start_phys, page_count)`.
    ///
    /// Empties the bump. Used when promoting the pool into the Proven Core
    /// frame allocator (`memory::frame_allocator`).
    pub fn take_remaining(&mut self) -> Option<(u64, u64)> {
        let pages = self.remaining_pages();
        if pages == 0 {
            return None;
        }
        let start = self.next;
        self.next = self.end;
        Some((start, pages))
    }
}

/// Choose a conventional-memory region suitable for the early HV pool.
///
/// Prefers spans at or above 1 MiB with at least `min_pages` pages.
/// Returns `(phys_start, page_count)` or `None`.
///
/// When `prefer_end` is non-zero, **prefer** a pool clipped into
/// `[1 MiB, prefer_end)` (identity EPT window on iron / QEMU). Only if no
/// clipped candidate fits do we fall back to the largest region above 1 MiB
/// (which breaks identity EPT on large-RAM R640 — caller should log that).
pub fn pick_conventional_region(
    regions: &[(u64, u64)],
    min_pages: u64,
) -> Option<(u64, u64)> {
    pick_conventional_region_prefer(regions, min_pages, 0)
}

/// Like [`pick_conventional_region`], but prefer pools below `prefer_end`.
///
/// `prefer_end == 0` disables the preference (legacy: largest above 1 MiB).
pub fn pick_conventional_region_prefer(
    regions: &[(u64, u64)],
    min_pages: u64,
    prefer_end: u64,
) -> Option<(u64, u64)> {
    const ONE_MIB: u64 = 1024 * 1024;
    let mut best_pref: Option<(u64, u64)> = None;
    let mut best_any: Option<(u64, u64)> = None;

    for &(start, pages) in regions {
        let end = start.saturating_add(pages.saturating_mul(PAGE_SIZE));
        let usable_start = core::cmp::max(start, ONE_MIB);
        if usable_start >= end {
            continue;
        }
        let usable_pages = (end - usable_start) / PAGE_SIZE;
        if usable_pages >= min_pages {
            match best_any {
                None => best_any = Some((usable_start, usable_pages)),
                Some((_, best_pages)) if usable_pages > best_pages => {
                    best_any = Some((usable_start, usable_pages));
                }
                _ => {}
            }
        }

        if prefer_end > ONE_MIB {
            let clip_end = core::cmp::min(end, prefer_end);
            if usable_start < clip_end {
                let clip_pages = (clip_end - usable_start) / PAGE_SIZE;
                if clip_pages >= min_pages {
                    match best_pref {
                        None => best_pref = Some((usable_start, clip_pages)),
                        Some((_, best_pages)) if clip_pages > best_pages => {
                            best_pref = Some((usable_start, clip_pages));
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    best_pref.or(best_any)
}

/// Largest conventional span whose usable start is at or above `above`.
///
/// Stage 46 iron product-ISO: CMOS reports 2 GiB LowMemory but the HV pool
/// stays in `[1MiB, PRECISE)` so E4 identity is unchanged. Leftover
/// conventional DRAM above PRECISE backs guest-UEFI report-RAM (GPA≠HPA,
/// ADR-004). Nested / `iso=0` do not call this.
pub fn pick_conventional_region_above(
    regions: &[(u64, u64)],
    min_pages: u64,
    above: u64,
) -> Option<(u64, u64)> {
    const ONE_MIB: u64 = 1024 * 1024;
    let floor = core::cmp::max(above, ONE_MIB);
    let mut best: Option<(u64, u64)> = None;
    for &(start, pages) in regions {
        let end = start.saturating_add(pages.saturating_mul(PAGE_SIZE));
        let usable_start = core::cmp::max(start, floor);
        if usable_start >= end {
            continue;
        }
        let usable_pages = (end - usable_start) / PAGE_SIZE;
        if usable_pages < min_pages {
            continue;
        }
        match best {
            None => best = Some((usable_start, usable_pages)),
            Some((_, best_pages)) if usable_pages > best_pages => {
                best = Some((usable_start, usable_pages));
            }
            _ => {}
        }
    }
    best
}

/// Sum of conventional pages whose usable start is at or above `above`.
pub fn conventional_pages_above(regions: &[(u64, u64)], above: u64) -> u64 {
    const ONE_MIB: u64 = 1024 * 1024;
    let floor = core::cmp::max(above, ONE_MIB);
    let mut n = 0u64;
    for &(start, pages) in regions {
        let end = start.saturating_add(pages.saturating_mul(PAGE_SIZE));
        let usable_start = core::cmp::max(start, floor);
        if usable_start >= end {
            continue;
        }
        n = n.saturating_add((end - usable_start) / PAGE_SIZE);
    }
    n
}

#[cfg(test)]
#[path = "mem_test.rs"]
mod mem_test;
