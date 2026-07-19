use super::*;

#[test]
fn marker_and_range() {
    assert_eq!(M3_GTIMER3_OK_MARKER, "RAYNU-V-M3-GTIMER3-OK");
    assert_eq!(M3_APIC_OK_MARKER, "RAYNU-V-M3-APIC-OK");
    assert_eq!(APIC_GPA, 0xFEE0_0000);
    assert!(is_x2apic_msr(0x808));
    assert!(!is_x2apic_msr(0x1B));
}

#[test]
fn mmio_version_readable() {
    let r = mmio_access(APIC_GPA + 0x30, false, 0).unwrap().unwrap();
    assert_eq!(r & 0xFF, 0x14);
}

#[test]
fn real_countdown_decreases_cur_count() {
    assert!(wrmsr(0x80F, 0x1FF).is_some()); // SVR
    assert!(wrmsr(0x83E, 0x3).is_some()); // ÷16
    assert!(wrmsr(0x832, (LVT_MASKED | 0xEF) as u64).is_some());
    assert!(!wrmsr(0x838, 0x0FFF_FFFF).unwrap()); // masked → no host arm
    let tmcct = 0x800 + (0x390 >> 4);
    let start = crate::arch::cpu::rdtsc();
    while crate::arch::cpu::rdtsc().wrapping_sub(start) < 2_000_000 {
        core::hint::spin_loop();
    }
    let c = rdmsr(tmcct).unwrap() as u32;
    assert!(c < 0x0FFF_FFFF, "CUR_COUNT should decrease (got {c:#x})");
}

#[test]
fn irr_isr_eoi_delivery() {
    assert!(wrmsr(0x80F, 0x1FF).is_some());
    assert!(wrmsr(0x832, 0xEF_u64).is_some()); // unmasked 0xEF
    assert!(wrmsr(0x838, 0x1000).unwrap()); // arms host timer flag
    assert!(host_timer_armed_for_guest());
    assert!(on_host_timer_fire());
    assert!(gtimer3_ok());
    let _ = take_gtimer3_latch(); // may already be consumed by prior test
    assert!(has_deliverable_irr());
    let v = take_deliverable_vector().expect("IRR→ISR");
    assert_eq!(v, 0xEF);
    assert!(apic_ok());
    let _ = take_apic_ok_latch();
    // Vector 0xEF → ISR word 7 at MMIO 0x170 / MSR 0x817.
    let isr7 = rdmsr(0x817).unwrap() as u32;
    assert_ne!(isr7 & (1 << (0xEF % 32)), 0);
    assert!(wrmsr(0x80B, 0).is_some());
    let isr7b = rdmsr(0x817).unwrap() as u32;
    assert_eq!(isr7b & (1 << (0xEF % 32)), 0);
}
