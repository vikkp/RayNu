use super::*;
use crate::arch::cpu::CPUID_ECX_VMX;

#[test]
fn fail_closed() {
    assert_eq!(
        check_msr(0x10, MsrAccess::Read),
        FirewallDecision::Allow
    );
    assert_eq!(
        check_msr(0x1b, MsrAccess::Write),
        FirewallDecision::Block
    );
}

#[test]
fn marker_stable() {
    assert_eq!(M3_CPUID_OK_MARKER, "RAYNU-V-M3-CPUID-OK");
}

#[test]
fn filter_leaf1_hides_vmx() {
    let r = filter_cpuid(1, 0);
    assert!(vmx_hidden(&r));
    assert_eq!(r.ecx & CPUID_ECX_VMX, 0);
    assert!(cpuid_filter_ok());
}

#[test]
fn filter_leaf0_passthrough_vendor() {
    let r = filter_cpuid(0, 0);
    // Host vendor string non-empty (GenuineIntel / AuthenticAMD / …).
    assert!(r.ebx != 0 || r.ecx != 0 || r.edx != 0);
}
