//! Guest linear → GPA walk for VMEXIT emulation (M3.11).
//!
//! [`va_to_gpa`] assumes 4-level long-mode paging and identity EPT (GPA=HPA)
//! for the E4 Linux path. Guest-UEFI alias EPT maps GPA → `ram_hpa + GPA`;
//! [`identity_map_not_present`] takes `ram_hpa` so table writes hit the slab.

/// Bits 51:12 of a paging-structure pointer / leaf frame.
const ADDR_MASK: u64 = 0x000f_ffff_ffff_f000;
const PRESENT: u64 = 1;
const RW: u64 = 1 << 1;
const USER: u64 = 1 << 2;
const ACCESSED: u64 = 1 << 5;
const DIRTY: u64 = 1 << 6;
const LARGE: u64 = 1 << 7;
const PCD: u64 = 1 << 4;
const PWT: u64 = 1 << 3;
const TWO_MIB: u64 = 2 * 1024 * 1024;
/// P|RW|US|A|D — CPL0 identity data/exec (NXE is off on guest-UEFI).
const LEAF_FLAGS: u64 = PRESENT | RW | USER | ACCESSED | DIRTY;
const LARGE_2M_FLAGS: u64 = LEAF_FLAGS | LARGE;
/// 2 MiB UC (PCD+PWT → PAT index 3). PCD-only is UC- (`73576cc` ASSERT).
const LARGE_2M_UC_FLAGS: u64 = LARGE_2M_FLAGS | PCD | PWT;

/// How a not-present hole was filled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdentityMapKind {
    Large2M,
    Page4K,
    /// Iron `3311ff3`: GUEST_CR3 was 0 (`fail=alloc`). Loaded SEC PML4.
    Cr3Sec,
    /// Iron `13e8bd2`: walker `fail=present` after SEC CR3; CPU still #PF NP.
    Rebuild4G,
    /// Iron `fdf07ba`: NP #PF in the PCI hole after RAM-only identity.
    Mmio2M,
}

/// Why [`identity_map_not_present`] refused to write a leaf.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdentityMapError {
    OutOfRam,
    TableOutOfRam,
    NeedAlloc,
    AlreadyPresent,
}

/// Translate a guest linear address to a GPA via the guest's CR3.
///
/// SAFETY: `cr3` is the guest CR3 GPA; page-table frames must be identity-mapped
/// in the host (true for RayNu-V bring-up EPT).
pub unsafe fn va_to_gpa(cr3: u64, gva: u64) -> Option<u64> {
    let pml4 = cr3 & ADDR_MASK;
    let e4 = read_entry(pml4, (gva >> 39) & 0x1ff)?;
    if (e4 & PRESENT) == 0 {
        return None;
    }

    let pdpt = e4 & ADDR_MASK;
    let e3 = read_entry(pdpt, (gva >> 30) & 0x1ff)?;
    if (e3 & PRESENT) == 0 {
        return None;
    }
    if (e3 & LARGE) != 0 {
        // 1 GiB page
        let base = e3 & 0x000f_ffff_c000_0000;
        return Some(base | (gva & 0x3fff_ffff));
    }

    let pd = e3 & ADDR_MASK;
    let e2 = read_entry(pd, (gva >> 21) & 0x1ff)?;
    if (e2 & PRESENT) == 0 {
        return None;
    }
    if (e2 & LARGE) != 0 {
        // 2 MiB page
        let base = e2 & 0x000f_ffff_ffe0_0000;
        return Some(base | (gva & 0x1f_ffff));
    }

    let pt = e2 & ADDR_MASK;
    let e1 = read_entry(pt, (gva >> 12) & 0x1ff)?;
    if (e1 & PRESENT) == 0 {
        return None;
    }
    let base = e1 & ADDR_MASK;
    Some(base | (gva & 0xfff))
}

