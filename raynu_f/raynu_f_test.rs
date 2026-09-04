//! RayNu-F gates (ADR-016). Host-only.
//!
//! * `raynu_f_scaffold_is_honest` — the subsystem is declared, outside the
//!   Proven Core, and does **not** yet run or boot an ISO.
//! * `raynu_f_tables_and_console_dispatch` — Stage 1: byte-exact UEFI tables
//!   with valid CRCs, trampolines that round-trip, and `ConOut.OutputString`
//!   landing on the console sink. Prints `RAYNU-V-RAYNU-F-TABLES-OK`.
//!
//! Neither prints the iron `ISO-INSTALL-OK`, and neither prints
//! `RAYNU-V-RAYNU-F-CONOUT-OK` (that requires a live guest VM-exit).

use super::services::{
    decode_trampoline, dispatch, encode_trampoline, is_service_call, output_string,
    trampoline_slot_gpa, ConsoleSink, GuestMem, ServiceArgs, ServiceId, EFI_INVALID_PARAMETER,
    EFI_NOT_READY, EFI_SUCCESS, EFI_UNSUPPORTED, OUTPUT_STRING_CAP_CHARS,
    RAYNU_F_SERVICE_PORT, TRAMPOLINE_SLOT_BYTES, TRAMPOLINE_SLOT_COUNT,
};
use super::tables::{
    build_firmware_image, crc32, get_u64, header_crc_valid, BuildError,
    BOOT_SERVICES_FN_COUNT, BOOT_SERVICES_SIZE, EFI_2_10_SYSTEM_TABLE_REVISION,
    EFI_BOOT_SERVICES_SIGNATURE, EFI_RUNTIME_SERVICES_SIGNATURE, EFI_SYSTEM_TABLE_SIGNATURE,
    EFI_TABLE_HEADER_SIZE, HANDLE_CONOUT, IMAGE_BOOT_SERVICES_OFF, IMAGE_BYTES,
    IMAGE_CONOUT_MODE_OFF, IMAGE_CONOUT_OFF, IMAGE_RUNTIME_SERVICES_OFF,
    IMAGE_SYSTEM_TABLE_OFF, IMAGE_VENDOR_OFF, RUNTIME_SERVICES_FN_COUNT, RUNTIME_SERVICES_SIZE,
    SIMPLE_TEXT_OUTPUT_MODE_PTR_OFF, SIMPLE_TEXT_OUTPUT_MODE_SIZE, SIMPLE_TEXT_OUTPUT_SIZE,
    SYSTEM_TABLE_BOOT_SERVICES_OFF, SYSTEM_TABLE_CONOUT_HANDLE_OFF, SYSTEM_TABLE_CONOUT_OFF,
    SYSTEM_TABLE_FIRMWARE_VENDOR_OFF, SYSTEM_TABLE_RUNTIME_SERVICES_OFF, SYSTEM_TABLE_SIZE,
};
use super::{
    raynu_f_boots_iso, raynu_f_is_functional, raynu_f_mutates_foreign_firmware_state,
    raynu_f_planned_protocol_count, raynu_f_protocol_has_dispatch, GuestFwProtocol,
    RAYNU_F_CONOUT_OK_MARKER, RAYNU_F_PLANNED_PROTOCOLS, RAYNU_F_RESIDUAL_NOTE,
    RAYNU_F_SCAFFOLD_OK_MARKER, RAYNU_F_TABLES_OK_MARKER,
};

/// Flat guest memory for tests: a `Vec` based at `base`.
struct MockGuest {
    base: u64,
    mem: Vec<u8>,
}

impl GuestMem for MockGuest {
    fn read(&self, addr: u64, buf: &mut [u8]) -> usize {
        if addr < self.base {
            return 0;
        }
        let off = (addr - self.base) as usize;
        if off >= self.mem.len() {
            return 0;
        }
        let n = buf.len().min(self.mem.len() - off);
        buf[..n].copy_from_slice(&self.mem[off..off + n]);
        n
    }
}

#[derive(Default)]
struct CaptureSink(Vec<u8>);

impl ConsoleSink for CaptureSink {
    fn write_byte(&mut self, b: u8) {
        self.0.push(b);
    }
}

fn utf16z(s: &str) -> Vec<u8> {
    let mut v = Vec::new();
    for ch in s.encode_utf16() {
        v.extend_from_slice(&ch.to_le_bytes());
    }
    v.extend_from_slice(&[0, 0]);
    v
}

