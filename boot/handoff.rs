//! UEFI → bare-metal handoff (ExitBootServices).
//!
//! Pillar: [Z]
//! Proven Core: **outside** (ADR-002)
//!
//! After this path, boot services / UEFI alloc / ConOut are gone.
//! COM1 (port I/O) and the firmware page tables remain usable for M1.0.
//! Building HV-owned page tables is deferred until M1.1 prep if needed;
//! OVMF identity maps remain valid for QEMU bring-up.

use crate::boot::mem;
use core::sync::atomic::{AtomicU64, Ordering};

#[cfg(target_os = "uefi")]
use crate::boot::serial;
#[cfg(target_os = "uefi")]
use uefi::boot;
#[cfg(target_os = "uefi")]
use uefi::mem::memory_map::{MemoryMap, MemoryType};

/// Distinctive M1.0 gate marker — must appear on COM1 *after* ExitBootServices.
pub const M1_EBS_OK_MARKER: &str = "RAYNU-V-M1-EBS-OK";

/// Cap leftover DRAM taken for guest-UEFI report-RAM (2 GiB CMOS lie).
/// Does not expand [`crate::memory::PRECISE_BYTES`]. Not invented HPA (ADR-004).
pub const REPORT_RAM_EXTRA_MAX_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const REPORT_RAM_EXTRA_2M: u64 = 2 * 1024 * 1024;

static REPORT_RAM_EXTRA_NEXT: AtomicU64 = AtomicU64::new(0);
static REPORT_RAM_EXTRA_END: AtomicU64 = AtomicU64::new(0);

/// Seed a 2 MiB-aligned bump from unused conventional DRAM above PRECISE.
///
/// Host CR3 is still the UEFI identity map, so these HPAs are reachable
/// without expanding the 512 MiB precise window. Nested / `iso=0` leave
/// this empty.
/// Returns the 2 MiB-aligned HPA, or 0 if the span cannot yield a frame.
pub fn seed_report_ram_extra(start: u64, bytes: u64) -> u64 {
    let aligned = start.saturating_add(REPORT_RAM_EXTRA_2M - 1) & !(REPORT_RAM_EXTRA_2M - 1);
    let end = start.saturating_add(bytes);
    let cap_end = aligned.saturating_add(REPORT_RAM_EXTRA_MAX_BYTES);
    let use_end = core::cmp::min(end, cap_end);
    if aligned == 0 || aligned.saturating_add(REPORT_RAM_EXTRA_2M) > use_end {
        REPORT_RAM_EXTRA_NEXT.store(0, Ordering::Release);
        REPORT_RAM_EXTRA_END.store(0, Ordering::Release);
        return 0;
    }
    REPORT_RAM_EXTRA_NEXT.store(aligned, Ordering::Release);
    REPORT_RAM_EXTRA_END.store(use_end, Ordering::Release);
    aligned
}

/// Take one exclusive 2 MiB HPA from the leftover-DRAM bump, or `None`.
pub fn take_report_ram_extra_2m() -> Option<u64> {
    loop {
        let n = REPORT_RAM_EXTRA_NEXT.load(Ordering::Acquire);
        let end = REPORT_RAM_EXTRA_END.load(Ordering::Acquire);
        if n == 0 || n.saturating_add(REPORT_RAM_EXTRA_2M) > end {
            return None;
        }
        match REPORT_RAM_EXTRA_NEXT.compare_exchange(
            n,
            n + REPORT_RAM_EXTRA_2M,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => return Some(n),
            Err(_) => core::hint::spin_loop(),
        }
    }
}

/// Result of leaving UEFI boot services.
pub struct Handoff {
    /// Early bump pool carved from conventional memory.
    pub frames: mem::FrameBump,
    /// Number of conventional regions observed in the final map.
    pub conventional_regions: usize,
    /// Total conventional pages (≥1 MiB) seen before picking the pool.
    pub conventional_pages_1m: u64,
}

