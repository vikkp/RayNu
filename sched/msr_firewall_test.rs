use super::*;
use crate::arch::cpu::CPUID_ECX_VMX;

#[test]
fn fail_closed_sensitive() {
    assert_eq!(
        check_msr(MSR_FEATURE_CONTROL, MsrAccess::Write),
        FirewallDecision::Block
    );
    assert_eq!(
        check_msr(MSR_APIC_BASE, MsrAccess::Write),
        FirewallDecision::Block
    );
    assert_eq!(
        classify_msr(MSR_VMX_BASIC, MsrAccess::Read),
        MsrAction::InjectGp
    );
}

#[test]
fn allow_list_early_linux() {
    assert_eq!(
        classify_msr(MSR_TSC, MsrAccess::Read),
        MsrAction::HostPassthrough
    );
    assert_eq!(classify_msr(MSR_EFER, MsrAccess::Write), MsrAction::VmcsEfer);
    assert_eq!(classify_msr(MSR_FS_BASE, MsrAccess::Read), MsrAction::VmcsFsBase);
    assert_eq!(classify_msr(MSR_LSTAR, MsrAccess::Write), MsrAction::Shadow);
    assert_eq!(
        classify_msr(MSR_APIC_BASE, MsrAccess::Read),
        MsrAction::HostPassthrough
    );
    assert_eq!(
        classify_msr(0xDEAD_BEEF, MsrAccess::Read),
        MsrAction::ReadZero
    );
}

#[test]
fn shadow_roundtrip() {
    shadow_write(MSR_LSTAR, 0xFFFF_FFFF_8100_0000);
    assert_eq!(shadow_read(MSR_LSTAR), 0xFFFF_FFFF_8100_0000);
    note_msr_emulated();
    assert!(msr_firewall_ok());
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
    assert_eq!(r.edx & crate::arch::cpu::CPUID_EDX_APIC, 0);
    assert!(cpuid_filter_ok());
}

#[test]
fn filter_leaf0_passthrough_vendor() {
    let r = filter_cpuid(0, 0);
    // Host vendor string non-empty (GenuineIntel / AuthenticAMD / …).
    assert!(r.ebx != 0 || r.ecx != 0 || r.edx != 0);
}
