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

#[cfg(target_os = "uefi")]
use crate::boot::serial;
#[cfg(target_os = "uefi")]
use uefi::boot;
#[cfg(target_os = "uefi")]
use uefi::mem::memory_map::{MemoryMap, MemoryType};

/// Distinctive M1.0 gate marker — must appear on COM1 *after* ExitBootServices.
pub const M1_EBS_OK_MARKER: &str = "RAYNU-V-M1-EBS-OK";

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

    let (pool_start, pool_pages) =
        mem::pick_conventional_region(&regions[..region_count], 16).unwrap_or((0, 0));

    let frames = if pool_pages > 0 {
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
}
