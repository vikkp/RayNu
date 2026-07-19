//! Guest linear → GPA walk for VMEXIT emulation (M3.11).
//!
//! Assumes 4-level long-mode paging and identity EPT for guest RAM (GPA=HPA).
//! Used to fetch instruction bytes at high kernel VAs (not host-identity).

/// Bits 51:12 of a paging-structure pointer / leaf frame.
const ADDR_MASK: u64 = 0x000f_ffff_ffff_f000;
const PRESENT: u64 = 1;
const LARGE: u64 = 1 << 7;

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
}