/// Copy `out.len()` bytes from guest linear `gva` into `out`.
///
/// SAFETY: same identity-EPT assumption as [`va_to_gpa`].
pub unsafe fn copy_from_guest_va(cr3: u64, gva: u64, out: &mut [u8]) -> Result<(), ()> {
    let mut i = 0usize;
    while i < out.len() {
        let cur = gva.wrapping_add(i as u64);
        let gpa = va_to_gpa(cr3, cur).ok_or(())?;
        // Read the rest of this page (or remaining bytes).
        let page_left = (0x1000 - (gpa & 0xfff)) as usize;
        let n = (out.len() - i).min(page_left);
        let src = gpa as *const u8;
        for j in 0..n {
            out[i + j] = core::ptr::read_volatile(src.add(j));
        }
        i += n;
    }
    Ok(())
}

unsafe fn read_entry(table_gpa: u64, index: u64) -> Option<u64> {
    if index > 511 {
        return None;
    }
    let p = (table_gpa as *const u64).add(index as usize);
    Some(core::ptr::read_volatile(p))
}

unsafe fn read_entry_ram(ram_hpa: u64, ram_len: u64, table_gpa: u64, index: u64) -> Option<u64> {
    if index > 511 {
        return None;
    }
    let off = (table_gpa & ADDR_MASK).checked_add(index * 8)?;
    if off.saturating_add(8) > ram_len {
        return None;
    }
    let p = (ram_hpa.wrapping_add(off)) as *const u64;
    // SAFETY: `off+8 <= ram_len` and `ram_hpa` is the slab/test buffer base.
    // KANI-TARGET: guest-UEFI alias-EPT page-table read (outside Proven Core walk).
    Some(core::ptr::read_volatile(p))
}

unsafe fn write_entry_ram(ram_hpa: u64, ram_len: u64, table_gpa: u64, index: u64, val: u64) -> bool {
    if index > 511 {
        return false;
    }
    let Some(off) = (table_gpa & ADDR_MASK).checked_add(index * 8) else {
        return false;
    };
    if off.saturating_add(8) > ram_len {
        return false;
    }
    let p = (ram_hpa.wrapping_add(off)) as *mut u64;
    // SAFETY: `off+8 <= ram_len` and `ram_hpa` is the exclusive guest-UEFI slab
    // (or the identity_map unit-test buffer).
    // KANI-TARGET: guest-UEFI alias-EPT page-table write (outside Proven Core walk).
    core::ptr::write_volatile(p, val);
    true
}