#[test]
fn raynu_f_scaffold_is_honest() {
    assert_eq!(RAYNU_F_SCAFFOLD_OK_MARKER, "RAYNU-V-RAYNU-F-SCAFFOLD-OK");
    assert_ne!(RAYNU_F_SCAFFOLD_OK_MARKER, "RAYNU-V-M7-ISO-INSTALL-OK");
    assert_ne!(RAYNU_F_TABLES_OK_MARKER, RAYNU_F_CONOUT_OK_MARKER);

    assert!(!raynu_f_is_functional());
    assert!(!raynu_f_boots_iso());
    assert!(!raynu_f_mutates_foreign_firmware_state());

    assert_eq!(raynu_f_planned_protocol_count(), 7);
    assert_eq!(RAYNU_F_PLANNED_PROTOCOLS[0], GuestFwProtocol::SystemTable);
    assert_eq!(RAYNU_F_PLANNED_PROTOCOLS[6], GuestFwProtocol::LoadStartImage);
    assert!(raynu_f_protocol_has_dispatch(GuestFwProtocol::SystemTable));
    assert!(raynu_f_protocol_has_dispatch(GuestFwProtocol::ConsoleSerial));
    assert!(!raynu_f_protocol_has_dispatch(GuestFwProtocol::BlockIo));
    assert!(!raynu_f_protocol_has_dispatch(GuestFwProtocol::LoadStartImage));

    assert!(RAYNU_F_RESIDUAL_NOTE.contains("ADR-016"));
    assert!(RAYNU_F_RESIDUAL_NOTE.contains("No third-party firmware state mutation"));
    assert!(RAYNU_F_RESIDUAL_NOTE.contains("no PE loader yet"));
    assert!(RAYNU_F_RESIDUAL_NOTE.contains("no launch yet"));
    assert!(RAYNU_F_RESIDUAL_NOTE.contains("not ISO-INSTALL-OK"));
    assert!(RAYNU_F_RESIDUAL_NOTE.contains("2b795a0"));

    #[cfg(not(target_os = "uefi"))]
    println!("{RAYNU_F_SCAFFOLD_OK_MARKER}");
}

