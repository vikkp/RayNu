    use super::{
    has_deliverable, ioapic_read, ioapic_write, is_hpet_split_2m_gpa, is_ioapic_gpa, lower_ata,
    pic_has_deliverable, pic_io, prefer_pit_once, prefer_pit_until_driver_ok,     raise_ata, raise_gsi,
    raise_pit, raise_virtio, reset, take_inject_vector, take_ioapic_ata_vector, take_ioapic_vector, take_pic_vector,
    arm_firmware_virtual_wire, arm_firmware_ata_gsi14, firmware_virtual_wire_armed, ioapic_ata_ready, pic_ata_ready,
    firmware_ata_vec, firmware_is_pit_vec,
    ATA_GSI, IOAPIC_GPA,
    IOAPIC_VERSION, PIT_IOAPIC_GSI, PIT_IRQ, VIRTIO_GSI, VIRTIO_ISO_GSI, VIRTIO_PIC_IRQ,
    ioapic_gsi2_armed, linux_ioapic_gsi2_programmed, note_linux_ioapic_write, prefer_pit_hold,
};
use crate::devices::guest_platform::{self, is_platform_sink_gpa};
use crate::devices::ide_cdrom::{
    present as present_iso, reset as reset_cd, write_placeholder_iso, MOCK_EFI_ISO_BYTES, ISO_SECTOR,
};

fn arm_product_iso() {
    reset();
    reset_cd();
    guest_platform::reset();
    let extra = MOCK_EFI_ISO_BYTES + ISO_SECTOR;
    let mut iso = vec![0u8; extra];
    write_placeholder_iso(&mut iso[..MOCK_EFI_ISO_BYTES]);
    assert!(present_iso(&iso, 9));
    assert!(crate::devices::ide_cdrom::product_iso_window_armed());
}

#[test]
fn lab_stub_does_not_arm_ioapic_or_inject() {
    reset();
    reset_cd();
    guest_platform::reset();
    assert!(!crate::devices::ide_cdrom::product_iso_window_armed());
    assert!(!is_ioapic_gpa(IOAPIC_GPA));
    assert!(!is_hpet_split_2m_gpa(IOAPIC_GPA));
    assert!(is_platform_sink_gpa(IOAPIC_GPA));
    raise_ata();
    raise_virtio();
    assert!(!has_deliverable());
    assert!(take_inject_vector().is_none());
    reset();
}

#[test]
fn product_iso_ioapic_unmask_then_gsi_injects() {
    arm_product_iso();
    assert!(is_ioapic_gpa(IOAPIC_GPA));
    assert!(is_ioapic_gpa(IOAPIC_GPA + 0x10));
    assert!(!is_ioapic_gpa(IOAPIC_GPA + 0x1000));
    assert!(is_hpet_split_2m_gpa(IOAPIC_GPA));
    assert!(!is_platform_sink_gpa(IOAPIC_GPA));
    raise_ata();
    assert!(
        take_inject_vector().is_none(),
        "masked IOAPIC keeps IRR but does not inject"
    );
    ioapic_write(0, 1);
    assert_eq!(ioapic_read(0x10), IOAPIC_VERSION);
    // Pin 14 low: vector 0x40, unmasked. IRR still set from raise_ata.
    ioapic_write(0, 0x10 + 2 * u32::from(ATA_GSI));
    ioapic_write(0x10, 0x40);
    assert!(has_deliverable());
    assert_eq!(take_inject_vector(), Some(0x40));
    assert!(take_inject_vector().is_none());
    lower_ata();
    reset();
    reset_cd();
    guest_platform::reset();
}

#[test]
fn product_iso_virtio_gsi_and_pic_fallback() {
    arm_product_iso();
    ioapic_write(0, 0x10 + 2 * u32::from(VIRTIO_GSI));
    ioapic_write(0x10, 0x51);
    raise_virtio();
    assert_eq!(take_inject_vector(), Some(0x51));
    ioapic_write(0, 0x10 + 2 * u32::from(VIRTIO_ISO_GSI));
    ioapic_write(0x10, 0x52);
    crate::devices::guest_irq::raise_virtio_iso();
    assert_eq!(take_inject_vector(), Some(0x52));
    assert_eq!(VIRTIO_ISO_GSI, 18);
    // IOAPIC consumed; PIC IRQ 11 still pending until ICW2 remaps ≥16.
    assert!(take_inject_vector().is_none());
    // Remap PIC: ICW1, ICW2=0x20, ICW3, ICW4; unmask IRQ 11.
    let _ = pic_io(0x20, false, 1, 0x11);
    let _ = pic_io(0x21, false, 1, 0x20);
    let _ = pic_io(0x21, false, 1, 0x04);
    let _ = pic_io(0x21, false, 1, 0x01);
    let _ = pic_io(0xA0, false, 1, 0x11);
    let _ = pic_io(0xA1, false, 1, 0x28);
    let _ = pic_io(0xA1, false, 1, 0x02);
    let _ = pic_io(0xA1, false, 1, 0x01);
    let _ = pic_io(0x21, false, 1, 0x00);
    let _ = pic_io(0xA1, false, 1, 0x00);
    raise_pic_for_test();
    assert_eq!(take_inject_vector(), Some(0x20 + VIRTIO_PIC_IRQ));
    reset();
    reset_cd();
    guest_platform::reset();
}

#[test]
fn product_iso_virtio_raises_pci_line_ioapic_pin() {
    arm_product_iso();
    ioapic_write(0, 0x10 + 2 * u32::from(VIRTIO_PIC_IRQ));
    ioapic_write(0x10, 0x53);
    raise_virtio();
    assert_eq!(
        take_inject_vector(),
        Some(0x53),
        "Linux uses PCI interrupt line 11 as IOAPIC pin 11 without _PRT"
    );
    reset();
    reset_cd();
    guest_platform::reset();
}

fn raise_pic_for_test() {
    raise_gsi(VIRTIO_PIC_IRQ);
}

#[test]
fn pic_icw2_below_16_does_not_inject() {
    arm_product_iso();
    let _ = pic_io(0x20, false, 1, 0x11);
    let _ = pic_io(0x21, false, 1, 0x08);
    let _ = pic_io(0x21, false, 1, 0x04);
    let _ = pic_io(0x21, false, 1, 0x01);
    let _ = pic_io(0x21, false, 1, 0x00);
    raise_ata();
    // IOAPIC still masked; PIC vector 0x08+14 would be exception range.
    assert!(take_inject_vector().is_none());
    reset();
    reset_cd();
    guest_platform::reset();
}

