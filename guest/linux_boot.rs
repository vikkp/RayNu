//! Linux x86_64 boot-protocol packing + synthetic / bzImage load (M3.2–M3.7).
//!
//! Pillar: [Z]
//! Proven Core: **outside** (ADR-002) — protocol glue only; frames claimed
//! via Proven Core `FrameAllocator` + `EptMap`.
//!
//! M3.2 places a proto-kernel page, initrd stub, cmdline, and packed
//! `boot_params` into GPA. M3.3 enters at 64-bit with RSI=`boot_params`.
//! M3.7 loads a bzImage-shaped payload (ESP or embedded minimal) and jumps
//! to the 64-bit entry at PM+0x200.

use crate::devices::serial_pio::{GUEST_EARLY_MAGIC, GUEST_SHELL_MAGIC};
use crate::memory::ept::{EptError, EptMap, EptPermissions, M2_BRINGUP_GUEST_ID};
use crate::memory::{FrameAllocator, PhysFrame};

/// COM1 marker when synthetic load + packing succeeds (M3.2 gate).
pub const M3_LOAD_OK_MARKER: &str = "RAYNU-V-M3-LOAD-OK";

/// COM1 marker when bzImage parse + place succeeds (M3.7 gate).
pub const M3_BZIMAGE_OK_MARKER: &str = "RAYNU-V-M3-BZIMAGE-OK";

/// Linux-style early console line the proto-kernel OUTs before the magic.
pub const PROTO_EARLY_LINE: &[u8] = b"Linux version RayNu-V-proto (early console)\n";

/// Line the M3.5 proto-init OUTs before the shell magic.
pub const PROTO_SHELL_LINE: &[u8] = b"init: RayNu-V proto shell\n";

/// `setup_header.header` magic — ASCII `"HdrS"` (LE).
pub const SETUP_HEADER_MAGIC: u32 = 0x5372_6448;

/// boot_params / zero-page size.
pub const BOOT_PARAMS_SIZE: usize = 4096;

/// Absolute offsets into `boot_params` (Linux `struct boot_params`).
pub const OFF_EXT_MEM_K: usize = 0x02; // screen_info.ext_mem_k (BIOS-88 fallback)
pub const OFF_ALT_MEM_K: usize = 0x1E0; // e801-style extended mem (KB above 1 MiB)
pub const OFF_E820_ENTRIES: usize = 0x1E8;
pub const OFF_SENTINEL: usize = 0x1EF;
pub const OFF_SETUP_SECTS: usize = 0x1F1;
pub const OFF_BOOT_FLAG: usize = 0x1FE;
pub const OFF_HEADER: usize = 0x202;
pub const OFF_VERSION: usize = 0x206;
pub const OFF_TYPE_OF_LOADER: usize = 0x210;
pub const OFF_LOADFLAGS: usize = 0x211;
pub const OFF_RAMDISK_IMAGE: usize = 0x218;
pub const OFF_RAMDISK_SIZE: usize = 0x21C;
pub const OFF_CODE32_START: usize = 0x214;
pub const OFF_CMD_LINE_PTR: usize = 0x228;
pub const OFF_KERNEL_ALIGNMENT: usize = 0x230;
pub const OFF_RELOCATABLE_KERNEL: usize = 0x234;
pub const OFF_PREF_ADDRESS: usize = 0x258;
pub const OFF_INIT_SIZE: usize = 0x260;
pub const OFF_E820_TABLE: usize = 0x2D0;

/// Size of one `boot_e820_entry` (addr + size + type).
pub const E820_ENTRY_SIZE: usize = 20;

/// Low conventional RAM end (below EBDA / VGA hole), matching classic PC maps.
pub const E820_LOW_RAM_END: u64 = 0x9_FC00;
/// Start of extended RAM above the 1 MiB hole.
pub const E820_HIGH_RAM_START: u64 = 0x10_0000;

/// `loadflags` bit 0 — `LOADED_HIGH` (kernel above 1 MiB).
pub const LOADFLAGS_LOADED_HIGH: u8 = 1 << 0;

/// 64-bit Linux entry offset within the protected-mode kernel image.
pub const BZIMAGE_ENTRY_OFFSET: usize = 0x200;