/// Fill a not-present identity leaf for `gva` in 4-level tables that live in
/// guest RAM at `ram_hpa` (alias EPT: GPA `g` → HPA `ram_hpa + g`).
///
/// Does **not** allocate new table pages. A missing PML4/PDPT entry, or a
/// 2 MiB hole that would extend past `ram_len`, is [`IdentityMapError::NeedAlloc`].
/// A present leaf is [`IdentityMapError::AlreadyPresent`] (protection #PF).
///
/// Iron `d5fceb1`: CpuDxe finished, then `#PF` `err=0` on `mov al,[0x80B000]`
/// (OVMF 4M MEMFD). Guest paging left that 2 MiB NP; EPT already identity-maps
/// the 32 MiB slab.
///
/// SAFETY: `ram_hpa` is the exclusive guest-UEFI 32 MiB slab (or a test
/// buffer whose table GPAs fit). `cr3` table frames must lie in `[0, ram_len)`.
pub unsafe fn identity_map_not_present(
    cr3: u64,
    gva: u64,
    ram_hpa: u64,
    ram_len: u64,
) -> Result<IdentityMapKind, IdentityMapError> {
    if ram_hpa == 0 || gva >= ram_len {
        return Err(IdentityMapError::OutOfRam);
    }
    let pml4 = cr3 & ADDR_MASK;
    if pml4 >= ram_len {
        return Err(IdentityMapError::TableOutOfRam);
    }
    let e4 = read_entry_ram(ram_hpa, ram_len, pml4, (gva >> 39) & 0x1ff)
        .ok_or(IdentityMapError::TableOutOfRam)?;
    if (e4 & PRESENT) == 0 {
        return Err(IdentityMapError::NeedAlloc);
    }
    let pdpt = e4 & ADDR_MASK;
    let e3 = read_entry_ram(ram_hpa, ram_len, pdpt, (gva >> 30) & 0x1ff)
        .ok_or(IdentityMapError::TableOutOfRam)?;
    if (e3 & PRESENT) == 0 {
        return Err(IdentityMapError::NeedAlloc);
    }
    if (e3 & LARGE) != 0 {
        return Err(IdentityMapError::AlreadyPresent);
    }
    let pd = e3 & ADDR_MASK;
    let idx2 = (gva >> 21) & 0x1ff;
    let e2 = read_entry_ram(ram_hpa, ram_len, pd, idx2).ok_or(IdentityMapError::TableOutOfRam)?;
    if (e2 & PRESENT) == 0 {
        let base = gva & !(TWO_MIB - 1);
        if base.saturating_add(TWO_MIB) > ram_len {
            return Err(IdentityMapError::NeedAlloc);
        }
        if !write_entry_ram(ram_hpa, ram_len, pd, idx2, base | LARGE_2M_FLAGS) {
            return Err(IdentityMapError::TableOutOfRam);
        }
        return Ok(IdentityMapKind::Large2M);
    }
    if (e2 & LARGE) != 0 {
        return Err(IdentityMapError::AlreadyPresent);
    }
    let pt = e2 & ADDR_MASK;
    let idx1 = (gva >> 12) & 0x1ff;
    let e1 = read_entry_ram(ram_hpa, ram_len, pt, idx1).ok_or(IdentityMapError::TableOutOfRam)?;
    if (e1 & PRESENT) != 0 {
        return Err(IdentityMapError::AlreadyPresent);
    }
    let base = gva & !0xFFF;
    if !write_entry_ram(ram_hpa, ram_len, pt, idx1, base | LEAF_FLAGS) {
        return Err(IdentityMapError::TableOutOfRam);
    }
    Ok(IdentityMapKind::Page4K)
}

/// PD (2 MiB) or 1 GiB PDPTE for `gva`. 0 if the walk cannot read an entry.
///
/// Iron `13e8bd2` dump: walker present vs CPU #PF NP.
/// Tables must sit in `[0, ram_len)`; `gva` may be a sink GPA above RAM.
pub unsafe fn identity_walk_pde(
    cr3: u64,
    gva: u64,
    ram_hpa: u64,
    ram_len: u64,
) -> u64 {
    if ram_hpa == 0 {
        return 0;
    }
    let pml4 = cr3 & ADDR_MASK;
    let Some(e4) = read_entry_ram(ram_hpa, ram_len, pml4, (gva >> 39) & 0x1ff) else {
        return 0;
    };
    if (e4 & PRESENT) == 0 {
        return e4;
    }
    let pdpt = e4 & ADDR_MASK;
    let Some(e3) = read_entry_ram(ram_hpa, ram_len, pdpt, (gva >> 30) & 0x1ff) else {
        return 0;
    };
    if (e3 & PRESENT) == 0 || (e3 & LARGE) != 0 {
        return e3;
    }
    let pd = e3 & ADDR_MASK;
    read_entry_ram(ram_hpa, ram_len, pd, (gva >> 21) & 0x1ff).unwrap_or(0)
}

/// OVMF 4M SEC page-table blob: PML4 + PDPT + 4 PDs.
pub const IDENTITY_4G_PAGES: u64 = 6;
pub const IDENTITY_4G_BYTES: u64 = IDENTITY_4G_PAGES * 4096;
/// 4 MiB firmware alias (`0xFFC00000`). EPT already maps this; guest PT
/// must be present or CpuDxe `#PF`s (`5db28e3` `cr2=0xffc00000`).
pub const IDENTITY_FLASH_FLOOR: u64 = 0xFFC0_0000;
/// Live xAPIC 2 MiB (EPT 4 KiB version page). Not a zero sink.
pub const IDENTITY_XAPIC_GPA: u64 = 0xFEE0_0000;
/// Iron `mtrr0=0x80000000` UC hole. Not RAM (ADR-004 / `fdf07ba` ASSERT).
pub const IDENTITY_MTRR_UC_FLOOR: u64 = 0x8000_0000;

