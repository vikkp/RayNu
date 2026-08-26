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
/// Non-leaf PDE/PDPTE: no PS, no D (SDM ignored; iron `54a8708` used `LEAF_FLAGS`).
const TABLE_FLAGS: u64 = PRESENT | RW | USER | ACCESSED;
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
    /// Iron `06b011a` / `d757a0a`: present+write `#PF` `err=0x3` after
    /// CR0.WP on a 2 MiB RAM leaf that covers DXE code and heap/stack.
    Split4K,
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

/// PML4E for `gva`, or 0 if the table cannot be read.
pub unsafe fn identity_walk_pml4e(
    cr3: u64,
    gva: u64,
    ram_hpa: u64,
    ram_len: u64,
) -> u64 {
    if ram_hpa == 0 {
        return 0;
    }
    let pml4 = cr3 & ADDR_MASK;
    read_entry_ram(ram_hpa, ram_len, pml4, (gva >> 39) & 0x1ff).unwrap_or(0)
}

/// PDPTE for `gva`, or 0 if the walk cannot read it.
pub unsafe fn identity_walk_pdpte(
    cr3: u64,
    gva: u64,
    ram_hpa: u64,
    ram_len: u64,
) -> u64 {
    let e4 = identity_walk_pml4e(cr3, gva, ram_hpa, ram_len);
    if (e4 & PRESENT) == 0 {
        return 0;
    }
    read_entry_ram(ram_hpa, ram_len, e4 & ADDR_MASK, (gva >> 30) & 0x1ff).unwrap_or(0)
}

/// CR0.WP write is allowed only if every present entry in the walk has R/W.
pub fn identity_walk_is_writable(pml4e: u64, pdpte: u64, pde: u64, pte: u64) -> bool {
    if (pml4e & PRESENT) == 0 || (pml4e & RW) == 0 {
        return false;
    }
    if (pdpte & PRESENT) == 0 || (pdpte & RW) == 0 {
        return false;
    }
    if (pdpte & LARGE) != 0 {
        return true;
    }
    if (pde & PRESENT) == 0 || (pde & RW) == 0 {
        return false;
    }
    if (pde & LARGE) != 0 {
        return true;
    }
    (pte & PRESENT) != 0 && (pte & RW) != 0
}

/// 4 KiB PTE for `gva`, or 0 if the walk is still a 2 MiB leaf / unreadable.
///
/// Iron `54a8708`: after SPLIT4K, PDE `0x219067` (PT at `0x219000`) but
/// `handle_pf` resumed `AlreadyPresent` until the identity cap. Dump the
/// PTE so a still-faulting RW leaf is obvious.
pub unsafe fn identity_walk_pte(
    cr3: u64,
    gva: u64,
    ram_hpa: u64,
    ram_len: u64,
) -> u64 {
    let e2 = identity_walk_pde(cr3, gva, ram_hpa, ram_len);
    if (e2 & PRESENT) == 0 || (e2 & LARGE) != 0 {
        return 0;
    }
    let pt = e2 & ADDR_MASK;
    read_entry_ram(ram_hpa, ram_len, pt, (gva >> 12) & 0x1ff).unwrap_or(0)
}

/// OVMF 4M SEC page-table blob: PML4 + PDPT + 4 PDs, plus 3 pages for
/// sign-extended 32-bit MMIO (PML4[511] PDPT + PDPT[510]/[511] PDs), plus
/// 2 overflow pages for leftover-high 32-bit hole CR2 (iron `577c9eb`
/// `cr2=0x9896808086` walks PML4[1]).
pub const IDENTITY_4G_PAGES: u64 = 11;
pub const IDENTITY_4G_BYTES: u64 = IDENTITY_4G_PAGES * 4096;
/// One 4 KiB PT per 2 MiB of the 32 MiB guest-UEFI slab (`gva >> 21`).
/// Iron `06b011a`: `#PF` `err=0x3` `cr2=0x1d1abb8` `pde=0x1c000e7`.
pub const IDENTITY_SPLIT_PT_PAGES: u64 = 16;
/// e820 reserved: 4G blob plus SPLIT4K PT pool. Do not zero the pool in
/// [`build_identity_4g`] (host tests use a 0xB000 buffer for the blob).
pub const IDENTITY_RESERVED_BYTES: u64 = IDENTITY_4G_BYTES + IDENTITY_SPLIT_PT_PAGES * 4096;
const IDENTITY_HIGH_PDPT_OFF: u64 = 0x6000;
const IDENTITY_HIGH_PD_8000_OFF: u64 = 0x7000;
const IDENTITY_HIGH_PD_C000_OFF: u64 = 0x8000;
const IDENTITY_OVERFLOW_PDPT_OFF: u64 = 0x9000;
const IDENTITY_OVERFLOW_PD_OFF: u64 = 0xA000;
/// 4 MiB firmware alias (`0xFFC00000`). EPT already maps this; guest PT
/// must be present or CpuDxe `#PF`s (`5db28e3` `cr2=0xffc00000`).
pub const IDENTITY_FLASH_FLOOR: u64 = 0xFFC0_0000;
/// Live xAPIC 2 MiB (EPT 4 KiB version page). Not a zero sink.
pub const IDENTITY_XAPIC_GPA: u64 = 0xFEE0_0000;
/// Iron `mtrr0=0x80000000` UC hole. Not RAM (ADR-004 / `fdf07ba` ASSERT).
pub const IDENTITY_MTRR_UC_FLOOR: u64 = 0x8000_0000;
/// Same GPA as guest-UEFI `GUEST_UEFI_HV_PML4` (not MEMFD `0x800000`).
pub const IDENTITY_HV_PML4: u64 = 0x200000;

/// Iron `124c1a8`: long-mode CR2 `0xffffffff96808086` is sign-extended
/// `0x96808086`. The CPU walks PML4[511], not the low 4G PDPT.
pub fn identity_signext32_gpa(gva: u64) -> Option<u64> {
    if gva >> 32 == 0xFFFF_FFFF {
        Some(gva as u32 as u64)
    } else {
        None
    }
}

/// Iron `577c9eb`: CR2 `0x9896808086` is leftover high dword `0x98` plus
/// 32-bit hole GPA `0x96808086` (not canonical sign-extend). Walks PML4[1].
pub fn identity_trunc32_hole_gpa(gva: u64) -> Option<u64> {
    let hi = gva >> 32;
    let lo = gva as u32 as u64;
    if hi != 0 && hi != 0xFFFF_FFFF && lo >= IDENTITY_MTRR_UC_FLOOR && lo < IDENTITY_FLASH_FLOOR {
        Some(lo)
    } else {
        None
    }
}

/// Canonical 32-bit hole GPA from a sign-extended or leftover-high CR2.
pub fn identity_hole32_gpa(gva: u64) -> Option<u64> {
    identity_signext32_gpa(gva).or_else(|| identity_trunc32_hole_gpa(gva))
}

fn identity_high_pd_off(pdpt_i: u64) -> Option<u64> {
    match pdpt_i {
        510 => Some(IDENTITY_HIGH_PD_8000_OFF),
        511 => Some(IDENTITY_HIGH_PD_C000_OFF),
        _ => None,
    }
}

fn identity_leaf_flags(gpa: u64, ram_len: u64) -> u64 {
    if gpa < ram_len {
        gpa | LARGE_2M_FLAGS
    } else if gpa < IDENTITY_MTRR_UC_FLOOR {
        // Iron 1a93cb8: PAT WB (`pat=0x7010600070406`) still ASSERT
        // `callerrip=0x1d25193` `lastmsr=0x23f`. MTRR default WB
        // (`mtrrdef=0xc06`) covers 0–2GiB; one UC pair at 2GiB
        // (`mtrr0=0x80000000`). CpuDxe RefreshGcdMemoryAttributes
        // software-walks NP `[ram_len, 2GiB)` vs MTRR WB. Guest PT
        // WB 2MiB for the mid-gap. Do **not** EPT-map `[32MiB, 2GiB)`
        // (iron `89c3731` RIP `0x27e22d5` executed hole RO zeros
        // when that window was R+X).
        gpa | LARGE_2M_FLAGS
    } else if gpa < 0x1_0000_0000 {
        // Iron 32ee302: WB 2MiB xAPIC/flash sit in firmware's 2–4GiB
        // MTRR UC (`mtrr0=0x80000000` `mtrr1=0x3fff80000800`).
        // Iron 73576cc: bulk **PCD-only** (PAT UC-) reopened ASSERT
        // `callerrip=0x1d25193`. Iron `8df2793`: PDPT[2]=`0x204067` (PD,
        // not 1GiB WB) then ASSERT with **no** xAPIC `#PF` — CpuDxe
        // software-walks NP 2–4GiB vs MTRR UC. PAT-UC PCD+PWT (index 3)
        // matches MTRR UC. Guest PT only; EPT still sink/scratch
        // (ADR-004).
        gpa | LARGE_2M_UC_FLAGS
    } else {
        0
    }
}

