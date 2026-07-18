//! Linux x86_64 boot-protocol packing + synthetic load (M3.2).
//!
//! Pillar: [Z]
//! Proven Core: **outside** (ADR-002) — protocol glue only; frames claimed
//! via Proven Core `FrameAllocator` + `EptMap`.
//!
//! M3.2 places a synthetic bzImage stub, initrd stub, cmdline, and packed
//! `boot_params` (zero page) into GPA. Does **not** enter the kernel (M3.3).

use crate::memory::ept::{EptError, EptMap, EptPermissions, M2_BRINGUP_GUEST_ID};
use crate::memory::{FrameAllocator, PhysFrame};

/// COM1 marker when synthetic load + packing succeeds (M3.2 gate).
pub const M3_LOAD_OK_MARKER: &str = "RAYNU-V-M3-LOAD-OK";

/// `setup_header.header` magic — ASCII `"HdrS"` (LE).
pub const SETUP_HEADER_MAGIC: u32 = 0x5372_6448;

/// boot_params / zero-page size.
pub const BOOT_PARAMS_SIZE: usize = 4096;

/// Absolute offsets into `boot_params` (Linux `struct boot_params`).
pub const OFF_E820_ENTRIES: usize = 0x1E8;
pub const OFF_SETUP_SECTS: usize = 0x1F1;
pub const OFF_BOOT_FLAG: usize = 0x1FE;
pub const OFF_HEADER: usize = 0x202;
pub const OFF_VERSION: usize = 0x206;
pub const OFF_TYPE_OF_LOADER: usize = 0x210;
pub const OFF_LOADFLAGS: usize = 0x211;
pub const OFF_RAMDISK_IMAGE: usize = 0x218;
pub const OFF_RAMDISK_SIZE: usize = 0x21C;
pub const OFF_CMD_LINE_PTR: usize = 0x228;
pub const OFF_E820_TABLE: usize = 0x2D0;

/// `loadflags` bit 0 — `LOADED_HIGH` (kernel above 1 MiB).
pub const LOADFLAGS_LOADED_HIGH: u8 = 1 << 0;

/// Synthetic kernel stub size (one page; not a real bzImage).
pub const SYNTH_KERNEL_SIZE: usize = 4096;
/// Synthetic initrd stub size.
pub const SYNTH_INITRD_SIZE: usize = 4096;

/// Default cmdline for the synthetic load (NUL-terminated in guest memory).
pub const DEFAULT_CMDLINE: &[u8] =
    b"earlyprintk=serial,ttyS0,115200 console=ttyS0,115200 acpi=off nokaslr maxcpus=1\0";

/// e820 type: usable RAM.
pub const E820_RAM: u32 = 1;

/// Result of placing synthetic kernel / initrd / boot_params in GPA.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BootLoadInfo {
    pub boot_params_phys: u64,
    pub kernel_phys: u64,
    pub initrd_phys: u64,
    pub cmdline_phys: u64,
    pub setup_magic: u32,
    pub ramdisk_image: u32,
    pub ramdisk_size: u32,
    pub cmd_line_ptr: u32,
}

/// Pack a Linux `boot_params` zero page.
///
/// Writes setup header magic, version, loader type, ramdisk ptrs, cmdline
/// pointer, and a single e820 usable entry covering `[0, mem_bytes)`.
pub fn pack_boot_params(
    buf: &mut [u8; BOOT_PARAMS_SIZE],
    ramdisk_image: u32,
    ramdisk_size: u32,
    cmd_line_ptr: u32,
    mem_bytes: u64,
) {
    buf.fill(0);

    buf[OFF_SETUP_SECTS] = 0;
    write_u16(buf, OFF_BOOT_FLAG, 0xAA55);
    write_u32(buf, OFF_HEADER, SETUP_HEADER_MAGIC);
    write_u16(buf, OFF_VERSION, 0x020C); // protocol 2.12
    buf[OFF_TYPE_OF_LOADER] = 0xFF; // undefined / custom
    buf[OFF_LOADFLAGS] = LOADFLAGS_LOADED_HIGH;
    write_u32(buf, OFF_RAMDISK_IMAGE, ramdisk_image);
    write_u32(buf, OFF_RAMDISK_SIZE, ramdisk_size);
    write_u32(buf, OFF_CMD_LINE_PTR, cmd_line_ptr);

    // One e820 RAM entry for the bring-up window.
    buf[OFF_E820_ENTRIES] = 1;
    write_u64(buf, OFF_E820_TABLE, 0);
    write_u64(buf, OFF_E820_TABLE + 8, mem_bytes);
    write_u32(buf, OFF_E820_TABLE + 16, E820_RAM);
}

/// Read back setup header magic from a packed buffer (host verify / serial).
pub fn setup_magic(buf: &[u8; BOOT_PARAMS_SIZE]) -> u32 {
    read_u32(buf, OFF_HEADER)
}