fn pic_init_unmask_all() {
    let _ = pic_io(0x20, false, 1, 0x11);
    let _ = pic_io(0x21, false, 1, 0x20);
    let _ = pic_io(0x21, false, 1, 0x04);
    let _ = pic_io(0x21, false, 1, 0x01);
    let _ = pic_io(0xA0, false, 1, 0x11);
    let _ = pic_io(0xA1, false, 1, 0x28);
    let _ = pic_io(0xA1, false, 1, 0x02);
    let _ = pic_io(0xA1, false, 1, 0x01);
    let _ = pic_io(0x21, false, 1, 0x00);
    let _ = pic_io(0xA1, false, 1, 0x00);
}

#[test]
fn product_iso_firmware_virtual_wire_pic_irq0() {
    arm_product_iso();
    raise_pit();
    assert!(
        !pic_has_deliverable(),
        "OVMF never ICW2: IRR latched but not deliverable (iron beb1576 pic=0)"
    );
    assert!(take_pic_vector().is_none());
    arm_firmware_virtual_wire();
    assert!(pic_has_deliverable(), "firmware virtual-wire PIC");
    assert_eq!(
        take_pic_vector(),
        Some(0x20 + PIT_IRQ),
        "firmware virtual-wire PIC IRQ0 vec 0x20"
    );
    assert!(take_pic_vector().is_none());
    // OVMF CpuSleep IDT[0x20] EOIs LAPIC, not PIC OCW2. Without AEOI the
    // next raise_pit would see ISR[0] and never inject again.
    raise_pit();
    assert!(
        pic_has_deliverable(),
        "firmware virtual-wire AEOI"
    );
    assert_eq!(
        take_pic_vector(),
        Some(0x20 + PIT_IRQ),
        "firmware virtual-wire AEOI repeats IRQ0"
    );
    reset();
    reset_cd();
    guest_platform::reset();
}

#[test]
fn product_iso_firmware_virtual_wire_gsi2_repeats() {
    arm_product_iso();
    raise_pit();
    assert!(
        !ioapic_gsi2_armed(),
        "OVMF leftover pin 2 masked (iron beb1576 gsi2=0)"
    );
    assert!(take_ioapic_vector().is_none());
    arm_firmware_virtual_wire();
    assert!(firmware_virtual_wire_armed());
    assert!(ioapic_gsi2_armed(), "firmware virtual-wire GSI 2");
    assert!(
        !linux_ioapic_gsi2_programmed(),
        "linux PIC before leftover GSI 2"
    );
    assert!(
        !crate::vmx::guest_uefi::guest_uefi_pic_before_lapic(true, true, false),
        "firmware virtual-wire GSI 2 beats PIC-first"
    );
    assert_eq!(
        take_ioapic_vector(),
        Some(0x20),
        "firmware virtual-wire GSI 2 vec 0x20"
    );
    raise_pit();
    assert_eq!(
        take_ioapic_vector(),
        Some(0x20),
        "firmware virtual-wire GSI 2 AEOI"
    );
    reset();
    reset_cd();
    guest_platform::reset();
}

#[test]
fn product_iso_linux_ioapic_gsi2_programmed_not_firmware_leftover() {
    arm_product_iso();
    arm_firmware_virtual_wire();
    assert!(ioapic_gsi2_armed());
    assert!(!linux_ioapic_gsi2_programmed(), "linux PIC before leftover GSI 2");
    ioapic_write(0, 0x10 + 2 * u32::from(PIT_IOAPIC_GSI));
    note_linux_ioapic_write(0);
    assert!(!linux_ioapic_gsi2_programmed());
    ioapic_write(0x10, 0x31);
    note_linux_ioapic_write(0x10);
    assert!(linux_ioapic_gsi2_programmed(), "linux PIC before leftover GSI 2");
    reset();
    reset_cd();
    guest_platform::reset();
}

#[test]
fn product_iso_linux_hold_virtio_ioapic_beats_pit() {
    arm_product_iso();
    pic_init_unmask_all();
    ioapic_write(0, 0x10 + 2 * u32::from(PIT_IOAPIC_GSI));
    ioapic_write(0x10, 0x31);
    ioapic_write(0, 0x10 + 2 * u32::from(VIRTIO_GSI));
    ioapic_write(0x10, 0x51);
    prefer_pit_hold(true);
    raise_pit();
    raise_virtio();
    assert_eq!(
        take_inject_vector(),
        Some(0x51),
        "linux PIT hold UART not virtio"
    );
    reset();
    reset_cd();
    guest_platform::reset();
}

#[test]
fn product_iso_firmware_ioapic_ata_beats_pit() {
    arm_product_iso();
    arm_firmware_virtual_wire();
    raise_pit();
    raise_ata();
    assert_eq!(
        take_ioapic_vector(),
        Some(0x20 + ATA_GSI),
        "IOAPIC I/O over PIT; firmware virtual-wire GSI 14"
    );
    assert_eq!(
        take_ioapic_vector(),
        Some(0x20),
        "PIT still deliverable after ATA"
    );
    reset();
    reset_cd();
    guest_platform::reset();
}

#[test]
fn product_iso_firmware_virtual_wire_gsi14_unmasked() {
    arm_product_iso();
    raise_ata();
    assert!(
        take_ioapic_vector().is_none(),
        "masked pin 14 keeps ATA IRR undeliverable"
    );
    arm_firmware_virtual_wire();
    assert_eq!(
        take_ioapic_vector(),
        Some(0x20 + ATA_GSI),
        "firmware virtual-wire GSI 14"
    );
    reset();
    reset_cd();
    guest_platform::reset();
}

#[test]
fn product_iso_firmware_arm_ata_gsi14_without_pit() {
    arm_product_iso();
    raise_ata();
    raise_pit();
    assert!(
        take_ioapic_vector().is_none(),
        "masked pin 14 keeps ATA IRR undeliverable"
    );
    assert!(!pic_has_deliverable());
    arm_firmware_ata_gsi14();
    assert!(
        !firmware_virtual_wire_armed(),
        "firmware arm ATA GSI 14 does not arm virtual-wire"
    );
    assert!(
        !ioapic_gsi2_armed(),
        "firmware arm ATA GSI 14 does not unmask PIT GSI 2"
    );
    assert!(
        pic_ata_ready(),
        "firmware PIC ATA ICW2: IRQ 14 deliverable without OVMF ICW2"
    );
    assert_eq!(
        take_pic_vector(),
        Some(0x20 + ATA_GSI),
        "firmware PIC ATA: take 0x2E not PIT 0x20"
    );
    assert!(
        !pic_has_deliverable(),
        "firmware arm ATA GSI 14 does not unmask PIC IRQ 0"
    );
    assert!(
        ioapic_ata_ready(),
        "firmware ATA over PIC: pin 14 ready"
    );
    assert_eq!(
        take_ioapic_vector(),
        Some(0x20 + ATA_GSI),
        "firmware arm ATA GSI 14"
    );
    assert!(
        !ioapic_ata_ready(),
        "take consumes pin 14"
    );
    assert!(
        take_ioapic_vector().is_none(),
        "PIT pin 2 stays masked"
    );
    reset();
    reset_cd();
    guest_platform::reset();
}