/// Exit boot services, seed the HV frame bump pool, prove COM1 still works.
///
/// # Safety
/// Caller must ensure no live UEFI protocol / pool references remain.
/// After return, do not use `uefi::println!`, boot services, or the global alloc.
#[cfg(target_os = "uefi")]
pub unsafe fn leave_firmware() -> Handoff {
    serial::write_line("boot: ExitBootServices — taking memory map ownership");

    // SAFETY: no outstanding boot-services references; COM1 is port I/O only.
    let mmap = unsafe { boot::exit_boot_services(MemoryType::LOADER_DATA) };

    // Firmware page tables remain active (UEFI identity map). We do not rebuild
    // them in M1.0; documenting that choice keeps the gate focused on EBS+serial.
    serial::write_line("boot: ExitBootServices returned; scanning conventional memory");

    let mut regions: [(u64, u64); 64] = [(0, 0); 64];
    let mut region_count = 0usize;
    let mut conventional_pages_1m = 0u64;

    for desc in mmap.entries() {
        if desc.ty != MemoryType::CONVENTIONAL {
            continue;
        }
        if region_count < regions.len() {
            regions[region_count] = (desc.phys_start, desc.page_count);
            region_count += 1;
        }
        let start = desc.phys_start;
        let end = start.saturating_add(desc.page_count.saturating_mul(mem::PAGE_SIZE));
        let usable = core::cmp::max(start, 1024 * 1024);
        if usable < end {
            conventional_pages_1m += (end - usable) / mem::PAGE_SIZE;
        }
    }

    serial::write_str("boot: conventional regions=");
    write_u64(region_count as u64);
    serial::write_str(" pages_above_1MiB=");
    write_u64(conventional_pages_1m);
    serial::write_byte(b'\n');

    // Identity EPT maps [0, PRECISE). Guest e820 is [0, GUEST_RAM). Virtio BAR
    // holes + shell slabs need free 2MiB leaves in [GUEST_RAM, PRECISE) that the
    // FrameAllocator does **not** own. Cap the HV pool at GUEST_RAM so that
    // window stays free (R640 previously filled pool to 512MiB → no BAR hole).
    // Stage 46 iron product-ISO holds (no E4 SHELL): prefer PRECISE so a
    // 256 MiB virtio-blk plus report-RAM fit. Nested / iso=0 stay GUEST_RAM.
    const MIN_PREF_PAGES: u64 = 256; // 1 MiB
    const MIN_POOL_PAGES: u64 = 16;
    let prefer_end = crate::mgmt::iso_install::product_iso_frame_pool_prefer_end(
        crate::mgmt::iso_install::product_iso_retained_bytes().is_some(),
        crate::arch::cpu::host_hypervisor_present(),
    );
    let precise_end = crate::memory::PRECISE_BYTES;
    let guest_ram = crate::guest::linux_boot::GUEST_RAM_BYTES;
    let (pool_start, pool_pages, in_window) = if let Some(p) =
        mem::pick_conventional_region_prefer(&regions[..region_count], MIN_PREF_PAGES, prefer_end)
    {
        (p.0, p.1, true)
    } else if let Some(p) =
        mem::pick_conventional_region_prefer(&regions[..region_count], MIN_POOL_PAGES, prefer_end)
    {
        (p.0, p.1, true)
    } else if let Some(p) =
        mem::pick_conventional_region_prefer(&regions[..region_count], MIN_POOL_PAGES, precise_end)
    {
        // Soft: still inside EPT, but may consume BAR/shell window.
        (p.0, p.1, true)
    } else if let Some(p) = mem::pick_conventional_region(&regions[..region_count], MIN_POOL_PAGES)
    {
        (p.0, p.1, false)
    } else {
        (0, 0, false)
    };

    let frames = if pool_pages > 0 {
        let pool_end = pool_start.saturating_add(pool_pages.saturating_mul(mem::PAGE_SIZE));
        if in_window && pool_end <= prefer_end && prefer_end > guest_ram {
            serial::write_line(
                "boot: frame pool product-ISO iron [1MiB,512MiB); Stage 46 hold (not E4 BAR/shell)",
            );
        } else if in_window && pool_end <= prefer_end {
            serial::write_line(
                "boot: frame pool clipped to guest RAM [1MiB,256MiB); BAR/shell window free",
            );
        } else if in_window && pool_end <= precise_end {
            serial::write_line(
                "boot: WARNING — frame pool into [256MiB,512MiB); BAR/shell may fail",
            );
        } else {
            serial::write_line(
                "boot: WARNING — frame pool outside precise EPT window; expect EPT faults",
            );
        }
        serial::write_str("boot: frame pool phys=0x");
        write_u64_hex(pool_start);
        serial::write_str(" pages=");
        write_u64(pool_pages);
        serial::write_byte(b'\n');
        mem::FrameBump::new(pool_start, pool_pages)
    } else {
        serial::write_line("boot: WARNING — no conventional pool ≥16 pages; empty bump");
        mem::FrameBump::new(0, 0)
    };

    if crate::mgmt::iso_install::product_iso_retained_bytes().is_some() {
        let above_pages =
            mem::conventional_pages_above(&regions[..region_count], precise_end);
        serial::write_str("boot: conventional above PRECISE pages=");
        write_u64(above_pages);
        serial::write_byte(b'\n');
        if crate::arch::cpu::host_hypervisor_present() {
            serial::write_line(
                "boot: report-RAM extra skip nested (Stage 46; not ISO-INSTALL-OK)",
            );
        } else if let Some((hs, hp)) =
            mem::pick_conventional_region_above(&regions[..region_count], 512, precise_end)
        {
            let bytes = hp.saturating_mul(mem::PAGE_SIZE);
            let extra_hpa = seed_report_ram_extra(hs, bytes);
            if extra_hpa != 0 {
                serial::write_str("boot: report-RAM extra hpa=0x");
                write_u64_hex(extra_hpa);
                serial::write_str(" bytes=");
                write_u64(bytes.min(REPORT_RAM_EXTRA_MAX_BYTES));
                serial::write_line(" (Stage 46; not ISO-INSTALL-OK)");
            } else {
                serial::write_line(
                    "boot: report-RAM extra skip align (Stage 46; not ISO-INSTALL-OK)",
                );
            }
        } else {
            serial::write_line(
                "boot: report-RAM extra skip none (Stage 46; not ISO-INSTALL-OK)",
            );
        }
    }

    // Prove COM1 works with boot services gone (M1.0 gate).
    serial::write_line(M1_EBS_OK_MARKER);

    // Smoke-allocate one frame so the pool is exercised (not required for gate).
    let mut frames = frames;
    if let Some(f) = frames.alloc_frame() {
        serial::write_str("boot: smoke frame phys=0x");
        write_u64_hex(f.0);
        serial::write_byte(b'\n');
    }

    Handoff {
        frames,
        conventional_regions: region_count,
        conventional_pages_1m,
    }
}