#[test]
fn raynu_f_tables_and_console_dispatch() {
    // --- spec constants ---------------------------------------------------
    assert_eq!(EFI_TABLE_HEADER_SIZE, 24);
    assert_eq!(SYSTEM_TABLE_SIZE, 120);
    assert_eq!(SYSTEM_TABLE_FIRMWARE_VENDOR_OFF, 24);
    assert_eq!(SYSTEM_TABLE_CONOUT_OFF, 64);
    assert_eq!(SYSTEM_TABLE_RUNTIME_SERVICES_OFF, 88);
    assert_eq!(SYSTEM_TABLE_BOOT_SERVICES_OFF, 96);
    assert_eq!(BOOT_SERVICES_FN_COUNT, 44);
    assert_eq!(BOOT_SERVICES_SIZE, 24 + 44 * 8);
    assert_eq!(RUNTIME_SERVICES_FN_COUNT, 14);
    assert_eq!(RUNTIME_SERVICES_SIZE, 24 + 14 * 8);
    assert_eq!(SIMPLE_TEXT_OUTPUT_MODE_PTR_OFF, 72);
    assert_eq!(SIMPLE_TEXT_OUTPUT_SIZE, 80);
    assert_eq!(SIMPLE_TEXT_OUTPUT_MODE_SIZE, 24);
    assert_eq!(EFI_2_10_SYSTEM_TABLE_REVISION, 0x0002_0064);
    assert_eq!(&EFI_SYSTEM_TABLE_SIGNATURE.to_le_bytes(), b"IBI SYST");
    assert_eq!(&EFI_BOOT_SERVICES_SIGNATURE.to_le_bytes(), b"BOOTSERV");
    assert_eq!(&EFI_RUNTIME_SERVICES_SIGNATURE.to_le_bytes(), b"RUNTSERV");

    // CRC32 known answer (IEEE, same as zlib / UEFI CalculateCrc32).
    assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    assert_eq!(crc32(b""), 0);

    // --- build ----------------------------------------------------------
    let base = 0x0080_0000u64;
    let mut img = vec![0u8; IMAGE_BYTES];
    assert_eq!(
        build_firmware_image(base + 1, &mut img),
        Err(BuildError::BaseUnaligned)
    );
    assert_eq!(
        build_firmware_image(base, &mut img[..0x100]),
        Err(BuildError::BufferTooSmall)
    );
    let layout = build_firmware_image(base, &mut img).expect("image builds");
    assert_eq!(layout.system_table, base + IMAGE_SYSTEM_TABLE_OFF as u64);
    assert_eq!(layout.boot_services, base + IMAGE_BOOT_SERVICES_OFF as u64);
    assert_eq!(layout.runtime_services, base + IMAGE_RUNTIME_SERVICES_OFF as u64);
    assert_eq!(layout.con_out, base + IMAGE_CONOUT_OFF as u64);
    assert_eq!(layout.con_out_mode, base + IMAGE_CONOUT_MODE_OFF as u64);
    assert_eq!(layout.vendor, base + IMAGE_VENDOR_OFF as u64);

    // Headers: signatures, revision, sizes, valid CRC32.
    let st = IMAGE_SYSTEM_TABLE_OFF;
    assert_eq!(get_u64(&img, st), EFI_SYSTEM_TABLE_SIGNATURE);
    assert!(header_crc_valid(&img, st));
    assert!(header_crc_valid(&img, IMAGE_BOOT_SERVICES_OFF));
    assert!(header_crc_valid(&img, IMAGE_RUNTIME_SERVICES_OFF));
    assert_eq!(get_u64(&img, IMAGE_BOOT_SERVICES_OFF), EFI_BOOT_SERVICES_SIGNATURE);
    assert_eq!(
        get_u64(&img, IMAGE_RUNTIME_SERVICES_OFF),
        EFI_RUNTIME_SERVICES_SIGNATURE
    );
    // Corrupt one byte → CRC must fail.
    let mut bad = img.clone();
    bad[st + SYSTEM_TABLE_CONOUT_OFF] ^= 0x01;
    assert!(!header_crc_valid(&bad, st));

    // System table pointers.
    assert_eq!(get_u64(&img, st + SYSTEM_TABLE_CONOUT_OFF), layout.con_out);
    assert_eq!(get_u64(&img, st + SYSTEM_TABLE_CONOUT_HANDLE_OFF), HANDLE_CONOUT);
    assert_eq!(
        get_u64(&img, st + SYSTEM_TABLE_BOOT_SERVICES_OFF),
        layout.boot_services
    );
    assert_eq!(
        get_u64(&img, st + SYSTEM_TABLE_RUNTIME_SERVICES_OFF),
        layout.runtime_services
    );
    assert_eq!(get_u64(&img, st + SYSTEM_TABLE_FIRMWARE_VENDOR_OFF), layout.vendor);
    // Vendor string is L"RayNu-F".
    let vendor: Vec<u8> = img[IMAGE_VENDOR_OFF..IMAGE_VENDOR_OFF + 16].to_vec();
    assert_eq!(vendor, utf16z("RayNu-F"));

    // ConOut protocol: OutputString slot (index 1) → its trampoline; Mode ptr.
    let co = IMAGE_CONOUT_OFF;
    let out_fn = get_u64(&img, co + 8);
    assert_eq!(
        out_fn,
        trampoline_slot_gpa(layout.trampolines, ServiceId::ConOutOutputString)
    );
    assert_eq!(
        get_u64(&img, co + SIMPLE_TEXT_OUTPUT_MODE_PTR_OFF),
        layout.con_out_mode
    );
    // Mode: MaxMode=1, Mode=0, Attribute=0x07, CursorVisible=1.
    assert_eq!(img[IMAGE_CONOUT_MODE_OFF], 1);
    assert_eq!(img[IMAGE_CONOUT_MODE_OFF + 8], 0x07);
    assert_eq!(img[IMAGE_CONOUT_MODE_OFF + 20], 1);

    // Boot services slot 0 (RaiseTPL) and 43 (CreateEventEx) → trampolines.
    let bs = IMAGE_BOOT_SERVICES_OFF + EFI_TABLE_HEADER_SIZE;
    assert_eq!(
        get_u64(&img, bs),
        trampoline_slot_gpa(layout.trampolines, ServiceId::boot_service(0))
    );
    assert_eq!(
        get_u64(&img, bs + 43 * 8),
        trampoline_slot_gpa(layout.trampolines, ServiceId::boot_service(43))
    );

    // --- trampolines ----------------------------------------------------
    // The OutputString fn pointer lands on a stub that decodes back to itself.
    let slot_off = (out_fn - base) as usize;
    let slot = &img[slot_off..slot_off + TRAMPOLINE_SLOT_BYTES];
    assert_eq!(decode_trampoline(slot), Some(ServiceId::ConOutOutputString));
    assert_eq!(&slot[..4], &[0x49, 0x89, 0xD2, 0xB8]);
    assert_eq!(&slot[8..12], &[0x66, 0xBA, 0x46, 0x52]);
    assert_eq!(slot[12], 0xEF);
    assert_eq!(slot[13], 0xC3);
    assert_eq!(RAYNU_F_SERVICE_PORT, 0x5246);
    assert_eq!(RAYNU_F_SERVICE_PORT.to_le_bytes(), [0x46, 0x52]);
    // All laid-out slots round-trip.
    for s in 0..TRAMPOLINE_SLOT_COUNT {
        let id = ServiceId::from_slot(s).expect("slot has id");
        assert_eq!(id.slot_index(), Some(s));
        assert_eq!(decode_trampoline(&encode_trampoline(id)), Some(id));
        assert!(id.is_known());
    }
    assert!(!ServiceId(0xDEAD).is_known());
    assert_eq!(decode_trampoline(&[0u8; 16]), None);
    // I/O exit classification: out dx,eax to 0x5246 only.
    assert!(is_service_call(RAYNU_F_SERVICE_PORT, false, 4));
    assert!(!is_service_call(RAYNU_F_SERVICE_PORT, true, 4));
    assert!(!is_service_call(RAYNU_F_SERVICE_PORT, false, 1));
    assert!(!is_service_call(0x3f8, false, 4));

    // --- dispatcher -----------------------------------------------------
    let text = "RN-F hello\r\n";
    let str_gpa = base + 0x1800;
    let mut mem = img.clone();
    let bytes = utf16z(text);
    mem[0x1800..0x1800 + bytes.len()].copy_from_slice(&bytes);
    let guest = MockGuest { base, mem };
    let mut sink = CaptureSink::default();

    let d = dispatch(
        ServiceId::ConOutOutputString,
        ServiceArgs {
            a1: layout.con_out,
            a2: str_gpa,
            a3: 0,
            a4: 0,
        },
        &guest,
        &mut sink,
    );
    assert_eq!(d.status, EFI_SUCCESS);
    assert_eq!(d.chars_out, text.encode_utf16().count());
    assert_eq!(sink.0, text.as_bytes());

    // Non-ASCII code unit becomes '?'.
    let mut sink2 = CaptureSink::default();
    let mut mem2 = img.clone();
    let b2 = utf16z("a\u{2603}b");
    mem2[0x1800..0x1800 + b2.len()].copy_from_slice(&b2);
    let guest2 = MockGuest { base, mem: mem2 };
    assert_eq!(output_string(&guest2, &mut sink2, str_gpa), 3);
    assert_eq!(sink2.0, b"a?b");

    // NULL string → EFI_INVALID_PARAMETER; TestString accepts without output.
    let mut sink3 = CaptureSink::default();
    let d = dispatch(
        ServiceId::ConOutOutputString,
        ServiceArgs { a1: layout.con_out, a2: 0, a3: 0, a4: 0 },
        &guest,
        &mut sink3,
    );
    assert_eq!(d.status, EFI_INVALID_PARAMETER);
    let d = dispatch(
        ServiceId::ConOutTestString,
        ServiceArgs { a1: layout.con_out, a2: str_gpa, a3: 0, a4: 0 },
        &guest,
        &mut sink3,
    );
    assert_eq!(d.status, EFI_SUCCESS);
    assert!(sink3.0.is_empty());

    // Cap: an unterminated string stops at OUTPUT_STRING_CAP_CHARS.
    let mut sink4 = CaptureSink::default();
    let mut mem4 = vec![0x41u8, 0x00];
    mem4 = mem4.repeat(OUTPUT_STRING_CAP_CHARS + 64);
    let guest4 = MockGuest { base, mem: mem4 };
    assert_eq!(output_string(&guest4, &mut sink4, base), OUTPUT_STRING_CAP_CHARS);

    // Honest unsupported / not-ready paths.
    let args = ServiceArgs { a1: 0, a2: 0, a3: 0, a4: 0 };
    assert_eq!(
        dispatch(ServiceId::boot_service(0), args, &guest, &mut sink3).status,
        EFI_UNSUPPORTED
    );
    assert_eq!(
        dispatch(ServiceId::runtime_service(0), args, &guest, &mut sink3).status,
        EFI_UNSUPPORTED
    );
    assert_eq!(
        dispatch(ServiceId::ConInReadKeyStroke, args, &guest, &mut sink3).status,
        EFI_NOT_READY
    );
    assert_eq!(
        dispatch(ServiceId::ConOutReset, args, &guest, &mut sink3).status,
        EFI_SUCCESS
    );
    assert_eq!(
        dispatch(ServiceId::ConOutSetMode, ServiceArgs { a2: 1, ..args }, &guest, &mut sink3)
            .status,
        EFI_UNSUPPORTED
    );
    assert_eq!(ServiceId::ConOutOutputString.name(), "ConOut.OutputString");
    assert_eq!(ServiceId::boot_service(3).name(), "BootServices");

    #[cfg(not(target_os = "uefi"))]
    println!("{RAYNU_F_TABLES_OK_MARKER}");
}
