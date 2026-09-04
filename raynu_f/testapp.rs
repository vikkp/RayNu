//! RayNu-F test application: a tiny, hand-encoded x86-64 UEFI app emitted as
//! a genuine PE32+ image. It is the first guest that will execute our tables.
//!
//! Pillar: [Z] · Proven Core: **outside** (ADR-016)
//!
//! `efi_main(ImageHandle: RCX, SystemTable: RDX)`:
//!
//! ```text
//! 48 83 EC 28             sub  rsp, 0x28          ; shadow space, 16-align
//! 48 8B 42 40             mov  rax, [rdx+0x40]    ; SystemTable->ConOut
//! 48 8B 15 31 00 00 00    mov  rdx, [rip+0x31]    ; rdx = *msg_ptr (relocated)
//! 48 89 C1                mov  rcx, rax           ; This
//! FF 50 08                call [rax+8]            ; ConOut->OutputString
//! 48 83 C4 28             add  rsp, 0x28
//! F4                      hlt                     ; done — clean stop
//! EB FD                   jmp  hlt
//! ```
//!
//! The message pointer is an absolute address stored in `.text` and fixed up
//! through a real `.reloc` `DIR64` entry, so loading at a base other than
//! `ImageBase` exercises the loader's relocation path. If relocation is wrong
//! the guest prints garbage or faults — a visible failure, not a silent pass.
//!
//! In-tree as data (no external toolchain), deterministic, and it goes
//! through the same loader a real `\EFI\BOOT\BOOTX64.EFI` will.

use super::pe::{
    DIR_BASERELOC, MACHINE_AMD64, OPT_DATA_DIRS_OFF, PE32PLUS_MAGIC, REL_ABSOLUTE, REL_DIR64,
    SUBSYSTEM_EFI_APPLICATION,
};

/// Preferred `ImageBase` in the PE header. The launch plan deliberately loads
/// elsewhere so relocation is exercised.
pub const TESTAPP_IMAGE_BASE: u64 = 0x0040_0000;
/// File and section geometry.
pub const TESTAPP_FILE_ALIGN: u32 = 0x200;
pub const TESTAPP_SECTION_ALIGN: u32 = 0x1000;
pub const TESTAPP_SIZE_OF_HEADERS: u32 = 0x200;
pub const TESTAPP_TEXT_RVA: u32 = 0x1000;
pub const TESTAPP_TEXT_RAW: u32 = 0x200;
pub const TESTAPP_RELOC_RVA: u32 = 0x2000;
pub const TESTAPP_RELOC_RAW: u32 = 0x400;
pub const TESTAPP_SIZE_OF_IMAGE: u32 = 0x3000;
/// Total file bytes emitted.
pub const TESTAPP_FILE_BYTES: usize = 0x600;
/// Entry RVA (start of `.text`).
pub const TESTAPP_ENTRY_RVA: u32 = TESTAPP_TEXT_RVA;
/// RVA of the relocated 8-byte message pointer cell inside `.text`.
pub const TESTAPP_MSG_PTR_RVA: u32 = TESTAPP_TEXT_RVA + 0x40;
/// RVA of the UTF-16 message.
pub const TESTAPP_MSG_RVA: u32 = TESTAPP_TEXT_RVA + 0x48;

/// The exact code bytes at the entry point.
pub const TESTAPP_CODE: [u8; 28] = [
    0x48, 0x83, 0xEC, 0x28, // sub rsp, 0x28
    0x48, 0x8B, 0x42, 0x40, // mov rax, [rdx+0x40]
    0x48, 0x8B, 0x15, 0x31, 0x00, 0x00, 0x00, // mov rdx, [rip+0x31]
    0x48, 0x89, 0xC1, // mov rcx, rax
    0xFF, 0x50, 0x08, // call [rax+8]
    0x48, 0x83, 0xC4, 0x28, // add rsp, 0x28
    0xF4, // hlt
    0xEB, 0xFD, // jmp hlt
];

/// What the app prints through `ConOut->OutputString`.
pub const TESTAPP_MESSAGE: &str = "RN-F ConOut via RayNu-F tables\r\n";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestAppError {
    BufferTooSmall,
}

fn put_u16(b: &mut [u8], off: usize, v: u16) {
    b[off..off + 2].copy_from_slice(&v.to_le_bytes());
}
fn put_u32(b: &mut [u8], off: usize, v: u32) {
    b[off..off + 4].copy_from_slice(&v.to_le_bytes());
}
fn put_u64(b: &mut [u8], off: usize, v: u64) {
    b[off..off + 8].copy_from_slice(&v.to_le_bytes());
}