#[test]
fn product_iso_firmware_ata_over_pic_beats_pit() {
    arm_product_iso();
    pic_init_unmask_all();
    // OVMF APIC-mode leftover: PIT IRQ 0 unmasked, ATA IRQ 14 still masked.
    let _ = pic_io(0x21, false, 1, 0xFA);
    let _ = pic_io(0xA1, false, 1, 0xFF);
    raise_pit();
    raise_ata();
    arm_firmware_ata_gsi14();
    assert!(
        pic_has_deliverable(),
        "HLT raise_pit leaves PIC IRQ 0 deliverable"
    );
    assert!(
        ioapic_ata_ready(),
        "firmware ATA over PIC: pin 14 ready while PIC IRQ 0 is live"
    );
    assert_eq!(
        take_pic_vector(),
        Some(0x20 + ATA_GSI),
        "firmware arm ATA GSI 14 unmasks PIC IRQ 14; skip_pit would not drop 0x2E"
    );
    assert_eq!(
        take_ioapic_vector(),
        Some(0x20 + ATA_GSI),
        "IOAPIC pin 14 still ready after PIC IRQ 14"
    );
    reset();
    reset_cd();
    guest_platform::reset();
}

#[test]
fn product_iso_firmware_pic_ata_repeats_without_eoi() {
    arm_product_iso();
    raise_ata();
    arm_firmware_ata_gsi14();
    assert!(
        pic_ata_ready(),
        "firmware PIC ATA"
    );
    assert_eq!(
        take_pic_vector(),
        Some(0x20 + ATA_GSI),
        "firmware PIC ATA IDENTIFY"
    );
    raise_ata();
    assert_eq!(
        take_pic_vector(),
        Some(0x20 + ATA_GSI),
        "firmware PIC ATA AEOI: PACKET after IDENTIFY without OCW2"
    );
    reset();
    reset_cd();
    guest_platform::reset();
}

fn pic_init_ovmf_edk2() {
    // EDK2 Legacy8259: master 0x68, slave 0x70. IRQ 14 → 0x76.
    let _ = pic_io(0x20, false, 1, 0x11);
    let _ = pic_io(0x21, false, 1, 0x68);
    let _ = pic_io(0x21, false, 1, 0x04);
    let _ = pic_io(0x21, false, 1, 0x01);
    let _ = pic_io(0xA0, false, 1, 0x11);
    let _ = pic_io(0xA1, false, 1, 0x70);
    let _ = pic_io(0xA1, false, 1, 0x02);
    let _ = pic_io(0xA1, false, 1, 0x01);
    let _ = pic_io(0x21, false, 1, 0xFB);
    let _ = pic_io(0xA1, false, 1, 0xFF);
}

#[test]
fn product_iso_firmware_ovmf_ata_vector_not_0x2e() {
    arm_product_iso();
    pic_init_ovmf_edk2();
    ioapic_write(0, 0x10 + 2 * u32::from(ATA_GSI));
    ioapic_write(0x10, 0x76);
    raise_ata();
    raise_pit();
    arm_firmware_ata_gsi14();
    assert_eq!(
        firmware_ata_vec(),
        0x76,
        "firmware OVMF ATA vector"
    );
    assert!(
        pic_ata_ready(),
        "firmware PIC ATA: IRQ 14 at EDK2 0x76"
    );
    assert_eq!(
        take_pic_vector(),
        Some(0x76),
        "do not clobber IOAPIC ATA vector: take PIC 0x76 not 0x2E"
    );
    assert_eq!(
        take_ioapic_ata_vector(),
        Some(0x76),
        "do not clobber IOAPIC ATA vector"
    );
    assert!(
        firmware_is_pit_vec(0x68),
        "firmware skip PIT inject: EDK2 IRQ 0 is 0x68"
    );
    assert!(!firmware_is_pit_vec(0x76));
    assert!(firmware_is_pit_vec(0x20));
    reset();
    reset_cd();
    guest_platform::reset();
}

#[test]
fn product_iso_firmware_leftover_0x2e_yields_pic_0x76() {
    arm_product_iso();
    arm_firmware_ata_gsi14();
    assert_eq!(firmware_ata_vec(), 0x2E, "early arm default ATA vec");
    pic_init_ovmf_edk2();
    raise_ata();
    arm_firmware_ata_gsi14();
    assert_eq!(
        firmware_ata_vec(),
        0x76,
        "do not inject leftover 0x2E after EDK2 ICW2"
    );
    assert!(pic_ata_ready(), "firmware PIC ATA at 0x76");
    assert_eq!(take_pic_vector(), Some(0x76));
    assert_eq!(
        take_ioapic_ata_vector(),
        Some(0x76),
        "do not inject leftover 0x2E: IOAPIC pin 14 synced to PIC 0x76"
    );
    reset();
    reset_cd();
    guest_platform::reset();
}

#[test]
fn product_iso_firmware_arm_does_not_clobber_pic_icw2() {
    arm_product_iso();
    arm_firmware_ata_gsi14();
    assert_eq!(firmware_ata_vec(), 0x2E, "early arm default ATA vec");
    // ICW1 + ICW2 only: ready stays false until ICW4.
    let _ = pic_io(0x20, false, 1, 0x11);
    let _ = pic_io(0x21, false, 1, 0x68);
    let _ = pic_io(0xA0, false, 1, 0x11);
    let _ = pic_io(0xA1, false, 1, 0x70);
    arm_firmware_ata_gsi14();
    assert_eq!(
        firmware_ata_vec(),
        0x76,
        "PIC ATA vector follows ICW2 even before ICW4 ready"
    );
    let _ = pic_io(0x21, false, 1, 0x04);
    let _ = pic_io(0x21, false, 1, 0x01);
    let _ = pic_io(0xA1, false, 1, 0x02);
    let _ = pic_io(0xA1, false, 1, 0x01);
    let _ = pic_io(0x21, false, 1, 0xFB);
    let _ = pic_io(0xA1, false, 1, 0xFF);
    raise_ata();
    arm_firmware_ata_gsi14();
    assert_eq!(
        firmware_ata_vec(),
        0x76,
        "do not clobber PIC ICW2: IRQ 14 stays 0x76 not 0x26"
    );
    assert!(!firmware_is_pit_vec(0x76));
    assert_eq!(take_pic_vector(), Some(0x76));
    assert_eq!(take_ioapic_ata_vector(), Some(0x76));
    reset();
    reset_cd();
    guest_platform::reset();
}