/// Capacity of [`build_minimal_bzimage`] output.
pub const MINIMAL_BZIMAGE_CAP: usize = 8192;

/// Synthetic kernel stub size (one page; not a real bzImage).
pub const SYNTH_KERNEL_SIZE: usize = 4096;
/// Synthetic initrd stub size.
pub const SYNTH_INITRD_SIZE: usize = 4096;

/// Default cmdline for the synthetic load (NUL-terminated in guest memory).
pub const DEFAULT_CMDLINE: &[u8] =
    b"earlyprintk=serial,ttyS0,115200 console=ttyS0,115200 acpi=off nokaslr maxcpus=1\0";

/// Cmdline for real Linux + initrd (`rdinit=/init` → M3.10 shell marker).
///
/// - `memmap=` — RAM if zeropage e820 is ignored (`append_e820_table` needs ≥2)
/// - `nolapic noapic` — avoid guest touching host LAPIC via identity EPT
/// - `lpj=` / `no_timer_check` / `idle=poll` — skip PIT/TSC calibrate + HLT idle
pub const REAL_LINUX_CMDLINE: &[u8] = b"earlyprintk=serial,ttyS0,115200 console=ttyS0,115200 rdinit=/init acpi=off nolapic noapic nokaslr maxcpus=1 lpj=4194304 no_timer_check idle=poll memmap=640K@0 memmap=1023M@1M\0";

/// Max initrd pages for [`load_bzimage_guest`] (~256 KiB).
pub const INITRD_MAX_PAGES: usize = 64;

/// Guest RAM size advertised via e820 / memmap (match QEMU `-m`).
pub const GUEST_RAM_BYTES: u64 = 1024 * 1024 * 1024;

/// e820 type: usable RAM.
pub const E820_RAM: u32 = 1;
/// e820 type: reserved (VGA / EBDA hole).
pub const E820_RESERVED: u32 = 2;

/// Result of placing kernel / initrd / boot_params in GPA.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BootLoadInfo {
    pub boot_params_phys: u64,
    /// Physical base of the protected-mode kernel image.
    pub kernel_phys: u64,
    /// 64-bit entry RIP (`kernel_phys + 0x200` for bzImage; same as base for synth).
    pub entry_phys: u64,
    pub initrd_phys: u64,
    /// Executable proto-init (same frame as synthetic initrd for M3.5).
    pub init_phys: u64,
    pub cmdline_phys: u64,
    pub setup_magic: u32,
    pub ramdisk_image: u32,
    pub ramdisk_size: u32,
    pub cmd_line_ptr: u32,
    /// True when loaded from a bzImage-shaped payload (M3.7).
    pub from_bzimage: bool,
    /// True when the image looks like a real (non-fixture) Linux bzImage (M3.8).
    pub is_real_linux: bool,
    /// True when a real cpio/gzip initrd was placed (M3.10).
    pub has_real_initrd: bool,
    /// Bytes reserved for PM image + decompress workspace.
    pub kernel_bytes: u64,
}

/// Images larger than the minimal fixture are treated as real Linux.
pub const REAL_LINUX_MIN_BYTES: usize = 32 * 1024;

/// Max PM / decompress workspace pages for [`load_bzimage_guest`] (~18 MiB).
/// Real kernels need `init_size` plus up to `kernel_alignment` (2 MiB) slack so
/// `startup_64` can place its relocate window at `align_up(load, 2M)`.
pub const BZIMAGE_MAX_PAGES: usize = 4608;

/// Align `addr` up to `align` (power-of-two). `align <= 1` returns `addr`.
pub fn align_up_u64(addr: u64, align: u64) -> u64 {
    if align <= 1 {
        return addr;
    }
    (addr + (align - 1)) & !(align - 1)
}

/// Contiguous bytes to reserve for a bzImage load.
///
/// Real Linux uses `[align_up(load, kernel_alignment), align_up(load)+init_size)`.
/// Reserving `init_size + alignment` from an arbitrary base lets us place the
/// PM image on a `kernel_alignment` boundary inside the allocation.
pub fn bzimage_workspace_bytes(pm_size: usize, init_size: usize, align: usize, real: bool) -> usize {
    let base = core::cmp::max(pm_size, init_size);
    if real && align > 4096 {
        base.saturating_add(align)
    } else {
        base
    }
}

