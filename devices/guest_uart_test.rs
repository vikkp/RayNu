use super::{pio, reassert_irq, reset};

#[test]
fn scratch_roundtrip_and_fifo_iir() {
    reset();
    crate::devices::guest_irq::reset();
    crate::devices::ide_cdrom::reset();
    let _ = pio(0x03FB, false, 0x00);
    let _ = pio(0x03FF, false, 0xA5);
    let (scr, _, _) = pio(0x03FF, true, 0);
    assert_eq!(scr, 0xA5);
    let _ = pio(0x03FA, false, 0x01);
    let (iir, _, _) = pio(0x03FA, true, 0);
    assert_eq!(iir, 0xC1, "16550 FIFO present, no IRQ");
    reset();
}

#[test]
fn product_iso_thre_raises_irq4() {
    reset();
    crate::devices::guest_irq::reset();
    crate::devices::ide_cdrom::reset();
    let extra =
        crate::devices::ide_cdrom::MOCK_EFI_ISO_BYTES + crate::devices::ide_cdrom::ISO_SECTOR;
    let mut iso = vec![0u8; extra];
    crate::devices::ide_cdrom::write_placeholder_iso(
        &mut iso[..crate::devices::ide_cdrom::MOCK_EFI_ISO_BYTES],
    );
    assert!(crate::devices::ide_cdrom::present(&iso, 9));
    crate::devices::guest_irq::ioapic_write(0, 0x10 + 2 * 4);
    crate::devices::guest_irq::ioapic_write(0x10, 0x24);
    let _ = pio(0x03F9, false, 0x02);
    assert_eq!(crate::devices::guest_irq::take_inject_vector(), Some(0x24));
    let (iir, _, _) = pio(0x03FA, true, 0);
    assert_eq!(iir, 0xC2);
    reassert_irq();
    assert!(
        crate::devices::guest_irq::take_inject_vector().is_none(),
        "IIR read cleared THRE"
    );
    crate::devices::ide_cdrom::reset();
    reset();
    crate::devices::guest_irq::reset();
}
