//! MSR / CPUID / CR access firewalls.
//!
//! Pillar: [V] · Proven Core · VERIFICATION: L0
//!
//! M3.1: CPUID leaf-1 hides VMX. M3.9: classify guest RDMSR/WRMSR so the
//! VMM can emulate via VMCS fields, host passthrough, shadows, or `#GP`.

use crate::arch::cpu::{self, CPUID_ECX_VMX, CPUID_EDX_APIC};

/// COM1 marker when guest CPUID leaf 1 is filtered (M3.1 gate).
pub const M3_CPUID_OK_MARKER: &str = "RAYNU-V-M3-CPUID-OK";

/// IA32_TIME_STAMP_COUNTER
pub const MSR_TSC: u32 = 0x10;
/// IA32_APIC_BASE — read OK; writes blocked (host APIC).
pub const MSR_APIC_BASE: u32 = 0x1B;
/// IA32_FEATURE_CONTROL — always block.
pub const MSR_FEATURE_CONTROL: u32 = 0x3A;
/// IA32_SPEC_CTRL
pub const MSR_SPEC_CTRL: u32 = 0x48;
/// IA32_PRED_CMD
pub const MSR_PRED_CMD: u32 = 0x49;
/// IA32_BIOS_SIGN_ID
pub const MSR_BIOS_SIGN_ID: u32 = 0x8B;
/// IA32_MTRRCAP
pub const MSR_MTRRCAP: u32 = 0xFE;
pub const MSR_SYSENTER_CS: u32 = 0x174;
pub const MSR_SYSENTER_ESP: u32 = 0x175;
pub const MSR_SYSENTER_EIP: u32 = 0x176;
pub const MSR_MISC_ENABLE: u32 = 0x1A0;
pub const MSR_PAT: u32 = 0x277;
pub const MSR_MTRR_DEF_TYPE: u32 = 0x2FF;
pub const MSR_EFER: u32 = 0xC000_0080;
pub const MSR_STAR: u32 = 0xC000_0081;
pub const MSR_LSTAR: u32 = 0xC000_0082;
pub const MSR_CSTAR: u32 = 0xC000_0083;
pub const MSR_SFMASK: u32 = 0xC000_0084;
pub const MSR_FS_BASE: u32 = 0xC000_0100;
pub const MSR_GS_BASE: u32 = 0xC000_0101;
pub const MSR_KERNEL_GS_BASE: u32 = 0xC000_0102;
pub const MSR_TSC_AUX: u32 = 0xC000_0103;

/// First VMX capability MSR (block the range).
pub const MSR_VMX_BASIC: u32 = 0x480;
/// Last commonly used VMX capability MSR.
pub const MSR_VMX_VMFUNC: u32 = 0x491;

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

/// How the VMM should satisfy a guest MSR access (M3.9).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MsrAction {
    /// `rdmsr`/`wrmsr` on the host (safe probes / TSC).
    HostPassthrough,
    /// Guest EFER in VMCS.
    VmcsEfer,
    /// Guest PAT in VMCS.
    VmcsPat,
    /// Guest SYSENTER_CS in VMCS.
    VmcsSysenterCs,
    /// Guest SYSENTER_ESP in VMCS.
    VmcsSysenterEsp,
    /// Guest SYSENTER_EIP in VMCS.
    VmcsSysenterEip,
    /// Guest FS base in VMCS.
    VmcsFsBase,
    /// Guest GS base in VMCS.
    VmcsGsBase,
    /// VMM-owned shadow (SPEC_CTRL and similar — not used by `syscall`).
    Shadow,
    /// Unknown read: return 0 (Linux probes).
    ReadZero,
    /// Unknown / blocked write: drop.
    IgnoreWrite,
    /// Inject `#GP(0)` (sensitive host control).
    InjectGp,
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

/// Set when at least one allow-listed MSR was emulated (M3.9).
static mut MSR_FW_OK: bool = false;

/// Guest MSR shadows (not in the VMCS). Syscall MSRs are host-passthrough.
static mut SHADOW_SPEC_CTRL: u64 = 0;

/// INVARIANTS:
///   - Sensitive host MSRs (FEATURE_CONTROL, VMX caps, APIC_BASE write) → InjectGp
///   - VMCS-backed guest state MSRs use Vmcs*
///   - Unknown defaults: read→0, write→ignore (bring-up; tighten later)
///
/// VERIFICATION: L0 — see msr_firewall_spec.rs
pub fn check_msr(index: u32, access: MsrAccess) -> FirewallDecision {
    match classify_msr(index, access) {
        MsrAction::InjectGp => FirewallDecision::Block,
        _ => FirewallDecision::Allow,
    }
}

