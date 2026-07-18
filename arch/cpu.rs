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
pub const IA32_VMX_CR0_FIXED0: u32 = 0x486;
pub const IA32_VMX_CR0_FIXED1: u32 = 0x487;
pub const IA32_VMX_CR4_FIXED0: u32 = 0x488;
pub const IA32_VMX_CR4_FIXED1: u32 = 0x489;

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

#[cfg(test)]
mod cpu_test {
    use super::*;

    #[test]
    fn constants_match_sdm() {
        assert_eq!(IA32_FEATURE_CONTROL, 0x3A);
        assert_eq!(IA32_VMX_BASIC, 0x480);
        assert_eq!(CR4_VMXE, 1 << 13);
    }
}
