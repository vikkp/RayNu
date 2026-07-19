//! Low-level x86_64 CPU helpers (CPUID, CR0/CR4, MSR).
//!
//! Pillar: [V] [D]
//! Proven Core: **outside** (helpers); callers in `vmx/` are Proven Core.
//! Intel SDM Vol. 2/3 — cited at call sites.

/// CPUID leaf 1 ECX bit 5 — VMX supported.
pub const CPUID_ECX_VMX: u32 = 1 << 5;

/// CR4 bit 13 — VMXE (VMX enable).
pub const CR4_VMXE: u64 = 1 << 13;

/// IA32_FEATURE_CONTROL (MSR 0x3A).
pub const IA32_FEATURE_CONTROL: u32 = 0x3A;
pub const FEATURE_CONTROL_LOCK: u64 = 1 << 0;
pub const FEATURE_CONTROL_VMXON_OUTSIDE_SMX: u64 = 1 << 2;

/// VMX capability MSRs (Intel SDM Vol. 3D / Appendix A).
pub const IA32_VMX_BASIC: u32 = 0x480;
pub const IA32_VMX_PINBASED_CTLS: u32 = 0x481;
pub const IA32_VMX_PROCBASED_CTLS: u32 = 0x482;
pub const IA32_VMX_EXIT_CTLS: u32 = 0x483;
pub const IA32_VMX_ENTRY_CTLS: u32 = 0x484;
pub const IA32_VMX_CR0_FIXED0: u32 = 0x486;
pub const IA32_VMX_CR0_FIXED1: u32 = 0x487;
pub const IA32_VMX_CR4_FIXED0: u32 = 0x488;
pub const IA32_VMX_CR4_FIXED1: u32 = 0x489;
pub const IA32_VMX_PROCBASED_CTLS2: u32 = 0x48B;
/// EPT/VPID capability (SDM Appendix A.10).
pub const IA32_VMX_EPT_VPID_CAP: u32 = 0x48C;
pub const IA32_VMX_TRUE_PINBASED_CTLS: u32 = 0x48D;
pub const IA32_VMX_TRUE_PROCBASED_CTLS: u32 = 0x48E;
pub const IA32_VMX_TRUE_EXIT_CTLS: u32 = 0x48F;
pub const IA32_VMX_TRUE_ENTRY_CTLS: u32 = 0x490;

pub const IA32_EFER: u32 = 0xC000_0080;
pub const IA32_FS_BASE: u32 = 0xC000_0100;
pub const IA32_GS_BASE: u32 = 0xC000_0101;
pub const IA32_SYSENTER_CS: u32 = 0x174;
pub const IA32_SYSENTER_ESP: u32 = 0x175;
pub const IA32_SYSENTER_EIP: u32 = 0x176;
pub const IA32_PAT: u32 = 0x277;

