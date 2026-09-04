//! RayNu-F memory services (UEFI 2.10 §7.2): `AllocatePages`, `FreePages`,
//! `AllocatePool`, `FreePool`, `GetMemoryMap`, `ExitBootServices`.
//!
//! Pillar: [Z] · Proven Core: **outside** (ADR-016)
//!
//! Two page regions, both managed host-side with a page bitmap + per-page
//! type byte:
//!
//! * **low** — a fixed window of the 32 MiB identity slab ([`POOL_BASE`],
//!   [`POOL_END`]);
//! * **high** — the report-RAM the hypervisor already EPT-maps for the
//!   product-ISO guest above the slab (`[32 MiB, …)`), configured by the
//!   launcher from what is actually mapped ([`PagePool::set_high_region`]).
//!
//! Honesty rule for the memory map (learned from GRUB on nested `7ee3a3b`):
//! **every byte we advertise as `EfiConventionalMemory` must be allocatable
//! by `AllocatePages(AllocateAddress)`**, and there must be enough of it for
//! a real loader (GRUB's `mm_init` wants a 32 MiB heap and walks every
//! conventional descriptor with `AllocateAddress`). Slab bytes that the
//! allocator does not manage are therefore reported as firmware-owned, not
//! conventional.
//!
//! Everything the guest can hand back to us (addresses, map keys) is
//! validated against the two regions. Pool allocations are page-granular
//! with a 16-byte header — simple and correct; a sub-page pool is a later
//! refinement if a loader needs it.

use super::launch_plan::{F2_APP_LOAD_BASE, F2_STACK_TOP, F2_TABLES_BASE};
use super::tables::IMAGE_BYTES;
use super::testapp::TESTAPP_SIZE_OF_IMAGE;

/// Guest-firmware pool window inside the slab.
pub const POOL_BASE: u64 = 0x00B0_0000;
pub const POOL_END: u64 = 0x01F0_0000;
pub const POOL_PAGES: usize = ((POOL_END - POOL_BASE) / 4096) as usize; // 5120
const BITMAP_WORDS: usize = POOL_PAGES / 64;

/// Upper bound on the high region we manage: 256 MiB (65536 pages). More
/// report-RAM may be EPT-mapped; we only advertise what we can allocate.
/// Bitmap 8 KiB + type bytes 64 KiB live in `.bss` (the state is a static).
pub const HIGH_MAX_PAGES: usize = 65536;
const HIGH_BITMAP_WORDS: usize = HIGH_MAX_PAGES / 64;

/// `EFI_MEMORY_TYPE`.
pub const EFI_RESERVED_MEMORY_TYPE: u32 = 0;
pub const EFI_LOADER_CODE: u32 = 1;
pub const EFI_LOADER_DATA: u32 = 2;
pub const EFI_BOOT_SERVICES_CODE: u32 = 3;
pub const EFI_BOOT_SERVICES_DATA: u32 = 4;
pub const EFI_RUNTIME_SERVICES_CODE: u32 = 5;
pub const EFI_RUNTIME_SERVICES_DATA: u32 = 6;
pub const EFI_CONVENTIONAL_MEMORY: u32 = 7;
pub const EFI_ACPI_RECLAIM_MEMORY: u32 = 9;
pub const EFI_ACPI_MEMORY_NVS: u32 = 10;
pub const EFI_MAX_MEMORY_TYPE: u32 = 15;

/// `EFI_ALLOCATE_TYPE`.
pub const ALLOCATE_ANY_PAGES: u32 = 0;
pub const ALLOCATE_MAX_ADDRESS: u32 = 1;
pub const ALLOCATE_ADDRESS: u32 = 2;

