use super::*;

#[test]
fn marker_and_range() {
    assert_eq!(M3_GTIMER3_OK_MARKER, "RAYNU-V-M3-GTIMER3-OK");
    assert_eq!(M3_APIC_OK_MARKER, "RAYNU-V-M3-APIC-OK");
    assert_eq!(APIC_GPA, 0xFEE0_0000);
    assert!(is_xapic_mmio_gpa(APIC_GPA));
    assert!(is_xapic_mmio_gpa(APIC_GPA + 0x30));
    assert!(!is_xapic_mmio_gpa(APIC_GPA + 0x1000));
    assert!(is_x2apic_msr(0x808));
    assert!(!is_x2apic_msr(0x1B));
}

#[test]
fn mmio_version_readable() {
    let r = mmio_access(APIC_GPA + 0x30, false, 0).unwrap().unwrap();
    assert_eq!(r & 0xFF, 0x14);
    assert_eq!(XAPIC_VERSION, 0x50014);
}

#[test]
fn fill_xapic_page_version_not_zero() {
    let mut page = [0u8; 4096];
    fill_xapic_page(&mut page);
    let ver = u32::from_le_bytes(page[0x30..0x34].try_into().unwrap());
    assert_eq!(ver, XAPIC_VERSION);
    assert_ne!(ver, 0);
    let svr = u32::from_le_bytes(page[0xF0..0xF4].try_into().unwrap());
    assert_eq!(svr & (1 << 8), 1 << 8);
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

#[test]
fn poll_timer_expiry_latches_irr() {
    assert!(wrmsr(0x80F, 0x1FF).is_some());
    assert!(wrmsr(0x83E, 0x3).is_some()); // ÷16
    assert!(wrmsr(0x832, 0xEF_u64).is_some());
    assert!(wrmsr(0x838, 0x1000).unwrap());
    let start = crate::arch::cpu::rdtsc();
    while crate::arch::cpu::rdtsc().wrapping_sub(start) < 8_000_000 {
        core::hint::spin_loop();
    }
    assert!(poll_timer_expiry());
    assert!(has_deliverable_irr());
}

#[test]
fn latch_irr_device_vector_then_eoi() {
    latch_irr(0x31);
    assert!(has_deliverable_irr());
    let v = take_deliverable_vector().expect("device IRR→ISR");
    assert_eq!(v, 0x31);
    assert!(wrmsr(0x80B, 0).is_some());
    assert!(!has_deliverable_irr());
}

#[test]
fn cr8_maps_to_tpr_class() {
    set_cr8(0);
    assert_eq!(cr8(), 0);
    assert_eq!(tpr() & 0xF0, 0);
    set_cr8(15);
    assert_eq!(cr8(), 15);
    assert_eq!(tpr(), 0xF0);
    set_tpr(0x20);
    assert_eq!(cr8(), 2);
    set_cr8(0);
    while take_deliverable_vector().is_some() {
        assert!(wrmsr(0x80B, 0).is_some());
    }
    latch_irr(0x31);
    assert!(has_deliverable_irr());
    set_cr8(3);
    assert!(!has_deliverable_irr());
    set_cr8(0);
    let v = take_deliverable_vector().expect("unmasked IRR");
    assert_eq!(v, 0x31);
    assert!(wrmsr(0x80B, 0).is_some());
}