#[cfg(target_os = "uefi")]
fn write_u64(mut n: u64) {
    let mut buf = [0u8; 20];
    let mut i = buf.len();
    if n == 0 {
        serial::write_byte(b'0');
        return;
    }
    while n > 0 {
        i -= 1;
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
    }
    for &b in &buf[i..] {
        serial::write_byte(b);
    }
}

#[cfg(target_os = "uefi")]
fn write_u64_hex(mut n: u64) {
    let mut buf = [0u8; 16];
    let mut i = buf.len();
    if n == 0 {
        serial::write_byte(b'0');
        return;
    }
    while n > 0 {
        i -= 1;
        let d = (n & 0xf) as u8;
        buf[i] = if d < 10 { b'0' + d } else { b'a' + (d - 10) };
        n >>= 4;
    }
    for &b in &buf[i..] {
        serial::write_byte(b);
    }
}

#[cfg(test)]
mod handoff_test {
    use super::*;

    #[test]
    fn marker_stable() {
        assert_eq!(M1_EBS_OK_MARKER, "RAYNU-V-M1-EBS-OK");
    }

    #[test]
    fn extra_2m_bump_aligns_and_exhausts() {
        assert_eq!(
            seed_report_ram_extra(0x2000_1000, 8 * 1024 * 1024),
            0x2020_0000
        );
        let a = take_report_ram_extra_2m().expect("first");
        assert_eq!(a, 0x2020_0000);
        let b = take_report_ram_extra_2m().expect("second");
        assert_eq!(b, 0x2040_0000);
        let c = take_report_ram_extra_2m().expect("third");
        assert_eq!(c, 0x2060_0000);
        assert!(take_report_ram_extra_2m().is_none());
        seed_report_ram_extra(0, 0);
        assert!(take_report_ram_extra_2m().is_none());
    }
}