fn identity_leaf_flags(gpa: u64, ram_len: u64) -> u64 {
    if gpa < ram_len || gpa >= IDENTITY_FLASH_FLOOR || gpa == IDENTITY_XAPIC_GPA {
        gpa | LARGE_2M_FLAGS
    } else {
        // Iron 73576cc: bulk UC 2MiB for [0x80000000, flash) reopened
        // fdf07ba ASSERT callerrip=0x1d25193 lastmsr=0x23f. Hole stays NP.
        // On-demand identity_map_mmio_2m fills one PAT-UC 2MiB.
        0
    }
}

/// Write a 4-level identity map at `pml4_gpa` (OVMF SEC 6-page layout).
///
/// Iron `3311ff3`: `#PF` `cr3=0x0` `fail=alloc` — PML4 at GPA 0 is empty.
/// Load this as guest CR3 so `0x80B000` / DxeCore RAM are present.
/// Iron `fdf07ba`: filling 4 GiB of WB 2 MiB leaves made MTRR UC (2–4 GiB)
/// look like RAM (`mtrr0=0x80000000` then ASSERT `callerrip=0x1d25193`).
/// Iron `73576cc`: bulk UC 2 MiB in that hole still ASSERTed (PAT UC- vs
/// MTRR UC). Only `[0, ram_len)` plus flash and xAPIC are present. The
/// MTRR hole stays NP; one PAT-UC 2 MiB is filled on sink `#PF`.
///
/// SAFETY: `ram_hpa` is the exclusive guest-UEFI slab (or a test buffer).
/// `pml4_gpa + IDENTITY_4G_BYTES` must lie in `[0, ram_len)`.
pub unsafe fn build_identity_4g(
    ram_hpa: u64,
    ram_len: u64,
    pml4_gpa: u64,
) -> Result<u64, IdentityMapError> {
    let pml4 = pml4_gpa & ADDR_MASK;
    if ram_hpa == 0 || pml4.saturating_add(IDENTITY_4G_BYTES) > ram_len {
        return Err(IdentityMapError::OutOfRam);
    }
    for i in 0..IDENTITY_4G_PAGES {
        let off = pml4 + i * 4096;
        // SAFETY: each page is inside `[pml4, pml4+IDENTITY_4G_BYTES)` ⊂ RAM.
        // KANI-TARGET: guest-UEFI SEC identity CR3 tables (outside Proven Core).
        core::ptr::write_bytes((ram_hpa.wrapping_add(off)) as *mut u8, 0, 4096);
    }
    let pdpt = pml4 + 0x1000;
    if !write_entry_ram(ram_hpa, ram_len, pml4, 0, pdpt | LEAF_FLAGS) {
        return Err(IdentityMapError::TableOutOfRam);
    }
    for g in 0..4u64 {
        let pd = pml4 + 0x2000 + g * 0x1000;
        if !write_entry_ram(ram_hpa, ram_len, pdpt, g, pd | LEAF_FLAGS) {
            return Err(IdentityMapError::TableOutOfRam);
        }
        for i in 0..512u64 {
            let gpa = (g * 512 + i) * TWO_MIB;
            let val = identity_leaf_flags(gpa, ram_len);
            if !write_entry_ram(ram_hpa, ram_len, pd, i, val) {
                return Err(IdentityMapError::TableOutOfRam);
            }
        }
    }
    Ok(pml4)
}

