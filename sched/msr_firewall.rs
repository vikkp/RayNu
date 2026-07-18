//! MSR / CPUID / CR access firewalls.
//!
//! Pillar: [V] · Proven Core · VERIFICATION: L0

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

#[cfg(test)]
#[path = "msr_firewall_test.rs"]
mod msr_firewall_test;