/// Descriptor-table register pointer (SGDT/SIDT).
#[derive(Clone, Copy)]
#[repr(C, packed)]
pub struct DescriptorTablePtr {
    pub limit: u16,
    pub base: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpuidResult {
    pub eax: u32,
    pub ebx: u32,
    pub ecx: u32,
    pub edx: u32,
}

/// Execute CPUID.
///
/// SAFETY: CPUID is a pure register instruction on x86_64.
/// KANI-TARGET: leaf bounds for known leaves used by RayNu-V.
#[inline]
pub unsafe fn cpuid(leaf: u32, subleaf: u32) -> CpuidResult {
    // Use the arch intrinsic — LLVM reserves RBX, so raw `asm!` with ebx fails.
    let r = core::arch::x86_64::__cpuid_count(leaf, subleaf);
    CpuidResult {
        eax: r.eax,
        ebx: r.ebx,
        ecx: r.ecx,
        edx: r.edx,
    }
}

/// True if CPUID.1:ECX.VMX is set.
pub fn vmx_supported() -> bool {
    // SAFETY: CPUID leaf 1 is architecturally defined.
    let r = unsafe { cpuid(1, 0) };
    (r.ecx & CPUID_ECX_VMX) != 0
}

#[inline]
pub unsafe fn read_cr0() -> u64 {
    let v: u64;
    core::arch::asm!("mov {}, cr0", out(reg) v, options(nomem, nostack, preserves_flags));
    v
}

#[inline]
pub unsafe fn write_cr0(v: u64) {
    core::arch::asm!("mov cr0, {}", in(reg) v, options(nostack, preserves_flags));
}

#[inline]
pub unsafe fn read_cr4() -> u64 {
    let v: u64;
    core::arch::asm!("mov {}, cr4", out(reg) v, options(nomem, nostack, preserves_flags));
    v
}

#[inline]
pub unsafe fn write_cr4(v: u64) {
    core::arch::asm!("mov cr4, {}", in(reg) v, options(nostack, preserves_flags));
}

#[inline]
pub unsafe fn rdmsr(msr: u32) -> u64 {
    let lo: u32;
    let hi: u32;
    core::arch::asm!(
        "rdmsr",
        in("ecx") msr,
        out("eax") lo,
        out("edx") hi,
        options(nostack, preserves_flags),
    );
    ((hi as u64) << 32) | (lo as u64)
}

#[inline]
pub unsafe fn wrmsr(msr: u32, value: u64) {
    let lo = value as u32;
    let hi = (value >> 32) as u32;
    core::arch::asm!(
        "wrmsr",
        in("ecx") msr,
        in("eax") lo,
        in("edx") hi,
        options(nostack, preserves_flags),
    );
}

/// Apply VMX CR0 fixed bits: `(cr0 & fixed1) | fixed0`.
pub unsafe fn adjust_cr0_for_vmx() {
    let fixed0 = rdmsr(IA32_VMX_CR0_FIXED0);
    let fixed1 = rdmsr(IA32_VMX_CR0_FIXED1);
    let cr0 = read_cr0();
    write_cr0((cr0 & fixed1) | fixed0);
}

/// Apply VMX CR4 fixed bits and set CR4.VMXE.
pub unsafe fn adjust_cr4_for_vmx() {
    let fixed0 = rdmsr(IA32_VMX_CR4_FIXED0);
    let fixed1 = rdmsr(IA32_VMX_CR4_FIXED1);
    let cr4 = read_cr4();
    write_cr4(((cr4 & fixed1) | fixed0) | CR4_VMXE);
}

/// Ensure IA32_FEATURE_CONTROL allows VMXON outside SMX.
///
/// If unlocked, sets Enable-VMX-outside-SMX and Lock (BIOS-equivalent for QEMU).
pub unsafe fn enable_feature_control_vmx() -> Result<(), FeatureControlError> {
    let mut feat = rdmsr(IA32_FEATURE_CONTROL);
    if (feat & FEATURE_CONTROL_LOCK) == 0 {
        feat |= FEATURE_CONTROL_LOCK | FEATURE_CONTROL_VMXON_OUTSIDE_SMX;
        wrmsr(IA32_FEATURE_CONTROL, feat);
        Ok(())
    } else if (feat & FEATURE_CONTROL_VMXON_OUTSIDE_SMX) == 0 {
        Err(FeatureControlError::LockedWithoutVmx)
    } else {
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeatureControlError {
    /// Lock bit set but VMX outside SMX not enabled (BIOS config).
    LockedWithoutVmx,
}

#[inline]
pub unsafe fn cli() {
    core::arch::asm!("cli", options(nomem, nostack, preserves_flags));
}

#[inline]
pub unsafe fn read_cr2() -> u64 {
    let v: u64;
    core::arch::asm!("mov {}, cr2", out(reg) v, options(nostack, nomem, preserves_flags));
    v
}

pub unsafe fn read_cr3() -> u64 {
    let v: u64;
    core::arch::asm!("mov {}, cr3", out(reg) v, options(nomem, nostack, preserves_flags));
    v
}

#[inline]
pub unsafe fn write_cr3(v: u64) {
    core::arch::asm!("mov cr3, {}", in(reg) v, options(nostack, preserves_flags));
}

#[inline]
pub unsafe fn read_dr7() -> u64 {
    let v: u64;
    core::arch::asm!("mov {}, dr7", out(reg) v, options(nomem, nostack, preserves_flags));
    v
}

#[inline]
pub unsafe fn read_rflags() -> u64 {
    let v: u64;
    core::arch::asm!("pushfq; pop {}", out(reg) v, options(nostack));
    v
}

macro_rules! read_segment {
    ($name:ident, $seg:literal) => {
        #[inline]
        pub unsafe fn $name() -> u16 {
            let v: u16;
            core::arch::asm!(
                concat!("mov {0:x}, ", $seg),
                out(reg) v,
                options(nomem, nostack, preserves_flags)
            );
            v
        }
    };
}

read_segment!(read_cs, "cs");
read_segment!(read_ss, "ss");
read_segment!(read_ds, "ds");
read_segment!(read_es, "es");
read_segment!(read_fs, "fs");
read_segment!(read_gs, "gs");

#[inline]
pub unsafe fn read_tr() -> u16 {
    let v: u16;
    core::arch::asm!("str {0:x}", out(reg) v, options(nomem, nostack, preserves_flags));
    v
}

#[inline]
pub unsafe fn read_ldtr() -> u16 {
    let v: u16;
    core::arch::asm!("sldt {0:x}", out(reg) v, options(nomem, nostack, preserves_flags));
    v
}

#[inline]
pub unsafe fn sgdt() -> DescriptorTablePtr {
    let mut p = DescriptorTablePtr { limit: 0, base: 0 };
    core::arch::asm!("sgdt [{}]", in(reg) &mut p, options(nostack, preserves_flags));
    p
}

#[inline]
pub unsafe fn lgdt(ptr: &DescriptorTablePtr) {
    core::arch::asm!("lgdt [{}]", in(reg) ptr, options(readonly, nostack, preserves_flags));
}

#[inline]
pub unsafe fn sidt() -> DescriptorTablePtr {
    let mut p = DescriptorTablePtr { limit: 0, base: 0 };
    core::arch::asm!("sidt [{}]", in(reg) &mut p, options(nostack, preserves_flags));
    p
}

/// Load the task register (LTR). Descriptor type must be available TSS (9).
#[inline]
pub unsafe fn load_tr(selector: u16) {
    core::arch::asm!("ltr {0:x}", in(reg) selector, options(nostack, preserves_flags));
}

/// Segment limit via LSL (0 if unusable/null).
#[inline]
pub unsafe fn segment_limit(selector: u16) -> u32 {
    if selector & 0xFFFC == 0 {
        return 0;
    }
    let mut limit: u32;
    let mut ok: u8;
    core::arch::asm!(
        "lsl {0:e}, {1:x}",
        "setz {2}",
        out(reg) limit,
        in(reg) selector,
        out(reg_byte) ok,
        options(nomem, nostack),
    );
    if ok == 0 {
        0
    } else {
        limit
    }
}

/// VMCS access-rights encoding from a GDT/LDT descriptor (SDM Vol. 3C).
///
/// SAFETY: `gdt_base` must point at a valid GDT; selector must be in-range or 0.
pub unsafe fn segment_access_rights(gdt_base: u64, selector: u16) -> u32 {
    if selector & 0xFFFC == 0 {
        return 1 << 16; // unusable
    }
    let index = (selector >> 3) as u64;
    let desc = (gdt_base as *const u64).add(index as usize);
    let d0 = core::ptr::read_unaligned(desc);
    // Byte 5 = access; bits 55:52 = G,D/B,L,AVL → VMCS AR[15:12].
    let access = ((d0 >> 40) & 0xFF) as u32;
    let ar = access | (((d0 >> 52) as u32 & 0x0F) << 12);
    ar & 0xF0FF
}

/// Segment base from a legacy GDT descriptor (system descriptors use 16 bytes).
pub unsafe fn segment_base(gdt_base: u64, selector: u16) -> u64 {
    if selector & 0xFFFC == 0 {
        return 0;
    }
    let index = (selector >> 3) as usize;
    let desc = (gdt_base as *const u64).add(index);
    let d0 = core::ptr::read_unaligned(desc);
    let mut base = (d0 >> 16) & 0xFF_FFFF;
    base |= (d0 >> 32) & 0xFF00_0000;
    // TSS/LDT 64-bit system descriptors have base in next qword high half.
    let type_s = ((d0 >> 40) & 0x1F) as u8;
    let s_bit = ((d0 >> 44) & 1) as u8;
    if s_bit == 0 && (type_s == 0x9 || type_s == 0xB || type_s == 0x2) {
        let d1 = core::ptr::read_unaligned(desc.add(1));
        base |= (d1 & 0xFFFF_FFFF) << 32;
    }
    base
}

/// CR0.WP — supervisor write-protect (OVMF keeps this set; page tables are often RO).
const CR0_WP: u64 = 1 << 16;

/// Clear the NX bit on the identity-mapped page containing `phys`.
///
/// OVMF often marks conventional-memory frames non-executable. A guest that
/// shares host CR3 cannot `HLT` from a bump-allocated page until NX is cleared.
/// Page-table pages themselves are typically read-only under CR0.WP, so this
/// temporarily clears WP around the PTE store.
///
/// SAFETY: `phys` is identity-mapped; single-CPU bring-up (CR3 reload for TLB).
pub unsafe fn clear_nx_identity(phys: u64) -> bool {
    const NX: u64 = 1 << 63;
    const PRESENT: u64 = 1;
    const LARGE: u64 = 1 << 7;
    let va = phys;
    let cr3 = read_cr3() & !0xfff;
    let pml4e = *((cr3 + ((va >> 39) & 0x1ff) * 8) as *const u64);
    if pml4e & PRESENT == 0 {
        return false;
    }
    let pdpt = pml4e & 0x000f_ffff_ffff_f000;
    let pdpte = *((pdpt + ((va >> 30) & 0x1ff) * 8) as *const u64);
    if pdpte & PRESENT == 0 {
        return false;
    }

    let entry_ptr: *mut u64;
    let entry: u64;
    if pdpte & LARGE != 0 {
        entry_ptr = (pdpt + ((va >> 30) & 0x1ff) * 8) as *mut u64;
        entry = pdpte;
    } else {
        let pd = pdpte & 0x000f_ffff_ffff_f000;
        let pde = *((pd + ((va >> 21) & 0x1ff) * 8) as *const u64);
        if pde & PRESENT == 0 {
            return false;
        }
        if pde & LARGE != 0 {
            entry_ptr = (pd + ((va >> 21) & 0x1ff) * 8) as *mut u64;
            entry = pde;
        } else {
            let pt = pde & 0x000f_ffff_ffff_f000;
            entry_ptr = (pt + ((va >> 12) & 0x1ff) * 8) as *mut u64;
            entry = *entry_ptr;
            if entry & PRESENT == 0 {
                return false;
            }
        }
    }

    let cr0 = read_cr0();
    write_cr0(cr0 & !CR0_WP);
    core::ptr::write_volatile(entry_ptr, entry & !NX);
    write_cr0(cr0);
    write_cr3(read_cr3());
    true
}

/// Adjust VM-execution control word against allowed0/allowed1 in an MSR.
pub unsafe fn adjust_vmx_controls(wanted: u32, msr: u32) -> u32 {
    let v = rdmsr(msr);
    let allowed0 = v as u32;
    let allowed1 = (v >> 32) as u32;
    (wanted | allowed0) & allowed1
}

pub unsafe fn true_ctl_msrs_supported() -> bool {
    (rdmsr(IA32_VMX_BASIC) & (1u64 << 55)) != 0
}

#[cfg(test)]
mod cpu_test {
    use super::*;

    #[test]
    fn constants_match_sdm() {
        assert_eq!(IA32_FEATURE_CONTROL, 0x3A);
        assert_eq!(IA32_VMX_BASIC, 0x480);
        assert_eq!(IA32_VMX_EPT_VPID_CAP, 0x48C);
        assert_eq!(CR4_VMXE, 1 << 13);
        assert_eq!(IA32_EFER, 0xC000_0080);
    }
}
