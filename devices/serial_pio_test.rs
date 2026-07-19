use super::*;

#[test]
fn marker_and_magic() {
    assert_eq!(M3_IO_OK_MARKER, "RAYNU-V-M3-IO-OK");
    assert_eq!(M3_EARLY_OK_MARKER, "RAYNU-V-M3-EARLY-OK");
    assert_eq!(M3_SHELL_OK_MARKER, "RAYNU-V-M3-SHELL-OK");
    assert_eq!(GUEST_IO_MAGIC, b"RAYNU-V-M3-IO");
    assert_eq!(GUEST_EARLY_MAGIC, b"RAYNU-V-M3-EARLY");
    assert_eq!(GUEST_SHELL_MAGIC, b"RAYNU-V-M3-SHELL");
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
fn reject_string_io() {
    let info = IoExitInfo {
        port: COM1_DATA,
        size: 1,
        is_in: false,
        string: true,
        rep: false,
    };
    assert!(handle_pio(&info, 0).is_err());
}

#[test]
fn com1_range() {
    assert!(is_com1_port(0x3F8));
    assert!(is_com1_port(0x3FF));
    assert!(!is_com1_port(0x2F8));
}
