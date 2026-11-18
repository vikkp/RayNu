use super::*;

fn lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|p| p.into_inner())
}

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
    let _guard = lock();
    let r = mmio_access(APIC_GPA + 0x30, false, 0).unwrap().unwrap();
    assert_eq!(r & 0xFF, 0x14);
    assert_eq!(XAPIC_VERSION, 0x50014);
}

#[test]
fn reset_restores_masked_lvt_and_version() {
    let _guard = lock();
    assert!(wrmsr(0x80F, 0x1FF).is_some());
    assert!(wrmsr(0x832, 0x20).is_some());
    reset();
    let r = mmio_access(APIC_GPA + 0x30, false, 0).unwrap().unwrap();
    assert_eq!(r & 0xFF, 0x14);
    let lvt = mmio_access(APIC_GPA + 0x320, false, 0).unwrap().unwrap();
    assert_eq!(lvt & (1 << 16), 1 << 16, "LVT timer masked after reset");
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
    let _guard = lock();
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
    let _guard = lock();
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
    let _guard = lock();
    reset();
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
fn relatch_if_idle_arms_a_stalled_one_shot() {
    let _guard = lock();
    reset();
    assert!(wrmsr(0x80F, 0x1FF).is_some());
    assert!(wrmsr(0x832, 0xEF).is_some());
    assert!(!relatch_if_idle(0), "INIT_COUNT 0 is a cancelled timer");
    assert!(wrmsr(0x838, 1000).is_some());
    assert!(!relatch_if_idle(0), "countdown still running");
    assert!(on_host_timer_fire());
    assert!(timer_vector_pending());
    assert!(!relatch_if_idle(0));
    assert_eq!(take_timer_vector(), Some(0xEF));
    assert!(relatch_if_idle(0));
    assert!(timer_vector_pending());
    assert!(!relatch_if_idle(1));
}

#[test]
fn take_timer_vector_leaves_a_higher_irr_bit() {
    let _guard = lock();
    reset();
    assert!(wrmsr(0x80F, 0x1FF).is_some());
    assert!(wrmsr(0x832, 0xEF).is_some());
    assert!(wrmsr(0x838, 1000).is_some());
    assert!(on_host_timer_fire());
    latch_irr(0xF0);
    assert_eq!(take_timer_vector(), Some(0xEF));
    assert!(has_irr_vec(0xF0));
    assert!(!timer_vector_pending());
}

#[test]
fn latch_irr_device_vector_then_eoi() {
    let _guard = lock();
    latch_irr(0x31);
    assert!(has_deliverable_irr());
    let v = take_deliverable_vector().expect("device IRR→ISR");
    assert_eq!(v, 0x31);
    assert!(wrmsr(0x80B, 0).is_some());
    assert!(!has_deliverable_irr());
}

#[test]
fn cr8_maps_to_tpr_class() {
    let _guard = lock();
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
    latch_irr(0x20);
    set_cr8(3);
    assert!(!has_deliverable_irr());
    assert!(has_pending_irr());
    let v = take_highest_irr().expect("firmware HLT ignores TPR");
    assert_eq!(v, 0x20);
    assert!(wrmsr(0x80B, 0).is_some());
    set_cr8(0);
}

#[test]
fn firmware_prefer_ata_irr_ignores_tpr() {
    let _guard = lock();
    while take_highest_irr().is_some() {
        assert!(wrmsr(0x80B, 0).is_some());
    }
    set_cr8(2);
    latch_irr(0x2E);
    assert!(has_irr_vec(0x2E), "firmware prefer ATA IRR");
    assert!(
        !has_deliverable_irr(),
        "0x2E class 0x20 is blocked when CR8=2"
    );
    assert!(take_deliverable_vector().is_none());
    let v = take_irr_vec(0x2E, true).expect("firmware prefer ATA IRR");
    assert_eq!(v, 0x2E);
    assert!(!has_irr_vec(0x2E));
    assert!(wrmsr(0x80B, 0).is_some());
    set_cr8(0);
}

#[test]
fn firmware_prefer_ata_irr_not_lvt() {
    let _guard = lock();
    while take_highest_irr().is_some() {
        assert!(wrmsr(0x80B, 0).is_some());
    }
    set_cr8(2);
    latch_irr(0x2E);
    latch_irr(0xEF);
    assert!(has_irr_vec(0x2E), "firmware prefer ATA IRR");
    assert!(has_irr_vec(0xEF));
    let v = take_irr_vec(0x2E, true).expect("firmware prefer ATA IRR not LVT");
    assert_eq!(v, 0x2E);
    assert!(has_irr_vec(0xEF), "LVT stays in IRR");
    assert!(!has_irr_vec(0x2E));
    let v = take_highest_irr().expect("LVT still pending");
    assert_eq!(v, 0xEF);
    assert!(wrmsr(0x80B, 0).is_some());
    set_cr8(0);
}

#[test]
fn firmware_lapic_timer_expiry_masked_uses_vec20() {
    let _guard = lock();
    assert!(wrmsr(0x80F, 0x1FF).is_some());
    assert!(wrmsr(0x832, (LVT_MASKED | 0xEF) as u64).is_some());
    while take_highest_irr().is_some() {
        assert!(wrmsr(0x80B, 0).is_some());
    }
    assert!(
        force_firmware_lapic_timer_expiry(),
        "firmware LAPIC timer expiry"
    );
    let v = take_highest_irr().expect("firmware LAPIC timer expiry IRR");
    assert_eq!(v, 0x20);
    assert!(wrmsr(0x80B, 0).is_some());
}

#[test]
fn firmware_lapic_timer_expiry_keeps_unmasked_vector() {
    let _guard = lock();
    assert!(wrmsr(0x80F, 0x1FF).is_some());
    assert!(wrmsr(0x832, 0x27u64).is_some());
    while take_highest_irr().is_some() {
        assert!(wrmsr(0x80B, 0).is_some());
    }
    assert!(
        force_firmware_lapic_timer_expiry(),
        "firmware LAPIC timer expiry"
    );
    let v = take_highest_irr().expect("keep OVMF LVT vector");
    assert_eq!(v, 0x27);
    assert!(wrmsr(0x80B, 0).is_some());
}

#[test]
fn icr_self_ipi_latches_irr_and_take_moves_to_isr() {
    let _guard = lock();
    reset();
    assert!(wrmsr(0x80F, 0x1FF).is_some());
    // Shorthand self (bit 18), assert (bit 14), fixed, vector 0xf6 (irq_work).
    let icr = (1u32 << 18) | (1 << 14) | 0xf6;
    assert!(mmio_access(APIC_GPA + 0x300, true, icr).is_some());
    let v = take_self_ipi().expect("self-IPI");
    assert_eq!(v, 0xf6);
    assert!(take_self_ipi().is_none());
    assert!(mmio_access(APIC_GPA + 0xB0, true, 0).is_some());
    // All-excluding-self is not this CPU.
    let excl = (3u32 << 18) | (1 << 14) | 0xfd;
    assert!(mmio_access(APIC_GPA + 0x300, true, excl).is_some());
    assert!(take_self_ipi().is_none());
    // INIT delivery is not a fixed interrupt.
    let init = (1u32 << 18) | (5 << 8) | 0xf6;
    assert!(mmio_access(APIC_GPA + 0x300, true, init).is_some());
    assert!(take_self_ipi().is_none());
    // No shorthand, destination APIC ID 0, is self.
    assert!(mmio_access(APIC_GPA + 0x310, true, 0).is_some());
    let noshort = (1u32 << 14) | 0xec;
    assert!(mmio_access(APIC_GPA + 0x300, true, noshort).is_some());
    assert_eq!(take_self_ipi(), Some(0xec));
    assert!(mmio_access(APIC_GPA + 0xB0, true, 0).is_some());
    reset();
}

#[test]
fn dest_hit_matches_physical_flat_and_cluster() {
    assert!(dest_hit(1, false, 1, 0, 0xFFFF_FFFF));
    assert!(dest_hit(0xFF, false, 3, 0, 0xFFFF_FFFF));
    assert!(!dest_hit(2, false, 1, 0, 0xFFFF_FFFF));
    // Flat: LDR bit 1, destination bit 1.
    assert!(dest_hit(0b0010, true, 1, 0b0010 << 24, 0xFFFF_FFFF));
    assert!(!dest_hit(0b0001, true, 1, 0b0010 << 24, 0xFFFF_FFFF));
    // Cluster model (DFR[31:28] == 0): cluster 0x1, logical bit 0b0010.
    let ldr = 0x12u32 << 24;
    assert!(dest_hit(0x12, true, 1, ldr, 0x0FFF_FFFF));
    assert!(!dest_hit(0x22, true, 1, ldr, 0x0FFF_FFFF), "other cluster");
    assert!(!dest_hit(0x11, true, 1, ldr, 0x0FFF_FFFF), "bit clear");
}

#[test]
fn smp_ipi_init_sipi_and_nmi_follow_the_destination() {
    let _guard = lock();
    let slot = crate::guest::slot::current();
    let reset_cpus = || {
        crate::guest::slot::set_vcpus(2);
        crate::guest::slot::set_vcpu_up(slot, 1, true);
        reset();
        crate::guest::slot::with_vcpu(slot, 1, reset);
    };
    reset_cpus();
    // Fixed IPI 0 → 1 sets vCPU 1's IRR and not vCPU 0's.
    assert!(mmio_access(APIC_GPA + 0x310, true, 1 << 24).is_some());
    assert!(mmio_access(APIC_GPA + 0x300, true, (1 << 14) | 0x30).is_some());
    let irr1 = crate::guest::slot::with_vcpu(slot, 1, || {
        mmio_access(APIC_GPA + 0x210, false, 0).unwrap().unwrap()
    });
    let irr0 = mmio_access(APIC_GPA + 0x210, false, 0).unwrap().unwrap();
    assert_ne!(irr1 & (1 << 16), 0, "vCPU 1 IRR");
    assert_eq!(irr0 & (1 << 16), 0, "vCPU 0 IRR");
    assert_eq!(
        crate::guest::slot::with_vcpu(slot, 1, take_self_ipi),
        Some(0x30)
    );

    // Shorthand 3 skips the sender. Shorthand 2 includes it.
    reset_cpus();
    assert!(mmio_access(APIC_GPA + 0x300, true, (3 << 18) | (1 << 14) | 0x31).is_some());
    assert!(take_self_ipi().is_none());
    assert_eq!(
        crate::guest::slot::with_vcpu(slot, 1, take_self_ipi),
        Some(0x31)
    );
    reset_cpus();
    assert!(mmio_access(APIC_GPA + 0x300, true, (2 << 18) | (1 << 14) | 0x32).is_some());
    assert_eq!(take_self_ipi(), Some(0x32));
    assert_eq!(
        crate::guest::slot::with_vcpu(slot, 1, take_self_ipi),
        Some(0x32)
    );

    // INIT assert then SIPI are latched for vCPU 1 and taken once.
    reset_cpus();
    assert!(mmio_access(APIC_GPA + 0x310, true, 1 << 24).is_some());
    assert!(mmio_access(APIC_GPA + 0x300, true, (5 << 8) | (1 << 14) | (1 << 15)).is_some());
    assert!(take_ap_init(1));
    assert!(!take_ap_init(1));
    assert!(mmio_access(APIC_GPA + 0x300, true, (6 << 8) | 0x08).is_some());
    assert_eq!(take_ap_sipi(1), Some(0x08));
    assert!(take_ap_sipi(1).is_none());

    // INIT de-assert is ignored. INIT and SIPI to vCPU 0 are ignored.
    assert!(mmio_access(APIC_GPA + 0x300, true, (5 << 8) | (1 << 15)).is_some());
    assert!(!take_ap_init(1));
    assert!(mmio_access(APIC_GPA + 0x310, true, 0).is_some());
    assert!(mmio_access(APIC_GPA + 0x300, true, (5 << 8) | (1 << 14)).is_some());
    assert!(!take_ap_init(0));
    assert!(mmio_access(APIC_GPA + 0x300, true, (6 << 8) | 0x09).is_some());
    assert!(take_ap_sipi(0).is_none());

    // NMI latches on the target.
    assert!(mmio_access(APIC_GPA + 0x310, true, 1 << 24).is_some());
    assert!(mmio_access(APIC_GPA + 0x300, true, (4 << 8) | (1 << 14)).is_some());
    assert!(crate::guest::slot::with_vcpu(slot, 1, nmi_pending));
    assert!(crate::guest::slot::with_vcpu(slot, 1, take_nmi));
    assert!(!crate::guest::slot::with_vcpu(slot, 1, nmi_pending));

    // reset under vCPU 1 gives ID 1 << 24.
    crate::guest::slot::with_vcpu(slot, 1, reset);
    let id = crate::guest::slot::with_vcpu(slot, 1, || {
        mmio_access(APIC_GPA + 0x20, false, 0).unwrap().unwrap()
    });
    assert_eq!(id, 1 << 24);

    // With one vCPU a remote fixed IPI does nothing and LDR reads 0.
    crate::guest::slot::set_vcpus(1);
    reset();
    assert!(mmio_access(APIC_GPA + 0xD0, true, 0xFF00_0000).is_some());
    assert_eq!(mmio_access(APIC_GPA + 0xD0, false, 0).unwrap().unwrap(), 0);
    assert!(mmio_access(APIC_GPA + 0x310, true, 1 << 24).is_some());
    assert!(mmio_access(APIC_GPA + 0x300, true, (1 << 14) | 0x33).is_some());
    assert!(take_self_ipi().is_none());

    crate::guest::slot::set_vcpus(1);
    crate::guest::slot::set_vcpu_up(slot, 1, false);
    reset();
}

#[test]
fn remote_lapic_writes_go_through_one_seam() {
    let src = include_str!("lapic_virt.rs");
    assert_eq!(src.matches("APIC_IRR.p_at(").count(), 1);
    assert_eq!(src.matches("SELF_IPI_MASK.p_at(").count(), 1);
    let irq = include_str!("guest_irq.rs");
    let msix = include_str!("guest_msix.rs");
    assert!(
        !irq.contains(".p_at("),
        "IOAPIC must not write a LAPIC bank"
    );
    assert!(
        !msix.contains(".p_at("),
        "MSI-X must not write a LAPIC bank"
    );
}
