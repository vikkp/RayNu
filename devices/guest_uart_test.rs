use super::{pio, poll_host_rx, push_host_rx, reassert_irq, reset};
use crate::boot::serial::{
    guest_tx_clear, set_guest_tx_test_sol_not_ready, set_linux_earlycon_share,
};

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
    let _ = pio(0x03FA, false, 0x01);
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

fn arm_product_iso_for_irq() {
    crate::devices::ide_cdrom::reset();
    let extra =
        crate::devices::ide_cdrom::MOCK_EFI_ISO_BYTES + crate::devices::ide_cdrom::ISO_SECTOR;
    let mut iso = vec![0u8; extra];
    crate::devices::ide_cdrom::write_placeholder_iso(
        &mut iso[..crate::devices::ide_cdrom::MOCK_EFI_ISO_BYTES],
    );
    assert!(crate::devices::ide_cdrom::present(&iso, 9));
}

#[test]
fn loopback_thr_appears_in_rbr() {
    reset();
    let _ = pio(0x03FB, false, 0x00);
    let _ = pio(0x03FC, false, 0x10);
    let (_, thr, _) = pio(0x03F8, false, b'A');
    assert!(thr.is_none(), "loopback must not emit host serial");
    let (lsr, _, _) = pio(0x03FD, true, 0);
    assert_eq!(lsr, 0x61);
    let (rbr, _, _) = pio(0x03F8, true, 0);
    assert_eq!(rbr, b'A');
    let (lsr2, _, _) = pio(0x03FD, true, 0);
    assert_eq!(lsr2, 0x60);
    reset();
}

#[test]
fn host_rx_raises_irq4_and_iir_is_c4() {
    reset();
    crate::devices::guest_irq::reset();
    arm_product_iso_for_irq();
    crate::devices::guest_irq::ioapic_write(0, 0x10 + 2 * 4);
    crate::devices::guest_irq::ioapic_write(0x10, 0x24);
    let _ = pio(0x03FA, false, 0x01);
    let _ = pio(0x03F9, false, 0x01);
    assert!(push_host_rx(b'k'));
    assert_eq!(crate::devices::guest_irq::take_inject_vector(), Some(0x24));
    let (iir, _, _) = pio(0x03FA, true, 0);
    assert_eq!(iir, 0xC4);
    let (rbr, _, _) = pio(0x03F8, true, 0);
    assert_eq!(rbr, b'k');
    reassert_irq();
    assert!(crate::devices::guest_irq::take_inject_vector().is_none());
    poll_host_rx();
    assert!(crate::devices::guest_irq::take_inject_vector().is_none());
    crate::devices::ide_cdrom::reset();
    reset();
    crate::devices::guest_irq::    reset();
}

#[test]
fn linux_earlycon_lsr_thre_follows_sol() {
    reset();
    guest_tx_clear();
    set_linux_earlycon_share(true);
    set_guest_tx_test_sol_not_ready(true);
    let (lsr, _, _) = pio(0x03FD, true, 0);
    assert_eq!(lsr & 0x60, 0, "THRE/TEMT clear while SOL not ready");
    set_guest_tx_test_sol_not_ready(false);
    let (lsr2, _, _) = pio(0x03FD, true, 0);
    assert_eq!(lsr2 & 0x60, 0x60);
    let src = include_str!("guest_uart.rs");
    assert!(src.contains("linux earlycon pace LSR THRE"));
    assert!(src.contains("Keep the 0x60/0x61 path until"));
    set_linux_earlycon_share(false);
    guest_tx_clear();
    reset();
}

#[test]
fn autoanswer_login_fills_rbr() {
    reset();
    let _ = pio(0x03FB, false, 0x00);
    for &b in b"login:" {
        let _ = pio(0x03F8, false, b);
    }
    let mut got = Vec::new();
    for _ in 0..crate::devices::guest_serial_answer::ROOT.len() {
        let (c, _, _) = pio(0x03F8, true, 0);
        got.push(c);
    }
    assert_eq!(got, crate::devices::guest_serial_answer::ROOT);
    reset();
}