/// Parsed bzImage layout (file-relative).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BzImageInfo {
    pub setup_sects: u8,
    pub pm_offset: usize,
    pub pm_size: usize,
    pub setup_magic: u32,
    pub entry_file_off: usize,
}

/// Write a classic PC e820 RAM map into `boot_params`.
///
/// Linux `append_e820_table()` **rejects maps with fewer than two entries**
/// and falls back to BIOS-e801 (~640 KiB). A single `[0, mem)` region is
/// therefore fatal for real kernels loaded above 1 MiB.
///
/// Layout (QEMU/SeaBIOS-style):
/// - `[0, E820_LOW_RAM_END)` RAM
/// - `[E820_LOW_RAM_END, E820_HIGH_RAM_START)` reserved (EBDA/VGA/BIOS hole)
/// - `[E820_HIGH_RAM_START, mem_bytes)` RAM
///
/// Also fills `alt_mem_k` (**u32** KB above 1 MiB) and `ext_mem_k` as e801/88
/// backups, and clears the setup sentinel so `sanitize_boot_params` keeps
/// the map.
pub fn write_e820_ram_map(buf: &mut [u8; BOOT_PARAMS_SIZE], mem_bytes: u64) {
    // Whole zeropage must be cleared before hdr/e820; callers zero `buf`.
    buf[OFF_SENTINEL] = 0;

    let high_end = if mem_bytes > E820_HIGH_RAM_START {
        mem_bytes
    } else {
        E820_HIGH_RAM_START + 16 * 1024 * 1024
    };
    let high_size = high_end - E820_HIGH_RAM_START;
    let hole_size = E820_HIGH_RAM_START - E820_LOW_RAM_END;

    // e820[0]: low conventional RAM
    write_u64(buf, OFF_E820_TABLE, 0);
    write_u64(buf, OFF_E820_TABLE + 8, E820_LOW_RAM_END);
    write_u32(buf, OFF_E820_TABLE + 16, E820_RAM);
    // e820[1]: reserved hole (EBDA / VGA / ROM)
    let e1 = OFF_E820_TABLE + E820_ENTRY_SIZE;
    write_u64(buf, e1, E820_LOW_RAM_END);
    write_u64(buf, e1 + 8, hole_size);
    write_u32(buf, e1 + 16, E820_RESERVED);
    // e820[2]: extended RAM above 1 MiB
    let e2 = OFF_E820_TABLE + 2 * E820_ENTRY_SIZE;
    write_u64(buf, e2, E820_HIGH_RAM_START);
    write_u64(buf, e2 + 8, high_size);
    write_u32(buf, e2 + 16, E820_RAM);
    buf[OFF_E820_ENTRIES] = 3;

    // e801 path uses alt_mem_k as KB above 1 MiB (u32 in modern boot_params).
    let alt_kb = (high_size / 1024) as u32;
    write_u32(buf, OFF_ALT_MEM_K, alt_kb);
    // BIOS-88 field is still u16; clamp.
    let ext_kb = (high_size / 1024).min(u16::MAX as u64) as u16;
    write_u16(buf, OFF_EXT_MEM_K, ext_kb);
}

/// Pack a Linux `boot_params` zero page.
///
/// Writes setup header magic, version, loader type, ramdisk ptrs, cmdline
/// pointer, and a two-entry e820 RAM map covering low + extended memory.
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

    write_e820_ram_map(buf, mem_bytes);
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
/// - `kernel` — 1 page 64-bit proto-kernel (earlyprintk-style OUT + HLT)
/// - `initrd` / `init` — 1 page proto-init (shell marker OUT + HLT)
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
        write_proto_init(initrd_phys);
        write_cmdline_bytes(cmdline_phys, DEFAULT_CMDLINE);
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
        entry_phys: kernel_phys,
        initrd_phys,
        init_phys: initrd_phys,
        cmdline_phys,
        setup_magic: magic,
        ramdisk_image: initrd_phys as u32,
        ramdisk_size: SYNTH_INITRD_SIZE as u32,
        cmd_line_ptr: cmdline_phys as u32,
        from_bzimage: false,
        is_real_linux: false,
        has_real_initrd: false,
        kernel_bytes: SYNTH_KERNEL_SIZE as u64,
    })
}

