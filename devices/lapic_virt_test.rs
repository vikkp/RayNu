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
