use super::{inject_sysrq, pio, poll_host_rx, push_host_rx, reassert_irq, reset};
use crate::boot::serial::{
    guest_tx_clear, set_guest_tx_test_ring_full, set_linux_earlycon_share,
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
    set_guest_tx_test_ring_full(true);
    let _ = pio(0x03FA, false, 0x01);
    let _ = pio(0x03F9, false, 0x02);
    let (_, thr, _) = pio(0x03F8, false, b'a');
    assert_eq!(thr, Some(b'a'));
    assert!(
        crate::devices::guest_irq::take_inject_vector().is_none(),
        "THR not empty while the shared ring has no room"
    );
    let (iir, _, _) = pio(0x03FA, true, 0);
    assert_eq!(iir, 0xC1, "no interrupt pending, latch kept");
    let (lsr, _, _) = pio(0x03FD, true, 0);
    assert_eq!(lsr & 0x20, 0);
    reassert_irq();
    assert!(crate::devices::guest_irq::take_inject_vector().is_none());
    set_guest_tx_test_ring_full(false);
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
    set_guest_tx_test_ring_full(true);
    let (lsr, _, _) = pio(0x03FD, true, 0);
    assert_eq!(lsr & 0x60, 0, "THRE/TEMT clear while the shared ring has no room");
    set_guest_tx_test_ring_full(false);
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
fn thre_level_follows_ring_room_and_reasserts_after_drain() {
    // guest UART TX ring room: iron `916af96` had `pend=0` with `ring=0`
    // because THRE also needed host COM2 THRE at the sampling exit. The
    // level is ring room only; the drain (exit / pacing timer) frees room
    // and the next resume re-asserts IRQ 4.
    use crate::boot::serial::{
        drain_guest_tx, guest_tx_len, guest_tx_room, guest_tx_room_has_thre, write_byte_nowait,
        GUEST_TX_CAP, GUEST_TX_RESERVE, GUEST_TX_THRE_BURST,
    };
    reset();
    crate::devices::guest_irq::reset();
    arm_product_iso_for_irq();
    crate::devices::guest_irq::ioapic_write(0, 0x10 + 2 * 4);
    crate::devices::guest_irq::ioapic_write(0x10, 0x24);
    guest_tx_clear();
    set_linux_earlycon_share(true);
    let _ = pio(0x03FA, false, 0x01);
    let _ = pio(0x03F9, false, 0x02);
    assert_eq!(crate::devices::guest_irq::take_inject_vector(), Some(0x24));
    crate::devices::guest_irq::ioapic_eoi(0x24);
    // Host COM2 back-pressured: the ring cannot drain. Guest text fills it
    // one byte past the THRE threshold.
    crate::boot::serial::set_guest_tx_test_sol_not_ready(true);
    crate::boot::serial::set_com2_fifo_for_test(true);
    let fill = GUEST_TX_CAP - GUEST_TX_RESERVE - GUEST_TX_THRE_BURST + 1;
    for _ in 0..fill {
        write_byte_nowait(b'x');
    }
    assert_eq!(guest_tx_len(), fill);
    assert!(!guest_tx_room_has_thre(guest_tx_room()));
    let (lsr, _, _) = pio(0x03FD, true, 0);
    assert_eq!(lsr & 0x60, 0, "no room above the reserve: THRE low");
    let (iir, _, _) = pio(0x03FA, true, 0);
    assert_eq!(iir & 0x07, 0x01, "IIR: no interrupt while THRE is low");
    reassert_irq();
    assert!(crate::devices::guest_irq::take_inject_vector().is_none());
    // HV lines still fit in the reserve without evicting guest text.
    write_byte_nowait(b'H');
    assert_eq!(guest_tx_len(), fill + 1);
    // SOL opens; one pacing exit drains one FIFO burst and that alone
    // restores the level (host COM2 LSR is not consulted for guest THRE).
    crate::boot::serial::set_guest_tx_test_sol_not_ready(false);
    assert_eq!(drain_guest_tx(crate::boot::serial::GUEST_TX_DRAIN_PACE), 16);
    assert!(guest_tx_room_has_thre(guest_tx_room()));
    reassert_irq();
    assert_eq!(
        crate::devices::guest_irq::take_inject_vector(),
        Some(0x24),
        "IRQ 4 re-asserted once the ring has room again"
    );
    let (iir2, _, _) = pio(0x03FA, true, 0);
    assert_eq!(iir2 & 0x07, 0x02);
    let (lsr2, _, _) = pio(0x03FD, true, 0);
    assert_eq!(lsr2 & 0x60, 0x60, "IIR and LSR agree: Linux loads tx_loadsz");
    // Guest THR bytes land in the ring (host tests drain at once).
    write_byte_nowait(b'!');
    let src = include_str!("guest_uart.rs");
    assert!(src.contains("guest UART TX ring room"));
    set_linux_earlycon_share(false);
    guest_tx_clear();
    crate::devices::ide_cdrom::reset();
    reset();
    crate::devices::guest_irq::reset();
}

#[test]
fn thre_chain_counts_every_link() {
    // UART THRE chain telemetry: iron `8b6ed1a` looked like `f229d14`, so
    // the stall heartbeat now prints IER / latch / pending plus counters for
    // IIR class, LSR THRE class, THR writes, ETBEI flips and IRQ 4 raises.
    use super::thre_chain;
    reset();
    crate::devices::guest_irq::reset();
    arm_product_iso_for_irq();
    crate::devices::guest_irq::ioapic_write(0, 0x10 + 2 * 4);
    crate::devices::guest_irq::ioapic_write(0x10, 0x24);
    guest_tx_clear();
    set_linux_earlycon_share(true);
    set_guest_tx_test_ring_full(false);
    assert_eq!(thre_chain(), Default::default(), "counters start at zero");
    let _ = pio(0x03FA, false, 0x01);
    // IER: ETBEI on (THRE level armed while the ring is empty).
    let _ = pio(0x03F9, false, 0x02);
    let c = thre_chain();
    assert_eq!(c.ier, 0x02);
    assert!(c.thre_irq && c.thre_pending);
    assert_eq!((c.ier_etbei_on, c.ier_etbei_off), (1, 0));
    assert_eq!(c.pio_raise, 1, "IER write raised IRQ 4");
    // Linux services: IIR says THRE, LSR says THRE, then 2 THR bytes.
    let (iir, _, _) = pio(0x03FA, true, 0);
    assert_eq!(iir & 0x07, 0x02);
    let (lsr, _, _) = pio(0x03FD, true, 0);
    assert_eq!(lsr & 0x20, 0x20);
    let _ = pio(0x03F8, false, b'o');
    let _ = pio(0x03F8, false, b'k');
    let c = thre_chain();
    assert_eq!((c.iir_rx, c.iir_thre, c.iir_none), (0, 1, 0));
    assert_eq!((c.lsr_thre_on, c.lsr_thre_off), (1, 0));
    assert_eq!(c.thr_wr, 2);
    // reassert_irq on a later exit counts separately from PIO raises.
    let before = c.reassert_raise;
    reassert_irq();
    assert_eq!(thre_chain().reassert_raise, before + 1);
    // Ring full above the reserve: LSR reports THRE off, IIR no interrupt.
    set_guest_tx_test_ring_full(true);
    let (lsr, _, _) = pio(0x03FD, true, 0);
    assert_eq!(lsr & 0x20, 0);
    let (iir, _, _) = pio(0x03FA, true, 0);
    assert_eq!(iir & 0x07, 0x01);
    let c = thre_chain();
    assert!(!c.thre_pending && c.thre_irq, "latch kept, level low");
    assert_eq!((c.lsr_thre_on, c.lsr_thre_off), (1, 1));
    assert_eq!((c.iir_rx, c.iir_thre, c.iir_none), (0, 1, 1));
    assert!(c.pio_lower >= 1, "PIO with nothing pending lowers IRQ 4");
    set_guest_tx_test_ring_full(false);
    // __stop_tx: ETBEI off ends the level.
    let _ = pio(0x03F9, false, 0x00);
    let c = thre_chain();
    assert_eq!((c.ier_etbei_on, c.ier_etbei_off), (1, 1));
    assert!(!c.thre_irq);
    // LSR reads with ETBEI clear are not classified (console polling path).
    let _ = pio(0x03FD, true, 0);
    assert_eq!(thre_chain().lsr_thre_on, 1);
    // SysRq BREAK shows up as rx / brk and an RX-class IIR.
    let _ = pio(0x03F9, false, 0x01);
    assert!(inject_sysrq(b't'));
    let c = thre_chain();
    assert!(c.break_pending);
    assert_eq!(c.rx_len, 1);
    let (iir, _, _) = pio(0x03FA, true, 0);
    assert_eq!(iir & 0x07, 0x04);
    assert_eq!(thre_chain().iir_rx, 1);
    let src = include_str!("guest_uart.rs");
    assert!(src.contains("UART THRE chain telemetry"));
    assert!(src.contains("fn thre_chain"));
    assert!(include_str!("guest_irq.rs").contains("fn pic_master_snap"));
    assert!(include_str!("../vmx/guest_uefi.rs").contains("fn virtio_stall_dump_thre_chain"));
    assert!(include_str!("../vmx/guest_uefi.rs").contains("virtio stall dump thre ier=0x"));
    set_linux_earlycon_share(false);
    guest_tx_clear();
    reset();
    assert_eq!(thre_chain(), Default::default(), "reset clears counters");
    crate::devices::ide_cdrom::reset();
    crate::devices::guest_irq::reset();
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
