use super::{
    has_deliverable, ioapic_read, ioapic_write, is_hpet_split_2m_gpa, is_ioapic_gpa, lower_ata,
    pic_io, raise_ata, raise_gsi, raise_virtio, reset, take_inject_vector, ATA_GSI, IOAPIC_GPA,
    IOAPIC_VERSION, VIRTIO_GSI, VIRTIO_ISO_GSI, VIRTIO_PIC_IRQ,
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