/// `EFI_MEMORY_DESCRIPTOR` is 40 bytes; version 1.
pub const MEMORY_DESCRIPTOR_SIZE: u64 = 40;
pub const MEMORY_DESCRIPTOR_VERSION: u32 = 1;
/// `EFI_MEMORY_WB` attribute for RAM.
pub const EFI_MEMORY_WB: u64 = 0x8;
/// Upper bound on descriptors we will emit (coalesced runs). A loader that
/// fragments the high region can produce many; 256 × 40 B is a 10 KiB map.
pub const MAX_DESCRIPTORS: usize = 256;

/// Pool header: `[magic u64][pages u64]` immediately before the returned pointer.
pub const POOL_HEADER_BYTES: u64 = 16;
pub const POOL_MAGIC: u64 = 0x5246_4C4F_4F50_0001; // "POOL" tagged 'RF'

/// `EFI_STATUS` values used here.
pub const EFI_SUCCESS: u64 = 0;
pub const EFI_INVALID_PARAMETER: u64 = 0x8000_0000_0000_0002;
pub const EFI_BUFFER_TOO_SMALL: u64 = 0x8000_0000_0000_0005;
pub const EFI_NOT_FOUND: u64 = 0x8000_0000_0000_000E;
pub const EFI_OUT_OF_RESOURCES: u64 = 0x8000_0000_0000_0009;

/// One coalesced run for the memory map.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemRun {
    pub typ: u32,
    pub start: u64,
    pub pages: u64,
}

/// Which managed region a page index refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Region {
    Low,
    High,
}

/// Host-side page allocator over the low pool window plus the optional high
/// report-RAM region.
#[derive(Clone)]
pub struct PagePool {
    used: [u64; BITMAP_WORDS],
    typ: [u8; POOL_PAGES],
    /// High region: base GPA and live page count (0 = not configured).
    high_base: u64,
    high_pages: usize,
    high_used: [u64; HIGH_BITMAP_WORDS],
    high_typ: [u8; HIGH_MAX_PAGES],
    /// Incremented on every `GetMemoryMap`; `ExitBootServices` must match.
    map_key: u64,
    /// Set once `ExitBootServices` succeeded.
    exited: bool,
    /// Successful `AllocatePages` calls (host marker bookkeeping).
    pub allocs: u32,
}

impl PagePool {
    pub const fn new() -> Self {
        PagePool {
            used: [0; BITMAP_WORDS],
            typ: [0; POOL_PAGES],
            high_base: 0,
            high_pages: 0,
            high_used: [0; HIGH_BITMAP_WORDS],
            high_typ: [0; HIGH_MAX_PAGES],
            map_key: 0,
            exited: false,
            allocs: 0,
        }
    }

    /// Configure the high region from what the hypervisor has EPT-mapped.
    /// `base` must be page-aligned and at or above [`POOL_END`]; `pages` is
    /// clamped to [`HIGH_MAX_PAGES`]. Returns the page count in effect.
    /// Only legal before any high allocation (launcher, pre-VMLAUNCH).
    pub fn set_high_region(&mut self, base: u64, pages: usize) -> usize {
        if base & 0xfff != 0 || base < POOL_END || pages == 0 {
            self.high_base = 0;
            self.high_pages = 0;
            return 0;
        }
        let n = pages.min(HIGH_MAX_PAGES);
        // Never let the region wrap or run past a 52-bit physical space.
        let Some(end) = base.checked_add((n as u64) * 4096) else {
            self.high_base = 0;
            self.high_pages = 0;
            return 0;
        };
        if end > (1u64 << 52) {
            self.high_base = 0;
            self.high_pages = 0;
            return 0;
        }
        self.high_base = base;
        self.high_pages = n;
        n
    }

    pub fn high_region(&self) -> Option<(u64, usize)> {
        if self.high_pages == 0 {
            None
        } else {
            Some((self.high_base, self.high_pages))
        }
    }

    fn pages_in(&self, r: Region) -> usize {
        match r {
            Region::Low => POOL_PAGES,
            Region::High => self.high_pages,
        }
    }

