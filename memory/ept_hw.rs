//! Hardware EPT page-table builder + guest pages (M2.0 / M2.1 / M3.13 / M3.20).
//!
//! Pillar: [V]
//! Proven Core: **inside** (ADR-002, ADR-004)
//! VERIFICATION: L1 — capability MSR gated; identity map only
//!
//! M3.20 tight precise EPT identity-maps GPA→HPA for [`PRECISE_MIB`] MiB
//! (QEMU `-m 512M`) with 2M pages. That covers OVMF page tables, the early
//! frame pool, and guest RAM (`GUEST_RAM_BYTES` = 256 MiB) while leaving the
//! local APIC at `0xFEE00000` unmapped by omission (no hole punch). Window is
//! strictly below 1 GiB (`RAYNU-V-M3-EPT3-OK`).
//!
//! M2.1 guest page: store a magic qword, run a short increment loop, then HLT.
//! M2.4: ISR at [`GUEST_ISR_OFF`] stores [`GUEST_IRQ_MAGIC`] then HLT again.
//! M3.0: after the loop, `mov edx,0x3f8` / `out dx,al` for each
//! [`crate::devices::serial_pio::GUEST_IO_MAGIC`] byte (`out imm8` cannot encode COM1).
//! M3.1: `cpuid` leaf 1, store filtered ECX, then `hlt`.

use crate::arch::cpu::{self, CPUID_ECX_VMX, IA32_VMX_EPT_VPID_CAP};
use crate::devices::serial_pio::GUEST_IO_MAGIC;

/// COM1 marker when the guest runs under EPT (M2.0 gate).
pub const M2_EPT_OK_MARKER: &str = "RAYNU-V-M2-EPT-OK";

/// COM1 marker when precise (non–4 GiB) EPT is installed (M3.13 gate).
pub const M3_EPT2_OK_MARKER: &str = "RAYNU-V-M3-EPT2-OK";

/// COM1 marker when precise EPT is tighter than 1 GiB (M3.20 gate).
pub const M3_EPT3_OK_MARKER: &str = "RAYNU-V-M3-EPT3-OK";

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

/// Guest CPUID leaf-1 ECX store (M3.1); dword after IRQ ack slot.
pub const GUEST_CPUID_ECX_OFF: u64 = GUEST_DATA_OFF + 24;

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

/// Legacy full identity window (pre–M3.13 scaffold).
pub const IDENTITY_GIB: u64 = 4;

/// 2 MiB leaf size used by the M3.20 tight builder.
pub const TWO_MIB: u64 = 2 * 1024 * 1024;

/// M3.20 tight precise identity window (MiB). Matches QEMU `-m 512M`.
/// Guest e820 stays at [`crate::guest::linux_boot::GUEST_RAM_BYTES`] (256 MiB).
/// Local APIC GPA `0xFEE00000` lies outside this range → EPT violation.
pub const PRECISE_MIB: u64 = 512;

/// Legacy whole-GiB count (M3.13). Live precise path uses [`PRECISE_MIB`].
pub const PRECISE_GIB: u64 = 1;

/// Byte length of the live precise identity window (M3.20: 512 MiB).
pub const PRECISE_BYTES: u64 = PRECISE_MIB * 1024 * 1024;

const _: () = assert!(PRECISE_BYTES < (1u64 << 30));
const _: () = assert!(PRECISE_BYTES % TWO_MIB == 0);

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

/// How many 4K frames [`build_identity_gib`] needs for `gib` GiB of identity.
pub fn frames_required_gib(page_size: EptPageSize, gib: u64) -> usize {
    match page_size {
        EptPageSize::OneGib => 2, // PML4 + PDPT
        EptPageSize::TwoMib => 2 + gib as usize, // PML4 + PDPT + one PD/GiB
    }
}

/// Frames for a whole-GiB identity map (legacy / tests).
pub fn frames_required(page_size: EptPageSize) -> usize {
    frames_required_gib(page_size, PRECISE_GIB)
}

/// Frames for the live M3.20 precise map (`[0, PRECISE_BYTES)` via 2M leaves).
pub fn frames_required_precise() -> usize {
    frames_required_2m_bytes(PRECISE_BYTES)
}

