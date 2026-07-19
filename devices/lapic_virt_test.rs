use super::*;

#[test]
fn marker_and_range() {
    assert_eq!(M3_GTIMER3_OK_MARKER, "RAYNU-V-M3-GTIMER3-OK");
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
fn internal_countdown_latches_gtimer3_guest_sees_stuck_cur() {
    assert!(wrmsr(0x80F, 0x1FF).is_some()); // SVR
    assert!(wrmsr(0x83E, 0x3).is_some()); // ÷16
    assert!(wrmsr(0x832, (LVT_MASKED | 0xEF) as u64).is_some());
    assert!(!wrmsr(0x838, 0x0FFF_FFFF).unwrap()); // no host arm
    let tmcct = 0x800 + (0x390 >> 4);
    let start = crate::arch::cpu::rdtsc();
    while crate::arch::cpu::rdtsc().wrapping_sub(start) < 2_000_000 {
        core::hint::spin_loop();
    }
    let c = rdmsr(tmcct).unwrap() as u32;
    // Guest-visible CUR_COUNT stays at INIT (calibrate-fail strategy).
    assert_eq!(c, 0x0FFF_FFFF);
    assert!(gtimer3_ok(), "internal model should latch GTIMER3");
    assert!(take_gtimer3_latch());
    assert!(!take_gtimer3_latch());
}