/// Parse a bzImage / `vmlinux` boot+setup header.
pub fn parse_bzimage(image: &[u8]) -> Result<BzImageInfo, ()> {
    if image.len() < 0x250 {
        return Err(());
    }
    let boot_flag = image[OFF_BOOT_FLAG] as u16 | ((image[OFF_BOOT_FLAG + 1] as u16) << 8);
    if boot_flag != 0xAA55 {
        return Err(());
    }
    let magic = read_u32(image, OFF_HEADER);
    if magic != SETUP_HEADER_MAGIC {
        return Err(());
    }
    let mut setup_sects = image[OFF_SETUP_SECTS];
    if setup_sects == 0 {
        setup_sects = 4;
    }
    let pm_offset = (setup_sects as usize + 1) * 512;
    if pm_offset + BZIMAGE_ENTRY_OFFSET >= image.len() {
        return Err(());
    }
    let pm_size = image.len() - pm_offset;
    if pm_size == 0 {
        return Err(());
    }
    Ok(BzImageInfo {
        setup_sects,
        pm_offset,
        pm_size,
        setup_magic: magic,
        entry_file_off: pm_offset + BZIMAGE_ENTRY_OFFSET,
    })
}

/// Build a minimal bzImage: setup header + proto-kernel at PM+0x200.
///
/// Used as ESP fixture / embedded fallback so M3.7 exercises real format
/// load while keeping the synthetic early/shell path alive until M3.8.
pub fn build_minimal_bzimage(out: &mut [u8; MINIMAL_BZIMAGE_CAP]) -> usize {
    out.fill(0);
    let setup_sects: u8 = 4;
    let pm_offset = (setup_sects as usize + 1) * 512;
    let entry_off = pm_offset + BZIMAGE_ENTRY_OFFSET;
    debug_assert!(entry_off + 1024 <= MINIMAL_BZIMAGE_CAP);

    out[OFF_SETUP_SECTS] = setup_sects;
    write_u16_slice(out, OFF_BOOT_FLAG, 0xAA55);
    write_u32_slice(out, OFF_HEADER, SETUP_HEADER_MAGIC);
    write_u16_slice(out, OFF_VERSION, 0x020C);
    out[OFF_TYPE_OF_LOADER] = 0xFF;
    out[OFF_LOADFLAGS] = LOADFLAGS_LOADED_HIGH;
    write_u32_slice(out, OFF_KERNEL_ALIGNMENT, 0x1000_0000);
    out[OFF_RELOCATABLE_KERNEL] = 1;
    write_u64_slice(out, OFF_PREF_ADDRESS, 0);
    write_u32_slice(out, OFF_INIT_SIZE, 4096);
    write_u32_slice(out, OFF_CODE32_START, 0);

    let mut page = [0u8; SYNTH_KERNEL_SIZE];
    // SAFETY: stack buffer owned by this function.
    unsafe {
        write_synth_kernel(page.as_mut_ptr() as u64);
    }
    // Proto fits well under a page; copy into PM at the 64-bit entry offset.
    let n = 1024.min(MINIMAL_BZIMAGE_CAP - entry_off);
    out[entry_off..entry_off + n].copy_from_slice(&page[..n]);

    // PM image is one page (entry at +0x200 inside it).
    pm_offset + 4096
}

