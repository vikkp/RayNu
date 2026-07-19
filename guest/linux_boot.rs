//! Linux x86_64 boot-protocol packing + synthetic load / proto-kernel (M3.2/M3.3).
//!
//! Pillar: [Z]
//! Proven Core: **outside** (ADR-002) — protocol glue only; frames claimed
//! via Proven Core `FrameAllocator` + `EptMap`.
//!
//! M3.2 places a proto-kernel page, initrd stub, cmdline, and packed
//! `boot_params` into GPA. M3.3 enters the proto-kernel at 64-bit with
//! RSI=`boot_params` (real bzImage deferred).

use crate::devices::serial_pio::{GUEST_EARLY_MAGIC, GUEST_SHELL_MAGIC};
use crate::memory::ept::{EptError, EptMap, EptPermissions, M2_BRINGUP_GUEST_ID};
use crate::memory::{FrameAllocator, PhysFrame};

/// COM1 marker when synthetic load + packing succeeds (M3.2 gate).
pub const M3_LOAD_OK_MARKER: &str = "RAYNU-V-M3-LOAD-OK";

/// Linux-style early console line the proto-kernel OUTs before the magic.
pub const PROTO_EARLY_LINE: &[u8] = b"Linux version RayNu-V-proto (early console)\n";

/// Line the M3.5 proto-init OUTs before the shell magic.
pub const PROTO_SHELL_LINE: &[u8] = b"init: RayNu-V proto shell\n";

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
    /// Executable proto-init (same frame as synthetic initrd for M3.5).
    pub init_phys: u64,
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
        init_phys: initrd_phys,
        cmdline_phys,
        setup_magic: magic,
        ramdisk_image: initrd_phys as u32,
        ramdisk_size: SYNTH_INITRD_SIZE as u32,
        cmd_line_ptr: cmdline_phys as u32,
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