/// Unused 32 MiB slab / OVMF debug-fill. Present + reserved phys bits.
/// Iron `d757a0a`: `#PF` `err=0x9` `pde=0xafafafafafafafaf`.
pub const IDENTITY_POISON_PTE: u64 = 0xAFAF_AFAF_AFAF_AFAF;

pub fn identity_pde_is_poison(pde: u64) -> bool {
    pde == IDENTITY_POISON_PTE
}

/// Rewrite every 2 MiB slot in a low-4G PD (0–2GiB WB, MTRR hole PAT-UC).
///
/// Iron `d757a0a`: SPLIT restored SEC PD at `pml4+0x2000` after firmware
/// promoted PDPT[0] to `0xc0000083` and 0xAF-filled the PD. One 2 MiB
/// leaf left DXE at `0x1d1e6cb` walking `pde=0xafafafafafafafaf`.
///
/// SAFETY: `pd_gpa` is a 4 KiB table in `[0, ram_len)`.
unsafe fn identity_refill_low4g_pd(
    ram_hpa: u64,
    ram_len: u64,
    pd_gpa: u64,
    pdpt_i: u64,
) -> Result<(), IdentityMapError> {
    if pdpt_i > 3 {
        return Ok(());
    }
    for i in 0..512u64 {
        let gpa = (pdpt_i * 512 + i) * TWO_MIB;
        let val = identity_leaf_flags(gpa, ram_len);
        if !write_entry_ram(ram_hpa, ram_len, pd_gpa, i, val) {
            return Err(IdentityMapError::TableOutOfRam);
        }
    }
    Ok(())
}

/// True when a PDE is a 4K PT (SPLIT4K). Do not overwrite with a 2 MiB leaf.
pub fn identity_pde_is_4k_table(e: u64) -> bool {
    (e & PRESENT) != 0 && (e & LARGE) == 0
}

/// Fill NP / 2 MiB slots; leave SPLIT4K 4K PTs.
///
/// Iron `162809f`: `maxpa=32` `mtrr1=0x80000800` `pml4e=0x1a02023` (no
/// PWT) then ASSERT `callerrip=0x1d25193` `lastmsr=0x23f` with **no**
/// 4G n=1. Firmware PDPT at `0x1a02000`; `pde20=0x2000083` only.
/// `[32MiB, 1GiB)` in PDPT[0] can stay NP vs MTRR WB. Do not smash
/// 4K tables in 32 MiB RAM (`54a8708`).
///
/// SAFETY: `pd_gpa` is a 4 KiB table in `[0, ram_len)`.
unsafe fn identity_refill_low4g_pd_keep_4k(
    ram_hpa: u64,
    ram_len: u64,
    pd_gpa: u64,
    pdpt_i: u64,
) -> Result<u32, IdentityMapError> {
    if pdpt_i > 3 {
        return Ok(0);
    }
    let mut n = 0u32;
    for i in 0..512u64 {
        let gpa = (pdpt_i * 512 + i) * TWO_MIB;
        let want = identity_leaf_flags(gpa, ram_len);
        let Some(e) = read_entry_ram(ram_hpa, ram_len, pd_gpa, i) else {
            continue;
        };
        if identity_pde_is_4k_table(e) {
            continue;
        }
        if e != want {
            if !write_entry_ram(ram_hpa, ram_len, pd_gpa, i, want) {
                return Err(IdentityMapError::TableOutOfRam);
            }
            n = n.saturating_add(1);
        }
    }
    Ok(n)
}

/// Other 1 GiB PDPT index in firmware's MTRR UC hole `[2GiB, 4GiB)`.
///
/// Iron PAT-UC `48c598a`/`855ba1c`: xAPIC `#PF` split PDPT[3] (`0xc0600083`)
/// then ASSERT `callerrip=0x1d25193`. PDPT[2] can stay a clean 1 GiB WB
/// page over `[2GiB, 3GiB)` — no RSVD, no `#PF`, WB vs MTRR UC.
pub fn identity_mtrr_uc_sibling_pdpt(pdpt_i: u64) -> Option<u64> {
    match pdpt_i {
        2 => Some(3),
        3 => Some(2),
        _ => None,
    }
}

/// True when a PDPTE is a present 1 GiB page (PS). Firmware may set RSVD
/// bits (`0xc0400083` bit 22, `0xc0600083` bits 21–22).
pub fn identity_pdpte_is_1g(e: u64) -> bool {
    (e & PRESENT) != 0 && (e & LARGE) != 0
}

/// Split a firmware 1 GiB PDPTE back to the SEC PD (RAM-only / UC leaves).
///
/// SAFETY: `ram_hpa` is the exclusive guest-UEFI slab (or a test buffer).
unsafe fn identity_split_1g_pdpte(
    ram_hpa: u64,
    ram_len: u64,
    pml4: u64,
    pdpt: u64,
    pdpt_i: u64,
) -> Result<u64, IdentityMapError> {
    if pdpt_i > 3 {
        return Err(IdentityMapError::NeedAlloc);
    }
    let e3 = read_entry_ram(ram_hpa, ram_len, pdpt, pdpt_i)
        .ok_or(IdentityMapError::TableOutOfRam)?;
    if (e3 & PRESENT) == 0 {
        return Err(IdentityMapError::NeedAlloc);
    }
    if (e3 & LARGE) == 0 {
        let pd = e3 & ADDR_MASK;
        if pd >= ram_len {
            return Err(IdentityMapError::TableOutOfRam);
        }
        return Ok(pd);
    }
    let sec_pd = pml4 + 0x2000 + pdpt_i * 0x1000;
    if pml4 != IDENTITY_HV_PML4 && pdpt != pml4 + 0x1000 {
        return Err(IdentityMapError::NeedAlloc);
    }
    if sec_pd.saturating_add(0x1000) > ram_len {
        return Err(IdentityMapError::TableOutOfRam);
    }
    if !write_entry_ram(ram_hpa, ram_len, pdpt, pdpt_i, sec_pd | LEAF_FLAGS) {
        return Err(IdentityMapError::TableOutOfRam);
    }
    identity_refill_low4g_pd(ram_hpa, ram_len, sec_pd, pdpt_i)?;
    Ok(sec_pd)
}

/// Point PDPT[`pdpt_i`] at the SEC PD and fill 2 MiB leaves (NP or 1GiB).
///
/// Iron `28f42d2`: `pde20=0x20000e7` (PDPT[0] mid-gap WB) still ASSERT
/// `callerrip=0x1d25193` `lastmsr=0x23f`. Live firmware PDPT `0x5000`
/// gets PDPT[0]/[2]/[3] from SPLIT + hole sync; PDPT[1] (1–2GiB) can
/// stay NP vs MTRR default WB. Do not refill PDPT[0] here (SPLIT4K
/// 4K tables in 32 MiB RAM). Guest PT only; do not EPT-map 1–2GiB.
///
/// SAFETY: `ram_hpa` is the exclusive guest-UEFI slab (or a test buffer).
unsafe fn identity_ensure_pdpt_2m(
    ram_hpa: u64,
    ram_len: u64,
    pml4: u64,
    pdpt: u64,
    pdpt_i: u64,
) -> Result<u64, IdentityMapError> {
    if pdpt_i > 3 {
        return Err(IdentityMapError::NeedAlloc);
    }
    if pml4 != IDENTITY_HV_PML4 && pdpt != pml4 + 0x1000 {
        return Err(IdentityMapError::NeedAlloc);
    }
    let e3 = read_entry_ram(ram_hpa, ram_len, pdpt, pdpt_i)
        .ok_or(IdentityMapError::TableOutOfRam)?;
    if (e3 & PRESENT) != 0 && (e3 & LARGE) == 0 {
        let pd = e3 & ADDR_MASK;
        if pd >= ram_len {
            return Err(IdentityMapError::TableOutOfRam);
        }
        identity_refill_low4g_pd(ram_hpa, ram_len, pd, pdpt_i)?;
        return Ok(pd);
    }
    let sec_pd = pml4 + 0x2000 + pdpt_i * 0x1000;
    if sec_pd.saturating_add(0x1000) > ram_len {
        return Err(IdentityMapError::TableOutOfRam);
    }
    if !write_entry_ram(ram_hpa, ram_len, pdpt, pdpt_i, sec_pd | LEAF_FLAGS) {
        return Err(IdentityMapError::TableOutOfRam);
    }
    identity_refill_low4g_pd(ram_hpa, ram_len, sec_pd, pdpt_i)?;
    Ok(sec_pd)
}