#[test]
fn product_iso_firmware_ioapic_0x76_beats_pic_0x2e() {
    arm_product_iso();
    arm_firmware_ata_gsi14();
    ioapic_write(0, 0x10 + 2 * u32::from(ATA_GSI));
    ioapic_write(0x10, 0x76);
    raise_ata();
    arm_firmware_ata_gsi14();
    assert_eq!(
        firmware_ata_vec(),
        0x76,
        "do not clobber IOAPIC ATA vector"
    );
    assert!(
        !pic_ata_ready(),
        "PIC default 0x2E must not beat OVMF IOAPIC 0x76"
    );
    assert_eq!(take_ioapic_ata_vector(), Some(0x76));
    reset();
    reset_cd();
    guest_platform::reset();
}

#[test]
fn product_iso_firmware_take_ioapic_ata_leaves_virtio() {
    arm_product_iso();
    ioapic_write(0, 0x10 + 2 * u32::from(VIRTIO_GSI));
    ioapic_write(0x10, 0x51);
    raise_virtio();
    arm_firmware_ata_gsi14();
    raise_ata();
    assert_eq!(
        take_ioapic_ata_vector(),
        Some(0x20 + ATA_GSI),
        "firmware take IOAPIC ATA: pin 14 not virtio"
    );
    assert_eq!(
        take_ioapic_vector(),
        Some(0x51),
        "firmware take IOAPIC ATA leaves virtio pending"
    );
    reset();
    reset_cd();
    guest_platform::reset();
}

#[test]
fn product_iso_firmware_ioapic_edge_ata_repeats_without_eoi() {
    arm_product_iso();
    arm_firmware_ata_gsi14();
    raise_ata();
    assert_eq!(
        take_ioapic_ata_vector(),
        Some(0x20 + ATA_GSI),
        "IDENTIFY take IOAPIC ATA"
    );
    assert!(
        take_ioapic_ata_vector().is_none(),
        "edge accept cleared IRR"
    );
    raise_ata();
    assert_eq!(
        take_ioapic_ata_vector(),
        Some(0x20 + ATA_GSI),
        "IOAPIC edge no remote IRR: PACKET after IDENTIFY without IOAPIC EOI"
    );
    reset();
    reset_cd();
    guest_platform::reset();
}

#[test]
fn product_iso_firmware_take_ioapic_ata_skips_virtio_only() {
    arm_product_iso();
    ioapic_write(0, 0x10 + 2 * u32::from(VIRTIO_GSI));
    ioapic_write(0x10, 0x51);
    raise_virtio();
    arm_firmware_ata_gsi14();
    assert!(
        take_ioapic_ata_vector().is_none(),
        "firmware take IOAPIC ATA: no pin 14"
    );
    assert_eq!(
        take_ioapic_vector(),
        Some(0x51),
        "firmware take IOAPIC ATA does not consume virtio"
    );
    reset();
    reset_cd();
    guest_platform::reset();
}

#[test]
fn product_iso_firmware_virtual_wire_pic_aeoi_clears_on_icw() {
    arm_product_iso();
    arm_firmware_virtual_wire();
    raise_pit();
    assert_eq!(take_pic_vector(), Some(0x20 + PIT_IRQ));
    pic_init_unmask_all();
    raise_pit();
    assert_eq!(take_pic_vector(), Some(0x20 + PIT_IRQ));
    raise_pit();
    assert!(
        !pic_has_deliverable(),
        "ICW4 without AEOI leaves ISR[0]; Linux OCW2 still required"
    );
    let _ = pic_io(0x20, false, 1, 0x20);
    assert!(pic_has_deliverable());
    reset();
    reset_cd();
    guest_platform::reset();
}

#[test]
fn product_iso_pit_irq0_injects_after_pic_ready() {
    arm_product_iso();
    pic_init_unmask_all();
    raise_pit();
    assert_eq!(take_inject_vector(), Some(0x20 + PIT_IRQ));
    reset();
    reset_cd();
    guest_platform::reset();
}

#[test]
fn product_iso_pic_deliverable_while_ovmf_ioapic_unmasked() {
    arm_product_iso();
    pic_init_unmask_all();
    ioapic_write(0, 0x10);
    ioapic_write(0x10, 0x30);
    raise_pit();
    assert!(pic_has_deliverable(), "linux PIC before LAPIC");
    assert_eq!(
        take_pic_vector(),
        Some(0x20 + PIT_IRQ),
        "virtual-wire PIT is PIC IRQ 0 even if OVMF left pin 0 unmasked"
    );
    assert!(
        take_inject_vector().is_none(),
        "PIT skips IOAPIC pin 0"
    );
    reset();
    reset_cd();
    guest_platform::reset();
}

#[test]
fn product_iso_pit_ioapic_gsi2_without_pic() {
    arm_product_iso();
    ioapic_write(0, 0x10 + 2 * u32::from(PIT_IOAPIC_GSI));
    ioapic_write(0x10, 0x30);
    raise_pit();
    assert_eq!(PIT_IOAPIC_GSI, 2);
    assert_eq!(
        take_inject_vector(),
        Some(0x30),
        "MADT IRQ0 ISO GSI 2"
    );
    reset();
    reset_cd();
    guest_platform::reset();
}

#[test]
fn product_iso_pit_skips_ioapic_pin0() {
    arm_product_iso();
    // OVMF leftover: pin 0 unmasked (vector 0x30).
    ioapic_write(0, 0x10);
    ioapic_write(0x10, 0x30);
    // Linux MADT ISO: pin 2 unmasked (vector 0x31).
    ioapic_write(0, 0x10 + 2 * u32::from(PIT_IOAPIC_GSI));
    ioapic_write(0x10, 0x31);
    raise_pit();
    assert_eq!(
        take_inject_vector(),
        Some(0x31),
        "PIT skips IOAPIC pin 0"
    );
    assert!(take_inject_vector().is_none());
    reset();
    reset_cd();
    guest_platform::reset();
}

#[test]
fn product_iso_gsi2_armed_beats_pic() {
    arm_product_iso();
    pic_init_unmask_all();
    ioapic_write(0, 0x10 + 2 * u32::from(PIT_IOAPIC_GSI));
    ioapic_write(0x10, 0x31);
    raise_pit();
    assert!(
        crate::devices::guest_irq::ioapic_gsi2_armed(),
        "linux GSI 2 before PIC"
    );
    assert!(pic_has_deliverable());
    assert_eq!(
        take_ioapic_vector(),
        Some(0x31),
        "linux GSI 2 before PIC"
    );
    reset();
    reset_cd();
    guest_platform::reset();
}

#[test]
fn product_iso_uart_beats_pit_and_virtio_beats_pit() {
    arm_product_iso();
    pic_init_unmask_all();
    raise_pit();
    raise_gsi(4);
    assert_eq!(
        take_inject_vector(),
        Some(0x24),
        "COM1 IRQ 4 must beat PIT so serial auto-answer is not starved"
    );
    assert_eq!(take_inject_vector(), Some(0x20 + PIT_IRQ));
    raise_pit();
    raise_virtio();
    assert_eq!(
        take_inject_vector(),
        Some(0x20 + VIRTIO_PIC_IRQ),
        "virtio PIC 11 (slave) must beat PIT"
    );
    reset();
    reset_cd();
    guest_platform::reset();
}

