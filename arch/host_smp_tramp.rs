//! 16-bit startup stub copied under 1 MiB (ADR-024).
//!
//! COFF has no 16-bit relocations, so the far jumps and the GDT base are
//! absolute slots patched when the stub is copied. Only label differences
//! are used as offsets.

use core::arch::global_asm;

global_asm!(
    r#"
.intel_syntax noprefix
.globl raynu_ap_tramp_start
.globl raynu_ap_tramp_end
.globl raynu_ap_t_jmp32
.globl raynu_ap_t_jmp64
.globl raynu_ap_t_gdtr_base
.globl raynu_ap_t_cr3
.globl raynu_ap_t_efer
.globl raynu_ap_t_stack
.globl raynu_ap_t_core
.globl raynu_ap_t_entry
.globl raynu_ap_tramp32
.globl raynu_ap_tramp64
.globl raynu_ap_t_gdt
.globl raynu_ap_gdtr_off
.globl raynu_ap_cr3_off
.globl raynu_ap_efer_off

raynu_ap_tramp_start:
.code16
    cli
    cld
    xor ebx, ebx
    mov bx, cs
    shl ebx, 4
    mov ax, cs
    mov ds, ax
    push ebx
    .byte 0xBB
raynu_ap_gdtr_off:
    .word 0
    lgdt [bx]
    pop ebx
    mov eax, cr0
    or eax, 1
    mov cr0, eax
    .byte 0x66, 0xEA
raynu_ap_t_jmp32:
    .long 0
    .word 0x08
.code32
raynu_ap_tramp32:
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov eax, 0x20
    mov cr4, eax
    .byte 0xB8
raynu_ap_cr3_off:
    .long 0
    mov eax, [ebx + eax]
    mov cr3, eax
    mov ecx, 0xC0000080
    rdmsr
    .byte 0xB8
raynu_ap_efer_off:
    .long 0
    or eax, [ebx + eax]
    wrmsr
    mov eax, cr0
    or eax, 0x80000001
    mov cr0, eax
    .byte 0xEA
raynu_ap_t_jmp64:
    .long 0
    .word 0x18
.code64
raynu_ap_tramp64:
    mov rsp, [rip + raynu_ap_t_stack]
    mov rdi, [rip + raynu_ap_t_core]
    mov rax, [rip + raynu_ap_t_entry]
    jmp rax
.balign 8
raynu_ap_t_gdt:
    .quad 0
    .quad 0x00CF9A000000FFFF
    .quad 0x00CF92000000FFFF
    .quad 0x00AF9A000000FFFF
raynu_ap_t_gdtr:
    .word 31
raynu_ap_t_gdtr_base:
    .long 0
.balign 8
raynu_ap_t_cr3:
    .long 0
raynu_ap_t_efer:
    .long 0
raynu_ap_t_stack:
    .quad 0
raynu_ap_t_core:
    .quad 0
raynu_ap_t_entry:
    .quad 0
raynu_ap_tramp_end:
.att_syntax prefix
"#
);

extern "C" {
    static raynu_ap_tramp_start: u8;
    static raynu_ap_tramp_end: u8;
    static raynu_ap_t_jmp32: u8;
    static raynu_ap_t_jmp64: u8;
    static raynu_ap_t_gdtr_base: u8;
    static raynu_ap_t_cr3: u8;
    static raynu_ap_t_efer: u8;
    static raynu_ap_t_stack: u8;
    static raynu_ap_t_core: u8;
    static raynu_ap_t_entry: u8;
    static raynu_ap_tramp32: u8;
    static raynu_ap_tramp64: u8;
    static raynu_ap_t_gdt: u8;
    static raynu_ap_gdtr_off: u8;
    static raynu_ap_cr3_off: u8;
    static raynu_ap_efer_off: u8;
}

/// # Safety
/// `sym` is one of the trampoline labels in this module.
unsafe fn off(sym: *const u8) -> usize {
    // SAFETY: both symbols are in the one trampoline blob. KANI-TARGET: tramp off
    sym.offset_from(&raynu_ap_tramp_start) as usize
}

pub fn tramp_len() -> usize {
    // SAFETY: the end label is in the trampoline blob.
    unsafe { off(&raynu_ap_tramp_end) }
}