/// Split firmware 1 GiB PDPT[2] and PDPT[3] (MTRR UC 2–4 GiB) back to SEC PDs.
///
/// Iron COM2 after sibling-on-xAPIC: `#PF` `pdpte2=0xc0400083` then
/// `identity MMIO n=4` `pde=0xfee000ff` then ASSERT `callerrip=0x1d25193`.
/// CpuDxe walks PDPT[2] in software (no extra `#PF`). RAM SPLIT n=2 uses
/// `pdpt_i=0` so sibling-of-current never touched the hole. Split both
/// hole GiBs on every identity map, including `0x1e9000`.
///
/// SAFETY: `ram_hpa` is the exclusive guest-UEFI slab (or a test buffer).
pub unsafe fn identity_split_mtrr_uc_hole(
    ram_hpa: u64,
    ram_len: u64,
    pml4: u64,
    pdpt: u64,
) -> u32 {
    let mut n = 0u32;
    for i in 2..=3u64 {
        let Some(e) = read_entry_ram(ram_hpa, ram_len, pdpt, i) else {
            continue;
        };
        if !identity_pdpte_is_1g(e) {
            continue;
        }
        if identity_split_1g_pdpte(ram_hpa, ram_len, pml4, pdpt, i).is_ok() {
            n += 1;
        }
    }
    n
}

/// Firmware PDPT GPA on iron after 4G (`pml4e=0x5a6d` / `0x5a6f`).
pub const IDENTITY_FW_PDPT_GPA: u64 = 0x5000;
/// Iron `be1b028` ASSERT `pml4e=0x5a6f` (PWT). Hardware PAT uses the leaf.
pub const IDENTITY_IRON_PML4E_PWT: u64 = 0x5A6F;
/// PDPT[3] / first 2 MiB of 3 GiB (dump `pdpte3=`).
pub const IDENTITY_MTRR_UC_3G: u64 = 0xC000_0000;
/// PDPT[1] / first 2 MiB of 1 GiB (dump `pde4000=` / `pdpte1=`).
pub const IDENTITY_WB_1G: u64 = 0x4000_0000;
/// 64 MiB in PDPT[0] mid-gap (dump `pde40=`). Iron `162809f` `pde20` only.
pub const IDENTITY_WB_64M: u64 = 0x400_0000;

fn identity_2m_is_wb_not_uc(e: u64) -> bool {
    (e & PRESENT) != 0 && (e & LARGE) != 0 && (e & (PCD | PWT)) != (PCD | PWT)
}

/// Restore PAT-UC 2 MiB in a hole PD. Leaves 4K tables alone (EPT scratch
/// PT stores). NP / poison / WB 2 MiB become PAT-UC.
///
/// SAFETY: `pd_gpa` is a 4 KiB table in `[0, ram_len)`.
unsafe fn identity_refresh_mtrr_uc_pd(
    ram_hpa: u64,
    ram_len: u64,
    pd_gpa: u64,
    pdpt_i: u64,
) -> u32 {
    let mut n = 0u32;
    if pdpt_i < 2 || pdpt_i > 3 {
        return 0;
    }
    for i in 0..512u64 {
        let gpa = (pdpt_i * 512 + i) * TWO_MIB;
        let want = identity_leaf_flags(gpa, ram_len);
        if want == 0 {
            continue;
        }
        let Some(e) = read_entry_ram(ram_hpa, ram_len, pd_gpa, i) else {
            continue;
        };
        if identity_pde_is_poison(e) || e == 0 || identity_2m_is_wb_not_uc(e) {
            if write_entry_ram(ram_hpa, ram_len, pd_gpa, i, want) {
                n += 1;
            }
        }
    }
    n
}

unsafe fn identity_sync_one_pdpt(
    ram_hpa: u64,
    ram_len: u64,
    pml4: u64,
    pdpt: u64,
) -> u32 {
    let mut n = identity_split_mtrr_uc_hole(ram_hpa, ram_len, pml4, pdpt);
    if let Some(e0) = read_entry_ram(ram_hpa, ram_len, pdpt, 0) {
        if (e0 & PRESENT) != 0 && (e0 & LARGE) == 0 {
            if identity_refill_low4g_pd_keep_4k(ram_hpa, ram_len, e0 & ADDR_MASK, 0).is_ok() {
                n += 1;
            }
        } else if identity_ensure_pdpt_2m(ram_hpa, ram_len, pml4, pdpt, 0).is_ok() {
            n += 1;
        }
    }
    if identity_ensure_pdpt_2m(ram_hpa, ram_len, pml4, pdpt, 1).is_ok() {
        n += 1;
    }
    for i in 2..=3u64 {
        let Some(e) = read_entry_ram(ram_hpa, ram_len, pdpt, i) else {
            continue;
        };
        if (e & PRESENT) == 0 || (e & LARGE) != 0 {
            continue;
        }
        n += identity_refresh_mtrr_uc_pd(ram_hpa, ram_len, e & ADDR_MASK, i);
    }
    n
}

/// Clear PWT/PCD on a table PML4E/PDPTE (not a 1GiB PS leaf).
///
/// Iron `be1b028`: 0–4GiB leaves match MTRR (`pde20=0x20000e7`
/// `pde4000=0x400000e7` `pde8000=0x800000ff`) then ASSERT
/// `callerrip=0x1d25193` `pml4e=0x5a6f` (bit 3 PWT). Hardware paging
/// uses the leaf PAT; EDK2 PageTableLib software-walks may OR non-leaf
/// PWT/PCD and see WT vs MTRR WB.
pub fn identity_clear_table_pwt_pcd(e: u64) -> u64 {
    if (e & PRESENT) == 0 || (e & LARGE) != 0 {
        e
    } else {
        e & !(PWT | PCD)
    }
}

/// Split 1 GiB and restore PAT-UC on the PDPT PML4[0] actually walks.
///
/// Iron `d7bfb23`: 4G `pde8000=0x800000ff` then SPLIT4K `pml4e=0x5a6d`
/// `pdpte2=0x204067` (PD) still ASSERT `callerrip=0x1d25193`. 4G filled
/// `pml4+0x1000`; CpuDxe software-walks the retargeted PDPT at `0x5000`
/// ([`IDENTITY_FW_PDPT_GPA`]). Iron `1de9389`: `pdpte3=0x205067` (PS
/// clear) still ASSERTed — 1GiB PDPT[3] is **not** the remaining cause.
/// Do **not** treat GPA `0x5000` as a PDPT until PML4[0] points there.
///
/// SAFETY: `ram_hpa` is the exclusive guest-UEFI slab (or a test buffer).
pub unsafe fn identity_sync_live_mtrr_uc_hole(
    ram_hpa: u64,
    ram_len: u64,
    cr3: u64,
) -> u32 {
    if ram_hpa == 0 {
        return 0;
    }
    let pml4 = cr3 & ADDR_MASK;
    if pml4 >= ram_len {
        return 0;
    }
    let Some(e4) = read_entry_ram(ram_hpa, ram_len, pml4, 0) else {
        return 0;
    };
    if (e4 & PRESENT) == 0 {
        return 0;
    }
    let e4c = identity_clear_table_pwt_pcd(e4);
    if e4c != e4 && !write_entry_ram(ram_hpa, ram_len, pml4, 0, e4c) {
        return 0;
    }
    let pdpt = e4c & ADDR_MASK;
    if pdpt >= ram_len {
        return 0;
    }
    for i in 0..4u64 {
        let Some(e) = read_entry_ram(ram_hpa, ram_len, pdpt, i) else {
            continue;
        };
        let c = identity_clear_table_pwt_pcd(e);
        if c != e {
            let _ = write_entry_ram(ram_hpa, ram_len, pdpt, i, c);
        }
    }
    identity_sync_one_pdpt(ram_hpa, ram_len, pml4, pdpt)
}