#[test]
fn product_iso_linux_pit_prefer_once_beats_uart() {
    arm_product_iso();
    pic_init_unmask_all();
    raise_pit();
    prefer_pit_once();
    raise_gsi(4);
    assert_eq!(
        take_inject_vector(),
        Some(0x20 + PIT_IRQ),
        "linux PIT prefer once beats UART"
    );
    raise_pit();
    raise_gsi(4);
    assert_eq!(
        take_inject_vector(),
        Some(0x24),
        "COM1 IRQ 4 must beat PIT after prefer-once is consumed"
    );
    reset();
    reset_cd();
    guest_platform::reset();
}

#[test]
fn product_iso_linux_pit_prefer_until_driver_ok_then_uart() {
    use crate::devices::guest_virtio_blk::{
        mmio_write, mmio_write_iso, present as present_virtio, reset as reset_virtio,
        virtio_needs_pit_over_uart, VIRTIO_STATUS_DRIVER_OK,
    };
    arm_product_iso();
    reset_virtio();
    assert!(present_virtio());
    assert!(
        virtio_needs_pit_over_uart(),
        "linux PIT prefer until DRIVER_OK"
    );
    pic_init_unmask_all();
    raise_pit();
    prefer_pit_until_driver_ok(virtio_needs_pit_over_uart());
    raise_gsi(4);
    assert_eq!(
        take_inject_vector(),
        Some(0x20 + PIT_IRQ),
        "prefer beats UART before DRIVER_OK"
    );
    mmio_write(0x14, 1, u64::from(VIRTIO_STATUS_DRIVER_OK));
    assert!(
        virtio_needs_pit_over_uart(),
        "ISO 00:03.0 still pending DRIVER_OK"
    );
    mmio_write_iso(0x14, 1, u64::from(VIRTIO_STATUS_DRIVER_OK));
    assert!(!virtio_needs_pit_over_uart());
    raise_pit();
    prefer_pit_until_driver_ok(virtio_needs_pit_over_uart());
    raise_gsi(4);
    assert_eq!(
        take_inject_vector(),
        Some(0x24),
        "COM1 IRQ 4 must beat PIT after both DRIVER_OK"
    );
    reset();
    reset_cd();
    reset_virtio();
    guest_platform::reset();
}

#[test]
fn product_iso_linux_pit_once_after_driver_ok_beats_uart_then_uart_wins() {
    use crate::devices::guest_virtio_blk::{
        mmio_write, mmio_write_iso, present as present_virtio, reset as reset_virtio,
        virtio_needs_pit_over_uart, VIRTIO_STATUS_DRIVER_OK,
    };
    use crate::vmx::guest_uefi::guest_uefi_linux_prefer_pit_during_apk;
    arm_product_iso();
    reset_virtio();
    assert!(present_virtio());
    mmio_write(0x14, 1, u64::from(VIRTIO_STATUS_DRIVER_OK));
    mmio_write_iso(0x14, 1, u64::from(VIRTIO_STATUS_DRIVER_OK));
    assert!(!virtio_needs_pit_over_uart());
    pic_init_unmask_all();
    assert!(
        guest_uefi_linux_prefer_pit_during_apk(virtio_needs_pit_over_uart()),
        "linux PIT once after DRIVER_OK"
    );
    raise_pit();
    raise_gsi(4);
    assert_eq!(
        take_inject_vector(),
        Some(0x20 + PIT_IRQ),
        "apk overlay PIT-once beats UART"
    );
    raise_pit();
    raise_gsi(4);
    assert_eq!(
        take_inject_vector(),
        Some(0x24),
        "COM1 IRQ 4 must beat PIT after prefer-once is consumed"
    );
    reset();
    reset_cd();
    reset_virtio();
    guest_platform::reset();
}

#[test]
fn product_iso_linux_hlt_pit_during_apk_until_login_then_uart() {
    use crate::devices::guest_serial_answer::{apk_overlay_needs_pit, note_tx, reset as reset_ans};
    use crate::devices::guest_virtio_blk::{
        mmio_write, mmio_write_iso, present as present_virtio, reset as reset_virtio,
        virtio_needs_pit_over_uart, VIRTIO_STATUS_DRIVER_OK,
    };
    use crate::vmx::guest_uefi::{
        guest_uefi_linux_hlt_prefer_pit_during_apk, guest_uefi_linux_hlt_uart_after_driver_ok,
        guest_uefi_linux_prefer_pit_during_apk, guest_uefi_linux_uart_prefer_pit_during_apk,
    };
    arm_product_iso();
    reset_virtio();
    reset_ans();
    assert!(present_virtio());
    mmio_write(0x14, 1, u64::from(VIRTIO_STATUS_DRIVER_OK));
    mmio_write_iso(0x14, 1, u64::from(VIRTIO_STATUS_DRIVER_OK));
    assert!(!virtio_needs_pit_over_uart());
    assert!(apk_overlay_needs_pit());
    pic_init_unmask_all();
    assert!(guest_uefi_linux_hlt_prefer_pit_during_apk(
        true,
        true,
        virtio_needs_pit_over_uart(),
        apk_overlay_needs_pit(),
    ));
    assert!(guest_uefi_linux_uart_prefer_pit_during_apk(
        true,
        virtio_needs_pit_over_uart(),
        apk_overlay_needs_pit(),
    ));
    assert!(guest_uefi_linux_prefer_pit_during_apk(
        virtio_needs_pit_over_uart()
    ));
    raise_pit();
    raise_gsi(4);
    assert_eq!(
        take_inject_vector(),
        Some(0x20 + PIT_IRQ),
        "linux HLT PIT during apk"
    );
    for &b in b"login:" {
        note_tx(b);
    }
    assert!(!apk_overlay_needs_pit());
    assert!(guest_uefi_linux_hlt_uart_after_driver_ok(
        true,
        true,
        virtio_needs_pit_over_uart(),
        apk_overlay_needs_pit(),
    ));
    raise_pit();
    raise_gsi(4);
    assert_eq!(
        take_inject_vector(),
        Some(0x24),
        "after login: UART beats PIT"
    );
    reset();
    reset_cd();
    reset_virtio();
    reset_ans();
    guest_platform::reset();
}