/// Claim load pages in an ADR-004 ownership registry (identity GPA=HPA).
pub fn claim_load_pages(pages: &[(u64, PhysFrame)]) -> Result<(), EptError> {
    let mut map = EptMap::new();
    for &(gpa, frame) in pages {
        map.map(
            M2_BRINGUP_GUEST_ID,
            gpa,
            frame,
            EptPermissions::READ_WRITE_EXECUTE,
        )?;
    }
    if !map.check_invariants() || map.len() != pages.len() {
        return Err(EptError::Invariant);
    }
    Ok(())
}

/// Allocate frames, pack synthetic assets, claim ownership, copy into GPA.
///
/// Layout (identity-mapped):
/// - `kernel` — 1 page synthetic stub (magic dword at +0)
/// - `initrd` — 1 page synthetic stub
/// - `cmdline` — 1 page with [`DEFAULT_CMDLINE`]
/// - `boot_params` — packed zero page
pub fn load_synthetic_guest(alloc: &mut FrameAllocator) -> Result<BootLoadInfo, ()> {
    let kernel = alloc.allocate_frame().ok_or(())?;
    let initrd = alloc.allocate_frame().ok_or(())?;
    let cmdline = alloc.allocate_frame().ok_or(())?;
    let boot_params = alloc.allocate_frame().ok_or(())?;

    let kernel_phys = kernel.to_phys();
    let initrd_phys = initrd.to_phys();
    let cmdline_phys = cmdline.to_phys();
    let boot_params_phys = boot_params.to_phys();

    claim_load_pages(&[
        (kernel_phys, kernel),
        (initrd_phys, initrd),
        (cmdline_phys, cmdline),
        (boot_params_phys, boot_params),
    ])
    .map_err(|_| ())?;

    // SAFETY: freshly allocated frames; identity-mapped by UEFI + EPT.
    unsafe {
        write_synth_kernel(kernel_phys);
        write_synth_initrd(initrd_phys);
        write_cmdline(cmdline_phys);
    }

    let mut bp = [0u8; BOOT_PARAMS_SIZE];
    pack_boot_params(
        &mut bp,
        initrd_phys as u32,
        SYNTH_INITRD_SIZE as u32,
        cmdline_phys as u32,
        64 * 1024 * 1024, // 64 MiB e820 window for bring-up
    );

    // SAFETY: owned boot_params frame.
    unsafe {
        core::ptr::copy_nonoverlapping(bp.as_ptr(), boot_params_phys as *mut u8, BOOT_PARAMS_SIZE);
    }

    let magic = setup_magic(&bp);
    if magic != SETUP_HEADER_MAGIC {
        return Err(());
    }

    Ok(BootLoadInfo {
        boot_params_phys,
        kernel_phys,
        initrd_phys,
        cmdline_phys,
        setup_magic: magic,
        ramdisk_image: initrd_phys as u32,
        ramdisk_size: SYNTH_INITRD_SIZE as u32,
        cmd_line_ptr: cmdline_phys as u32,
    })
}

/// ASCII-ish stub marker at the start of the synthetic kernel page (`M32KRNL`).
pub const SYNTH_KERNEL_MAGIC: u64 = 0x4D33_324B_524E_4C00;

unsafe fn write_synth_kernel(phys: u64) {
    core::ptr::write_bytes(phys as *mut u8, 0, SYNTH_KERNEL_SIZE);
    core::ptr::write_unaligned(phys as *mut u64, SYNTH_KERNEL_MAGIC);
}

unsafe fn write_synth_initrd(phys: u64) {
    core::ptr::write_bytes(phys as *mut u8, 0, SYNTH_INITRD_SIZE);
    // Minimal "cpio" lookalike tag for debug (not a real archive).
    let tag = b"RAYNUINITRD";
    core::ptr::copy_nonoverlapping(tag.as_ptr(), phys as *mut u8, tag.len());
}

unsafe fn write_cmdline(phys: u64) {
    core::ptr::write_bytes(phys as *mut u8, 0, 4096);
    core::ptr::copy_nonoverlapping(DEFAULT_CMDLINE.as_ptr(), phys as *mut u8, DEFAULT_CMDLINE.len());
}

fn write_u16(buf: &mut [u8], off: usize, v: u16) {
    buf[off] = v as u8;
    buf[off + 1] = (v >> 8) as u8;
}

fn write_u32(buf: &mut [u8], off: usize, v: u32) {
    for i in 0..4 {
        buf[off + i] = (v >> (8 * i)) as u8;
    }
}

fn write_u64(buf: &mut [u8], off: usize, v: u64) {
    for i in 0..8 {
        buf[off + i] = (v >> (8 * i)) as u8;
    }
}

fn read_u32(buf: &[u8], off: usize) -> u32 {
    let mut v = 0u32;
    for i in 0..4 {
        v |= (buf[off + i] as u32) << (8 * i);
    }
    v
}

#[cfg(test)]
#[path = "linux_boot_test.rs"]
mod linux_boot_test;