/// Emit the test app as a PE32+ file into `out` (≥ `TESTAPP_FILE_BYTES`).
/// Returns the byte count written.
pub fn build_test_app(out: &mut [u8]) -> Result<usize, TestAppError> {
    if out.len() < TESTAPP_FILE_BYTES {
        return Err(TestAppError::BufferTooSmall);
    }
    let f = &mut out[..TESTAPP_FILE_BYTES];
    for b in f.iter_mut() {
        *b = 0;
    }

    // DOS header: "MZ" + e_lfanew = 0x80.
    f[0] = b'M';
    f[1] = b'Z';
    put_u32(f, 0x3C, 0x80);

    // PE signature + COFF header.
    let pe = 0x80usize;
    f[pe..pe + 4].copy_from_slice(b"PE\0\0");
    let coff = pe + 4;
    put_u16(f, coff, MACHINE_AMD64);
    put_u16(f, coff + 2, 2); // NumberOfSections
    put_u32(f, coff + 4, 0); // TimeDateStamp
    put_u32(f, coff + 8, 0); // PointerToSymbolTable
    put_u32(f, coff + 12, 0); // NumberOfSymbols
    let size_of_opt: u16 = 240; // PE32+ with 16 data directories
    put_u16(f, coff + 16, size_of_opt);
    // EXECUTABLE_IMAGE | LARGE_ADDRESS_AWARE
    put_u16(f, coff + 18, 0x0002 | 0x0020);

    // Optional header (PE32+).
    let opt = coff + 20;
    put_u16(f, opt, PE32PLUS_MAGIC);
    f[opt + 2] = 14; // MajorLinkerVersion (cosmetic)
    f[opt + 3] = 0;
    put_u32(f, opt + 4, TESTAPP_TEXT_RAW); // SizeOfCode
    put_u32(f, opt + 8, 0); // SizeOfInitializedData
    put_u32(f, opt + 12, 0); // SizeOfUninitializedData
    put_u32(f, opt + 16, TESTAPP_ENTRY_RVA);
    put_u32(f, opt + 20, TESTAPP_TEXT_RVA); // BaseOfCode
    put_u64(f, opt + 24, TESTAPP_IMAGE_BASE);
    put_u32(f, opt + 32, TESTAPP_SECTION_ALIGN);
    put_u32(f, opt + 36, TESTAPP_FILE_ALIGN);
    // OS/Image/Subsystem versions left 0.
    put_u32(f, opt + 56, TESTAPP_SIZE_OF_IMAGE);
    put_u32(f, opt + 60, TESTAPP_SIZE_OF_HEADERS);
    put_u32(f, opt + 64, 0); // CheckSum
    put_u16(f, opt + 68, SUBSYSTEM_EFI_APPLICATION);
    put_u16(f, opt + 70, 0x0040); // DllCharacteristics: DYNAMIC_BASE
    put_u64(f, opt + 72, 0x1000); // SizeOfStackReserve
    put_u64(f, opt + 80, 0x1000); // SizeOfStackCommit
    put_u64(f, opt + 88, 0); // SizeOfHeapReserve
    put_u64(f, opt + 96, 0); // SizeOfHeapCommit
    put_u32(f, opt + 104, 0); // LoaderFlags
    put_u32(f, opt + 108, 16); // NumberOfRvaAndSizes
    // Base relocation directory.
    let d = opt + OPT_DATA_DIRS_OFF + DIR_BASERELOC * 8;
    put_u32(f, d, TESTAPP_RELOC_RVA);
    put_u32(f, d + 4, 12);

    // Section table.
    let st = opt + size_of_opt as usize;
    // .text
    f[st..st + 5].copy_from_slice(b".text");
    put_u32(f, st + 8, 0x100); // VirtualSize
    put_u32(f, st + 12, TESTAPP_TEXT_RVA);
    put_u32(f, st + 16, TESTAPP_TEXT_RAW); // SizeOfRawData
    put_u32(f, st + 20, TESTAPP_TEXT_RAW); // PointerToRawData
    // CODE | EXECUTE | READ | WRITE (pointer cell is written by the loader)
    put_u32(f, st + 36, 0x0000_0020 | 0x2000_0000 | 0x4000_0000 | 0x8000_0000);
    // .reloc
    let st2 = st + 40;
    f[st2..st2 + 6].copy_from_slice(b".reloc");
    put_u32(f, st2 + 8, 12);
    put_u32(f, st2 + 12, TESTAPP_RELOC_RVA);
    put_u32(f, st2 + 16, TESTAPP_FILE_ALIGN);
    put_u32(f, st2 + 20, TESTAPP_RELOC_RAW);
    // INITIALIZED_DATA | DISCARDABLE | READ
    put_u32(f, st2 + 36, 0x0000_0040 | 0x0200_0000 | 0x4000_0000);

    // .text raw data.
    let text = TESTAPP_TEXT_RAW as usize;
    f[text..text + TESTAPP_CODE.len()].copy_from_slice(&TESTAPP_CODE);
    // Message pointer cell (absolute, relative to ImageBase; relocated on load).
    put_u64(
        f,
        text + (TESTAPP_MSG_PTR_RVA - TESTAPP_TEXT_RVA) as usize,
        TESTAPP_IMAGE_BASE + TESTAPP_MSG_RVA as u64,
    );
    // UTF-16LE message + NUL.
    let mut off = text + (TESTAPP_MSG_RVA - TESTAPP_TEXT_RVA) as usize;
    for ch in TESTAPP_MESSAGE.encode_utf16() {
        put_u16(f, off, ch);
        off += 2;
    }
    put_u16(f, off, 0);

    // .reloc raw data: one block for page 0x1000 with a DIR64 at +0x40 and an
    // ABSOLUTE pad entry.
    let rl = TESTAPP_RELOC_RAW as usize;
    put_u32(f, rl, TESTAPP_TEXT_RVA); // PageRVA
    put_u32(f, rl + 4, 12); // BlockSize
    put_u16(
        f,
        rl + 8,
        (REL_DIR64 << 12) | ((TESTAPP_MSG_PTR_RVA - TESTAPP_TEXT_RVA) as u16 & 0xfff),
    );
    put_u16(f, rl + 10, REL_ABSOLUTE << 12);

    Ok(TESTAPP_FILE_BYTES)
}