    fn base_of(&self, r: Region) -> u64 {
        match r {
            Region::Low => POOL_BASE,
            Region::High => self.high_base,
        }
    }

    #[inline]
    fn is_used(&self, r: Region, i: usize) -> bool {
        let w = match r {
            Region::Low => self.used[i / 64],
            Region::High => self.high_used[i / 64],
        };
        (w >> (i % 64)) & 1 == 1
    }
    #[inline]
    fn set_used(&mut self, r: Region, i: usize, on: bool) {
        let w = match r {
            Region::Low => &mut self.used[i / 64],
            Region::High => &mut self.high_used[i / 64],
        };
        if on {
            *w |= 1u64 << (i % 64);
        } else {
            *w &= !(1u64 << (i % 64));
        }
    }
    #[inline]
    fn typ_of(&self, r: Region, i: usize) -> u32 {
        u32::from(match r {
            Region::Low => self.typ[i],
            Region::High => self.high_typ[i],
        })
    }
    #[inline]
    fn set_typ(&mut self, r: Region, i: usize, t: u32) {
        match r {
            Region::Low => self.typ[i] = t as u8,
            Region::High => self.high_typ[i] = t as u8,
        }
    }

    fn used_in(&self, r: Region) -> usize {
        match r {
            Region::Low => self.used.iter().map(|w| w.count_ones() as usize).sum(),
            Region::High => self.high_used.iter().map(|w| w.count_ones() as usize).sum(),
        }
    }

    /// Free pages across both regions.
    pub fn free_pages(&self) -> usize {
        POOL_PAGES - self.used_in(Region::Low) + self.high_pages - self.used_in(Region::High)
    }

    /// Free pages in the low slab pool only.
    pub fn free_low_pages(&self) -> usize {
        POOL_PAGES - self.used_in(Region::Low)
    }

    pub fn exited(&self) -> bool {
        self.exited
    }

    pub fn map_key(&self) -> u64 {
        self.map_key
    }

    /// Region and page index of a page-aligned managed address.
    fn page_of(&self, addr: u64) -> Option<(Region, usize)> {
        if addr & 0xfff != 0 {
            return None;
        }
        if (POOL_BASE..POOL_END).contains(&addr) {
            return Some((Region::Low, ((addr - POOL_BASE) / 4096) as usize));
        }
        if self.high_pages != 0 {
            let end = self.high_base + (self.high_pages as u64) * 4096;
            if (self.high_base..end).contains(&addr) {
                return Some((Region::High, ((addr - self.high_base) / 4096) as usize));
            }
        }
        None
    }

    /// First free run of `pages` in `r` ending at or before page `max_end_page`.
    fn find_run(&self, r: Region, pages: usize, max_end_page: usize) -> Option<usize> {
        let mut run = 0usize;
        let limit = max_end_page.min(self.pages_in(r));
        for i in 0..limit {
            if self.is_used(r, i) {
                run = 0;
            } else {
                run += 1;
                if run == pages {
                    return Some(i + 1 - pages);
                }
            }
        }
        None
    }

    /// Highest page-end index in `r` allowed by an inclusive max *end*
    /// address, or `None` if the whole region lies above it.
    fn end_page_for_max(&self, r: Region, max_addr: u64) -> Option<usize> {
        let base = self.base_of(r);
        if max_addr < base {
            return None;
        }
        // `max_addr` is the last byte the allocation may occupy.
        Some(((max_addr.saturating_add(1) - base) / 4096) as usize)
    }

    fn mark(&mut self, r: Region, first: usize, pages: usize, typ: u32) {
        for i in first..first + pages {
            self.set_used(r, i, true);
            self.set_typ(r, i, typ);
        }
    }

