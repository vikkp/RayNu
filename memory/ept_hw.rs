//! Hardware EPT page-table builder + guest pages (M2.0 / M2.1).
//!
//! Pillar: [V]
//! Proven Core: **inside** (ADR-002, ADR-004)
//! VERIFICATION: L1 — capability MSR gated; identity map only
//!
//! Builds a 4-level EPT that identity-maps GPA→HPA for `[0, 4 GiB)` using
//! 1 GiB pages when available, else 2 MiB pages. That covers OVMF page tables
//! and the early frame pool so a long-mode guest sharing host CR3 can run
//! under EPT.
//!
//! M2.1 guest page: store a magic qword, run a short increment loop, then HLT.
//! M2.4: ISR at [`GUEST_ISR_OFF`] stores [`GUEST_IRQ_MAGIC`] then HLT again.

use crate::arch::cpu::{self, IA32_VMX_EPT_VPID_CAP};

/// COM1 marker when the guest runs under EPT (M2.0 gate).
pub const M2_EPT_OK_MARKER: &str = "RAYNU-V-M2-EPT-OK";

/// COM1 marker when guest store + loop are verified after HLT (M2.1 gate).
pub const M2_GUEST_OK_MARKER: &str = "RAYNU-V-M2-GUEST-OK";

/// Magic value the guest writes via EPT (ASCII-ish `M21STORE`).
pub const GUEST_STORE_MAGIC: u64 = 0x4D32_3153_544F_5245;

/// Guest increment-loop trip count (written to data slot +8).
pub const GUEST_LOOP_ITERS: u64 = 4;

/// Offset of the data region within the guest code page.
pub const GUEST_DATA_OFF: u64 = 0x800;

/// Offset of the M2.4 ISR within the guest code page.
pub const GUEST_ISR_OFF: u64 = 0x100;

/// Guest IRQ ack slot (qword after magic + loop counter).
pub const GUEST_IRQ_SLOT_OFF: u64 = GUEST_DATA_OFF + 16;

/// Magic value the injected ISR writes (ASCII-ish `M24IRQOK`).
pub const GUEST_IRQ_MAGIC: u64 = 0x4D32_3449_5251_4F4B;

/// Secondary proc-based: enable EPT (SDM Vol. 3).
pub const SECONDARY_ENABLE_EPT: u32 = 1 << 1;

/// EPT capability: 4-level page walk supported.
const EPT_CAP_WALK4: u64 = 1 << 6;
/// EPT memory type WB supported for EPTP.
const EPT_CAP_WB: u64 = 1 << 14;
/// 2 MiB pages.
const EPT_CAP_2M: u64 = 1 << 16;
/// 1 GiB pages.
const EPT_CAP_1G: u64 = 1 << 17;

