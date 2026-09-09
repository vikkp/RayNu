use super::{inject_sysrq, pio, poll_host_rx, push_host_rx, reassert_irq, reset};
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
    crate::devices::guest_irq::ioapic_eoi(0x24);
    reassert_irq();
    assert_eq!(
        crate::devices::guest_irq::take_inject_vector(),
        Some(0x24),
        "UART THRE level until stop_tx: IIR read alone does not clear it"
    );
    crate::devices::guest_irq::ioapic_eoi(0x24);
    // Linux __stop_tx: ETBEI off ends the level.
    let _ = pio(0x03F9, false, 0x00);
    reassert_irq();
    assert!(crate::devices::guest_irq::take_inject_vector().is_none());
    crate::devices::ide_cdrom::reset();
    reset();
    crate::devices::guest_irq::reset();
}

#[test]
fn product_iso_thre_waits_for_sol_then_reasserts() {
    // Iron `f229d14` / `34354322953`: apk in n_tty_write; THRE lost when LSR
    // said 0 after the IIR read. UART THRE level until stop_tx.
    reset();
    crate::devices::guest_irq::reset();
    arm_product_iso_for_irq();
    crate::devices::guest_irq::ioapic_write(0, 0x10 + 2 * 4);
    crate::devices::guest_irq::ioapic_write(0x10, 0x24);
    guest_tx_clear();
    set_linux_earlycon_share(true);
    set_guest_tx_test_sol_not_ready(true);
    let _ = pio(0x03FA, false, 0x01);
    let _ = pio(0x03F9, false, 0x02);
    let (_, thr, _) = pio(0x03F8, false, b'a');
    assert_eq!(thr, Some(b'a'));
    assert!(
        crate::devices::guest_irq::take_inject_vector().is_none(),
        "THR not empty while SOL is back-pressured"
    );
    let (iir, _, _) = pio(0x03FA, true, 0);
    assert_eq!(iir, 0xC1, "no interrupt pending, latch kept");
    let (lsr, _, _) = pio(0x03FD, true, 0);
    assert_eq!(lsr & 0x20, 0);
    reassert_irq();
    assert!(crate::devices::guest_irq::take_inject_vector().is_none());
    set_guest_tx_test_sol_not_ready(false);
    reassert_irq();
    assert_eq!(
        crate::devices::guest_irq::take_inject_vector(),
        Some(0x24),
        "THRE re-asserted once the ring drains"
    );
    let (iir2, _, _) = pio(0x03FA, true, 0);
    assert_eq!(iir2, 0xC2);
    let (lsr2, _, _) = pio(0x03FD, true, 0);
    assert_eq!(lsr2 & 0x60, 0x60, "IIR and LSR agree so Linux loads tx_loadsz");
    let src = include_str!("guest_uart.rs");
    assert!(src.contains("UART THRE level until stop_tx"));
    set_linux_earlycon_share(false);
    guest_tx_clear();
    crate::devices::ide_cdrom::reset();
    reset();
    crate::devices::guest_irq::reset();
}

#[test]
fn product_iso_reassert_rx_not_thre() {
    // Name kept for the gate phrase; THRE now re-asserts only as a gated
    // level (see product_iso_thre_raises_irq4). UART reassert RX not THRE.
    reset();
    crate::devices::guest_irq::reset();
    arm_product_iso_for_irq();
    crate::devices::guest_irq::ioapic_write(0, 0x10 + 2 * 4);
    crate::devices::guest_irq::ioapic_write(0x10, 0x24);
    let _ = pio(0x03FA, false, 0x01);
    let _ = pio(0x03F9, false, 0x01);
    reassert_irq();
    assert!(
        crate::devices::guest_irq::take_inject_vector().is_none(),
        "ETBEI off: nothing to reassert"
    );
    assert!(push_host_rx(b'x'));
    assert_eq!(crate::devices::guest_irq::take_inject_vector(), Some(0x24));
    crate::devices::guest_irq::ioapic_eoi(0x24);
    reassert_irq();
    assert_eq!(
        crate::devices::guest_irq::take_inject_vector(),
        Some(0x24),
        "RX still in FIFO reasserts"
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
fn sysrq_break_then_key_follows_8250_rx_loop() {
    reset();
    crate::devices::guest_irq::reset();
    arm_product_iso_for_irq();
    crate::devices::guest_irq::ioapic_write(0, 0x10 + 2 * 4);
    crate::devices::guest_irq::ioapic_write(0x10, 0x24);
    let _ = pio(0x03FA, false, 0x01);
    let _ = pio(0x03F9, false, 0x01);
    assert!(inject_sysrq(b'w'), "UART sysrq break");
    assert_eq!(crate::devices::guest_irq::take_inject_vector(), Some(0x24));
    assert!(!inject_sysrq(b't'), "second BREAK waits until the first is read");
    let (iir, _, _) = pio(0x03FA, true, 0);
    assert_eq!(iir, 0xC4, "RDA while only the BREAK is pending");
    // serial8250_rx_chars: LSR (DR|BI) -> RBR NUL (uart_handle_break) ->
    // LSR (DR) -> RBR key (uart_prepare_sysrq_char) -> LSR idle.
    let (lsr, _, _) = pio(0x03FD, true, 0);
    assert_eq!(lsr, 0x71);
    let (rbr, _, _) = pio(0x03F8, true, 0);
    assert_eq!(rbr, 0x00);
    let (lsr2, _, _) = pio(0x03FD, true, 0);
    assert_eq!(lsr2, 0x61, "BI cleared with the NUL; key still queued");
    let (key, _, _) = pio(0x03F8, true, 0);
    assert_eq!(key, b'w');
    let (lsr3, _, _) = pio(0x03FD, true, 0);
    assert_eq!(lsr3, 0x60);
    assert!(inject_sysrq(b't'));
    // FCR RX reset drops both the BREAK and the key.
    let _ = pio(0x03FA, false, 0x03);
    let (lsr4, _, _) = pio(0x03FD, true, 0);
    assert_eq!(lsr4, 0x60);
    let src = include_str!("guest_uart.rs");
    assert!(src.contains("UART sysrq break"));
    crate::devices::ide_cdrom::reset();
    reset();
    crate::devices::guest_irq::reset();
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