#[test]
fn product_iso_linux_pit_hold_until_login_not_consumed() {
    use crate::devices::guest_serial_answer::{apk_overlay_needs_pit, note_tx, reset as reset_ans};
    use crate::devices::guest_virtio_blk::{
        mmio_write, mmio_write_iso, present as present_virtio, reset as reset_virtio,
        virtio_both_driver_ok, virtio_linux_probe_started, virtio_needs_pit_over_uart,
        VIRTIO_STATUS_DRIVER_OK,
    };
    use crate::vmx::guest_uefi::{
        guest_uefi_linux_prefer_pit_hold, guest_uefi_linux_raise_pit_on_resume,
        guest_uefi_linux_raise_pit_on_resume_due, guest_uefi_linux_pit_resume_elapsed,
        LINUX_PIT_RESUME_MIN_TSC,
    };
    arm_product_iso();
    reset_virtio();
    reset_ans();
    assert!(present_virtio());
    mmio_write(0x14, 1, u64::from(VIRTIO_STATUS_DRIVER_OK));
    mmio_write_iso(0x14, 1, u64::from(VIRTIO_STATUS_DRIVER_OK));
    assert!(!virtio_needs_pit_over_uart());
    assert!(virtio_linux_probe_started());
    assert!(virtio_both_driver_ok());
    assert!(apk_overlay_needs_pit());
    pic_init_unmask_all();
    assert!(guest_uefi_linux_prefer_pit_hold(
        virtio_needs_pit_over_uart(),
        apk_overlay_needs_pit(),
        virtio_linux_probe_started(),
        virtio_both_driver_ok(),
    ));
    assert!(guest_uefi_linux_raise_pit_on_resume(
        apk_overlay_needs_pit(),
        virtio_needs_pit_over_uart(),
        virtio_linux_probe_started(),
        virtio_both_driver_ok(),
    ));
    assert!(guest_uefi_linux_raise_pit_on_resume_due(
        apk_overlay_needs_pit(),
        virtio_both_driver_ok(),
        LINUX_PIT_RESUME_MIN_TSC,
        0,
        LINUX_PIT_RESUME_MIN_TSC,
    ));
    assert!(
        !guest_uefi_linux_raise_pit_on_resume_due(
            apk_overlay_needs_pit(),
            virtio_both_driver_ok(),
            100,
            1,
            LINUX_PIT_RESUME_MIN_TSC,
        ),
        "linux PIT resume paced"
    );
    assert!(guest_uefi_linux_pit_resume_elapsed(0, 0, LINUX_PIT_RESUME_MIN_TSC));
    raise_pit();
    raise_gsi(4);
    assert_eq!(
        take_inject_vector(),
        Some(0x20 + PIT_IRQ),
        "linux PIT hold until login"
    );
    let _ = pic_io(0x20, false, 1, 0x20);
    raise_pit();
    raise_gsi(4);
    assert_eq!(
        take_inject_vector(),
        Some(0x20 + PIT_IRQ),
        "hold is not consumed — idle=poll still sees jiffies"
    );
    let _ = pic_io(0x20, false, 1, 0x20);
    raise_pit();
    raise_virtio();
    assert_eq!(
        take_inject_vector(),
        Some(0x20 + VIRTIO_PIC_IRQ),
        "linux PIT hold UART not virtio"
    );
    crate::devices::guest_irq::lower_virtio();
    for &b in b"login:" {
        note_tx(b);
    }
    assert!(!apk_overlay_needs_pit());
    assert!(!guest_uefi_linux_prefer_pit_hold(
        virtio_needs_pit_over_uart(),
        apk_overlay_needs_pit(),
        virtio_linux_probe_started(),
        virtio_both_driver_ok(),
    ));
    raise_pit();
    raise_gsi(4);
    assert_eq!(
        take_inject_vector(),
        Some(0x24),
        "after login: UART beats PIT"
    );
    reset();
    reset_cd();
    reset_virtio();
    reset_ans();
    guest_platform::reset();
}

#[test]
fn product_iso_virtio_shared_intx_survives_sibling_isr_read() {
    use crate::devices::guest_virtio_blk::{
        drain_queue, mmio_read, mmio_read_iso, mmio_write, mmio_write_iso, present as present_virtio,
        reset as reset_virtio, virtio_isr_latched,
    };
    arm_product_iso();
    reset_virtio();
    assert!(present_virtio());
    pic_init_unmask_all();
    mmio_write(0x300, 2, 1);
    mmio_write_iso(0x300, 2, 1);
    let _ = drain_queue(|_| None);
    assert!(virtio_isr_latched(), "virtio shared INTx");
    assert_eq!(mmio_read(0x100, 1), 1, "disk ISR read-to-clear");
    assert!(
        virtio_isr_latched(),
        "ISO ISR still latched after disk ISR read"
    );
    assert_eq!(
        take_pic_vector(),
        Some(0x20 + VIRTIO_PIC_IRQ),
        "shared PIC 11 stays pending for the sibling"
    );
    assert_eq!(mmio_read_iso(0x100, 1), 1);
    reset();
    reset_cd();
    reset_virtio();
    guest_platform::reset();
}

#[test]
fn product_iso_virtio_shared_intx_hold_zero_disk_isr() {
    use crate::devices::guest_virtio_blk::{
        drain_queue, mmio_read, mmio_write_iso, present as present_virtio,
        reset as reset_virtio, virtio_isr_latched, virtio_shared_intx_hold,
    };
    arm_product_iso();
    reset_virtio();
    assert!(present_virtio());
    pic_init_unmask_all();
    mmio_write_iso(0x300, 2, 1);
    let _ = drain_queue(|_| None);
    assert!(virtio_isr_latched(), "ISO kick latches ISR");
    assert!(virtio_shared_intx_hold(true), "virtio shared INTx hold");
    assert_eq!(mmio_read(0x100, 1), 0, "disk ISR already clear");
    assert!(
        virtio_isr_latched(),
        "ISO ISR still latched after zero disk ISR read"
    );
    assert_eq!(
        take_pic_vector(),
        Some(0x20 + VIRTIO_PIC_IRQ),
        "PIC 11 stays pending when sibling ISR is 1"
    );
    reset();
    reset_cd();
    reset_virtio();
    guest_platform::reset();
}