/// Frames for [`build_identity_2m_bytes`]: PML4 + PDPT + one PD per GiB spanned.
pub fn frames_required_2m_bytes(bytes: u64) -> usize {
    if bytes == 0 {
        return 0;
    }
    let gibs = (bytes + (1 << 30) - 1) >> 30;
    2 + gibs as usize
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

/// Require 4-level walk + 2 MiB leaves (M3.20 tight precise path).
///
/// SAFETY: MSR is architecturally defined when VMX is present.
pub unsafe fn ensure_2m_capable() -> Result<(), EptHwError> {
    let cap = cpu::rdmsr(IA32_VMX_EPT_VPID_CAP);
    if (cap & EPT_CAP_WALK4) == 0 || (cap & EPT_CAP_2M) == 0 {
        return Err(EptHwError::Unsupported);
    }
    Ok(())
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

/// Build identity EPT for `[0, gib GiB)` into caller-owned frames.
///
/// Returns the EPTP value to VMWRITE.
///
/// SAFETY: each frame in `frames` is exclusively owned, writable, identity-mapped
/// in the host page tables; interrupts should be masked.
pub unsafe fn build_identity_gib(
    page_size: EptPageSize,
    gib: u64,
    frames: &mut [u64],
) -> Result<u64, EptHwError> {
    if gib == 0 || gib > 512 {
        return Err(EptHwError::Unsupported);
    }
    let need = frames_required_gib(page_size, gib);
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
            for i in 0..gib {
                let hpa = i << 30;
                core::ptr::write_volatile(pdpt_entries.add(i as usize), ept_leaf_large(hpa, mt));
            }
        }
        EptPageSize::TwoMib => {
            for i in 0..gib {
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

/// Build identity EPT for `[0, bytes)` using 2 MiB leaves (partial last GiB OK).
///
/// `bytes` must be a non-zero multiple of [`TWO_MIB`], at most 512 GiB, and must
/// not reach the local APIC MMIO base (hole-by-omission).
///
/// SAFETY: see [`build_identity_gib`].
pub unsafe fn build_identity_2m_bytes(
    bytes: u64,
    frames: &mut [u64],
) -> Result<u64, EptHwError> {
    if bytes == 0 || bytes & (TWO_MIB - 1) != 0 {
        return Err(EptHwError::Unsupported);
    }
    if bytes > crate::arch::apic::DEFAULT_APIC_PHYS {
        return Err(EptHwError::Unsupported);
    }
    let need = frames_required_2m_bytes(bytes);
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

    let two_m_pages = bytes / TWO_MIB;
    let gibs = (two_m_pages + 511) / 512;
    for i in 0..gibs {
        let pd = frames[2 + i as usize];
        core::ptr::write_volatile(pdpt_entries.add(i as usize), ept_link(pd));
        let pd_entries = pd as *mut u64;
        let base_j = i * 512;
        let end_j = core::cmp::min(base_j + 512, two_m_pages);
        for j in base_j..end_j {
            let hpa = j * TWO_MIB;
            let slot = (j - base_j) as usize;
            core::ptr::write_volatile(pd_entries.add(slot), ept_leaf_large(hpa, mt));
        }
    }

    Ok(pack_eptp(pml4, mt))
}

/// M3.20: identity-map [`PRECISE_BYTES`] with 2M leaves (APIC stays outside).
///
/// SAFETY: see [`build_identity_gib`].
pub unsafe fn build_precise_identity(frames: &mut [u64]) -> Result<u64, EptHwError> {
    build_identity_2m_bytes(PRECISE_BYTES, frames)
}

/// Frames for [`build_single_2m_identity`]: PML4 + PDPT + one PD.
pub fn frames_required_single_2m() -> usize {
    3
}

/// M4.0: identity-map a single 2 MiB GPA==HPA leaf (private guest slab).
///
/// `hpa_2m` must be 2 MiB-aligned and lie inside [`PRECISE_BYTES`].
///
/// SAFETY: see [`build_identity_gib`].
pub unsafe fn build_single_2m_identity(
    hpa_2m: u64,
    frames: &mut [u64],
) -> Result<u64, EptHwError> {
    if hpa_2m & (TWO_MIB - 1) != 0 {
        return Err(EptHwError::Unsupported);
    }
    if hpa_2m >= PRECISE_BYTES || hpa_2m.saturating_add(TWO_MIB) > PRECISE_BYTES {
        return Err(EptHwError::Unsupported);
    }
    let need = frames_required_single_2m();
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
    let pd = frames[2];
    let pml4_i = (hpa_2m >> 39) & 0x1ff;
    let pdpt_i = (hpa_2m >> 30) & 0x1ff;
    let pd_i = (hpa_2m >> 21) & 0x1ff;
    core::ptr::write_volatile((pml4 as *mut u64).add(pml4_i as usize), ept_link(pdpt));
    core::ptr::write_volatile((pdpt as *mut u64).add(pdpt_i as usize), ept_link(pd));
    core::ptr::write_volatile((pd as *mut u64).add(pd_i as usize), ept_leaf_large(hpa_2m, mt));
    Ok(pack_eptp(pml4, mt))
}

/// Clear one 2 MiB identity leaf in an existing precise EPT (unmap from G0).
///
/// SAFETY: `pml4_phys` from [`build_precise_identity`]; `gpa_2m` 2 MiB-aligned.
pub unsafe fn clear_2m_identity_leaf(pml4_phys: u64, gpa_2m: u64) -> Result<(), EptHwError> {
    if gpa_2m & (TWO_MIB - 1) != 0 {
        return Err(EptHwError::Unsupported);
    }
    let pml4_i = (gpa_2m >> 39) & 0x1ff;
    let e0 = core::ptr::read_volatile((pml4_phys as *const u64).add(pml4_i as usize));
    if e0 & 0b111 == 0 {
        return Ok(());
    }
    let pdpt = e0 & !0xfff;
    let pdpt_i = (gpa_2m >> 30) & 0x1ff;
    let e1 = core::ptr::read_volatile((pdpt as *const u64).add(pdpt_i as usize));
    if e1 & 0b111 == 0 {
        return Ok(());
    }
    if (e1 & (1 << 7)) != 0 {
        return Err(EptHwError::Unsupported); // unexpected 1G leaf in precise path
    }
    let pd = e1 & !0xfff;
    let pd_i = (gpa_2m >> 21) & 0x1ff;
    core::ptr::write_volatile((pd as *mut u64).add(pd_i as usize), 0);
    Ok(())
}

/// M4.0 guest-1 page: SHELL CPUID hypercall then HLT (private EPT slab).
///
/// SAFETY: `page_phys` is a writable identity-mapped frame.
pub unsafe fn write_guest_shell_cpuid_page(page_phys: u64) {
    let p = page_phys as *mut u8;
    core::ptr::write_bytes(p, 0, 4096);
    let leaf = crate::devices::serial_pio::SHELL_CPUID_LEAF;
    let sub = crate::devices::serial_pio::SHELL_CPUID_SUBLEAF;
    let mut o = 0usize;
    // mov eax, imm32
    core::ptr::write_volatile(p.add(o), 0xB8);
    core::ptr::write_volatile(p.add(o + 1), (leaf & 0xff) as u8);
    core::ptr::write_volatile(p.add(o + 2), ((leaf >> 8) & 0xff) as u8);
    core::ptr::write_volatile(p.add(o + 3), ((leaf >> 16) & 0xff) as u8);
    core::ptr::write_volatile(p.add(o + 4), ((leaf >> 24) & 0xff) as u8);
    o += 5;
    // mov ecx, imm32
    core::ptr::write_volatile(p.add(o), 0xB9);
    core::ptr::write_volatile(p.add(o + 1), (sub & 0xff) as u8);
    core::ptr::write_volatile(p.add(o + 2), ((sub >> 8) & 0xff) as u8);
    core::ptr::write_volatile(p.add(o + 3), ((sub >> 16) & 0xff) as u8);
    core::ptr::write_volatile(p.add(o + 4), ((sub >> 24) & 0xff) as u8);
    o += 5;
    // cpuid
    core::ptr::write_volatile(p.add(o), 0x0F);
    core::ptr::write_volatile(p.add(o + 1), 0xA2);
    o += 2;
    // hlt ; jmp $
    core::ptr::write_volatile(p.add(o), 0xF4);
    core::ptr::write_volatile(p.add(o + 1), 0xEB);
    core::ptr::write_volatile(p.add(o + 2), 0xFE);
}

/// Offsets within the M4.0 G1 2 MiB slab (code/stack/IDT/page tables).
pub const G1_SLAB_OFF_CODE: u64 = 0;
pub const G1_SLAB_OFF_STACK: u64 = 0x1000;
pub const G1_SLAB_OFF_IDT: u64 = 0x2000;
pub const G1_SLAB_OFF_PML4: u64 = 0x3000;
pub const G1_SLAB_OFF_PDPT: u64 = 0x4000;
pub const G1_SLAB_OFF_PD: u64 = 0x5000;

/// Build long-mode guest page tables in the G1 slab: one 2 MiB identity map
/// at `slab_base` (VA == GPA). Returns guest CR3 (PML4 HPA/GPA).
///
/// Required because G1's EPT only maps this slab — sharing the host CR3 would
/// EPT-fault on page-table walks into low memory.
///
/// SAFETY: `slab_base` is a writable identity-mapped 2 MiB region; offsets
/// [`G1_SLAB_OFF_PML4`].. are free for tables.
pub unsafe fn write_guest_identity_2m_tables(slab_base: u64) -> u64 {
    debug_assert_eq!(slab_base & (TWO_MIB - 1), 0);
    let pml4 = slab_base + G1_SLAB_OFF_PML4;
    let pdpt = slab_base + G1_SLAB_OFF_PDPT;
    let pd = slab_base + G1_SLAB_OFF_PD;
    core::ptr::write_bytes(pml4 as *mut u8, 0, 4096);
    core::ptr::write_bytes(pdpt as *mut u8, 0, 4096);
    core::ptr::write_bytes(pd as *mut u8, 0, 4096);

    // Present | Writable (supervisor). NX clear on the 2M leaf so code fetches.
    let present_rw: u64 = 0b011;
    let leaf_2m: u64 = present_rw | (1 << 7) | (slab_base & !0x1f_ffff);

    let pml4_i = ((slab_base >> 39) & 0x1ff) as usize;
    let pdpt_i = ((slab_base >> 30) & 0x1ff) as usize;
    let pd_i = ((slab_base >> 21) & 0x1ff) as usize;
    core::ptr::write_volatile((pml4 as *mut u64).add(pml4_i), pdpt | present_rw);
    core::ptr::write_volatile((pdpt as *mut u64).add(pdpt_i), pd | present_rw);
    core::ptr::write_volatile((pd as *mut u64).add(pd_i), leaf_2m);
    pml4
}

/// Legacy `[0, 4 GiB)` identity (kept for host tests / rollback).
///
/// SAFETY: see [`build_identity_gib`].
pub unsafe fn build_identity_4g(
    page_size: EptPageSize,
    frames: &mut [u64],
) -> Result<u64, EptHwError> {
    build_identity_gib(page_size, IDENTITY_GIB, frames)
}

/// True if `gpa` has a present leaf in the EPT rooted at `pml4_phys`.
///
/// SAFETY: `pml4_phys` is a valid EPT PML4 from this module's builders.
pub unsafe fn gpa_is_mapped(pml4_phys: u64, gpa: u64) -> bool {
    let pml4_i = (gpa >> 39) & 0x1ff;
    let e0 = core::ptr::read_volatile((pml4_phys as *const u64).add(pml4_i as usize));
    if e0 & 0b111 == 0 {
        return false;
    }
    let pdpt = e0 & !0xfff;
    let pdpt_i = (gpa >> 30) & 0x1ff;
    let e1 = core::ptr::read_volatile((pdpt as *const u64).add(pdpt_i as usize));
    if e1 & 0b111 == 0 {
        return false;
    }
    if (e1 & (1 << 7)) != 0 {
        return true; // 1G leaf
    }
    let pd = e1 & !0xfff;
    let pd_i = (gpa >> 21) & 0x1ff;
    let e2 = core::ptr::read_volatile((pd as *const u64).add(pd_i as usize));
    if e2 & 0b111 == 0 {
        return false;
    }
    if (e2 & (1 << 7)) != 0 {
        return true; // 2M leaf
    }
    let pt = e2 & !0xfff;
    let pt_i = (gpa >> 12) & 0x1ff;
    let e3 = core::ptr::read_volatile((pt as *const u64).add(pt_i as usize));
    (e3 & 0b111) != 0
}

/// Frames needed beyond identity build to punch the local-APIC MMIO hole.
pub const APIC_HOLE_EXTRA_FRAMES: usize = 2;

/// Leave GPA [`crate::arch::apic::DEFAULT_APIC_PHYS`] not-present in an identity EPT.
///
/// Splits the covering 1G/2M leaf down to 4K and clears the APIC PTE so guest
/// APIC MMIO causes EPT violation (M3.11). `scratch` = [PD frame, PT frame].
///
/// SAFETY: `pml4_phys` from [`build_identity_4g`]; scratch frames exclusively owned.
pub unsafe fn punch_apic_mmio_hole(
    pml4_phys: u64,
    scratch: &mut [u64; APIC_HOLE_EXTRA_FRAMES],
) -> Result<(), EptHwError> {
    let apic = crate::arch::apic::DEFAULT_APIC_PHYS;
    let mt = eptp_memory_type();
    let pdpt = {
        let e = core::ptr::read_volatile((pml4_phys as *const u64).add(0));
        if e & 0b111 == 0 {
            return Err(EptHwError::Unsupported);
        }
        e & !0xfff
    };
    let pdpt_i = (apic >> 30) as usize;
    let mut pdpt_e = core::ptr::read_volatile((pdpt as *const u64).add(pdpt_i));
    let pd = if (pdpt_e & (1 << 7)) != 0 {
        // 1G leaf → split to 512×2M
        let pd = scratch[0];
        if pd & 0xfff != 0 {
            return Err(EptHwError::Unsupported);
        }
        core::ptr::write_bytes(pd as *mut u8, 0, 4096);
        let base = (pdpt_i as u64) << 30;
        let pd_entries = pd as *mut u64;
        for j in 0..512u64 {
            core::ptr::write_volatile(
                pd_entries.add(j as usize),
                ept_leaf_large(base + (j << 21), mt),
            );
        }
        pdpt_e = ept_link(pd);
        core::ptr::write_volatile((pdpt as *mut u64).add(pdpt_i), pdpt_e);
        pd
    } else {
        pdpt_e & !0xfff
    };

    let pd_i = ((apic >> 21) & 0x1ff) as usize;
    let mut pd_e = core::ptr::read_volatile((pd as *const u64).add(pd_i));
    let pt = if (pd_e & (1 << 7)) != 0 {
        let pt = scratch[1];
        if pt & 0xfff != 0 {
            return Err(EptHwError::Unsupported);
        }
        core::ptr::write_bytes(pt as *mut u8, 0, 4096);
        let base = (apic >> 21) << 21;
        let pt_entries = pt as *mut u64;
        for j in 0..512u64 {
            let hpa = base + (j << 12);
            // 4K leaf: R/W/X + MT, no large bit
            let leaf = ept_rwe() | ((mt & 0x7) << 3) | (hpa & !0xfff);
            core::ptr::write_volatile(pt_entries.add(j as usize), leaf);
        }
        pd_e = ept_link(pt);
        core::ptr::write_volatile((pd as *mut u64).add(pd_i), pd_e);
        pt
    } else {
        pd_e & !0xfff
    };

    let pt_i = ((apic >> 12) & 0x1ff) as usize;
    core::ptr::write_volatile((pt as *mut u64).add(pt_i), 0); // not present
    invept_global();
    Ok(())
}

/// INVEPT type 2 — all-context (invalidate all EPT-derived TLB).
///
/// Call after EPT edits and before switching to a new EPTP (M4.0 G1).
pub unsafe fn invept_global() {
    #[repr(C, align(16))]
    struct Desc {
        eptp: u64,
        reserved: u64,
    }
    let desc = Desc {
        eptp: 0,
        reserved: 0,
    };
    let typ: u64 = 2;
    // Ignore CF/ZF failure — nested KVM may synthesize; next walk is still coherent.
    core::arch::asm!(
        "invept {typ}, [{desc}]",
        typ = in(reg) typ,
        desc = in(reg) core::ptr::addr_of!(desc),
        options(nostack),
    );
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
/// - `+0`: store MAGIC, loop 4×, COM1 OUT magic, CPUID leaf 1, store ECX, `hlt`
/// - [`GUEST_ISR_OFF`]: ISR stores [`GUEST_IRQ_MAGIC`], then `hlt`
/// - [`GUEST_DATA_OFF`]: magic + counter + IRQ ack + CPUID ECX
///
/// SAFETY: `page_phys` is a writable identity-mapped frame.
pub unsafe fn write_guest_store_page(page_phys: u64) {
    let p = page_phys as *mut u8;
    core::ptr::write_bytes(p, 0, 4096);

    let data = page_phys + GUEST_DATA_OFF;
    let counter = data + 8;
    let cpuid_ecx = page_phys + GUEST_CPUID_ECX_OFF;
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

    // M3.0: out each magic byte to COM1 via DX (imm8 OUT only has an 8-bit port).
    // Reload EDX before every OUT — host Rust clobbers GPRs before VMRESUME.
    for &byte in GUEST_IO_MAGIC {
        // mov edx, 0x3F8
        core::ptr::write_volatile(p.add(o), 0xBA);
        core::ptr::write_volatile(p.add(o + 1), 0xF8);
        core::ptr::write_volatile(p.add(o + 2), 0x03);
        core::ptr::write_volatile(p.add(o + 3), 0x00);
        core::ptr::write_volatile(p.add(o + 4), 0x00);
        o += 5;
        // mov al, imm8
        core::ptr::write_volatile(p.add(o), 0xB0);
        core::ptr::write_volatile(p.add(o + 1), byte);
        o += 2;
        // out dx, al
        core::ptr::write_volatile(p.add(o), 0xEE);
        o += 1;
    }

    // M3.1: CPUID leaf 1, store filtered ECX for host verify.
    // mov eax, 1
    core::ptr::write_volatile(p.add(o), 0xB8);
    core::ptr::write_volatile(p.add(o + 1), 0x01);
    core::ptr::write_volatile(p.add(o + 2), 0x00);
    core::ptr::write_volatile(p.add(o + 3), 0x00);
    core::ptr::write_volatile(p.add(o + 4), 0x00);
    o += 5;
    // xor ecx, ecx
    core::ptr::write_volatile(p.add(o), 0x31);
    core::ptr::write_volatile(p.add(o + 1), 0xC9);
    o += 2;
    // cpuid
    core::ptr::write_volatile(p.add(o), 0x0F);
    core::ptr::write_volatile(p.add(o + 1), 0xA2);
    o += 2;
    // movabs rax, cpuid_ecx_slot
    core::ptr::write_volatile(p.add(o), 0x48);
    core::ptr::write_volatile(p.add(o + 1), 0xB8);
    write_u64_le(p.add(o + 2), cpuid_ecx);
    o += 10;
    // mov [rax], ecx
    core::ptr::write_volatile(p.add(o), 0x89);
    core::ptr::write_volatile(p.add(o + 1), 0x08);
    o += 2;

    // hlt ; jmp $
    core::ptr::write_volatile(p.add(o), 0xF4);
    core::ptr::write_volatile(p.add(o + 1), 0xEB);
    core::ptr::write_volatile(p.add(o + 2), 0xFE);
    o += 3;

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

/// Read back M3.1 filtered CPUID leaf-1 ECX (VMX bit must be clear).
///
/// SAFETY: guest ran CPUID + store; page is the bring-up code frame.
pub unsafe fn verify_guest_cpuid_filtered(page_phys: u64) -> bool {
    let slot = (page_phys + GUEST_CPUID_ECX_OFF) as *const u32;
    let ecx = core::ptr::read_volatile(slot);
    ecx != 0 && (ecx & CPUID_ECX_VMX) == 0
}

/// Read back M2.4/M2.5 ISR ack from the code page.
///
/// SAFETY: guest ISR has run (or not); page is the bring-up code frame.
pub unsafe fn verify_guest_irq(page_phys: u64) -> bool {
    let slot = (page_phys + GUEST_IRQ_SLOT_OFF) as *const u64;
    core::ptr::read_volatile(slot) == GUEST_IRQ_MAGIC
}

/// Clear the IRQ ack slot so a later ISR run can be distinguished (M2.5).
///
/// SAFETY: page is the bring-up code frame; guest not running.
pub unsafe fn clear_guest_irq(page_phys: u64) {
    let slot = (page_phys + GUEST_IRQ_SLOT_OFF) as *mut u64;
    core::ptr::write_volatile(slot, 0);
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
        assert_eq!(frames_required(EptPageSize::TwoMib), 3); // legacy 1 GiB @ 2M
        assert_eq!(frames_required_gib(EptPageSize::TwoMib, 4), 6);
        assert_eq!(frames_required_precise(), 3); // 512 MiB @ 2M → one PD
        assert_eq!(frames_required_2m_bytes(PRECISE_BYTES), 3);
        assert_eq!(PRECISE_BYTES, 512 * 1024 * 1024);
        assert!(PRECISE_BYTES < (1 << 30), "M3.20 window must be < 1 GiB");
        assert!(PRECISE_BYTES > crate::guest::linux_boot::GUEST_RAM_BYTES);
        assert!(PRECISE_BYTES <= crate::arch::apic::DEFAULT_APIC_PHYS);
        assert_eq!(PRECISE_BYTES % TWO_MIB, 0);
        assert_eq!(M3_EPT2_OK_MARKER, "RAYNU-V-M3-EPT2-OK");
        assert_eq!(M3_EPT3_OK_MARKER, "RAYNU-V-M3-EPT3-OK");
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

    #[test]
    fn guest_store_page_encodes_com1_out_dx() {
        let mut page = [0u8; 4096];
        let phys = page.as_mut_ptr() as u64;
        unsafe { write_guest_store_page(phys) };
        // Find first `mov edx, 0x3F8` (BA F8 03 00 00) then `mov al,'R'` / `out dx,al`.
        let mut found = false;
        let mut i = 0usize;
        while i + 8 < 256 {
            if page[i] == 0xBA
                && page[i + 1] == 0xF8
                && page[i + 2] == 0x03
                && page[i + 3] == 0x00
                && page[i + 4] == 0x00
                && page[i + 5] == 0xB0
                && page[i + 6] == b'R'
                && page[i + 7] == 0xEE
            {
                found = true;
                break;
            }
            i += 1;
        }
        assert!(found, "expected mov edx,0x3f8 / mov al,'R' / out dx,al");
        // Must not use 8-bit imm OUT (E6 F8) — that targets port 0xF8, not COM1.
        assert!(!page[..256].windows(2).any(|w| w == [0xE6, 0xF8]));
    }

    #[test]
    fn guest_store_page_encodes_cpuid_leaf1() {
        let mut page = [0u8; 4096];
        let phys = page.as_mut_ptr() as u64;
        unsafe { write_guest_store_page(phys) };
        // Find `mov eax,1` / `xor ecx,ecx` / `cpuid`.
        let mut found = false;
        let mut i = 0usize;
        while i + 9 < 256 {
            if page[i] == 0xB8
                && page[i + 1] == 0x01
                && page[i + 2] == 0x00
                && page[i + 3] == 0x00
                && page[i + 4] == 0x00
                && page[i + 5] == 0x31
                && page[i + 6] == 0xC9
                && page[i + 7] == 0x0F
                && page[i + 8] == 0xA2
            {
                found = true;
                break;
            }
            i += 1;
        }
        assert!(found, "expected mov eax,1 / xor ecx,ecx / cpuid before HLT");
    }
}
