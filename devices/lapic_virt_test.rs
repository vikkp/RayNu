use super::*;

#[test]
fn marker_and_range() {
    assert_eq!(M3_GTIMER3_OK_MARKER, "RAYNU-V-M3-GTIMER3-OK");
    assert_eq!(APIC_GPA, 0xFEE0_0000);
    assert!(is_x2apic_msr(0x808));
    assert!(!is_x2apic_msr(0x1B));
}

#[test]
fn init_count_arms_guest_timer() {
    assert!(wrmsr(0x80F, 0x1FF).is_some()); // SVR enable
    assert!(wrmsr(0x832, 0x0000_00EF).is_some()); // LVT timer unmasked
    let armed = wrmsr(0x838, 0x1000).unwrap();
    assert!(armed);
    assert!(host_timer_armed_for_guest());
    let v = take_guest_timer_inject().unwrap();
    assert_eq!(v, 0xEF);
    assert!(gtimer3_ok());
    assert!(!host_timer_armed_for_guest());
}

#[test]
fn mmio_version_readable() {
    let r = mmio_access(APIC_GPA + 0x30, false, 0).unwrap().unwrap();
    assert_eq!(r & 0xFF, 0x14);
}

#[test]
fn cur_count_decreases_while_masked() {
    // Linux calibrate: masked LVTT + large INIT_COUNT, then poll TMCCT.
    assert!(wrmsr(0x80F, 0x1FF).is_some());
    assert!(wrmsr(0x83E, 0x3).is_some()); // ÷16
    assert!(wrmsr(0x832, (LVT_MASKED | 0xEF) as u64).is_some());
    assert!(!wrmsr(0x838, 0x0FFF_FFFF).unwrap()); // masked → no host arm
    let tmcct_msr = 0x800 + (0x390 >> 4); // 0x839
    let c1 = rdmsr(tmcct_msr).unwrap() as u32;
    // Spin on TSC so CUR_COUNT must move.
    let start = crate::arch::cpu::rdtsc();
    while crate::arch::cpu::rdtsc().wrapping_sub(start) < 500_000 {
        core::hint::spin_loop();
    }
    let c2 = rdmsr(tmcct_msr).unwrap() as u32;
    assert!(c1 > 0, "initial CUR_COUNT");
    assert!(c2 < c1, "CUR_COUNT must fall during calibrate ({c1} → {c2})");
    assert!(!host_timer_armed_for_guest());
}
