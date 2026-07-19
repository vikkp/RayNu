use super::*;

#[test]
fn marker_and_magic() {
    assert_eq!(M3_IO_OK_MARKER, "RAYNU-V-M3-IO-OK");
    assert_eq!(M3_EARLY_OK_MARKER, "RAYNU-V-M3-EARLY-OK");
    assert_eq!(M3_SHELL_OK_MARKER, "RAYNU-V-M3-SHELL-OK");
    assert_eq!(M3_LINUX_EARLY_OK_MARKER, "RAYNU-V-M3-LINUX-EARLY-OK");
    assert_eq!(GUEST_IO_MAGIC, b"RAYNU-V-M3-IO");
    assert_eq!(GUEST_EARLY_MAGIC, b"RAYNU-V-M3-EARLY");
    assert_eq!(GUEST_SHELL_MAGIC, b"RAYNU-V-M3-SHELL");
    assert_eq!(LINUX_BANNER_PREFIX, b"Linux version ");
    assert_eq!(SHELL_CPUID_LEAF, 0x524E_550A);
    assert_eq!(SHELL_CPUID_SUBLEAF, 0x5348_454C);
}

#[test]
fn shell_cpuid_latch_sets_guest_shell_ok() {
    // M3.19: CPUID hypercall alone latches SHELL (no IRQ4 COM1 TX).
    note_shell_cpuid();
    assert!(guest_shell_ok());
}

#[test]
fn lcr_dlab_shadow_roundtrip() {
    let out = IoExitInfo {
        port: COM1_LCR,
        size: 1,
        is_in: false,
        string: false,
        rep: false,
    };
    assert!(handle_pio(&out, 0x80).is_ok()); // set DLAB
    let inp = IoExitInfo {
        port: COM1_LCR,
        size: 1,
        is_in: true,
        string: false,
        rep: false,
    };
    let rax = handle_pio(&inp, 0).unwrap().unwrap();
    assert_eq!(rax & 0xFF, 0x80);
}

#[test]
fn parse_out_imm_com1() {
    // size=1 (bits2:0=0), OUT (bit3=0), imm (bit6=1), port=0x3F8
    let qual = (0x3F8u64 << 16) | (1 << 6);
    let info = parse_qualification(qual);
    assert_eq!(info.port, 0x3F8);
    assert_eq!(info.size, 1);
    assert!(!info.is_in);
    assert!(!info.string);
}

#[test]
fn parse_in_dx() {
    let qual = (0x3FDu64 << 16) | (1 << 3);
    let info = parse_qualification(qual);
    assert!(info.is_in);
    assert_eq!(info.port, 0x3FD);
}

#[test]
fn handle_out_advances_magic() {
    // Reset is not exported; drive through handle_pio with full sequence.
    // Fresh process state from prior tests may have progressed — only check API.
    let info = IoExitInfo {
        port: COM1_DATA,
        size: 1,
        is_in: false,
        string: false,
        rep: false,
    };
    assert!(handle_pio(&info, b'X' as u64).is_ok());
}

#[test]
fn string_io_stubbed() {
    let info = IoExitInfo {
        port: COM1_DATA,
        size: 1,
        is_in: false,
        string: true,
        rep: false,
    };
    assert!(handle_pio(&info, 0).is_ok());
}

#[test]
fn port61_refresh_toggles() {
    let inp = IoExitInfo {
        port: 0x61,
        size: 1,
        is_in: true,
        string: false,
        rep: false,
    };
    let a = handle_pio(&inp, 0).unwrap().unwrap() & 0xFF;
    let b = handle_pio(&inp, 0).unwrap().unwrap() & 0xFF;
    assert_ne!(a & 0x10, b & 0x10, "refresh bit must toggle");
}

#[test]
fn com1_range() {
    assert!(is_com1_port(0x3F8));
    assert!(is_com1_port(0x3FF));
    assert!(!is_com1_port(0x2F8));
}

#[test]
fn com1_etbei_raises_tx_irq() {
    // Clear DLAB so port 0x3F9 is IER (not divisor latch).
    let lcr = IoExitInfo {
        port: COM1_LCR,
        size: 1,
        is_in: false,
        string: false,
        rep: false,
    };
    assert!(handle_pio(&lcr, 0x03).is_ok());
    let ier = IoExitInfo {
        port: COM1_IER,
        size: 1,
        is_in: false,
        string: false,
        rep: false,
    };
    assert!(handle_pio(&ier, 0x02).is_ok()); // ETBEI
    assert!(com1_tx_irq_pending());
    let iir = IoExitInfo {
        port: COM1_IIR_FCR,
        size: 1,
        is_in: true,
        string: false,
        rep: false,
    };
    let v = handle_pio(&iir, 0).unwrap().unwrap() & 0xFF;
    assert_eq!(v, 0x02); // THRE
    assert!(!com1_tx_irq_pending());
}