pub fn tramp_bytes() -> &'static [u8] {
    let n = tramp_len();
    // SAFETY: the blob is n bytes of executable static data. KANI-TARGET: tramp bytes
    unsafe { core::slice::from_raw_parts(&raynu_ap_tramp_start, n) }
}

pub struct TrampSlots {
    pub jmp32: usize,
    pub jmp64: usize,
    pub gdtr_base: usize,
    pub cr3: usize,
    pub efer: usize,
    pub stack: usize,
    pub core: usize,
    pub entry: usize,
    pub tramp32: usize,
    pub tramp64: usize,
    pub gdt: usize,
    pub gdtr_off: usize,
    pub cr3_off: usize,
    pub efer_off: usize,
}

pub fn slots() -> TrampSlots {
    // SAFETY: each label is inside the trampoline blob.
    unsafe {
        TrampSlots {
            jmp32: off(&raynu_ap_t_jmp32),
            jmp64: off(&raynu_ap_t_jmp64),
            gdtr_base: off(&raynu_ap_t_gdtr_base),
            cr3: off(&raynu_ap_t_cr3),
            efer: off(&raynu_ap_t_efer),
            stack: off(&raynu_ap_t_stack),
            core: off(&raynu_ap_t_core),
            entry: off(&raynu_ap_t_entry),
            tramp32: off(&raynu_ap_tramp32),
            tramp64: off(&raynu_ap_tramp64),
            gdt: off(&raynu_ap_t_gdt),
            gdtr_off: off(&raynu_ap_gdtr_off),
            cr3_off: off(&raynu_ap_cr3_off),
            efer_off: off(&raynu_ap_efer_off),
        }
    }
}

/// Copy the stub to `dest` (the low page) and patch the absolute slots.
///
/// # Safety
/// `dest` is a writable identity-mapped page of at least [`tramp_len`] bytes.
/// Called on core 0 before any other core executes it.
pub unsafe fn install(dest: *mut u8, low_base: u64, cr3: u32, efer_bits: u32) {
    let n = tramp_len();
    let src = tramp_bytes();
    core::ptr::copy_nonoverlapping(src.as_ptr(), dest, n);
    let s = slots();
    let p32 = (low_base + s.tramp32 as u64) as u32;
    let p64 = (low_base + s.tramp64 as u64) as u32;
    let gdt = (low_base + s.gdt as u64) as u32;
    core::ptr::write_unaligned(dest.add(s.jmp32) as *mut u32, p32);
    core::ptr::write_unaligned(dest.add(s.jmp64) as *mut u32, p64);
    core::ptr::write_unaligned(dest.add(s.gdtr_base) as *mut u32, gdt);
    core::ptr::write_unaligned(dest.add(s.cr3) as *mut u32, cr3);
    core::ptr::write_unaligned(dest.add(s.efer) as *mut u32, efer_bits);
    core::ptr::write_unaligned(dest.add(s.gdtr_off) as *mut u16, (s.gdtr_base - 2) as u16);
    core::ptr::write_unaligned(dest.add(s.cr3_off) as *mut u32, s.cr3 as u32);
    core::ptr::write_unaligned(dest.add(s.efer_off) as *mut u32, s.efer as u32);
}

/// Patch the per-core slots in an already installed stub.
///
/// # Safety
/// `dest` is the installed stub. No core is executing it.
pub unsafe fn patch_core(dest: *mut u8, stack: u64, core: u64, entry: u64) {
    let s = slots();
    core::ptr::write_unaligned(dest.add(s.stack) as *mut u64, stack);
    core::ptr::write_unaligned(dest.add(s.core) as *mut u64, core);
    core::ptr::write_unaligned(dest.add(s.entry) as *mut u64, entry);
}

#[cfg(test)]
mod tramp_test {
    use super::*;

    #[test]
    fn blob_is_a_cli_under_a_page() {
        let b = tramp_bytes();
        assert!(b.len() < 1024, "len {}", b.len());
        assert!(b.len() > 32);
        assert_eq!(b[0], 0xFA);
        let s = slots();
        for off in [
            s.jmp32, s.jmp64, s.gdtr_base, s.cr3, s.efer, s.stack, s.core, s.entry, s.tramp32,
            s.tramp64, s.gdt, s.gdtr_off, s.cr3_off, s.efer_off,
        ] {
            assert!(off < b.len(), "slot {off} len {}", b.len());
        }
    }
}