/// Write a 4-level identity map at `pml4_gpa` (OVMF SEC 6-page layout plus
/// 3 high-half pages).
///
/// Iron `3311ff3`: `#PF` `cr3=0x0` `fail=alloc` — PML4 at GPA 0 is empty.
/// Load this as guest CR3 so `0x80B000` / DxeCore RAM are present.
/// Iron `fdf07ba`: filling 4 GiB of WB 2 MiB leaves made MTRR UC (2–4 GiB)
/// look like RAM (`mtrr0=0x80000000` then ASSERT `callerrip=0x1d25193`).
/// Iron `73576cc`: bulk **PCD-only** (PAT UC-) still ASSERTed. Iron
/// `8df2793`: hole PD present, 2 MiB leaves NP, ASSERT with no xAPIC
/// `#PF`. `[2GiB, 4GiB)` is PAT-UC PCD+PWT. Iron `1a93cb8`: `[ram_len, 2GiB)`
/// is guest-PT WB (MTRR default WB); EPT still does not map that window.
/// Iron `124c1a8`: PML4[511] is linked so a sign-extended 32-bit CR2 can
/// take an on-demand 2 MiB leaf (GPA stays zero-extended; not bulk 2–4 GiB).
/// Iron `577c9eb`: leftover-high CR2 uses overflow PDPT+PD (PML4[1]).
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
    // Sign-extended 32-bit MMIO walks PML4[511] (iron 124c1a8). PDPT[510]
    // covers VA 0xFFFFFFFF80000000 (GPA 0x80000000); PDPT[511] covers
    // 0xFFFFFFFFC0000000 (GPA 0xC0000000). Leaves stay NP until a #PF.
    let high_pdpt = pml4 + IDENTITY_HIGH_PDPT_OFF;
    if !write_entry_ram(ram_hpa, ram_len, pml4, 511, high_pdpt | LEAF_FLAGS) {
        return Err(IdentityMapError::TableOutOfRam);
    }
    if !write_entry_ram(
        ram_hpa,
        ram_len,
        high_pdpt,
        510,
        (pml4 + IDENTITY_HIGH_PD_8000_OFF) | LEAF_FLAGS,
    ) {
        return Err(IdentityMapError::TableOutOfRam);
    }
    if !write_entry_ram(
        ram_hpa,
        ram_len,
        high_pdpt,
        511,
        (pml4 + IDENTITY_HIGH_PD_C000_OFF) | LEAF_FLAGS,
    ) {
        return Err(IdentityMapError::TableOutOfRam);
    }
    Ok(pml4)
}

/// Present a 2 MiB UC leaf for a sink GPA (PCI hole / MTRR UC). Tables exist.
///
/// Splits a firmware 1 GiB PDPTE (`eb4b27d` `pde=0xc0400083`, iron
/// `7413554` `pdpte=0xc0600083` over xAPIC) back to the SEC PD.
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
    let pml4_i = (gva >> 39) & 0x1ff;
    let tables_ok = pml4.saturating_add(IDENTITY_4G_BYTES) <= ram_len;
    let leftover = identity_trunc32_hole_gpa(gva).is_some();
    let mut e4 = read_entry_ram(ram_hpa, ram_len, pml4, pml4_i)
        .ok_or(IdentityMapError::TableOutOfRam)?;
    if (e4 & PRESENT) == 0 {
        if pml4_i == 511 && tables_ok {
            let high_pdpt = pml4 + IDENTITY_HIGH_PDPT_OFF;
            if !write_entry_ram(ram_hpa, ram_len, pml4, 511, high_pdpt | LEAF_FLAGS) {
                return Err(IdentityMapError::TableOutOfRam);
            }
            e4 = high_pdpt | LEAF_FLAGS;
        } else if leftover && tables_ok {
            let ov = pml4 + IDENTITY_OVERFLOW_PDPT_OFF;
            if !write_entry_ram(ram_hpa, ram_len, pml4, pml4_i, ov | LEAF_FLAGS) {
                return Err(IdentityMapError::TableOutOfRam);
            }
            e4 = ov | LEAF_FLAGS;
        } else {
            return Err(IdentityMapError::NeedAlloc);
        }
    }
    let pdpt = e4 & ADDR_MASK;
    let pdpt_i = (gva >> 30) & 0x1ff;
    let mut e3 = read_entry_ram(ram_hpa, ram_len, pdpt, pdpt_i)
        .ok_or(IdentityMapError::TableOutOfRam)?;
    if (e3 & PRESENT) == 0 {
        if let Some(off) = identity_high_pd_off(pdpt_i) {
            if pml4_i != 511 || !tables_ok {
                return Err(IdentityMapError::NeedAlloc);
            }
            let pd_gpa = pml4 + off;
            if !write_entry_ram(ram_hpa, ram_len, pdpt, pdpt_i, pd_gpa | LEAF_FLAGS) {
                return Err(IdentityMapError::TableOutOfRam);
            }
            e3 = pd_gpa | LEAF_FLAGS;
        } else if leftover && tables_ok {
            let ov = pml4 + IDENTITY_OVERFLOW_PD_OFF;
            if !write_entry_ram(ram_hpa, ram_len, pdpt, pdpt_i, ov | LEAF_FLAGS) {
                return Err(IdentityMapError::TableOutOfRam);
            }
            e3 = ov | LEAF_FLAGS;
        } else {
            return Err(IdentityMapError::NeedAlloc);
        }
    }
    // Iron COM2: pdpte2=0xc0400083 at RAM SPLIT n=2 and at xAPIC #PF.
    // Sibling-of-current only ran when pdpt_i was 2 or 3, so the 0x1e9000
    // split left 2–3GiB as a 1GiB WB page. CpuDxe software-walks it.
    // Iron d7bfb23: 4G PAT-UC then firmware PDPT 0x5000; sync live PDPT.
    // Iron 1de9389: do this on MMIO/SPLIT4K/4G, not on the preemption tick
    // (CpuDxe MTRR walk at `0x1d6be4`).
    let _ = identity_sync_live_mtrr_uc_hole(ram_hpa, ram_len, cr3);
    e3 = read_entry_ram(ram_hpa, ram_len, pdpt, pdpt_i)
        .ok_or(IdentityMapError::TableOutOfRam)?;
    let pd = if (e3 & LARGE) != 0 {
        // Iron eb4b27d / a428202: 1GiB PDPTE pde=0xc0400083. Firmware may
        // retarget PML4[0] at a PDPT in MEMFD so pdpt != pml4+0x1000
        // (iron a428202 printed identity MMIO fail). Restore the SEC PD
        // for this GiB into the PDPT the CPU actually walked.
        identity_split_1g_pdpte(ram_hpa, ram_len, pml4, pdpt, pdpt_i)?
    } else {
        let pd = e3 & ADDR_MASK;
        if pd >= ram_len {
            return Err(IdentityMapError::TableOutOfRam);
        }
        pd
    };
    let idx2 = (gva >> 21) & 0x1ff;
    let mut e2 = read_entry_ram(ram_hpa, ram_len, pd, idx2).ok_or(IdentityMapError::TableOutOfRam)?;
    if identity_pde_is_poison(e2) {
        identity_refill_low4g_pd(ram_hpa, ram_len, pd, pdpt_i)?;
        e2 = read_entry_ram(ram_hpa, ram_len, pd, idx2).ok_or(IdentityMapError::TableOutOfRam)?;
    }
    // Iron 124c1a8 / 577c9eb: high-half or leftover-high VA, leaf GPA is
    // the zero-extended 32-bit hole so EPT scratch still applies.
    let leaf_gpa = identity_hole32_gpa(gva).unwrap_or(gva) & !(TWO_MIB - 1);
    let flags = identity_leaf_flags(leaf_gpa, ram_len);
    let want = if flags != 0 {
        flags
    } else {
        leaf_gpa | LARGE_2M_UC_FLAGS
    };
    if e2 == want {
        return Err(IdentityMapError::AlreadyPresent);
    }
    if !write_entry_ram(ram_hpa, ram_len, pd, idx2, want) {
        return Err(IdentityMapError::TableOutOfRam);
    }
    Ok(IdentityMapKind::Mmio2M)
}