/// Place a bzImage into GPA: PM kernel, initrd (real or proto), cmdline, boot_params.
///
/// When `initrd_blob` is present and `image` is a real Linux bzImage, the blob
/// is placed as the ramdisk (M3.10). Otherwise a one-page proto-init is used.
pub fn load_bzimage_guest(
    alloc: &mut FrameAllocator,
    image: &[u8],
    initrd_blob: Option<&[u8]>,
) -> Result<BootLoadInfo, ()> {
    let info = parse_bzimage(image)?;
    let pm = &image[info.pm_offset..];
    let real = image.len() >= REAL_LINUX_MIN_BYTES;
    if real && image.len() > OFF_RELOCATABLE_KERNEL && image[OFF_RELOCATABLE_KERNEL] == 0 {
        // Non-relocatable images must sit at pref_address; our allocator cannot.
        return Err(());
    }
    // Real kernels need `init_size` workspace for in-place decompress, plus
    // alignment slack: `startup_64` sets output=`align_up(load, kernel_alignment)`
    // and uses `[output, output+init_size)` (stack/relocated ZO live at the top).
    let init_size = if image.len() > OFF_INIT_SIZE + 4 {
        read_u32(image, OFF_INIT_SIZE) as usize
    } else {
        0
    };
    let align = if image.len() > OFF_KERNEL_ALIGNMENT + 4 {
        let a = read_u32(image, OFF_KERNEL_ALIGNMENT) as usize;
        if a == 0 {
            0x20_0000
        } else {
            a
        }
    } else {
        0x20_0000
    };
    let need = bzimage_workspace_bytes(info.pm_size, init_size, align, real);
    let pages = (need + 4095) / 4096;
    if pages == 0 || pages > BZIMAGE_MAX_PAGES {
        return Err(());
    }

    let use_real_initrd = real && initrd_blob.map(|b| !b.is_empty()).unwrap_or(false);
    let initrd_len = if use_real_initrd {
        initrd_blob.map(|b| b.len()).unwrap_or(0)
    } else {
        SYNTH_INITRD_SIZE
    };
    let initrd_pages = (initrd_len + 4095) / 4096;
    if initrd_pages == 0 || initrd_pages > INITRD_MAX_PAGES {
        return Err(());
    }

    let raw = alloc.allocate_contiguous(pages as u64).ok_or(())?;
    let initrd = alloc.allocate_contiguous(initrd_pages as u64).ok_or(())?;
    let cmdline = alloc.allocate_frame().ok_or(())?;
    let boot_params = alloc.allocate_frame().ok_or(())?;

    let raw_phys = raw.to_phys();
    let alloc_bytes = (pages as u64) * 4096;
    let kernel_phys = if real {
        let aligned = align_up_u64(raw_phys, align as u64);
        let window = core::cmp::max(info.pm_size, init_size) as u64;
        if aligned + window > raw_phys + alloc_bytes {
            return Err(());
        }
        aligned
    } else {
        raw_phys
    };
    let initrd_phys = initrd.to_phys();
    let cmdline_phys = cmdline.to_phys();
    let boot_params_phys = boot_params.to_phys();
    let entry_phys = kernel_phys + BZIMAGE_ENTRY_OFFSET as u64;

    // Ownership smoke: claim endpoints + metadata (full map may exceed MAP_CAP).
    let mut claim = [(0u64, PhysFrame::from_phys(0)); 10];
    let mut nclaim = 0usize;
    claim[nclaim] = (raw_phys, raw);
    nclaim += 1;
    if kernel_phys != raw_phys {
        claim[nclaim] = (kernel_phys, PhysFrame::from_phys(kernel_phys));
        nclaim += 1;
    }
    if pages > 1 {
        let last = PhysFrame::from_phys(raw_phys + alloc_bytes - 4096);
        claim[nclaim] = (last.to_phys(), last);
        nclaim += 1;
    }
    claim[nclaim] = (initrd_phys, initrd);
    nclaim += 1;
    if initrd_pages > 1 {
        let last = PhysFrame::from_phys(initrd_phys + ((initrd_pages as u64) - 1) * 4096);
        claim[nclaim] = (last.to_phys(), last);
        nclaim += 1;
    }
    claim[nclaim] = (cmdline_phys, cmdline);
    nclaim += 1;
    claim[nclaim] = (boot_params_phys, boot_params);
    nclaim += 1;
    claim_load_pages(&claim[..nclaim]).map_err(|_| ())?;

    // SAFETY: freshly allocated identity-mapped frames.
    unsafe {
        core::ptr::write_bytes(raw_phys as *mut u8, 0, pages * 4096);
        core::ptr::copy_nonoverlapping(pm.as_ptr(), kernel_phys as *mut u8, info.pm_size);
        core::ptr::write_bytes(initrd_phys as *mut u8, 0, initrd_pages * 4096);
        if use_real_initrd {
            let blob = initrd_blob.unwrap();
            core::ptr::copy_nonoverlapping(blob.as_ptr(), initrd_phys as *mut u8, blob.len());
            write_cmdline_bytes(cmdline_phys, REAL_LINUX_CMDLINE);
        } else {
            write_proto_init(initrd_phys);
            write_cmdline_bytes(cmdline_phys, DEFAULT_CMDLINE);
        }
    }

    let mut bp = [0u8; BOOT_PARAMS_SIZE];
    // Copy setup header from the bzImage (boot_params mirrors those offsets).
    let hdr_end = core::cmp::min(image.len(), 0x280);
    if hdr_end > 0x1F1 {
        bp[0x1F1..hdr_end].copy_from_slice(&image[0x1F1..hdr_end]);
    }
    bp[OFF_TYPE_OF_LOADER] = 0xFF;
    bp[OFF_LOADFLAGS] |= LOADFLAGS_LOADED_HIGH;
    write_u32(&mut bp, OFF_CODE32_START, kernel_phys as u32);
    write_u32(&mut bp, OFF_RAMDISK_IMAGE, initrd_phys as u32);
    write_u32(&mut bp, OFF_RAMDISK_SIZE, initrd_len as u32);
    write_u32(&mut bp, OFF_CMD_LINE_PTR, cmdline_phys as u32);
    // Cover the load address + decompress window (real kernels may sit high).
    // Must be ≥2 e820 entries — Linux rejects a single-region map.
    // Real guests match QEMU `-m 1G` (512M is a known alloc_low_pages footgun).
    let mem_bytes = if real {
        GUEST_RAM_BYTES
    } else {
        64 * 1024 * 1024u64
    };
    write_e820_ram_map(&mut bp, mem_bytes);

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
        entry_phys,
        initrd_phys,
        init_phys: initrd_phys,
        cmdline_phys,
        setup_magic: magic,
        ramdisk_image: initrd_phys as u32,
        ramdisk_size: initrd_len as u32,
        cmd_line_ptr: cmdline_phys as u32,
        from_bzimage: true,
        is_real_linux: real,
        has_real_initrd: use_real_initrd,
        kernel_bytes: alloc_bytes,
    })
}