/// Present a 2 MiB UC leaf for a sink GPA (PCI hole / MTRR UC). Tables exist.
///
/// Splits a firmware 1 GiB PDPTE (`eb4b27d` `pde=0xc0400083`) back to the
/// SEC PD. Does **not** back the GPA with RAM (ADR-004). EPT sink-resumes.
///
/// SAFETY: `ram_hpa` is the exclusive guest-UEFI slab (or a test buffer).
/// `cr3` table frames must lie in `[0, ram_len)`.
pub unsafe fn identity_map_mmio_2m(
    cr3: u64,
    gva: u64,
    ram_hpa: u64,
    ram_len: u64,
) -> Result<IdentityMapKind, IdentityMapError> {
    if ram_hpa == 0 {
        return Err(IdentityMapError::OutOfRam);
    }
    let pml4 = cr3 & ADDR_MASK;
    if pml4 >= ram_len {
        return Err(IdentityMapError::TableOutOfRam);
    }
    let e4 = read_entry_ram(ram_hpa, ram_len, pml4, (gva >> 39) & 0x1ff)
        .ok_or(IdentityMapError::TableOutOfRam)?;
    if (e4 & PRESENT) == 0 {
        return Err(IdentityMapError::NeedAlloc);
    }
    let pdpt = e4 & ADDR_MASK;
    let pdpt_i = (gva >> 30) & 0x1ff;
    let e3 = read_entry_ram(ram_hpa, ram_len, pdpt, pdpt_i)
        .ok_or(IdentityMapError::TableOutOfRam)?;
    if (e3 & PRESENT) == 0 {
        return Err(IdentityMapError::NeedAlloc);
    }
    let pd = if (e3 & LARGE) != 0 {
        // Iron eb4b27d: 1GiB PDPTE pde=0xc0400083 (reserved bits 29:13).
        // Restore the SEC PD for this GiB; do not resume on AlreadyPresent.
        let expected_pdpt = pml4 + 0x1000;
        if pdpt != expected_pdpt || pdpt_i > 3 {
            return Err(IdentityMapError::NeedAlloc);
        }
        let pd = pml4 + 0x2000 + pdpt_i * 0x1000;
        if !write_entry_ram(ram_hpa, ram_len, pdpt, pdpt_i, pd | LEAF_FLAGS) {
            return Err(IdentityMapError::TableOutOfRam);
        }
        pd
    } else {
        e3 & ADDR_MASK
    };
    let idx2 = (gva >> 21) & 0x1ff;
    let e2 = read_entry_ram(ram_hpa, ram_len, pd, idx2).ok_or(IdentityMapError::TableOutOfRam)?;
    let base = gva & !(TWO_MIB - 1);
    let want = base | LARGE_2M_UC_FLAGS;
    if e2 == want {
        return Err(IdentityMapError::AlreadyPresent);
    }
    if !write_entry_ram(ram_hpa, ram_len, pd, idx2, want) {
        return Err(IdentityMapError::TableOutOfRam);
    }
    Ok(IdentityMapKind::Mmio2M)
}

#[cfg(test)]
mod guest_pt_test {
    use super::*;

    #[repr(C, align(4096))]
    struct PageTable([u64; 512]);

    unsafe fn set_entry(table: &mut PageTable, index: usize, val: u64) {
        core::ptr::write_volatile(table.0.as_mut_ptr().add(index), val);
    }

    #[test]
    fn walk_4k_identity_map() {
        // VA 0x4000 → PTE index 4 (0x4000 >> 12)
        let mut pml4 = PageTable([0u64; 512]);
        let mut pdpt = PageTable([0u64; 512]);
        let mut pd = PageTable([0u64; 512]);
        let mut pt = PageTable([0u64; 512]);
        let pml4_gpa = pml4.0.as_mut_ptr() as u64;
        let pdpt_gpa = pdpt.0.as_mut_ptr() as u64;
        let pd_gpa = pd.0.as_mut_ptr() as u64;
        let pt_gpa = pt.0.as_mut_ptr() as u64;
        // SAFETY: exclusive test tables; volatile so the walk sees stores.
        unsafe {
            set_entry(&mut pml4, 0, pdpt_gpa | PRESENT);
            set_entry(&mut pdpt, 0, pd_gpa | PRESENT);
            set_entry(&mut pd, 0, pt_gpa | PRESENT);
            set_entry(&mut pt, 4, 0x4000 | PRESENT);
            let gpa = va_to_gpa(pml4_gpa, 0x4000).unwrap();
            assert_eq!(gpa, 0x4000);
            core::hint::black_box(&pml4);
            core::hint::black_box(&pdpt);
            core::hint::black_box(&pd);
            core::hint::black_box(&pt);
        }
    }