#[test]
fn product_iso_virtio_flush_without_notify_raises_intx() {
    use crate::devices::guest_virtio_blk::{
        attach_disk, drain_queue, mmio_write, present as present_virtio, reset as reset_virtio,
        virtio_isr_latched, VIRTIO_BLK_T_FLUSH,
    };
    arm_product_iso();
    reset_virtio();
    assert!(present_virtio());
    pic_init_unmask_all();
    let mut disk = vec![0u8; 4096];
    // SAFETY: host test owns `disk` until reset_virtio.
    assert!(unsafe { attach_disk(disk.as_mut_ptr() as u64, disk.len()) });
    let mut guest = vec![0u8; 4096];
    mmio_write(0x16, 2, 0);
    mmio_write(0x18, 2, 4);
    mmio_write(0x20, 8, 0);
    mmio_write(0x28, 8, 256);
    mmio_write(0x30, 8, 512);
    mmio_write(0x1C, 2, 1);
    let hdr_gpa = 0x300u64;
    guest[hdr_gpa as usize..hdr_gpa as usize + 4]
        .copy_from_slice(&VIRTIO_BLK_T_FLUSH.to_le_bytes());
    guest[0x700] = 0xFF;
    fn put_desc(mem: &mut [u8], i: u16, addr: u64, len: u32, flags: u16, next: u16) {
        let o = (i as usize) * 16;
        mem[o..o + 8].copy_from_slice(&addr.to_le_bytes());
        mem[o + 8..o + 12].copy_from_slice(&len.to_le_bytes());
        mem[o + 12..o + 14].copy_from_slice(&flags.to_le_bytes());
        mem[o + 14..o + 16].copy_from_slice(&next.to_le_bytes());
    }
    put_desc(&mut guest, 0, hdr_gpa, 16, 1, 1);
    put_desc(&mut guest, 1, 0x700, 1, 2, 0);
    guest[256 + 2..256 + 4].copy_from_slice(&1u16.to_le_bytes());
    guest[256 + 4..256 + 6].copy_from_slice(&0u16.to_le_bytes());
    let base = guest.as_ptr() as u64;
    let glen = guest.len() as u64;
    let n = drain_queue(|gpa| {
        if gpa < glen {
            Some(base + gpa)
        } else {
            None
        }
    });
    assert_eq!(n, 0, "FLUSH has no OUT bytes");
    assert_eq!(guest[0x700], 0, "FLUSH status OK");
    assert!(virtio_isr_latched(), "virtio drain FLUSH");
    assert_eq!(
        take_pic_vector(),
        Some(0x20 + VIRTIO_PIC_IRQ),
        "apk overlay FLUSH without kick still raises PIC 11"
    );
    reset();
    reset_cd();
    reset_virtio();
    guest_platform::reset();
}

#[test]
fn product_iso_virtio_pic_level_intx_retriggers_after_eoi() {
    arm_product_iso();
    pic_init_unmask_all();
    raise_virtio();
    assert_eq!(
        take_pic_vector(),
        Some(0x20 + VIRTIO_PIC_IRQ),
        "linux virtio PIC level INTx"
    );
    // Linux handle_edge_irq EOIs before vp_interrupt reads ISR.
    let _ = pic_io(0xA0, false, 1, 0x20);
    let _ = pic_io(0x20, false, 1, 0x20);
    assert_eq!(
        take_pic_vector(),
        Some(0x20 + VIRTIO_PIC_IRQ),
        "level INTx stays pending until device ISR read"
    );
    crate::devices::guest_irq::lower_virtio();
    let _ = pic_io(0xA0, false, 1, 0x20);
    let _ = pic_io(0x20, false, 1, 0x20);
    assert!(take_pic_vector().is_none(), "lower_virtio deasserts INTx");
    reset();
    reset_cd();
    guest_platform::reset();
}

#[test]
fn product_iso_virtio_pic_irq11_yields_pit_during_hold() {
    use crate::devices::guest_serial_answer::reset as reset_ans;
    arm_product_iso();
    reset_ans();
    pic_init_unmask_all();
    prefer_pit_hold(true);
    raise_virtio();
    raise_pit();
    assert_eq!(
        take_pic_vector(),
        Some(0x20 + VIRTIO_PIC_IRQ),
        "first collision still delivers virtio PIC 11"
    );
    let _ = pic_io(0xA0, false, 1, 0x20);
    let _ = pic_io(0x20, false, 1, 0x20);
    assert_eq!(
        take_pic_vector(),
        Some(0x20 + PIT_IRQ),
        "linux PIC IRQ11 yield PIT"
    );
    let _ = pic_io(0x20, false, 1, 0x20);
    raise_pit();
    assert_eq!(
        take_pic_vector(),
        Some(0x20 + VIRTIO_PIC_IRQ),
        "after PIT turn, level INTx still pending"
    );
    reset();
    reset_cd();
    reset_ans();
    guest_platform::reset();
}

#[test]
fn product_iso_virtio_pic_irq11_three_to_one_after_mount() {
    use crate::devices::guest_serial_answer::{
        apk_media_mounted, note_tx, reset as reset_ans,
    };
    arm_product_iso();
    reset_ans();
    pic_init_unmask_all();
    prefer_pit_hold(true);
    for &b in b"Mounting boot media: ok." {
        note_tx(b);
    }
    assert!(apk_media_mounted(), "linux PIC IRQ11 yield until mount");
    raise_virtio();
    raise_pit();
    assert_eq!(
        take_pic_vector(),
        Some(0x20 + VIRTIO_PIC_IRQ),
        "linux PIC IRQ11 yield 3 after mount: first"
    );
    let _ = pic_io(0xA0, false, 1, 0x20);
    let _ = pic_io(0x20, false, 1, 0x20);
    raise_pit();
    assert_eq!(
        take_pic_vector(),
        Some(0x20 + VIRTIO_PIC_IRQ),
        "linux PIC IRQ11 yield 3 after mount: second"
    );
    let _ = pic_io(0xA0, false, 1, 0x20);
    let _ = pic_io(0x20, false, 1, 0x20);
    raise_pit();
    assert_eq!(
        take_pic_vector(),
        Some(0x20 + VIRTIO_PIC_IRQ),
        "linux PIC IRQ11 yield 3 after mount: third arms yield"
    );
    let _ = pic_io(0xA0, false, 1, 0x20);
    let _ = pic_io(0x20, false, 1, 0x20);
    raise_pit();
    assert_eq!(
        take_pic_vector(),
        Some(0x20 + PIT_IRQ),
        "linux PIC IRQ11 yield 3 after mount"
    );
    reset();
    reset_cd();
    reset_ans();
    guest_platform::reset();
}

#[test]
fn product_iso_linux_x86_64_unmasks_virtio_pic_irq11() {
    arm_product_iso();
    // Linux x86_64 IRQ0_VECTOR 0x30 / slave 0x38. Leave IRQ 11 masked.
    let _ = pic_io(0x20, false, 1, 0x11);
    let _ = pic_io(0x21, false, 1, 0x30);
    let _ = pic_io(0x21, false, 1, 0x04);
    let _ = pic_io(0x21, false, 1, 0x01);
    let _ = pic_io(0xA0, false, 1, 0x11);
    let _ = pic_io(0xA1, false, 1, 0x38);
    let _ = pic_io(0xA1, false, 1, 0x02);
    let _ = pic_io(0xA1, false, 1, 0x01);
    let _ = pic_io(0x21, false, 1, 0xFB); // unmask cascade only
    let _ = pic_io(0xA1, false, 1, 0xFF); // all slave masked
    raise_virtio();
    assert_eq!(
        take_pic_vector(),
        Some(0x38 + VIRTIO_PIC_IRQ - 8),
        "linux PIC IRQ11 unmask; linux PIC IRQ0 vec 0x30"
    );
    reset();
    reset_cd();
    guest_platform::reset();
}