/// Write the M3.3 64-bit proto-kernel at `phys`.
///
/// Entry convention (Linux x86_64): `RSI` = `boot_params`. Verifies HdrS at
/// `[rsi+0x202]`, OUTs [`PROTO_EARLY_LINE`] then [`GUEST_EARLY_MAGIC`], HLT.
///
/// SAFETY: `phys` is an owned writable identity-mapped frame.
pub unsafe fn write_synth_kernel(phys: u64) {
    let p = phys as *mut u8;
    core::ptr::write_bytes(p, 0, SYNTH_KERNEL_SIZE);
    let mut o = 0usize;

    // test rsi, rsi
    core::ptr::write_volatile(p.add(o), 0x48);
    core::ptr::write_volatile(p.add(o + 1), 0x85);
    core::ptr::write_volatile(p.add(o + 2), 0xF6);
    o += 3;
    // jz fail (rel8 patched below)
    let jz_off = o;
    core::ptr::write_volatile(p.add(o), 0x74);
    core::ptr::write_volatile(p.add(o + 1), 0x00);
    o += 2;

    // cmp dword [rsi+0x202], SETUP_HEADER_MAGIC
    core::ptr::write_volatile(p.add(o), 0x81);
    core::ptr::write_volatile(p.add(o + 1), 0xBE);
    core::ptr::write_volatile(p.add(o + 2), 0x02);
    core::ptr::write_volatile(p.add(o + 3), 0x02);
    core::ptr::write_volatile(p.add(o + 4), 0x00);
    core::ptr::write_volatile(p.add(o + 5), 0x00);
    let magic = SETUP_HEADER_MAGIC.to_le_bytes();
    for i in 0..4 {
        core::ptr::write_volatile(p.add(o + 6 + i), magic[i]);
    }
    o += 10;
    // jne fail (rel8 patched below)
    let jne_off = o;
    core::ptr::write_volatile(p.add(o), 0x75);
    core::ptr::write_volatile(p.add(o + 1), 0x00);
    o += 2;

    // jmp outs (over the fail stub)
    let jmp_outs_off = o;
    core::ptr::write_volatile(p.add(o), 0xEB);
    core::ptr::write_volatile(p.add(o + 1), 0x00);
    o += 2;

    // fail: hlt ; jmp $
    let fail_off = o;
    core::ptr::write_volatile(p.add(o), 0xF4);
    core::ptr::write_volatile(p.add(o + 1), 0xEB);
    core::ptr::write_volatile(p.add(o + 2), 0xFE);
    o += 3;

    let outs_off = o;
    o = emit_com1_string(p, o, PROTO_EARLY_LINE);
    o = emit_com1_string(p, o, GUEST_EARLY_MAGIC);

    // hlt ; jmp $
    core::ptr::write_volatile(p.add(o), 0xF4);
    core::ptr::write_volatile(p.add(o + 1), 0xEB);
    core::ptr::write_volatile(p.add(o + 2), 0xFE);
    o += 3;

    let jz_rel = (fail_off as isize) - (jz_off as isize + 2);
    let jne_rel = (fail_off as isize) - (jne_off as isize + 2);
    let jmp_rel = (outs_off as isize) - (jmp_outs_off as isize + 2);
    debug_assert!((-128..128).contains(&jz_rel));
    debug_assert!((-128..128).contains(&jne_rel));
    debug_assert!((-128..128).contains(&jmp_rel));
    core::ptr::write_volatile(p.add(jz_off + 1), jz_rel as u8);
    core::ptr::write_volatile(p.add(jne_off + 1), jne_rel as u8);
    core::ptr::write_volatile(p.add(jmp_outs_off + 1), jmp_rel as u8);

    debug_assert!(o <= SYNTH_KERNEL_SIZE);
}