    /// `AllocatePages`. `memory` is the in/out `*Memory` value. Returns
    /// `(status, new_memory)`.
    ///
    /// `AllocateAnyPages` and `AllocateMaxAddress` try the low pool first so
    /// firmware-internal allocations stay in the slab; a loader that wants
    /// the big region gets it as soon as the slab pool cannot satisfy it.
    pub fn allocate_pages(
        &mut self,
        alloc_type: u32,
        mem_type: u32,
        pages: u64,
        memory: u64,
    ) -> (u64, u64) {
        if pages == 0 || pages as usize > POOL_PAGES.max(self.high_pages) {
            return (EFI_INVALID_PARAMETER, memory);
        }
        if mem_type >= EFI_MAX_MEMORY_TYPE
            || mem_type == EFI_CONVENTIONAL_MEMORY
            || mem_type == EFI_RESERVED_MEMORY_TYPE
        {
            // Spec: callers may not allocate Conventional/Reserved.
            return (EFI_INVALID_PARAMETER, memory);
        }
        let n = pages as usize;
        let regions = [Region::Low, Region::High];
        let found: Option<(Region, usize)> = match alloc_type {
            ALLOCATE_ANY_PAGES => regions
                .iter()
                .find_map(|&r| self.find_run(r, n, self.pages_in(r)).map(|p| (r, p))),
            ALLOCATE_MAX_ADDRESS => regions.iter().find_map(|&r| {
                let end = self.end_page_for_max(r, memory)?;
                self.find_run(r, n, end).map(|p| (r, p))
            }),
            ALLOCATE_ADDRESS => {
                let Some((r, p)) = self.page_of(memory) else {
                    return (EFI_NOT_FOUND, memory);
                };
                if p + n > self.pages_in(r) {
                    return (EFI_NOT_FOUND, memory);
                }
                if (p..p + n).any(|i| self.is_used(r, i)) {
                    return (EFI_NOT_FOUND, memory);
                }
                Some((r, p))
            }
            _ => return (EFI_INVALID_PARAMETER, memory),
        };
        let Some((r, first)) = found else {
            return if alloc_type == ALLOCATE_MAX_ADDRESS {
                (EFI_NOT_FOUND, memory)
            } else {
                (EFI_OUT_OF_RESOURCES, memory)
            };
        };
        self.mark(r, first, n, mem_type);
        self.allocs = self.allocs.saturating_add(1);
        (EFI_SUCCESS, self.base_of(r) + (first as u64) * 4096)
    }

    /// `FreePages`.
    pub fn free_pages_at(&mut self, memory: u64, pages: u64) -> u64 {
        let Some((r, p)) = self.page_of(memory) else {
            return EFI_NOT_FOUND;
        };
        let n = pages as usize;
        if n == 0 || p + n > self.pages_in(r) || (p..p + n).any(|i| !self.is_used(r, i)) {
            return EFI_NOT_FOUND;
        }
        for i in p..p + n {
            self.set_used(r, i, false);
            self.set_typ(r, i, 0);
        }
        EFI_SUCCESS
    }

    /// Pages needed for an `AllocatePool(size)` including the header.
    pub const fn pool_pages_for(size: u64) -> u64 {
        (size.saturating_add(POOL_HEADER_BYTES) + 4095) / 4096
    }

    /// Emit the (used, type) runs of one region through `push`.
    fn region_runs(&self, r: Region, push: &mut dyn FnMut(u32, u64, u64)) {
        let base = self.base_of(r);
        let total = self.pages_in(r);
        let mut i = 0usize;
        while i < total {
            let used = self.is_used(r, i);
            let t = if used { self.typ_of(r, i) } else { EFI_CONVENTIONAL_MEMORY };
            let mut j = i + 1;
            while j < total {
                let u2 = self.is_used(r, j);
                let t2 = if u2 { self.typ_of(r, j) } else { EFI_CONVENTIONAL_MEMORY };
                if u2 != used || t2 != t {
                    break;
                }
                j += 1;
            }
            push(t, base + (i as u64) * 4096, base + (j as u64) * 4096);
            i = j;
        }
    }