/// SPLIT4K slot for a 32 MiB GPA: one PT per 2 MiB (`0x1d1abb8` → slot 14).
pub fn identity_split_pt_slot(gva: u64) -> u64 {
    (gva >> 21) % IDENTITY_SPLIT_PT_PAGES
}

/// GPA of the SPLIT4K PT for `gva` under HV/SEC PML4 `pml4`.
pub fn identity_split_pt_gpa(pml4: u64, gva: u64) -> u64 {
    (pml4 & ADDR_MASK) + IDENTITY_4G_BYTES + identity_split_pt_slot(gva) * 4096
}

/// Split a present 2 MiB RAM leaf to 512 4 KiB RW PTEs, or OR R/W on the
/// **whole walk** (PML4/PDPT/PD/PTE). Do **not** rebuild 4G (`fdf07ba`).
///
/// Iron `06b011a`: after RAM 1GiB SPLIT n=2, `#PF` `err=0x3` `cr2=0x1d1abb8`
/// `pde=0x1c000e7` `rip=0x1de592` (`CR0.WP` stack push).
/// Iron `89c3731`: SPLIT4K n=2 then `pte=0x1d1a067` already RW and stop.
/// CR0.WP ANDs R/W through PML4/PDPT; a RO PML4E still #PFs a RW 4K leaf.
///
/// SAFETY: `ram_hpa` is the exclusive guest-UEFI slab (or a test buffer).
/// `cr3` table frames and the SPLIT4K PT must lie in `[0, ram_len)`.
pub unsafe fn identity_fix_ram_wp(
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
    let _ = identity_sync_live_mtrr_uc_hole(ram_hpa, ram_len, cr3);
    let idx4 = (gva >> 39) & 0x1ff;
    let mut e4 = read_entry_ram(ram_hpa, ram_len, pml4, idx4)
        .ok_or(IdentityMapError::TableOutOfRam)?;
    if (e4 & PRESENT) == 0 {
        return Err(IdentityMapError::NeedAlloc);
    }
    let mut changed = false;
    if (e4 & RW) == 0 {
        e4 |= RW;
        if !write_entry_ram(ram_hpa, ram_len, pml4, idx4, e4) {
            return Err(IdentityMapError::TableOutOfRam);
        }
        changed = true;
    }
    let pdpt = e4 & ADDR_MASK;
    let idx3 = (gva >> 30) & 0x1ff;
    let mut e3 = read_entry_ram(ram_hpa, ram_len, pdpt, idx3)
        .ok_or(IdentityMapError::TableOutOfRam)?;
    if (e3 & PRESENT) == 0 {
        return Err(IdentityMapError::NeedAlloc);
    }
    if (e3 & RW) == 0 {
        e3 |= RW;
        if !write_entry_ram(ram_hpa, ram_len, pdpt, idx3, e3) {
            return Err(IdentityMapError::TableOutOfRam);
        }
        changed = true;
    }
    if (e3 & LARGE) != 0 {
        return if changed {
            Ok(IdentityMapKind::Split4K)
        } else {
            Err(IdentityMapError::NeedAlloc)
        };
    }
    let pd = e3 & ADDR_MASK;
    let idx2 = (gva >> 21) & 0x1ff;
    let mut e2 = read_entry_ram(ram_hpa, ram_len, pd, idx2).ok_or(IdentityMapError::TableOutOfRam)?;
    if (e2 & PRESENT) == 0 {
        return Err(IdentityMapError::NeedAlloc);
    }
    if (e2 & LARGE) != 0 {
        let phys_2m = e2 & 0x000f_ffff_ffe0_0000;
        let pt = identity_split_pt_gpa(pml4, gva);
        if pt.saturating_add(4096) > ram_len {
            return Err(IdentityMapError::TableOutOfRam);
        }
        for i in 0..512u64 {
            let leaf = phys_2m.wrapping_add(i * 4096) | LEAF_FLAGS;
            if !write_entry_ram(ram_hpa, ram_len, pt, i, leaf) {
                return Err(IdentityMapError::TableOutOfRam);
            }
        }
        if !write_entry_ram(ram_hpa, ram_len, pd, idx2, pt | TABLE_FLAGS) {
            return Err(IdentityMapError::TableOutOfRam);
        }
        return Ok(IdentityMapKind::Split4K);
    }
    if (e2 & RW) == 0 {
        e2 |= RW;
        if !write_entry_ram(ram_hpa, ram_len, pd, idx2, e2) {
            return Err(IdentityMapError::TableOutOfRam);
        }
        changed = true;
    }
    let pt = e2 & ADDR_MASK;
    let idx1 = (gva >> 12) & 0x1ff;
    let e1 = read_entry_ram(ram_hpa, ram_len, pt, idx1).ok_or(IdentityMapError::TableOutOfRam)?;
    if (e1 & PRESENT) == 0 {
        return Err(IdentityMapError::NeedAlloc);
    }
    if (e1 & RW) == 0 {
        if !write_entry_ram(ram_hpa, ram_len, pt, idx1, e1 | RW) {
            return Err(IdentityMapError::TableOutOfRam);
        }
        changed = true;
    }
    if changed {
        Ok(IdentityMapKind::Split4K)
    } else {
        Err(IdentityMapError::AlreadyPresent)
    }
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
        let mut ram = vec![0u8; 0xB000];
        let ram_hpa = ram.as_mut_ptr() as u64;
        let ram_len = 32 * 1024 * 1024;
        // SAFETY: exclusive 11-page buffer is the SEC PML4/PDPT/PD blob plus high half plus overflow.
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
            assert_eq!(pci, 0xC000_0000 | LARGE_2M_UC_FLAGS);
            let mtrr_uc = read_entry_ram(ram_hpa, ram_len, 0x4000, 0).unwrap();
            assert_eq!(mtrr_uc, IDENTITY_MTRR_UC_FLOOR | LARGE_2M_UC_FLAGS);
            let above_ram = read_entry_ram(ram_hpa, ram_len, 0x2000, 16).unwrap();
            assert_eq!(above_ram, ram_len | LARGE_2M_FLAGS);
            let gib1 = read_entry_ram(ram_hpa, ram_len, 0x3000, 0).unwrap();
            assert_eq!(gib1, 0x4000_0000 | LARGE_2M_FLAGS);
            let flash = read_entry_ram(ram_hpa, ram_len, 0x5000, 0x1FE).unwrap();
            assert_eq!(flash, IDENTITY_FLASH_FLOOR | LARGE_2M_UC_FLAGS);
            let xapic = read_entry_ram(ram_hpa, ram_len, 0x5000, 0x1F7).unwrap();
            assert_eq!(xapic, IDENTITY_XAPIC_GPA | LARGE_2M_UC_FLAGS);
            assert_eq!(
                identity_map_mmio_2m(0, 0xC01D_F1B7, ram_hpa, ram_len),
                Err(IdentityMapError::AlreadyPresent)
            );
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
        let mut ram = vec![0u8; 0xB000];
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
            let r = identity_map_mmio_2m(0, 0x8000_0008, ram_hpa, ram_len);
            assert!(
                matches!(
                    r,
                    Ok(IdentityMapKind::Mmio2M) | Err(IdentityMapError::AlreadyPresent)
                ),
                "{r:?}"
            );
            let pdpt2 = read_entry_ram(ram_hpa, ram_len, 0x1000, 2).unwrap();
            assert_eq!(pdpt2, 0x4000 | LEAF_FLAGS);
            let pde = read_entry_ram(ram_hpa, ram_len, 0x4000, 0).unwrap();
            assert_eq!(pde, IDENTITY_MTRR_UC_FLOOR | LARGE_2M_UC_FLAGS);
        }
    }

    #[test]
    fn identity_map_mmio_splits_1g_retargeted_pdpt() {
        // Iron a428202: CR3=0x200000, pde=0xc0400083, identity MMIO fail.
        // PML4[0] pointed at a PDPT that was not pml4+0x1000.
        let mut ram = vec![0u8; 0x210000];
        let ram_hpa = ram.as_mut_ptr() as u64;
        let ram_len = 32 * 1024 * 1024;
        unsafe {
            let cr3 = build_identity_4g(ram_hpa, ram_len, IDENTITY_HV_PML4).expect("build 4G");
            assert_eq!(cr3, IDENTITY_HV_PML4);
            write_entry_ram(ram_hpa, ram_len, IDENTITY_HV_PML4, 0, 0x8000 | LEAF_FLAGS);
            write_entry_ram(ram_hpa, ram_len, 0x8000, 2, 0xC040_0083);
            let r = identity_map_mmio_2m(IDENTITY_HV_PML4, 0x8000_0008, ram_hpa, ram_len);
            assert!(
                matches!(
                    r,
                    Ok(IdentityMapKind::Mmio2M) | Err(IdentityMapError::AlreadyPresent)
                ),
                "{r:?}"
            );
            let pdpt2 = read_entry_ram(ram_hpa, ram_len, 0x8000, 2).unwrap();
            assert_eq!(pdpt2, 0x204000 | LEAF_FLAGS);
            let pde = read_entry_ram(ram_hpa, ram_len, 0x204000, 0).unwrap();
            assert_eq!(pde, IDENTITY_MTRR_UC_FLOOR | LARGE_2M_UC_FLAGS);
        }
    }

    #[test]
    fn identity_map_mmio_splits_xapic_rsvd_1g() {
        // Iron 7413554: after SPLIT4K resumed, #PF cr2=0xfee00020 err=0x9
        // pml4e=0x5a6f (PDPT at 0x5000) pdpte=0xc0600083 (1GiB + RSVD).
        let mut ram = vec![0u8; 0x210000];
        let ram_hpa = ram.as_mut_ptr() as u64;
        let ram_len = 32 * 1024 * 1024;
        let cr2 = 0xFEE0_0020u64;
        unsafe {
            let cr3 = build_identity_4g(ram_hpa, ram_len, IDENTITY_HV_PML4).expect("build 4G");
            assert_eq!(cr3, IDENTITY_HV_PML4);
            write_entry_ram(ram_hpa, ram_len, IDENTITY_HV_PML4, 0, 0x5000 | PRESENT);
            write_entry_ram(ram_hpa, ram_len, 0x5000, 2, 0xC040_0083);
            write_entry_ram(ram_hpa, ram_len, 0x5000, 3, 0xC060_0083);
            assert_eq!(
                identity_walk_pdpte(IDENTITY_HV_PML4, cr2, ram_hpa, ram_len),
                0xC060_0083
            );
            assert_eq!(identity_mtrr_uc_sibling_pdpt(3), Some(2));
            assert_eq!(identity_mtrr_uc_sibling_pdpt(2), Some(3));
            assert_eq!(identity_mtrr_uc_sibling_pdpt(0), None);
            let r = identity_map_mmio_2m(IDENTITY_HV_PML4, cr2, ram_hpa, ram_len);
            assert!(
                matches!(
                    r,
                    Ok(IdentityMapKind::Mmio2M) | Err(IdentityMapError::AlreadyPresent)
                ),
                "{r:?}"
            );
            let pdpt3 = read_entry_ram(ram_hpa, ram_len, 0x5000, 3).unwrap();
            assert_eq!(pdpt3, 0x205000 | LEAF_FLAGS);
            assert_eq!((pdpt3 & LARGE), 0);
            let pdpt2 = read_entry_ram(ram_hpa, ram_len, 0x5000, 2).unwrap();
            assert_eq!(pdpt2, 0x204000 | LEAF_FLAGS);
            assert_eq!((pdpt2 & LARGE), 0);
            let hole2 = read_entry_ram(ram_hpa, ram_len, 0x204000, 0).unwrap();
            assert_eq!(
                hole2,
                IDENTITY_MTRR_UC_FLOOR | LARGE_2M_UC_FLAGS,
                "2-3GiB PAT-UC 2MiB; not WB 1GiB and not NP"
            );
            let pde = identity_walk_pde(IDENTITY_HV_PML4, cr2, ram_hpa, ram_len);
            assert_eq!(pde, IDENTITY_XAPIC_GPA | LARGE_2M_UC_FLAGS);
        }
    }

    #[test]
    fn identity_sync_live_mtrr_uc_hole_splits_fw_pdpt3() {
        // Iron d7bfb23: 4G pde8000=0x800000ff then firmware pml4e=0x5a6d
        // PDPT at 0x5000. PDPT[2] can already be a table while PDPT[3] is
        // still 1GiB WB (CpuDxe software-walk, no extra #PF).
        let mut ram = vec![0u8; 0x220000];
        let ram_hpa = ram.as_mut_ptr() as u64;
        let ram_len = 32 * 1024 * 1024;
        unsafe {
            let cr3 = build_identity_4g(ram_hpa, ram_len, IDENTITY_HV_PML4).expect("build 4G");
            assert_eq!(cr3, IDENTITY_HV_PML4);
            write_entry_ram(ram_hpa, ram_len, IDENTITY_HV_PML4, 0, 0x5000 | PRESENT);
            write_entry_ram(ram_hpa, ram_len, 0x5000, 0, 0x202000 | LEAF_FLAGS);
            write_entry_ram(ram_hpa, ram_len, 0x5000, 2, 0xC040_0083);
            write_entry_ram(ram_hpa, ram_len, 0x5000, 3, 0xC060_0083);
            let n = identity_sync_live_mtrr_uc_hole(ram_hpa, ram_len, IDENTITY_HV_PML4);
            assert!(n >= 2, "split both hole 1GiB PDPTEs, n={n}");
            let pdpt3 = read_entry_ram(ram_hpa, ram_len, 0x5000, 3).unwrap();
            assert_eq!(pdpt3, 0x205000 | LEAF_FLAGS);
            assert_eq!((pdpt3 & LARGE), 0);
            let pde3 = read_entry_ram(ram_hpa, ram_len, pdpt3 & !0xFFF, 0).unwrap();
            assert_eq!(pde3, IDENTITY_MTRR_UC_3G | LARGE_2M_UC_FLAGS);
            let pdpt2 = read_entry_ram(ram_hpa, ram_len, 0x5000, 2).unwrap();
            assert_eq!(pdpt2, 0x204000 | LEAF_FLAGS);
            assert_eq!((pdpt2 & LARGE), 0);
            let hole2 = read_entry_ram(ram_hpa, ram_len, pdpt2 & !0xFFF, 0).unwrap();
            assert_eq!(hole2, IDENTITY_MTRR_UC_FLOOR | LARGE_2M_UC_FLAGS);
            let pdpt1 = read_entry_ram(ram_hpa, ram_len, 0x5000, 1).unwrap();
            assert_eq!(pdpt1, 0x203000 | LEAF_FLAGS, "live PDPT[1] NP vs MTRR WB");
            assert_eq!(
                identity_walk_pde(IDENTITY_HV_PML4, IDENTITY_WB_1G, ram_hpa, ram_len),
                IDENTITY_WB_1G | LARGE_2M_FLAGS
            );
            let r = identity_fix_ram_wp(IDENTITY_HV_PML4, 0x1D1_ABB8, ram_hpa, ram_len);
            assert!(
                matches!(
                    r,
                    Ok(IdentityMapKind::Split4K) | Err(IdentityMapError::AlreadyPresent)
                ),
                "{r:?}"
            );
            let pdpt3b = read_entry_ram(ram_hpa, ram_len, 0x5000, 3).unwrap();
            assert_eq!((pdpt3b & LARGE), 0);
        }
    }

    #[test]
    fn identity_sync_clears_iron_pml4e_pwt() {
        // Iron be1b028 ASSERT pml4e=0x5a6f (PWT) after 0-4GiB leaves matched.
        assert_eq!(identity_clear_table_pwt_pcd(IDENTITY_IRON_PML4E_PWT), 0x5A67);
        assert_eq!(identity_clear_table_pwt_pcd(0xC040_0083), 0xC040_0083);
        let mut ram = vec![0u8; 0x220000];
        let ram_hpa = ram.as_mut_ptr() as u64;
        let ram_len = 32 * 1024 * 1024;
        unsafe {
            let cr3 = build_identity_4g(ram_hpa, ram_len, IDENTITY_HV_PML4).expect("build 4G");
            assert_eq!(cr3, IDENTITY_HV_PML4);
            write_entry_ram(ram_hpa, ram_len, IDENTITY_HV_PML4, 0, IDENTITY_IRON_PML4E_PWT);
            write_entry_ram(ram_hpa, ram_len, 0x5000, 0, 0x202000 | LEAF_FLAGS);
            write_entry_ram(ram_hpa, ram_len, 0x5000, 2, 0xC040_0083);
            write_entry_ram(ram_hpa, ram_len, 0x5000, 3, 0xC060_0083);
            let _n = identity_sync_live_mtrr_uc_hole(ram_hpa, ram_len, IDENTITY_HV_PML4);
            let pml4e = read_entry_ram(ram_hpa, ram_len, IDENTITY_HV_PML4, 0).unwrap();
            assert_eq!(pml4e & (1 << 3), 0, "PWT cleared pml4e=0x{pml4e:x}");
            assert_eq!(pml4e & 0xFFF_FFFF_FFFF_F000, IDENTITY_FW_PDPT_GPA);
        }
    }

    #[test]
    fn identity_sync_fills_pdpt0_keep_4k() {
        // Iron 162809f: maxpa=32 pml4e=0x1a02023 pde20=0x2000083, no 4G;
        // firmware PD sparse; SPLIT4K 4K tables must survive.
        assert!(identity_pde_is_4k_table(0x211000 | TABLE_FLAGS));
        assert!(!identity_pde_is_4k_table(0x2000083));
        let mut ram = vec![0u8; 0x220000];
        let ram_hpa = ram.as_mut_ptr() as u64;
        let ram_len = 32 * 1024 * 1024;
        unsafe {
            let cr3 = build_identity_4g(ram_hpa, ram_len, IDENTITY_HV_PML4).expect("build 4G");
            assert_eq!(cr3, IDENTITY_HV_PML4);
            write_entry_ram(ram_hpa, ram_len, IDENTITY_HV_PML4, 0, 0x5000 | PRESENT | RW | ACCESSED);
            write_entry_ram(ram_hpa, ram_len, 0x5000, 0, 0x210000 | PRESENT | RW);
            write_entry_ram(ram_hpa, ram_len, 0x210000, 16, 0x2000083);
            write_entry_ram(ram_hpa, ram_len, 0x210000, 0, 0x211000 | TABLE_FLAGS);
            write_entry_ram(ram_hpa, ram_len, 0x211000, 0, 0x1000 | LEAF_FLAGS);
            let _n = identity_sync_live_mtrr_uc_hole(ram_hpa, ram_len, IDENTITY_HV_PML4);
            assert_eq!(
                identity_walk_pde(IDENTITY_HV_PML4, IDENTITY_WB_64M, ram_hpa, ram_len),
                IDENTITY_WB_64M | LARGE_2M_FLAGS
            );
            assert_eq!(
                identity_walk_pde(IDENTITY_HV_PML4, 0x2000000, ram_hpa, ram_len),
                0x2000000 | LARGE_2M_FLAGS
            );
            let split4k = read_entry_ram(ram_hpa, ram_len, 0x210000, 0).unwrap();
            assert_eq!(split4k, 0x211000 | TABLE_FLAGS, "SPLIT4K PT must survive");
            let pte = read_entry_ram(ram_hpa, ram_len, 0x211000, 0).unwrap();
            assert_eq!(pte, 0x1000 | LEAF_FLAGS);
        }
    }

    #[test]
    fn identity_sync_skips_gpa_5000_until_pml4_retarget() {
        // Do not treat low-RAM 0x5000 as a PDPT while PML4[0] still walks
        // pml4+0x1000 (iron 4G window before CpuDxe retarget).
        let mut ram = vec![0u8; 0x220000];
        let ram_hpa = ram.as_mut_ptr() as u64;
        let ram_len = 32 * 1024 * 1024;
        unsafe {
            let cr3 = build_identity_4g(ram_hpa, ram_len, IDENTITY_HV_PML4).expect("build 4G");
            assert_eq!(cr3, IDENTITY_HV_PML4);
            write_entry_ram(ram_hpa, ram_len, IDENTITY_FW_PDPT_GPA, 3, 0xC060_0083);
            let _n = identity_sync_live_mtrr_uc_hole(ram_hpa, ram_len, IDENTITY_HV_PML4);
            let planted = read_entry_ram(ram_hpa, ram_len, IDENTITY_FW_PDPT_GPA, 3).unwrap();
            assert_eq!(planted, 0xC060_0083);
        }
    }

    #[test]
    fn identity_map_mmio_signext32_high_half() {
        // Iron 124c1a8: identity MMIO n=2 then #PF cr2=0xffffffff96808086
        // err=0x2 pde=0 (PML4[511] walk; leaf must be GPA 0x96800000).
        let mut ram = vec![0u8; 0xB000];
        let ram_hpa = ram.as_mut_ptr() as u64;
        let ram_len = 32 * 1024 * 1024;
        let cr2 = 0xFFFF_FFFF_9680_8086u64;
        assert_eq!(identity_signext32_gpa(cr2), Some(0x9680_8086));
        assert_eq!(identity_signext32_gpa(0x8000_0008), None);
        unsafe {
            let cr3 = build_identity_4g(ram_hpa, ram_len, 0).expect("build 4G");
            assert_eq!(cr3, 0);
            let pml4_511 = read_entry_ram(ram_hpa, ram_len, 0, 511).unwrap();
            assert_eq!(pml4_511, 0x6000 | LEAF_FLAGS);
            assert_eq!(identity_walk_pde(0, cr2, ram_hpa, ram_len), 0);
            let kind = identity_map_mmio_2m(0, cr2, ram_hpa, ram_len).expect("signext");
            assert_eq!(kind, IdentityMapKind::Mmio2M);
            let pde = identity_walk_pde(0, cr2, ram_hpa, ram_len);
            assert_eq!(pde, 0x9680_0000 | LARGE_2M_UC_FLAGS);
            // Iron 8df2793: low 4G [2GiB, 4GiB) is PAT-UC at 4G rebuild so
            // CpuDxe software-walks UC not NP. High-half is a separate PD.
            let low = identity_walk_pde(0, 0x9680_8086, ram_hpa, ram_len);
            assert_eq!(low, 0x9680_0000 | LARGE_2M_UC_FLAGS);
            assert_eq!(
                identity_map_mmio_2m(0, cr2, ram_hpa, ram_len),
                Err(IdentityMapError::AlreadyPresent)
            );
            let c0 = 0xFFFF_FFFF_C020_0000u64;
            let kind = identity_map_mmio_2m(0, c0, ram_hpa, ram_len).expect("c000");
            assert_eq!(kind, IdentityMapKind::Mmio2M);
            assert_eq!(
                identity_walk_pde(0, c0, ram_hpa, ram_len),
                0xC020_0000 | LARGE_2M_UC_FLAGS
            );
        }
    }

    #[test]
    fn identity_map_mmio_trunc32_leftover_high() {
        // Iron 577c9eb: CR2 0x9896808086 walks PML4[1]; leaf GPA is 0x96800000.
        let mut ram = vec![0u8; 0xB000];
        let ram_hpa = ram.as_mut_ptr() as u64;
        let ram_len = 32 * 1024 * 1024;
        let cr2 = 0x0000_0098_9680_8086u64;
        assert_eq!(identity_trunc32_hole_gpa(cr2), Some(0x9680_8086));
        assert_eq!(identity_hole32_gpa(cr2), Some(0x9680_8086));
        assert_eq!(identity_signext32_gpa(cr2), None);
        assert_eq!(identity_trunc32_hole_gpa(0xFFFF_FFFF_9680_8086), None);
        unsafe {
            let cr3 = build_identity_4g(ram_hpa, ram_len, 0).expect("build 4G");
            assert_eq!(cr3, 0);
            assert_eq!(identity_walk_pde(0, cr2, ram_hpa, ram_len), 0);
            let kind = identity_map_mmio_2m(0, cr2, ram_hpa, ram_len).expect("trunc32");
            assert_eq!(kind, IdentityMapKind::Mmio2M);
            let pde = identity_walk_pde(0, cr2, ram_hpa, ram_len);
            assert_eq!(pde, 0x9680_0000 | LARGE_2M_UC_FLAGS);
            let pml4_1 = read_entry_ram(ram_hpa, ram_len, 0, 1).unwrap();
            assert_eq!(pml4_1, IDENTITY_OVERFLOW_PDPT_OFF | LEAF_FLAGS);
        }
    }

    #[test]
    fn identity_map_mmio_splits_ram_1g_heap() {
        // Iron 471391f: PDPT[0] = 0xc0000083 covering VA 0x1e9000.
        // Iron d757a0a: firmware 0xAF-filled the SEC PD after the 1GiB
        // promote; SPLIT must refill so 0x1d1e6cb is WB not poison.
        let mut ram = vec![0u8; 0xB000];
        let ram_hpa = ram.as_mut_ptr() as u64;
        let ram_len = 32 * 1024 * 1024;
        unsafe {
            let cr3 = build_identity_4g(ram_hpa, ram_len, 0).expect("build 4G");
            assert_eq!(cr3, 0);
            for i in 0..512u64 {
                write_entry_ram(ram_hpa, ram_len, 0x2000, i, IDENTITY_POISON_PTE);
            }
            write_entry_ram(ram_hpa, ram_len, 0x1000, 0, 0xC000_0083);
            write_entry_ram(ram_hpa, ram_len, 0x1000, 2, 0xC040_0083);
            write_entry_ram(ram_hpa, ram_len, 0x1000, 3, 0xC060_0083);
            assert_eq!(identity_walk_pde(0, 0x1E9000, ram_hpa, ram_len), 0xC000_0083);
            assert!(identity_pdpte_is_1g(0xC040_0083));
            assert!(identity_pdpte_is_1g(0xC060_0083));
            let r = identity_map_mmio_2m(0, 0x1E9000, ram_hpa, ram_len);
            assert!(
                matches!(
                    r,
                    Ok(IdentityMapKind::Mmio2M) | Err(IdentityMapError::AlreadyPresent)
                ),
                "{r:?}"
            );
            let pdpt0 = read_entry_ram(ram_hpa, ram_len, 0x1000, 0).unwrap();
            assert_eq!(pdpt0, 0x2000 | LEAF_FLAGS);
            let pdpt2 = read_entry_ram(ram_hpa, ram_len, 0x1000, 2).unwrap();
            assert_eq!((pdpt2 & LARGE), 0, "RAM split must split PDPT[2] 1GiB WB");
            let hole2 = read_entry_ram(ram_hpa, ram_len, pdpt2 & !0xFFF, 0).unwrap();
            assert_eq!(
                hole2,
                IDENTITY_MTRR_UC_FLOOR | LARGE_2M_UC_FLAGS,
                "2-3GiB PAT-UC 2MiB; not WB 1GiB and not NP"
            );
            let pde = identity_walk_pde(0, 0x1E9000, ram_hpa, ram_len);
            assert_eq!(pde, LARGE_2M_FLAGS);
            let code = identity_walk_pde(0, 0x1D1_E6CB, ram_hpa, ram_len);
            assert_eq!(code, 0x1C0_0000 | LARGE_2M_FLAGS);
            assert!(!identity_pde_is_poison(code));
            let mid = identity_walk_pde(0, ram_len, ram_hpa, ram_len);
            assert_eq!(mid, ram_len | LARGE_2M_FLAGS);
        }
    }

    #[test]
    fn identity_map_mmio_refills_poison_pd() {
        // Iron d757a0a SPLIT n=3: PD pointer live, PD[14]=0xAF, err=0x9.
        let mut ram = vec![0u8; 0xB000];
        let ram_hpa = ram.as_mut_ptr() as u64;
        let ram_len = 32 * 1024 * 1024;
        unsafe {
            let cr3 = build_identity_4g(ram_hpa, ram_len, 0).expect("build 4G");
            assert_eq!(cr3, 0);
            write_entry_ram(ram_hpa, ram_len, 0x2000, 14, IDENTITY_POISON_PTE);
            assert_eq!(
                identity_walk_pde(0, 0x1D1_E6CB, ram_hpa, ram_len),
                IDENTITY_POISON_PTE
            );
            let r = identity_map_mmio_2m(0, 0x1D1_E6CB, ram_hpa, ram_len);
            assert!(
                matches!(
                    r,
                    Ok(IdentityMapKind::Mmio2M) | Err(IdentityMapError::AlreadyPresent)
                ),
                "{r:?}"
            );
            assert_eq!(
                identity_walk_pde(0, 0x1D1_E6CB, ram_hpa, ram_len),
                0x1C0_0000 | LARGE_2M_FLAGS
            );
            assert_eq!(
                identity_walk_pde(0, 0x1E9000, ram_hpa, ram_len),
                LARGE_2M_FLAGS
            );
        }
    }

    #[test]
    fn identity_fix_ram_wp_splits_2m_to_4k() {
        // Iron 06b011a: err=0x3 cr2=0x1d1abb8 pde=0x1c000e7 rip=0x1de592.
        let mut ram = vec![0u8; IDENTITY_RESERVED_BYTES as usize];
        let ram_hpa = ram.as_mut_ptr() as u64;
        let ram_len = 32 * 1024 * 1024;
        let cr2 = 0x1D1_ABB8u64;
        assert_eq!(identity_split_pt_slot(cr2), 14);
        assert_eq!(IDENTITY_RESERVED_BYTES, 0x1B000);
        unsafe {
            let cr3 = build_identity_4g(ram_hpa, ram_len, 0).expect("build 4G");
            assert_eq!(cr3, 0);
            assert_eq!(
                identity_walk_pde(0, cr2, ram_hpa, ram_len),
                0x1C0_0000 | LARGE_2M_FLAGS
            );
            let kind = identity_fix_ram_wp(0, cr2, ram_hpa, ram_len).expect("SPLIT4K");
            assert_eq!(kind, IdentityMapKind::Split4K);
            let pde = identity_walk_pde(0, cr2, ram_hpa, ram_len);
            let pt = identity_split_pt_gpa(0, cr2);
            assert_eq!(pde, pt | TABLE_FLAGS);
            assert_eq!((pde & LARGE), 0);
            assert_eq!(
                identity_walk_pte(0, cr2, ram_hpa, ram_len),
                (cr2 & !0xFFF) | LEAF_FLAGS
            );
            let pte = read_entry_ram(ram_hpa, ram_len, pt, (cr2 >> 12) & 0x1ff).unwrap();
            assert_eq!(pte, (cr2 & !0xFFF) | LEAF_FLAGS);
            assert_ne!(pte & RW, 0);
            let first = read_entry_ram(ram_hpa, ram_len, pt, 0).unwrap();
            assert_eq!(first, 0x1C0_0000 | LEAF_FLAGS);
            assert_eq!(identity_split_pt_slot(0x1DE_592), 0);
            assert_eq!(
                identity_fix_ram_wp(0, cr2, ram_hpa, ram_len),
                Err(IdentityMapError::AlreadyPresent)
            );
            // Iron 89c3731: leaf already RW but CR0.WP still #PF if PML4E.W=0.
            let pml4e = identity_walk_pml4e(0, cr2, ram_hpa, ram_len);
            assert_ne!(pml4e & RW, 0);
            write_entry_ram(ram_hpa, ram_len, 0, 0, pml4e & !RW);
            assert!(!identity_walk_is_writable(
                identity_walk_pml4e(0, cr2, ram_hpa, ram_len),
                identity_walk_pdpte(0, cr2, ram_hpa, ram_len),
                identity_walk_pde(0, cr2, ram_hpa, ram_len),
                identity_walk_pte(0, cr2, ram_hpa, ram_len),
            ));
            assert_eq!(
                identity_fix_ram_wp(0, cr2, ram_hpa, ram_len).expect("walk RW"),
                IdentityMapKind::Split4K
            );
            assert_ne!(identity_walk_pml4e(0, cr2, ram_hpa, ram_len) & RW, 0);
            assert!(identity_walk_is_writable(
                identity_walk_pml4e(0, cr2, ram_hpa, ram_len),
                identity_walk_pdpte(0, cr2, ram_hpa, ram_len),
                identity_walk_pde(0, cr2, ram_hpa, ram_len),
                identity_walk_pte(0, cr2, ram_hpa, ram_len),
            ));
        }
    }
}