    #[test]
    fn walk_2m_large() {
        let mut pml4 = PageTable([0u64; 512]);
        let mut pdpt = PageTable([0u64; 512]);
        let mut pd = PageTable([0u64; 512]);
        let pml4_gpa = pml4.0.as_mut_ptr() as u64;
        let pdpt_gpa = pdpt.0.as_mut_ptr() as u64;
        let pd_gpa = pd.0.as_mut_ptr() as u64;
        unsafe {
            set_entry(&mut pml4, 0, pdpt_gpa | PRESENT);
            set_entry(&mut pdpt, 0, pd_gpa | PRESENT);
            set_entry(&mut pd, 2, (2 << 21) | PRESENT | LARGE);
            let gpa = va_to_gpa(pml4_gpa, (2 << 21) + 0x123).unwrap();
            assert_eq!(gpa, (2 << 21) + 0x123);
            core::hint::black_box(&pml4);
            core::hint::black_box(&pdpt);
            core::hint::black_box(&pd);
        }
    }

    #[test]
    fn identity_map_np_2m_memfd() {
        // Iron d5fceb1: PD[4] for GPA 0x80B000 is NP after CpuDxe.
        let mut ram = vec![0u8; 0x4000];
        let ram_hpa = ram.as_mut_ptr() as u64;
        let ram_len = 32 * 1024 * 1024;
        // SAFETY: exclusive 16KiB buffer; PML4/PDPT GPAs 0/0x1000 fit; PD at 0x2000.
        unsafe {
            write_entry_ram(ram_hpa, ram_len, 0, 0, 0x1000 | PRESENT);
            write_entry_ram(ram_hpa, ram_len, 0x1000, 0, 0x2000 | PRESENT);
            let kind =
                identity_map_not_present(0, 0x80B000, ram_hpa, ram_len).expect("map MEMFD");
            assert_eq!(kind, IdentityMapKind::Large2M);
            let pde = read_entry_ram(ram_hpa, ram_len, 0x2000, 4).unwrap();
            assert_eq!(pde, 0x800000 | LARGE_2M_FLAGS);
            assert_eq!(
                identity_map_not_present(0, 0x80B000, ram_hpa, ram_len),
                Err(IdentityMapError::AlreadyPresent)
            );
        }
    }

    #[test]
    fn identity_map_np_4k_leaf() {
        let mut ram = vec![0u8; 0x4000];
        let ram_hpa = ram.as_mut_ptr() as u64;
        let ram_len = 32 * 1024 * 1024;
        // SAFETY: exclusive 16KiB buffer; PT GPA 0x3000 fits for 4K leaf.
        unsafe {
            write_entry_ram(ram_hpa, ram_len, 0, 0, 0x1000 | PRESENT);
            write_entry_ram(ram_hpa, ram_len, 0x1000, 0, 0x2000 | PRESENT);
            write_entry_ram(ram_hpa, ram_len, 0x2000, 0, 0x3000 | PRESENT);
            let kind = identity_map_not_present(0, 0x4000, ram_hpa, ram_len).expect("map 4K");
            assert_eq!(kind, IdentityMapKind::Page4K);
            let pte = read_entry_ram(ram_hpa, ram_len, 0x3000, 4).unwrap();
            assert_eq!(pte, 0x4000 | LEAF_FLAGS);
        }
    }

    #[test]
    fn identity_map_np_rejects_oob_and_missing_root() {
        let mut ram = vec![0u8; 0x1000];
        let ram_hpa = ram.as_mut_ptr() as u64;
        let ram_len = 32 * 1024 * 1024;
        // SAFETY: exclusive 4KiB PML4; no PDPT so NeedAlloc; OOB gva does not walk.
        unsafe {
            assert_eq!(
                identity_map_not_present(0, ram_len, ram_hpa, ram_len),
                Err(IdentityMapError::OutOfRam)
            );
            assert_eq!(
                identity_map_not_present(0, 0x80B000, ram_hpa, ram_len),
                Err(IdentityMapError::NeedAlloc)
            );
        }
    }