    /// Coalesced memory map in address order: the whole slab, then the high
    /// region if configured. `slab_bytes` bounds the final slab run. Returns
    /// the number of runs written.
    ///
    /// Slab bytes outside the pool window are firmware-owned
    /// (`EfiBootServicesData`) or `EfiReservedMemoryType` (live page tables,
    /// GDT) — never `EfiConventionalMemory`, because `AllocatePages` cannot
    /// hand them out (see module docs).
    pub fn memory_map(&self, slab_bytes: u64, out: &mut [MemRun; MAX_DESCRIPTORS]) -> usize {
        let mut n = 0usize;
        let mut push = |typ: u32, start: u64, end: u64| {
            if end <= start || n >= MAX_DESCRIPTORS {
                return;
            }
            let pages = (end - start) / 4096;
            if n > 0 && out[n - 1].typ == typ && out[n - 1].start + out[n - 1].pages * 4096 == start
            {
                out[n - 1].pages += pages;
            } else {
                out[n] = MemRun { typ, start, pages };
                n += 1;
            }
        };
        // Fixed windows below the pool (identity PTs, RayNu-F tables, app,
        // stack+GDT). PTs and GDT are live for the guest: Reserved. The
        // unused slack between them is ours, not the loader's.
        push(EFI_RESERVED_MEMORY_TYPE, 0, F2_TABLES_BASE);
        push(
            EFI_BOOT_SERVICES_DATA,
            F2_TABLES_BASE,
            F2_TABLES_BASE + IMAGE_BYTES as u64,
        );
        push(
            EFI_BOOT_SERVICES_DATA,
            F2_TABLES_BASE + IMAGE_BYTES as u64,
            F2_APP_LOAD_BASE,
        );
        push(
            EFI_LOADER_CODE,
            F2_APP_LOAD_BASE,
            F2_APP_LOAD_BASE + TESTAPP_SIZE_OF_IMAGE as u64,
        );
        push(
            EFI_BOOT_SERVICES_DATA,
            F2_APP_LOAD_BASE + TESTAPP_SIZE_OF_IMAGE as u64,
            F2_STACK_TOP - 0x1_0000,
        );
        push(EFI_RESERVED_MEMORY_TYPE, F2_STACK_TOP - 0x1_0000, F2_STACK_TOP + 0x1000);
        push(EFI_BOOT_SERVICES_DATA, F2_STACK_TOP + 0x1000, POOL_BASE);
        self.region_runs(Region::Low, &mut push);
        push(EFI_RESERVED_MEMORY_TYPE, POOL_END, slab_bytes);
        if self.high_pages != 0 {
            self.region_runs(Region::High, &mut push);
        }
        n
    }

    /// Advance and return the map key for a `GetMemoryMap` call.
    pub fn next_map_key(&mut self) -> u64 {
        self.map_key = self.map_key.wrapping_add(1);
        self.map_key
    }

    /// `ExitBootServices(ImageHandle, MapKey)`.
    pub fn exit_boot_services(&mut self, map_key: u64) -> u64 {
        if map_key == 0 || map_key != self.map_key {
            return EFI_INVALID_PARAMETER;
        }
        self.exited = true;
        EFI_SUCCESS
    }
}

/// Serialize one `EFI_MEMORY_DESCRIPTOR` (v1, 40 bytes).
pub fn encode_descriptor(run: &MemRun, out: &mut [u8; 40]) {
    out[0..4].copy_from_slice(&run.typ.to_le_bytes());
    out[4..8].copy_from_slice(&0u32.to_le_bytes());
    out[8..16].copy_from_slice(&run.start.to_le_bytes());
    out[16..24].copy_from_slice(&0u64.to_le_bytes()); // VirtualStart
    out[24..32].copy_from_slice(&run.pages.to_le_bytes());
    out[32..40].copy_from_slice(&EFI_MEMORY_WB.to_le_bytes());
}