/// Identity-map window: first 4 GiB (covers UEFI + early HV pool).
pub const IDENTITY_GIB: u64 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EptHwError {
    /// Nested/host CPU lacks required EPT features.
    Unsupported,
    /// Not enough caller-supplied frames.
    OutOfFrames,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EptPageSize {
    OneGib,
    TwoMib,
}

/// How many 4K frames [`build_identity_4g`] needs for the chosen page size.
pub fn frames_required(page_size: EptPageSize) -> usize {
    match page_size {
        EptPageSize::OneGib => 2, // PML4 + PDPT
        EptPageSize::TwoMib => 2 + IDENTITY_GIB as usize, // PML4 + PDPT + one PD/GiB
    }
}

/// Probe IA32_VMX_EPT_VPID_CAP for a usable identity-map strategy.
///
/// SAFETY: MSR is architecturally defined when VMX is present.
pub unsafe fn select_page_size() -> Result<EptPageSize, EptHwError> {
    let cap = cpu::rdmsr(IA32_VMX_EPT_VPID_CAP);
    if (cap & EPT_CAP_WALK4) == 0 {
        return Err(EptHwError::Unsupported);
    }
    if (cap & EPT_CAP_1G) != 0 {
        Ok(EptPageSize::OneGib)
    } else if (cap & EPT_CAP_2M) != 0 {
        Ok(EptPageSize::TwoMib)
    } else {
        Err(EptHwError::Unsupported)
    }
}

/// EPTP memory type: WB (6) if supported, else UC (0).
unsafe fn eptp_memory_type() -> u64 {
    let cap = cpu::rdmsr(IA32_VMX_EPT_VPID_CAP);
    if (cap & EPT_CAP_WB) != 0 {
        6
    } else {
        0
    }
}

/// Pack an EPTP value (SDM Vol. 3C §24.6.11).
pub fn pack_eptp(pml4_phys: u64, memory_type: u64) -> u64 {
    let walk_len_minus_1: u64 = 3; // 4-level
    (memory_type & 0x7) | ((walk_len_minus_1 & 0x7) << 3) | (pml4_phys & !0xfff)
}

fn ept_rwe() -> u64 {
    0b111 // read | write | execute
}

fn ept_leaf_large(hpa: u64, memory_type: u64) -> u64 {
    // bit 7 = large page; bits 5:3 = EPT memory type
    ept_rwe() | ((memory_type & 0x7) << 3) | (1 << 7) | (hpa & !0xfff)
}

fn ept_link(next_phys: u64) -> u64 {
    ept_rwe() | (next_phys & !0xfff)
}

/// Build identity EPT for `[0, 4 GiB)` into caller-owned frames.
///
/// Returns the EPTP value to VMWRITE.
///
/// SAFETY: each frame in `frames` is exclusively owned, writable, identity-mapped
/// in the host page tables; interrupts should be masked.
pub unsafe fn build_identity_4g(
    page_size: EptPageSize,
    frames: &mut [u64],
) -> Result<u64, EptHwError> {
    let need = frames_required(page_size);
    if frames.len() < need {
        return Err(EptHwError::OutOfFrames);
    }
    for &f in frames.iter().take(need) {
        if f & 0xfff != 0 {
            return Err(EptHwError::Unsupported);
        }
        core::ptr::write_bytes(f as *mut u8, 0, 4096);
    }

    let mt = eptp_memory_type();
    let pml4 = frames[0];
    let pdpt = frames[1];
    let pml4_entries = pml4 as *mut u64;
    let pdpt_entries = pdpt as *mut u64;

    core::ptr::write_volatile(pml4_entries, ept_link(pdpt));

    match page_size {
        EptPageSize::OneGib => {
            for i in 0..IDENTITY_GIB {
                let hpa = i << 30;
                core::ptr::write_volatile(pdpt_entries.add(i as usize), ept_leaf_large(hpa, mt));
            }
        }
        EptPageSize::TwoMib => {
            for i in 0..IDENTITY_GIB {
                let pd = frames[2 + i as usize];
                core::ptr::write_volatile(pdpt_entries.add(i as usize), ept_link(pd));
                let pd_entries = pd as *mut u64;
                for j in 0..512u64 {
                    let hpa = (i << 30) + (j << 21);
                    core::ptr::write_volatile(pd_entries.add(j as usize), ept_leaf_large(hpa, mt));
                }
            }
        }
    }

    Ok(pack_eptp(pml4, mt))
}

fn write_u64_le(p: *mut u8, v: u64) {
    for i in 0..8 {
        unsafe {
            core::ptr::write_volatile(p.add(i), ((v >> (8 * i)) & 0xff) as u8);
        }
    }
}

/// Write M2.1/M2.4 guest code into an owned frame (identity GPA/HPA).
///
/// Layout:
/// - `+0`: store MAGIC, loop 4×, `hlt`
/// - [`GUEST_ISR_OFF`]: ISR stores [`GUEST_IRQ_MAGIC`], then `hlt`
/// - [`GUEST_DATA_OFF`]: magic + counter + IRQ ack slot
///
/// SAFETY: `page_phys` is a writable identity-mapped frame.
pub unsafe fn write_guest_store_page(page_phys: u64) {
    let p = page_phys as *mut u8;
    core::ptr::write_bytes(p, 0, 4096);

    let data = page_phys + GUEST_DATA_OFF;
    let counter = data + 8;
    let mut o = 0usize;

    // movabs rax, MAGIC
    core::ptr::write_volatile(p.add(o), 0x48);
    core::ptr::write_volatile(p.add(o + 1), 0xB8);
    write_u64_le(p.add(o + 2), GUEST_STORE_MAGIC);
    o += 10;

    // mov moffs64, rax  (store RAX to absolute data address)
    core::ptr::write_volatile(p.add(o), 0x48);
    core::ptr::write_volatile(p.add(o + 1), 0xA3);
    write_u64_le(p.add(o + 2), data);
    o += 10;

    // mov ecx, GUEST_LOOP_ITERS
    core::ptr::write_volatile(p.add(o), 0x48);
    core::ptr::write_volatile(p.add(o + 1), 0xC7);
    core::ptr::write_volatile(p.add(o + 2), 0xC1);
    core::ptr::write_volatile(p.add(o + 3), GUEST_LOOP_ITERS as u8);
    core::ptr::write_volatile(p.add(o + 4), 0);
    core::ptr::write_volatile(p.add(o + 5), 0);
    core::ptr::write_volatile(p.add(o + 6), 0);
    o += 7;

    // movabs rbx, counter
    core::ptr::write_volatile(p.add(o), 0x48);
    core::ptr::write_volatile(p.add(o + 1), 0xBB);
    write_u64_le(p.add(o + 2), counter);
    o += 10;

    // loop body:
    //   inc qword [rbx]   ; 48 FF 03
    //   loop body         ; E2 FB  (rel8 = -5)
    core::ptr::write_volatile(p.add(o), 0x48);
    core::ptr::write_volatile(p.add(o + 1), 0xFF);
    core::ptr::write_volatile(p.add(o + 2), 0x03);
    o += 3;
    core::ptr::write_volatile(p.add(o), 0xE2);
    core::ptr::write_volatile(p.add(o + 1), 0xFB); // -5 → back to inc
    o += 2;

    // hlt ; jmp $
    core::ptr::write_volatile(p.add(o), 0xF4);
    core::ptr::write_volatile(p.add(o + 1), 0xEB);
    core::ptr::write_volatile(p.add(o + 2), 0xFE);

    debug_assert!(o < GUEST_ISR_OFF as usize);
    write_guest_isr(page_phys);
}

/// Write the M2.4 ISR at [`GUEST_ISR_OFF`].
unsafe fn write_guest_isr(page_phys: u64) {
    let p = (page_phys + GUEST_ISR_OFF) as *mut u8;
    let irq_slot = page_phys + GUEST_IRQ_SLOT_OFF;
    let mut o = 0usize;

    // movabs rax, IRQ_MAGIC
    core::ptr::write_volatile(p.add(o), 0x48);
    core::ptr::write_volatile(p.add(o + 1), 0xB8);
    write_u64_le(p.add(o + 2), GUEST_IRQ_MAGIC);
    o += 10;

    // mov moffs64, rax
    core::ptr::write_volatile(p.add(o), 0x48);
    core::ptr::write_volatile(p.add(o + 1), 0xA3);
    write_u64_le(p.add(o + 2), irq_slot);
    o += 10;

    // hlt ; jmp $
    core::ptr::write_volatile(p.add(o), 0xF4);
    core::ptr::write_volatile(p.add(o + 1), 0xEB);
    core::ptr::write_volatile(p.add(o + 2), 0xFE);
}

/// Build a guest IDT with one 64-bit interrupt gate at `vector`.
///
/// SAFETY: `idt_phys` is an owned writable frame; `handler` is executable
/// guest code (identity-mapped).
pub unsafe fn write_guest_idt(idt_phys: u64, handler: u64, cs: u16, vector: u8) {
    let base = idt_phys as *mut u8;
    core::ptr::write_bytes(base, 0, 4096);

    let off = (vector as usize) * 16;
    let d0 = (handler & 0xFFFF)
        | ((cs as u64) << 16)
        | (0x8Eu64 << 40) // P=1, DPL=0, type=interrupt gate
        | (((handler >> 16) & 0xFFFF) << 48);
    let d1 = (handler >> 32) & 0xFFFF_FFFF;
    let slot = (idt_phys as *mut u64).add(off / 8);
    core::ptr::write_unaligned(slot, d0);
    core::ptr::write_unaligned(slot.add(1), d1);
}

/// Read back M2.1 guest stores from the code page's data region.
///
/// SAFETY: `page_phys` is the guest page previously passed to
/// [`write_guest_store_page`]; guest has exited.
pub unsafe fn verify_guest_store(page_phys: u64) -> bool {
    let data = (page_phys + GUEST_DATA_OFF) as *const u64;
    let magic = core::ptr::read_volatile(data);
    let iters = core::ptr::read_volatile(data.add(1));
    magic == GUEST_STORE_MAGIC && iters == GUEST_LOOP_ITERS
}

/// Read back M2.4 ISR ack from the code page.
///
/// SAFETY: guest ISR has run (or not); page is the bring-up code frame.
pub unsafe fn verify_guest_irq(page_phys: u64) -> bool {
    let slot = (page_phys + GUEST_IRQ_SLOT_OFF) as *const u64;
    core::ptr::read_volatile(slot) == GUEST_IRQ_MAGIC
}

#[cfg(test)]
mod ept_hw_test {
    use super::*;

    #[test]
    fn marker_stable() {
        assert_eq!(M2_EPT_OK_MARKER, "RAYNU-V-M2-EPT-OK");
        assert_eq!(M2_GUEST_OK_MARKER, "RAYNU-V-M2-GUEST-OK");
        assert_eq!(GUEST_STORE_MAGIC, 0x4D32_3153_544F_5245);
        assert_eq!(GUEST_LOOP_ITERS, 4);
        assert_eq!(GUEST_IRQ_MAGIC, 0x4D32_3449_5251_4F4B);
        assert_eq!(GUEST_ISR_OFF, 0x100);
    }

    #[test]
    fn guest_idt_gate_encodes_handler() {
        let mut idt = [0u8; 4096];
        let phys = idt.as_mut_ptr() as u64;
        let handler = 0x1_0000_0100u64;
        // SAFETY: test buffer.
        unsafe { write_guest_idt(phys, handler, 0x08, 0x21) };
        let slot = unsafe { (phys as *const u64).add((0x21 * 16) / 8) };
        let d0 = unsafe { core::ptr::read_unaligned(slot) };
        let d1 = unsafe { core::ptr::read_unaligned(slot.add(1)) };
        assert_eq!(d0 & 0xFFFF, handler & 0xFFFF);
        assert_eq!((d0 >> 16) & 0xFFFF, 0x08);
        assert_eq!((d0 >> 40) & 0xFF, 0x8E);
        assert_eq!(d1 & 0xFFFF_FFFF, (handler >> 32) & 0xFFFF_FFFF);
    }

    #[test]
    fn frames_required_counts() {
        assert_eq!(frames_required(EptPageSize::OneGib), 2);
        assert_eq!(frames_required(EptPageSize::TwoMib), 6);
    }

    #[test]
    fn pack_eptp_walk4_wb() {
        let eptp = pack_eptp(0x2000, 6);
        assert_eq!(eptp & 0x7, 6);
        assert_eq!((eptp >> 3) & 0x7, 3);
        assert_eq!(eptp & !0xfff, 0x2000);
    }

    #[test]
    fn guest_store_page_encodes_magic_address() {
        let mut page = [0u8; 4096];
        let phys = page.as_mut_ptr() as u64;
        // SAFETY: test buffer is writable; phys is the buffer address.
        unsafe { write_guest_store_page(phys) };
        // Opcode stream starts with movabs rax, imm64
        assert_eq!(page[0], 0x48);
        assert_eq!(page[1], 0xB8);
        let mut imm = 0u64;
        for i in 0..8 {
            imm |= (page[2 + i] as u64) << (8 * i);
        }
        assert_eq!(imm, GUEST_STORE_MAGIC);
        // Data slots start zeroed
        assert_eq!(
            unsafe { core::ptr::read_unaligned((page.as_ptr() as u64 + GUEST_DATA_OFF) as *const u64) },
            0
        );
    }
}
