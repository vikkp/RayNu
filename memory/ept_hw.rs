//! Hardware EPT page-table builder (M2.0).
//!
//! Pillar: [V]
//! Proven Core: **inside** (ADR-002, ADR-004)
//! VERIFICATION: L1 — capability MSR gated; identity map only
//!
//! Builds a 4-level EPT that identity-maps GPA→HPA for `[0, 4 GiB)` using
//! 1 GiB pages when available, else 2 MiB pages. That covers OVMF page tables
//! and the early frame pool so a long-mode guest sharing host CR3 can run
//! under EPT.

use crate::arch::cpu::{self, IA32_VMX_EPT_VPID_CAP};

/// COM1 marker when the guest runs under EPT (M2.0 gate).
pub const M2_EPT_OK_MARKER: &str = "RAYNU-V-M2-EPT-OK";

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

/// Write a tiny guest entry into an owned frame: `hlt` then spin.
///
/// SAFETY: `page_phys` is a writable identity-mapped frame.
pub unsafe fn write_guest_hlt_page(page_phys: u64) {
    let p = page_phys as *mut u8;
    core::ptr::write_bytes(p, 0, 4096);
    // hlt
    core::ptr::write_volatile(p, 0xF4);
    // jmp $ (EB FE)
    core::ptr::write_volatile(p.add(1), 0xEB);
    core::ptr::write_volatile(p.add(2), 0xFE);
}

#[cfg(test)]
mod ept_hw_test {
    use super::*;

    #[test]
    fn marker_stable() {
        assert_eq!(M2_EPT_OK_MARKER, "RAYNU-V-M2-EPT-OK");
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
}
