use super::{
    pci_id_is_qemu_e1000, rx_desc_packet_len, tx_desc_done, E1000_DEVICE, E1000_VENDOR, FRAME_MAX,
    RX_DESC_DD, RX_DESC_EOP, TX_DESC_DD,
};

#[test]
fn qemu_e1000_pci_id_is_8086_100e_only() {
    assert!(pci_id_is_qemu_e1000(E1000_VENDOR, E1000_DEVICE));
    assert!(!pci_id_is_qemu_e1000(0x8086, 0x100F)); // e1000e / 82545 — not Phase C
    assert!(!pci_id_is_qemu_e1000(0x14e4, 0x165f)); // Broadcom — Phase 0 census
    assert!(!pci_id_is_qemu_e1000(0x8086, 0x1572)); // X710 — not Phase C
}

#[test]
fn rx_desc_parse_requires_dd_eop_no_error() {
    assert_eq!(rx_desc_packet_len(0, 0, 64), None);
    assert_eq!(rx_desc_packet_len(RX_DESC_DD, 0, 64), None);
    assert_eq!(rx_desc_packet_len(RX_DESC_EOP, 0, 64), None);
    assert_eq!(
        rx_desc_packet_len(RX_DESC_DD | RX_DESC_EOP, 1, 64),
        None
    );
    assert_eq!(rx_desc_packet_len(RX_DESC_DD | RX_DESC_EOP, 0, 0), None);
    assert_eq!(
        rx_desc_packet_len(RX_DESC_DD | RX_DESC_EOP, 0, 9000),
        None
    );
    assert_eq!(
        rx_desc_packet_len(RX_DESC_DD | RX_DESC_EOP, 0, 64),
        Some(64)
    );
    assert_eq!(
        rx_desc_packet_len(RX_DESC_DD | RX_DESC_EOP, 0, FRAME_MAX as u16),
        Some(FRAME_MAX)
    );
}

#[test]
fn tx_desc_dd_bit() {
    assert!(!tx_desc_done(0));
    assert!(tx_desc_done(TX_DESC_DD));
}

/// Mocked DMA/parse fuzz: every status/error/length combo is non-panicking.
#[test]
fn fuzz_rx_desc_parse_never_panics() {
    for status in 0u8..=255 {
        for errors in [0u8, 1, 0x02, 0x80, 0xff] {
            for length in [0u16, 1, 14, 60, 64, 1514, 1515, 2048, 65535] {
                let _ = rx_desc_packet_len(status, errors, length);
            }
        }
    }
}

/// Bounded Kani check: good frames stay in (0, FRAME_MAX].
#[cfg(kani)]
#[kani::proof]
fn kani_rx_desc_packet_len_bounds() {
    let status: u8 = kani::any();
    let errors: u8 = kani::any();
    let length: u16 = kani::any();
    if let Some(n) = rx_desc_packet_len(status, errors, length) {
        assert!(n > 0 && n <= FRAME_MAX);
        assert_ne!(status & RX_DESC_DD, 0);
        assert_ne!(status & RX_DESC_EOP, 0);
        assert_eq!(errors, 0);
    }
}
