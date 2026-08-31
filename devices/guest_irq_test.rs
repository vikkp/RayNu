    use super::{
    has_deliverable, ioapic_read, ioapic_write, is_hpet_split_2m_gpa, is_ioapic_gpa, lower_ata,
    pic_has_deliverable, pic_io, pic_shadow_out, prefer_pit_once, prefer_pit_until_driver_ok,     raise_ata, raise_gsi,
    raise_pit,     raise_nested_iso0_pit, raise_nested_iso0_ata, raise_virtio, reset, take_inject_vector, take_ioapic_ata_vector, take_ioapic_vector, take_pic_vector, take_nested_iso0_pit,
    take_nested_iso0_pit_or_edk2, take_nested_iso0_ata, nested_iso0_irq0_vec, nested_iso0_irq14_vec, NESTED_ISO0_EDK2_IRQ0, NESTED_ISO0_EDK2_IRQ14,
    arm_firmware_virtual_wire, arm_firmware_ata_gsi14, firmware_virtual_wire_armed, ioapic_ata_ready, pic_ata_ready,
    firmware_ata_vec, firmware_is_pit_vec,
    ATA_GSI, IOAPIC_GPA,
    IOAPIC_VERSION, PIT_IOAPIC_GSI, PIT_IRQ, VIRTIO_GSI, VIRTIO_ISO_GSI, VIRTIO_PIC_IRQ,
    ioapic_gsi2_armed,
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
fn nested_iso0_firmware_hlt_pit_follows_icw2() {
    reset();
    reset_cd();
    guest_platform::reset();
    assert!(!crate::devices::ide_cdrom::product_iso_window_armed());
    // Guest-visible INs stay RAZ/WI; OUTs shadow ICW2 0x68 (EDK2 Legacy8259).
    assert_eq!(guest_platform::io(0x20, true, 1, 0xFFFF) as u8, 0);
    pic_shadow_out(0x20, 1, 0x11);
    pic_shadow_out(0x21, 1, 0x68);
    pic_shadow_out(0x21, 1, 0x04);
    pic_shadow_out(0x21, 1, 0x01);
    pic_shadow_out(0xA0, 1, 0x11);
    pic_shadow_out(0xA1, 1, 0x70);
    pic_shadow_out(0xA1, 1, 0x02);
    pic_shadow_out(0xA1, 1, 0x01);
    pic_shadow_out(0x21, 1, 0xFE);
    pic_shadow_out(0xA1, 1, 0xFF);
    assert_eq!(guest_platform::io(0x20, true, 1, 0xFFFF) as u8, 0);
    raise_nested_iso0_pit();
    assert!(take_inject_vector().is_none(), "product take stays window-armed");
    assert_eq!(
        take_nested_iso0_pit(),
        Some(0x68),
        "nested iso=0 firmware HLT PIT"
    );
    assert!(take_nested_iso0_pit().is_none());
    raise_nested_iso0_pit();
    assert_eq!(
        take_nested_iso0_pit_or_edk2(),
        0x68,
        "nested iso=0 EDK2 IRQ0 after take"
    );
    reset();
}

#[test]
fn nested_iso0_firmware_hlt_ata_edk2_irq14() {
    reset();
    reset_cd();
    guest_platform::reset();
    assert!(!crate::devices::ide_cdrom::product_iso_window_armed());
    assert_eq!(
        nested_iso0_irq14_vec(),
        NESTED_ISO0_EDK2_IRQ14,
        "nested iso=0 firmware HLT ATA"
    );
    raise_nested_iso0_ata();
    assert!(
        take_nested_iso0_ata().is_none(),
        "no ICW: pic_take still needs ready"
    );
    pic_shadow_out(0x20, 1, 0x11);
    pic_shadow_out(0x21, 1, 0x68);
    pic_shadow_out(0x21, 1, 0x04);
    pic_shadow_out(0x21, 1, 0x01);
    pic_shadow_out(0xA0, 1, 0x11);
    pic_shadow_out(0xA1, 1, 0x70);
    pic_shadow_out(0xA1, 1, 0x02);
    pic_shadow_out(0xA1, 1, 0x01);
    pic_shadow_out(0x21, 1, 0xFB);
    pic_shadow_out(0xA1, 1, 0xBF);
    raise_nested_iso0_ata();
    assert_eq!(
        take_nested_iso0_ata(),
        Some(0x76),
        "nested iso=0 firmware HLT ATA; do not inject leftover 0x2E"
    );
    reset();
}

#[test]
fn nested_iso0_edk2_irq0_when_pic_take_none() {
    reset();
    reset_cd();
    guest_platform::reset();
    assert!(!crate::devices::ide_cdrom::product_iso_window_armed());
    assert_eq!(nested_iso0_irq0_vec(), NESTED_ISO0_EDK2_IRQ0);
    raise_nested_iso0_pit();
    assert!(
        take_nested_iso0_pit().is_none(),
        "no ICW: pic_take still needs ready"
    );
    assert_eq!(
        take_nested_iso0_pit_or_edk2(),
        0x68,
        "nested iso=0 EDK2 IRQ0"
    );
    pic_shadow_out(0x20, 1, 0x11);
    pic_shadow_out(0x21, 1, 0x68);
    assert_eq!(nested_iso0_irq0_vec(), 0x68, "ICW2 without ICW4");
    raise_nested_iso0_pit();
    assert!(take_nested_iso0_pit().is_none(), "ICW4 pending");
    assert_eq!(
        take_nested_iso0_pit_or_edk2(),
        0x68,
        "nested iso=0 EDK2 IRQ0 follows ICW2"
    );
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
fn product_iso_srst_clear_raises_ata_irq() {
    arm_product_iso();
    pic_init_ovmf_edk2();
    arm_firmware_ata_gsi14();
    let _ = crate::devices::ide_cdrom::ata_io(0x3F6, false, 1, 0x04);
    let _ = crate::devices::ide_cdrom::ata_io(0x3F6, false, 1, 0x00);
    assert!(
        ioapic_ata_ready() || pic_ata_ready(),
        "firmware SRST ATA IRQ"
    );
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