    #[test]
    fn build_identity_4g_maps_memfd() {
        let mut ram = vec![0u8; 0x6000];
        let ram_hpa = ram.as_mut_ptr() as u64;
        let ram_len = 32 * 1024 * 1024;
        // SAFETY: exclusive 6-page buffer is the SEC PML4/PDPT/PD blob.
        unsafe {
            let cr3 = build_identity_4g(ram_hpa, ram_len, 0).expect("build 4G");
            assert_eq!(cr3, 0);
            let pde = read_entry_ram(ram_hpa, ram_len, 0x2000, 4).unwrap();
            assert_eq!(pde, 0x800000 | LARGE_2M_FLAGS);
            assert_eq!(
                identity_map_not_present(0, 0x80B000, ram_hpa, ram_len),
                Err(IdentityMapError::AlreadyPresent)
            );
            assert_eq!(
                identity_walk_pde(0, 0x80B000, ram_hpa, ram_len),
                0x800000 | LARGE_2M_FLAGS
            );
            let pci = read_entry_ram(ram_hpa, ram_len, 0x5000, 0).unwrap();
            assert_eq!(pci, 0);
            let mtrr_uc = read_entry_ram(ram_hpa, ram_len, 0x4000, 0).unwrap();
            assert_eq!(mtrr_uc, 0);
            let above_ram = read_entry_ram(ram_hpa, ram_len, 0x2000, 16).unwrap();
            assert_eq!(above_ram, 0);
            let flash = read_entry_ram(ram_hpa, ram_len, 0x5000, 0x1FE).unwrap();
            assert_eq!(flash, IDENTITY_FLASH_FLOOR | LARGE_2M_FLAGS);
            let xapic = read_entry_ram(ram_hpa, ram_len, 0x5000, 0x1F7).unwrap();
            assert_eq!(xapic, IDENTITY_XAPIC_GPA | LARGE_2M_FLAGS);
            let kind = identity_map_mmio_2m(0, 0xC01D_F1B7, ram_hpa, ram_len).expect("mmio");
            assert_eq!(kind, IdentityMapKind::Mmio2M);
            let uc = read_entry_ram(ram_hpa, ram_len, 0x5000, 0).unwrap();
            assert_eq!(uc, 0xC000_0000 | LARGE_2M_UC_FLAGS);
            assert_eq!(
                identity_map_mmio_2m(0, 0xC01D_F1B7, ram_hpa, ram_len),
                Err(IdentityMapError::AlreadyPresent)
            );
        }
    }

    #[test]
    fn identity_map_mmio_splits_rsvd_1g() {
        // Iron eb4b27d: PDPT[2] = 0xc0400083 (1GiB + reserved bit 22).
        let mut ram = vec![0u8; 0x6000];
        let ram_hpa = ram.as_mut_ptr() as u64;
        let ram_len = 32 * 1024 * 1024;
        unsafe {
            let cr3 = build_identity_4g(ram_hpa, ram_len, 0).expect("build 4G");
            assert_eq!(cr3, 0);
            write_entry_ram(ram_hpa, ram_len, 0x1000, 2, 0xC040_0083);
            assert_eq!(
                identity_walk_pde(0, 0x8000_0008, ram_hpa, ram_len),
                0xC040_0083
            );
            let kind = identity_map_mmio_2m(0, 0x8000_0008, ram_hpa, ram_len).expect("split");
            assert_eq!(kind, IdentityMapKind::Mmio2M);
            let pdpt2 = read_entry_ram(ram_hpa, ram_len, 0x1000, 2).unwrap();
            assert_eq!(pdpt2, 0x4000 | LEAF_FLAGS);
            let pde = read_entry_ram(ram_hpa, ram_len, 0x4000, 0).unwrap();
            assert_eq!(pde, IDENTITY_MTRR_UC_FLOOR | LARGE_2M_UC_FLAGS);
        }
    }
}