#[test]
fn product_iso_linux_early_kernel_uart_beats_pit() {
    use crate::devices::guest_serial_answer::{apk_overlay_needs_pit, reset as reset_ans};
    use crate::devices::guest_virtio_blk::{
        mmio_write, present as present_virtio, reset as reset_virtio, virtio_both_driver_ok,
        virtio_linux_probe_started, virtio_needs_pit_over_uart,
    };
    use crate::vmx::guest_uefi::{
        guest_uefi_linux_pit_jiffies_now, guest_uefi_linux_prefer_pit_hold,
        guest_uefi_linux_raise_pit_on_resume,
    };
    arm_product_iso();
    reset_virtio();
    reset_ans();
    assert!(present_virtio());
    assert!(
        virtio_needs_pit_over_uart(),
        "firmware queue-arm still pending DRIVER_OK"
    );
    assert!(
        !virtio_linux_probe_started(),
        "linux PIT after virtio probe"
    );
    assert!(!virtio_both_driver_ok());
    assert!(apk_overlay_needs_pit(), "PHASE_LOGIN from boot");
    assert!(
        !guest_uefi_linux_pit_jiffies_now(
            virtio_needs_pit_over_uart(),
            apk_overlay_needs_pit(),
            virtio_linux_probe_started(),
            virtio_both_driver_ok(),
        ),
        "iron c815ccc: do not inject PIT during APIC setup"
    );
    pic_init_unmask_all();
    assert!(!guest_uefi_linux_prefer_pit_hold(
        virtio_needs_pit_over_uart(),
        apk_overlay_needs_pit(),
        virtio_linux_probe_started(),
        virtio_both_driver_ok(),
    ));
    raise_pit();
    raise_gsi(4);
    assert_eq!(
        take_inject_vector(),
        Some(0x24),
        "early kernel UART beats PIT"
    );
    mmio_write(0x14, 1, 1);
    assert!(virtio_linux_probe_started(), "linux PIT after virtio probe");
    assert!(guest_uefi_linux_prefer_pit_hold(
        virtio_needs_pit_over_uart(),
        apk_overlay_needs_pit(),
        virtio_linux_probe_started(),
        virtio_both_driver_ok(),
    ));
    assert!(
        !guest_uefi_linux_raise_pit_on_resume(
            apk_overlay_needs_pit(),
            virtio_needs_pit_over_uart(),
            virtio_linux_probe_started(),
            virtio_both_driver_ok(),
        ),
        "linux PIT raise after DRIVER_OK not probe"
    );
    raise_pit();
    raise_gsi(4);
    assert_eq!(
        take_inject_vector(),
        Some(0x20 + PIT_IRQ),
        "after DEVICE_STATUS, PIT hold for virtio probe"
    );
    reset();
    reset_cd();
    reset_virtio();
    reset_ans();
    guest_platform::reset();
}

#[test]
fn lab_stub_raise_pit_does_not_inject() {
    reset();
    reset_cd();
    guest_platform::reset();
    raise_pit();
    assert!(!has_deliverable());
    reset();
}

#[test]
fn ioapic_level_keeps_irr_until_eoi_then_retries() {
    arm_product_iso();
    ioapic_write(0, 0x10 + 2 * u32::from(VIRTIO_PIC_IRQ));
    ioapic_write(0x10, 0x53 | (1 << 15));
    raise_virtio();
    assert_eq!(take_inject_vector(), Some(0x53));
    assert!(
        take_inject_vector().is_none(),
        "remote IRR blocks re-inject until EOI"
    );
    crate::devices::guest_irq::ioapic_eoi(0x53);
    assert_eq!(
        take_inject_vector(),
        Some(0x53),
        "level + line still high retries after EOI"
    );
    crate::devices::guest_irq::ioapic_eoi(0x53);
    crate::devices::guest_irq::lower_virtio();
    crate::devices::guest_irq::ioapic_eoi(0x53);
    assert!(take_inject_vector().is_none());
    assert_eq!(crate::devices::guest_irq::take_ioapic_vector(), None);
    reset();
    reset_cd();
    guest_platform::reset();
}

#[test]
fn pic_master_snap_counts_irq0_and_irq4_takes() {
    // UART THRE chain telemetry: the stall heartbeat prints master
    // IRR/IMR/ISR plus INTA counts so iron can say whether IRQ 4 was ever
    // taken while `apk` sat in `n_tty_write`.
    use crate::devices::guest_irq::pic_master_snap;
    arm_product_iso();
    let s0 = pic_master_snap();
    assert_eq!((s0.take_irq0, s0.take_irq4), (0, 0));
    assert!(!s0.ready, "8259 not programmed yet");
    let _ = pic_io(0x20, false, 1, 0x11);
    let _ = pic_io(0x21, false, 1, 0x20);
    let _ = pic_io(0x21, false, 1, 0x04);
    let _ = pic_io(0x21, false, 1, 0x01);
    let _ = pic_io(0xA0, false, 1, 0x11);
    let _ = pic_io(0xA1, false, 1, 0x28);
    let _ = pic_io(0xA1, false, 1, 0x02);
    let _ = pic_io(0xA1, false, 1, 0x01);
    let _ = pic_io(0x21, false, 1, 0xEE);
    let _ = pic_io(0xA1, false, 1, 0xFF);
    raise_pit();
    raise_gsi(4);
    let s1 = pic_master_snap();
    assert!(s1.ready);
    assert_eq!(s1.imr, 0xEE);
    assert_eq!(s1.irr & 0x11, 0x11, "IRQ 0 and IRQ 4 latched");
    assert_eq!(s1.isr, 0);
    // No PIT preference armed: UART beats PIT on the master.
    assert_eq!(take_pic_vector(), Some(0x24));
    let s2 = pic_master_snap();
    assert_eq!((s2.take_irq0, s2.take_irq4), (0, 1));
    assert_eq!(s2.isr & 0x10, 0x10, "IRQ 4 in service until EOI");
    let _ = pic_io(0x20, false, 1, 0x64);
    assert_eq!(take_pic_vector(), Some(0x20));
    let s3 = pic_master_snap();
    assert_eq!((s3.take_irq0, s3.take_irq4), (1, 1));
    let _ = pic_io(0x20, false, 1, 0x60);
    assert_eq!(pic_master_snap().isr, 0);
    reset();
    let s4 = pic_master_snap();
    assert_eq!((s4.take_irq0, s4.take_irq4), (0, 0), "reset clears INTA counts");
    assert_eq!((s4.irr, s4.imr, s4.isr, s4.ready), (0, 0xFF, 0, false));
    reset_cd();
    guest_platform::reset();
}
