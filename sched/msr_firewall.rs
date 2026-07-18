//! MSR / CPUID / CR access firewalls.
//!
//! Pillar: [V] · Proven Core · VERIFICATION: L0

use crate::arch::cpu::{self, CPUID_ECX_VMX};

/// COM1 marker when guest CPUID leaf 1 is filtered (M3.1 gate).
pub const M3_CPUID_OK_MARKER: &str = "RAYNU-V-M3-CPUID-OK";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MsrAccess {
    Read,
    Write,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FirewallDecision {
    Allow,
    Block,
}

/// Filtered CPUID register set returned to the guest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpuidRegs {
    pub eax: u32,
    pub ebx: u32,
    pub ecx: u32,
    pub edx: u32,
}

/// Set when leaf 1 was filtered with VMX hidden from the guest.
static mut CPUID_FILTER_OK: bool = false;

/// INVARIANTS:
///   - Sensitive host MSRs (e.g. IA32_EFER writes that subvert HV) are Block
///   - Unknown MSRs default to Block (fail closed)
///
/// VERIFICATION: L0 — see msr_firewall_spec.rs
pub fn check_msr(index: u32, access: MsrAccess) -> FirewallDecision {
    // Stub allow-list: TSC + SPEC_CTRL read; everything else blocked.
    match (index, access) {
        (0x10, MsrAccess::Read) => FirewallDecision::Allow, // IA32_TIME_STAMP_COUNTER
        (0x48, MsrAccess::Read) => FirewallDecision::Allow, // IA32_SPEC_CTRL (read)
        _ => FirewallDecision::Block,
    }
}

/// Emulate guest CPUID: host result, then policy filters.
///
/// M3.1 policy: leaf 1 clears ECX.VMX so the guest cannot see nested VT-x.
pub fn filter_cpuid(leaf: u32, subleaf: u32) -> CpuidRegs {
    // SAFETY: CPUID is architecturally defined for these leaves.
    let r = unsafe { cpu::cpuid(leaf, subleaf) };
    let mut out = CpuidRegs {
        eax: r.eax,
        ebx: r.ebx,
        ecx: r.ecx,
        edx: r.edx,
    };
    if leaf == 1 {
        out.ecx &= !CPUID_ECX_VMX;
        note_leaf1_filtered(out.ecx);
    }
    out
}

fn note_leaf1_filtered(ecx: u32) {
    if (ecx & CPUID_ECX_VMX) == 0 {
        // SAFETY: single-threaded VMEXIT / test path.
        unsafe {
            CPUID_FILTER_OK = true;
        }
    }
}

pub fn cpuid_filter_ok() -> bool {
    // SAFETY: written on BSP VMEXIT path; read after filter completes.
    unsafe { CPUID_FILTER_OK }
}

/// True if ECX.VMX is clear in a filtered leaf-1 result.
pub fn vmx_hidden(regs: &CpuidRegs) -> bool {
    (regs.ecx & CPUID_ECX_VMX) == 0
}

#[cfg(test)]
#[path = "msr_firewall_test.rs"]
mod msr_firewall_test;