/// Classify a guest MSR access for emulation.
pub fn classify_msr(index: u32, access: MsrAccess) -> MsrAction {
    if (MSR_VMX_BASIC..=MSR_VMX_VMFUNC).contains(&index) {
        return MsrAction::InjectGp;
    }
    match (index, access) {
        (MSR_FEATURE_CONTROL, _) => MsrAction::InjectGp,
        (MSR_APIC_BASE, MsrAccess::Write) => MsrAction::InjectGp,
        (MSR_APIC_BASE, MsrAccess::Read) => MsrAction::HostPassthrough,

        (MSR_TSC, MsrAccess::Read) => MsrAction::HostPassthrough,
        (MSR_TSC, MsrAccess::Write) => MsrAction::IgnoreWrite,

        (MSR_EFER, _) => MsrAction::VmcsEfer,
        (MSR_PAT, _) => MsrAction::VmcsPat,
        (MSR_SYSENTER_CS, _) => MsrAction::VmcsSysenterCs,
        (MSR_SYSENTER_ESP, _) => MsrAction::VmcsSysenterEsp,
        (MSR_SYSENTER_EIP, _) => MsrAction::VmcsSysenterEip,
        (MSR_FS_BASE, _) => MsrAction::VmcsFsBase,
        (MSR_GS_BASE, _) => MsrAction::VmcsGsBase,

        // Must hit real hardware: `syscall`/`sysret`/`swapgs` read these MSRs
        // directly. Shadow-only broke `/init` (kernel RIP + user RSP → #DF).
        (MSR_STAR, _)
        | (MSR_LSTAR, _)
        | (MSR_CSTAR, _)
        | (MSR_SFMASK, _)
        | (MSR_KERNEL_GS_BASE, _)
        | (MSR_TSC_AUX, _) => MsrAction::HostPassthrough,
        (MSR_SPEC_CTRL, _) => MsrAction::Shadow,
        (MSR_PRED_CMD, MsrAccess::Write) => MsrAction::IgnoreWrite,
        (MSR_PRED_CMD, MsrAccess::Read) => MsrAction::ReadZero,

        (MSR_BIOS_SIGN_ID, MsrAccess::Read)
        | (MSR_MTRRCAP, MsrAccess::Read)
        | (MSR_MISC_ENABLE, MsrAccess::Read)
        | (MSR_MTRR_DEF_TYPE, MsrAccess::Read) => MsrAction::HostPassthrough,
        (MSR_BIOS_SIGN_ID, MsrAccess::Write)
        | (MSR_MTRRCAP, MsrAccess::Write)
        | (MSR_MISC_ENABLE, MsrAccess::Write)
        | (MSR_MTRR_DEF_TYPE, MsrAccess::Write) => MsrAction::IgnoreWrite,

        (_, MsrAccess::Read) => MsrAction::ReadZero,
        (_, MsrAccess::Write) => MsrAction::IgnoreWrite,
    }
}

/// Read a shadowed MSR value.
pub fn shadow_read(index: u32) -> u64 {
    // SAFETY: single-threaded VMEXIT path.
    unsafe {
        match index {
            MSR_SPEC_CTRL => SHADOW_SPEC_CTRL,
            _ => 0,
        }
    }
}

/// Write a shadowed MSR value.
pub fn shadow_write(index: u32, value: u64) {
    // SAFETY: single-threaded VMEXIT path.
    unsafe {
        if index == MSR_SPEC_CTRL {
            SHADOW_SPEC_CTRL = value;
        }
    }
}

pub fn note_msr_emulated() {
    // SAFETY: single-threaded VMEXIT / test path.
    unsafe {
        MSR_FW_OK = true;
    }
}

pub fn msr_firewall_ok() -> bool {
    // SAFETY: written on BSP VMEXIT path; read for gate / tests.
    unsafe { MSR_FW_OK }
}

/// Emulate guest CPUID: host result, then policy filters.
///
/// M3.1 policy: leaf 1 clears ECX.VMX so the guest cannot see nested VT-x.
/// M3.10: also clear EDX.APIC so Linux does not program the host LAPIC page
/// (identity EPT would alias GPA 0xFEE00000 onto the real APIC).
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
        out.edx &= !CPUID_EDX_APIC;
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
