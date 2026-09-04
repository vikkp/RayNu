//! RayNu-F memory services (UEFI 2.10 §7.2): `AllocatePages`, `FreePages`,
//! `AllocatePool`, `FreePool`, `GetMemoryMap`, `ExitBootServices`.
//!
//! Pillar: [Z] · Proven Core: **outside** (ADR-016)
//!
//! The guest firmware pool is a fixed window of the 32 MiB identity slab
//! managed host-side with a page bitmap + per-page type byte. Everything the
//! guest can hand back to us (addresses, map keys) is validated against that
//! window. Pool allocations are page-granular with a 16-byte header — simple
//! and correct; a sub-page pool is a later refinement if a loader needs it.

use super::launch_plan::{F2_APP_LOAD_BASE, F2_STACK_TOP, F2_TABLES_BASE};
use super::tables::IMAGE_BYTES;
use super::testapp::TESTAPP_SIZE_OF_IMAGE;

/// Guest-firmware pool window inside the slab.
pub const POOL_BASE: u64 = 0x00B0_0000;
pub const POOL_END: u64 = 0x01F0_0000;
pub const POOL_PAGES: usize = ((POOL_END - POOL_BASE) / 4096) as usize; // 5120
const BITMAP_WORDS: usize = POOL_PAGES / 64;

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
/// Upper bound on descriptors we will emit (coalesced runs; typical < 20).
pub const MAX_DESCRIPTORS: usize = 128;

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

/// Host-side page allocator over the pool window.
#[derive(Clone)]
pub struct PagePool {
    used: [u64; BITMAP_WORDS],
    typ: [u8; POOL_PAGES],
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
            map_key: 0,
            exited: false,
            allocs: 0,
        }
    }

    #[inline]
    fn is_used(&self, i: usize) -> bool {
        (self.used[i / 64] >> (i % 64)) & 1 == 1
    }
    #[inline]
    fn set_used(&mut self, i: usize, on: bool) {
        if on {
            self.used[i / 64] |= 1u64 << (i % 64);
        } else {
            self.used[i / 64] &= !(1u64 << (i % 64));
        }
    }

    pub fn free_pages(&self) -> usize {
        POOL_PAGES - self.used.iter().map(|w| w.count_ones() as usize).sum::<usize>()
    }

    pub fn exited(&self) -> bool {
        self.exited
    }

    pub fn map_key(&self) -> u64 {
        self.map_key
    }

    fn page_of(addr: u64) -> Option<usize> {
        if addr < POOL_BASE || addr >= POOL_END || addr & 0xfff != 0 {
            return None;
        }
        Some(((addr - POOL_BASE) / 4096) as usize)
    }

    fn find_run(&self, pages: usize, max_end_page: usize) -> Option<usize> {
        let mut run = 0usize;
        let limit = max_end_page.min(POOL_PAGES);
        for i in 0..limit {
            if self.is_used(i) {
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

    fn mark(&mut self, first: usize, pages: usize, typ: u32) {
        for i in first..first + pages {
            self.set_used(i, true);
            self.typ[i] = typ as u8;
        }
    }

    /// `AllocatePages`. `memory` is the in/out `*Memory` value. Returns
    /// `(status, new_memory)`.
    pub fn allocate_pages(
        &mut self,
        alloc_type: u32,
        mem_type: u32,
        pages: u64,
        memory: u64,
    ) -> (u64, u64) {
        if pages == 0 || pages as usize > POOL_PAGES {
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
        let first = match alloc_type {
            ALLOCATE_ANY_PAGES => self.find_run(n, POOL_PAGES),
            ALLOCATE_MAX_ADDRESS => {
                // Highest allowed *end* address is `memory`.
                if memory < POOL_BASE {
                    return (EFI_NOT_FOUND, memory);
                }
                let end_page = ((memory.saturating_add(1) - POOL_BASE) / 4096) as usize;
                self.find_run(n, end_page)
            }
            ALLOCATE_ADDRESS => {
                let Some(p) = Self::page_of(memory) else {
                    return (EFI_NOT_FOUND, memory);
                };
                if p + n > POOL_PAGES {
                    return (EFI_NOT_FOUND, memory);
                }
                if (p..p + n).any(|i| self.is_used(i)) {
                    return (EFI_NOT_FOUND, memory);
                }
                Some(p)
            }
            _ => return (EFI_INVALID_PARAMETER, memory),
        };
        let Some(first) = first else {
            return (EFI_OUT_OF_RESOURCES, memory);
        };
        self.mark(first, n, mem_type);
        self.allocs = self.allocs.saturating_add(1);
        (EFI_SUCCESS, POOL_BASE + (first as u64) * 4096)
    }

    /// `FreePages`.
    pub fn free_pages_at(&mut self, memory: u64, pages: u64) -> u64 {
        let Some(p) = Self::page_of(memory) else {
            return EFI_NOT_FOUND;
        };
        let n = pages as usize;
        if n == 0 || p + n > POOL_PAGES || (p..p + n).any(|i| !self.is_used(i)) {
            return EFI_NOT_FOUND;
        }
        for i in p..p + n {
            self.set_used(i, false);
            self.typ[i] = 0;
        }
        EFI_SUCCESS
    }

    /// Pages needed for an `AllocatePool(size)` including the header.
    pub const fn pool_pages_for(size: u64) -> u64 {
        (size.saturating_add(POOL_HEADER_BYTES) + 4095) / 4096
    }

    /// Coalesced memory map of the whole slab, in address order. `slab_bytes`
    /// bounds the final reserved run. Returns the number of runs written.
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
        // stack+GDT). PTs and GDT are live for the guest: Reserved.
        push(EFI_RESERVED_MEMORY_TYPE, 0, F2_TABLES_BASE);
        push(
            EFI_BOOT_SERVICES_DATA,
            F2_TABLES_BASE,
            F2_TABLES_BASE + IMAGE_BYTES as u64,
        );
        push(
            EFI_CONVENTIONAL_MEMORY,
            F2_TABLES_BASE + IMAGE_BYTES as u64,
            F2_APP_LOAD_BASE,
        );
        push(
            EFI_LOADER_CODE,
            F2_APP_LOAD_BASE,
            F2_APP_LOAD_BASE + TESTAPP_SIZE_OF_IMAGE as u64,
        );
        push(
            EFI_CONVENTIONAL_MEMORY,
            F2_APP_LOAD_BASE + TESTAPP_SIZE_OF_IMAGE as u64,
            F2_STACK_TOP - 0x1_0000,
        );
        push(EFI_RESERVED_MEMORY_TYPE, F2_STACK_TOP - 0x1_0000, F2_STACK_TOP + 0x1000);
        push(EFI_CONVENTIONAL_MEMORY, F2_STACK_TOP + 0x1000, POOL_BASE);
        // Pool: walk pages, emitting runs by (used, type).
        let mut i = 0usize;
        while i < POOL_PAGES {
            let used = self.is_used(i);
            let t = if used { self.typ[i] as u32 } else { EFI_CONVENTIONAL_MEMORY };
            let mut j = i + 1;
            while j < POOL_PAGES {
                let u2 = self.is_used(j);
                let t2 = if u2 { self.typ[j] as u32 } else { EFI_CONVENTIONAL_MEMORY };
                if u2 != used || t2 != t {
                    break;
                }
                j += 1;
            }
            push(
                t,
                POOL_BASE + (i as u64) * 4096,
                POOL_BASE + (j as u64) * 4096,
            );
            i = j;
        }
        push(EFI_RESERVED_MEMORY_TYPE, POOL_END, slab_bytes);
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