/// Emit `mov edx,0x3f8` / `mov al,imm` / `out dx,al` for each byte.
unsafe fn emit_com1_string(p: *mut u8, mut o: usize, bytes: &[u8]) -> usize {
    for &byte in bytes {
        core::ptr::write_volatile(p.add(o), 0xBA);
        core::ptr::write_volatile(p.add(o + 1), 0xF8);
        core::ptr::write_volatile(p.add(o + 2), 0x03);
        core::ptr::write_volatile(p.add(o + 3), 0x00);
        core::ptr::write_volatile(p.add(o + 4), 0x00);
        o += 5;
        core::ptr::write_volatile(p.add(o), 0xB0);
        core::ptr::write_volatile(p.add(o + 1), byte);
        o += 2;
        core::ptr::write_volatile(p.add(o), 0xEE);
        o += 1;
    }
    o
}

/// Write M3.5 proto-init at `phys` (synthetic initrd frame doubles as init).
///
/// OUTs [`PROTO_SHELL_LINE`] then [`GUEST_SHELL_MAGIC`], then HLT.
///
/// SAFETY: `phys` is an owned writable identity-mapped frame.
pub unsafe fn write_proto_init(phys: u64) {
    let p = phys as *mut u8;
    core::ptr::write_bytes(p, 0, SYNTH_INITRD_SIZE);
    let mut o = 0usize;
    o = emit_com1_string(p, o, PROTO_SHELL_LINE);
    o = emit_com1_string(p, o, GUEST_SHELL_MAGIC);
    core::ptr::write_volatile(p.add(o), 0xF4);
    core::ptr::write_volatile(p.add(o + 1), 0xEB);
    core::ptr::write_volatile(p.add(o + 2), 0xFE);
    o += 3;
    debug_assert!(o <= SYNTH_INITRD_SIZE);
}

unsafe fn write_cmdline_bytes(phys: u64, cmdline: &[u8]) {
    core::ptr::write_bytes(phys as *mut u8, 0, 4096);
    let n = cmdline.len().min(4096);
    core::ptr::copy_nonoverlapping(cmdline.as_ptr(), phys as *mut u8, n);
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

fn write_u16_slice(buf: &mut [u8], off: usize, v: u16) {
    write_u16(buf, off, v);
}

fn write_u32_slice(buf: &mut [u8], off: usize, v: u32) {
    write_u32(buf, off, v);
}

fn write_u64_slice(buf: &mut [u8], off: usize, v: u64) {
    write_u64(buf, off, v);
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
