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

use super::services::FirmwareState;
use super::events::TimeSource;
use std::cell::{Cell, RefCell};

/// Flat guest memory for tests: a `Vec` based at `base`.
struct MockGuest {
    base: u64,
    mem: RefCell<Vec<u8>>,
}

impl MockGuest {
    fn new(base: u64, mem: Vec<u8>) -> Self {
        MockGuest {
            base,
            mem: RefCell::new(mem),
        }
    }
    fn u64_at(&self, addr: u64) -> u64 {
        let mut b = [0u8; 8];
        assert_eq!(self.read(addr, &mut b), 8);
        u64::from_le_bytes(b)
    }
    fn put_u64(&self, addr: u64, v: u64) {
        assert_eq!(self.write(addr, &v.to_le_bytes()), 8);
    }
}

impl GuestMem for MockGuest {
    fn read(&self, addr: u64, buf: &mut [u8]) -> usize {
        if addr < self.base {
            return 0;
        }
        let mem = self.mem.borrow();
        let off = (addr - self.base) as usize;
        if off >= mem.len() {
            return 0;
        }
        let n = buf.len().min(mem.len() - off);
        buf[..n].copy_from_slice(&mem[off..off + n]);
        n
    }
    fn write(&self, addr: u64, buf: &[u8]) -> usize {
        if addr < self.base {
            return 0;
        }
        let mut mem = self.mem.borrow_mut();
        let off = (addr - self.base) as usize;
        if off >= mem.len() {
            return 0;
        }
        let n = buf.len().min(mem.len() - off);
        mem[off..off + n].copy_from_slice(&buf[..n]);
        n
    }
}

#[derive(Default)]
struct CaptureSink(Vec<u8>, Vec<u8>);

impl ConsoleSink for CaptureSink {
    fn write_byte(&mut self, b: u8) {
        self.0.push(b);
    }
    fn has_input(&self) -> bool {
        !self.1.is_empty()
    }
    fn read_input(&mut self) -> Option<u8> {
        if self.1.is_empty() {
            None
        } else {
            Some(self.1.remove(0))
        }
    }
}

/// Manual firmware clock that advances `step` per read, so blocking waits
/// terminate deterministically in tests.
struct ManualClock {
    now: Cell<u64>,
    step: u64,
}

impl TimeSource for ManualClock {
    fn now_100ns(&self) -> u64 {
        let v = self.now.get();
        self.now.set(v + self.step);
        v
    }
}

const SLAB: u64 = 32 * 1024 * 1024;

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
    assert!(raynu_f_protocol_has_dispatch(GuestFwProtocol::MemoryServices));
    assert!(raynu_f_protocol_has_dispatch(GuestFwProtocol::TimerTick));
    assert!(!raynu_f_protocol_has_dispatch(GuestFwProtocol::BlockIo));
    assert!(!raynu_f_protocol_has_dispatch(GuestFwProtocol::LoadStartImage));

    assert!(RAYNU_F_RESIDUAL_NOTE.contains("ADR-016"));
    assert!(RAYNU_F_RESIDUAL_NOTE.contains("No third-party firmware state mutation"));
    assert!(RAYNU_F_RESIDUAL_NOTE.contains("F2b closed on nested VT-x"));
    assert!(RAYNU_F_RESIDUAL_NOTE.contains("F3 host-proven"));
    assert!(RAYNU_F_RESIDUAL_NOTE.contains("owned host-side firmware clock"));
    assert!(RAYNU_F_RESIDUAL_NOTE.contains("handles/protocols/LoadImage/StartImage and runtime services are EFI_UNSUPPORTED"));
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
    let mut st = FirmwareState::new();
    let clk = ManualClock {
        now: Cell::new(1_000_000),
        step: 100_000,
    };
    let text = "RN-F hello\r\n";
    let str_gpa = base + 0x1800;
    let mut mem = img.clone();
    let bytes = utf16z(text);
    mem[0x1800..0x1800 + bytes.len()].copy_from_slice(&bytes);
    let guest = MockGuest::new(base, mem);
    let mut sink = CaptureSink::default();

    let d = dispatch(
        ServiceId::ConOutOutputString,
        ServiceArgs::regs(layout.con_out, str_gpa, 0, 0),
        &guest,
        &mut sink,
        &mut st,
        &clk,
        SLAB,
    );
    assert_eq!(d.status, EFI_SUCCESS);
    assert_eq!(d.chars_out, text.encode_utf16().count());
    assert_eq!(sink.0, text.as_bytes());

    // Non-ASCII code unit becomes '?'.
    let mut sink2 = CaptureSink::default();
    let mut mem2 = img.clone();
    let b2 = utf16z("a\u{2603}b");
    mem2[0x1800..0x1800 + b2.len()].copy_from_slice(&b2);
    let guest2 = MockGuest::new(base, mem2);
    assert_eq!(output_string(&guest2, &mut sink2, str_gpa), 3);
    assert_eq!(sink2.0, b"a?b");

    // NULL string -> EFI_INVALID_PARAMETER; TestString accepts without output.
    let mut sink3 = CaptureSink::default();
    let d = dispatch(
        ServiceId::ConOutOutputString,
        ServiceArgs::regs(layout.con_out, 0, 0, 0),
        &guest,
        &mut sink3,
        &mut st,
        &clk,
        SLAB,
    );
    assert_eq!(d.status, EFI_INVALID_PARAMETER);
    let d = dispatch(
        ServiceId::ConOutTestString,
        ServiceArgs::regs(layout.con_out, str_gpa, 0, 0),
        &guest,
        &mut sink3,
        &mut st,
        &clk,
        SLAB,
    );
    assert_eq!(d.status, EFI_SUCCESS);
    assert!(sink3.0.is_empty());

    // Cap: an unterminated string stops at OUTPUT_STRING_CAP_CHARS.
    let mut sink4 = CaptureSink::default();
    let mut mem4 = vec![0x41u8, 0x00];
    mem4 = mem4.repeat(OUTPUT_STRING_CAP_CHARS + 64);
    let guest4 = MockGuest::new(base, mem4);
    assert_eq!(output_string(&guest4, &mut sink4, base), OUTPUT_STRING_CAP_CHARS);

    // Honest unsupported / not-ready paths.
    let args = ServiceArgs::regs(0, 0, 0, 0);
    // Protocol/handle services are F4; runtime services are later.
    assert_eq!(
        dispatch(ServiceId::HandleProtocol, args, &guest, &mut sink3, &mut st, &clk, SLAB).status,
        EFI_UNSUPPORTED
    );
    assert_eq!(
        dispatch(ServiceId::runtime_service(0), args, &guest, &mut sink3, &mut st, &clk, SLAB).status,
        EFI_UNSUPPORTED
    );
    assert_eq!(
        dispatch(ServiceId::ConInReadKeyStroke, ServiceArgs::regs(0, base + 0x1900, 0, 0), &guest, &mut sink3, &mut st, &clk, SLAB).status,
        EFI_NOT_READY
    );
    assert_eq!(
        dispatch(ServiceId::ConOutReset, args, &guest, &mut sink3, &mut st, &clk, SLAB).status,
        EFI_SUCCESS
    );
    assert_eq!(
        dispatch(ServiceId::ConOutSetMode, ServiceArgs::regs(0, 1, 0, 0), &guest, &mut sink3, &mut st, &clk, SLAB)
            .status,
        EFI_UNSUPPORTED
    );
    assert_eq!(ServiceId::ConOutOutputString.name(), "ConOut.OutputString");
    assert_eq!(ServiceId::boot_service(17).name(), "BootServices");
    assert_eq!(ServiceId::boot_service(3).name(), "FreePages");
    assert_eq!(ServiceId::WaitForEvent.name(), "WaitForEvent");

    #[cfg(not(target_os = "uefi"))]
    println!("{RAYNU_F_TABLES_OK_MARKER}");
}

#[test]
fn raynu_f_loader_testapp_and_plan() {
    use super::launch_plan::{
        plan_f2, PlanError, F2_APP_LOAD_BASE, F2_CR0, F2_CR4, F2_EFER, F2_IDENTITY_PML4,
        F2_IMAGE_HANDLE, F2_STACK_TOP, F2_TABLES_BASE,
    };
    use super::pe::{load_pe32plus, parse_pe32plus, section, PeError, SUBSYSTEM_EFI_APPLICATION};
    use super::tables::{get_u64, IMAGE_SYSTEM_TABLE_OFF, SYSTEM_TABLE_CONOUT_OFF};
    use super::testapp::{
        build_test_app, TestAppError, TESTAPP_CODE, TESTAPP_ENTRY_RVA, TESTAPP_FAIL_MSG_RVA,
        TESTAPP_FAIL_PTR_RVA, TESTAPP_FILE_BYTES, TESTAPP_HLT_FAIL_OFF, TESTAPP_HLT_OK_OFF,
        TESTAPP_IMAGE_BASE, TESTAPP_MESSAGE, TESTAPP_MSG_PTR_RVA, TESTAPP_MSG_RVA,
        TESTAPP_SIZE_OF_IMAGE,
    };
    use super::RAYNU_F_LOADER_OK_MARKER;

    // --- test app emits a genuine PE32+ ----------------------------------
    let mut file = vec![0u8; TESTAPP_FILE_BYTES];
    assert_eq!(build_test_app(&mut file[..16]), Err(TestAppError::BufferTooSmall));
    assert_eq!(build_test_app(&mut file), Ok(TESTAPP_FILE_BYTES));
    assert_eq!(&file[0..2], b"MZ");
    assert_eq!(&file[0x80..0x84], b"PE\0\0");

    let pe = parse_pe32plus(&file).expect("parses");
    assert_eq!(pe.machine, 0x8664);
    assert_eq!(pe.num_sections, 2);
    assert_eq!(pe.entry_rva, TESTAPP_ENTRY_RVA);
    assert_eq!(pe.image_base, TESTAPP_IMAGE_BASE);
    assert_eq!(pe.size_of_image, TESTAPP_SIZE_OF_IMAGE);
    assert_eq!(pe.subsystem, SUBSYSTEM_EFI_APPLICATION);
    assert_eq!(pe.reloc_dir, (0x2000, 12));
    let text = section(&file, &pe, 0).unwrap();
    assert_eq!(&text.name[..5], b".text");
    assert_eq!(text.virtual_address, 0x1000);
    let reloc = section(&file, &pe, 1).unwrap();
    assert_eq!(&reloc.name[..6], b".reloc");
    assert!(section(&file, &pe, 2).is_none());

    // --- parser rejects non-images honestly --------------------------------
    assert_eq!(parse_pe32plus(&file[..0x10]), Err(PeError::TooShort));
    let mut bad = file.clone();
    bad[0] = b'X';
    assert_eq!(parse_pe32plus(&bad), Err(PeError::NotMz));
    let mut bad = file.clone();
    bad[0x84] = 0x4C; // machine → i386
    bad[0x85] = 0x01;
    assert_eq!(parse_pe32plus(&bad), Err(PeError::NotAmd64));
    let mut bad = file.clone();
    bad[0x98] = 0x0B; // optional magic → PE32 (0x10B)
    bad[0x99] = 0x01;
    assert_eq!(parse_pe32plus(&bad), Err(PeError::NotPe32Plus));

    // --- load at ImageBase: no relocation needed ------------------------------
    let mut img = vec![0xAAu8; TESTAPP_SIZE_OF_IMAGE as usize];
    let l0 = load_pe32plus(&file, TESTAPP_IMAGE_BASE, &mut img).expect("loads at base");
    assert_eq!(l0.relocs_applied, 0);
    assert_eq!(l0.sections_loaded, 2);
    assert_eq!(l0.entry, TESTAPP_IMAGE_BASE + TESTAPP_ENTRY_RVA as u64);
    assert_eq!(&img[0x1000..0x1000 + TESTAPP_CODE.len()], &TESTAPP_CODE);
    assert_eq!(
        get_u64(&img, TESTAPP_MSG_PTR_RVA as usize),
        TESTAPP_IMAGE_BASE + TESTAPP_MSG_RVA as u64
    );
    // Gap after headers is zero-filled (not the 0xAA poison).
    assert!(img[0x200..0x1000].iter().all(|&b| b == 0));

    // --- load at the F2 base: DIR64 relocation fixes the message pointer -----
    let mut img2 = vec![0u8; TESTAPP_SIZE_OF_IMAGE as usize];
    let l1 = load_pe32plus(&file, F2_APP_LOAD_BASE, &mut img2).expect("loads relocated");
    assert_eq!(l1.relocs_applied, 2);
    assert_eq!(
        get_u64(&img2, TESTAPP_FAIL_PTR_RVA as usize),
        F2_APP_LOAD_BASE + TESTAPP_FAIL_MSG_RVA as u64
    );
    assert_eq!(l1.entry, F2_APP_LOAD_BASE + TESTAPP_ENTRY_RVA as u64);
    assert_eq!(
        get_u64(&img2, TESTAPP_MSG_PTR_RVA as usize),
        F2_APP_LOAD_BASE + TESTAPP_MSG_RVA as u64
    );
    // Code bytes are untouched by relocation; the message decodes.
    assert_eq!(&img2[0x1000..0x1000 + TESTAPP_CODE.len()], &TESTAPP_CODE);
    let msg_off = TESTAPP_MSG_RVA as usize;
    let mut decoded = String::new();
    let mut i = msg_off;
    loop {
        let ch = u16::from_le_bytes([img2[i], img2[i + 1]]);
        if ch == 0 {
            break;
        }
        decoded.push(char::from_u32(ch as u32).unwrap());
        i += 2;
    }
    assert_eq!(decoded, TESTAPP_MESSAGE);

    // RIP-relative loads land on the pointer cells: OK `mov rdx,[rip+0x1E]`
    // at +0x9B (next 0xA2) -> 0xC0; FAIL `mov rdx,[rip+0x12]` at +0xAF
    // (next 0xB6) -> 0xC8. HLTs at +0xA5 / +0xB9; fail label at +0xA8.
    assert_eq!(0xA2 + 0x1E, (TESTAPP_MSG_PTR_RVA - 0x1000) as usize);
    assert_eq!(0xB6 + 0x12, (TESTAPP_FAIL_PTR_RVA - 0x1000) as usize);
    assert_eq!(TESTAPP_CODE[TESTAPP_HLT_OK_OFF as usize], 0xF4);
    assert_eq!(TESTAPP_CODE[TESTAPP_HLT_FAIL_OFF as usize], 0xF4);
    assert_eq!(&TESTAPP_CODE[0xA8..0xAC], &[0x48, 0x8B, 0x43, 0x40]); // fail label
    // jnz rel32 targets all resolve to the fail label (+0xA8).
    for (at, rel) in [(0x39usize, 0x69i64), (0x55, 0x4D), (0x70, 0x32), (0x8E, 0x14)] {
        assert_eq!(&TESTAPP_CODE[at..at + 2], &[0x0F, 0x85]);
        let r = i32::from_le_bytes(TESTAPP_CODE[at + 2..at + 6].try_into().unwrap()) as i64;
        assert_eq!(r, rel);
        assert_eq!(at as i64 + 6 + r, 0xA8);
    }
    // Boot-services slot offsets the app hard-codes (24 + idx*8).
    assert_eq!(0x28, 24 + 2 * 8); // AllocatePages
    assert_eq!(0x50, 24 + 7 * 8); // CreateEvent
    assert_eq!(0x58, 24 + 8 * 8); // SetTimer
    assert_eq!(0x60, 24 + 9 * 8); // WaitForEvent
    assert_eq!(0xF8, 24 + 28 * 8); // Stall

    // Loader refuses a too-small destination and an unaligned base.
    let mut small = vec![0u8; 0x100];
    assert_eq!(
        load_pe32plus(&file, F2_APP_LOAD_BASE, &mut small),
        Err(PeError::DestinationTooSmall)
    );
    assert_eq!(
        load_pe32plus(&file, F2_APP_LOAD_BASE + 0x10, &mut img2),
        Err(PeError::LoadBaseUnaligned)
    );
    // Unsupported relocation type is an error, not a silent skip.
    let mut bad = file.clone();
    // Entry is LE u16 0xA040: type nibble is the high nibble of byte 0x409.
    bad[0x409] = 0x30; // type 3 (HIGHLOW), offset high bits 0 → 0x3040
    let hi = u16::from_le_bytes([bad[0x408], bad[0x409]]) >> 12;
    assert_eq!(hi, 3);
    assert_eq!(
        load_pe32plus(&bad, F2_APP_LOAD_BASE, &mut img2),
        Err(PeError::RelocUnsupportedType(3))
    );

    // --- launch plan ---------------------------------------------------------
    let plan = plan_f2(32 * 1024 * 1024).expect("plan fits 32 MiB");
    assert_eq!(plan.identity_pml4, F2_IDENTITY_PML4);
    assert_eq!(plan.tables_base, F2_TABLES_BASE);
    assert_eq!(plan.system_table, F2_TABLES_BASE + IMAGE_SYSTEM_TABLE_OFF as u64);
    assert_eq!(plan.rdx, plan.system_table);
    assert_eq!(plan.rcx, F2_IMAGE_HANDLE);
    assert_eq!(plan.app_load_base, F2_APP_LOAD_BASE);
    assert_ne!(plan.app_load_base, plan.app_image_base); // relocation exercised
    assert_eq!(plan.stack_top, F2_STACK_TOP);
    assert_eq!(plan.rsp % 16, 8); // MS x64 ABI at entry
    assert_eq!(plan.cr0, F2_CR0);
    assert_eq!(plan.cr4, F2_CR4);
    assert_eq!(plan.efer, F2_EFER);
    assert_eq!(plan.cr3, F2_IDENTITY_PML4);
    assert_eq!(plan.cr0 & 0x8000_0001, 0x8000_0001); // PG | PE
    assert_eq!(plan.efer & 0x500, 0x500); // LME | LMA
    assert_eq!(plan.cr4 & 0x20, 0x20); // PAE
    assert_eq!(plan_f2(0x0090_0000), Err(PlanError::SlabTooSmall));
    // The app's ConOut load (`mov rax,[rdx+0x40]`) matches the table layout.
    assert_eq!(SYSTEM_TABLE_CONOUT_OFF, 0x40);

    #[cfg(not(target_os = "uefi"))]
    println!("{RAYNU_F_LOADER_OK_MARKER}");
}

#[test]
fn raynu_f_launch_flag_is_opt_in() {
    use crate::boot::raynu_f_flag::{
        force_for_test, requested, RAYNU_F_FLAG_PATH, RAYNU_F_REQUESTED_MARKER,
    };
    // Default boot path: not requested. Host probe is a no-op.
    crate::boot::raynu_f_flag::probe();
    assert!(!requested());
    assert_eq!(RAYNU_F_FLAG_PATH, "\\EFI\\RayNu\\raynuf.txt");
    assert!(RAYNU_F_REQUESTED_MARKER.contains("ADR-016"));
    assert!(RAYNU_F_REQUESTED_MARKER.contains("not ISO-INSTALL-OK"));
    force_for_test(true);
    assert!(requested());
    force_for_test(false);
    assert!(!requested());
}

#[test]
fn raynu_f_memory_events_timer_services() {
    use super::events::{
        Events, WaitOutcome, EVT_NOTIFY_SIGNAL, EVT_TIMER, TIMER_CANCEL, TIMER_PERIODIC,
        TIMER_RELATIVE, TPL_APPLICATION, TPL_CALLBACK, TPL_NOTIFY, WAIT_KEY_SLOT,
    };
    use super::memory::{
        PagePool, ALLOCATE_ADDRESS, ALLOCATE_ANY_PAGES, ALLOCATE_MAX_ADDRESS,
        EFI_BUFFER_TOO_SMALL, EFI_CONVENTIONAL_MEMORY, EFI_LOADER_DATA, EFI_NOT_FOUND,
        EFI_OUT_OF_RESOURCES, EFI_RESERVED_MEMORY_TYPE, MEMORY_DESCRIPTOR_SIZE, POOL_BASE,
        POOL_END, POOL_HEADER_BYTES, POOL_MAGIC, POOL_PAGES,
    };
    use super::services::{
        dispatch, stack_arg, FirmwareState, ServiceArgs, ServiceId, EFI_INVALID_PARAMETER,
        EFI_NOT_READY, EFI_SUCCESS, EFI_UNSUPPORTED, STACK_ARG5_OFF,
    };
    use super::tables::{build_firmware_image, get_u64, CONIN_WAIT_KEY_EVENT, IMAGE_BYTES};
    use super::RAYNU_F_SERVICES_OK_MARKER;

    const EFI_TIMEOUT: u64 = 0x8000_0000_0000_0012;

    // Guest = whole 32 MiB slab as a Vec (tests are cheap; it is zeroed).
    let base = 0u64;
    let mut mem = vec![0u8; SLAB as usize];
    let tb = 0x0080_0000usize;
    build_firmware_image(tb as u64, &mut mem[tb..tb + IMAGE_BYTES]).unwrap();
    // ConIn.WaitForKey is a real tagged handle (slot 0).
    let conin = 0x0080_14A0usize;
    assert_eq!(get_u64(&mem, conin + 16), CONIN_WAIT_KEY_EVENT);
    assert_eq!(CONIN_WAIT_KEY_EVENT, Events::handle_for(WAIT_KEY_SLOT));
    let guest = MockGuest::new(base, mem);
    let mut sink = CaptureSink::default();
    let mut st = FirmwareState::new();
    let clk = ManualClock {
        now: Cell::new(10_000_000),
        step: 50_000, // 5 ms per read
    };
    // Scratch guest area for out-params and a fake stack.
    let scratch = 0x0100_0000u64; // inside the pool window but unused by allocator? no—use below pool
    let scratch = scratch.min(POOL_BASE - 0x2000); // 0xAFE000: between GDT page and pool
    let stack = scratch + 0x1000; // fake RSP (return address slot at [rsp])
    let p_out = scratch; // generic 8-byte out-param
    let p_out2 = scratch + 8;
    let p_out3 = scratch + 16;
    let p_out4 = scratch + 24;
    let p_dver = scratch + 32;

    // --- TPL ------------------------------------------------------------
    let d = dispatch(ServiceId::RaiseTPL, ServiceArgs::regs(TPL_NOTIFY, 0, 0, 0), &guest, &mut sink, &mut st, &clk, SLAB);
    assert_eq!(d.status, TPL_APPLICATION); // returns old TPL
    assert_eq!(st.events.tpl(), TPL_NOTIFY);
    // WaitForEvent is only legal at TPL_APPLICATION.
    guest.put_u64(p_out, CONIN_WAIT_KEY_EVENT);
    let d = dispatch(ServiceId::WaitForEvent, ServiceArgs::regs(1, p_out, p_out2, 0), &guest, &mut sink, &mut st, &clk, SLAB);
    assert_eq!(d.status, EFI_UNSUPPORTED);
    dispatch(ServiceId::RestoreTPL, ServiceArgs::regs(TPL_APPLICATION, 0, 0, 0), &guest, &mut sink, &mut st, &clk, SLAB);
    assert_eq!(st.events.tpl(), TPL_APPLICATION);

    // --- AllocatePages / FreePages ----------------------------------------
    assert_eq!(POOL_PAGES, 5120);
    assert_eq!(st.pool.free_pages(), POOL_PAGES);
    guest.put_u64(p_out, 0);
    let d = dispatch(ServiceId::AllocatePages, ServiceArgs::regs(ALLOCATE_ANY_PAGES as u64, EFI_LOADER_DATA as u64, 4, p_out), &guest, &mut sink, &mut st, &clk, SLAB);
    assert_eq!(d.status, EFI_SUCCESS);
    assert!(d.alloc_ok);
    let a1 = guest.u64_at(p_out);
    assert_eq!(a1, POOL_BASE);
    assert_eq!(st.pool.free_pages(), POOL_PAGES - 4);
    // MaxAddress: must end at or below the given address.
    guest.put_u64(p_out, POOL_BASE + 0x8000 - 1);
    let d = dispatch(ServiceId::AllocatePages, ServiceArgs::regs(ALLOCATE_MAX_ADDRESS as u64, EFI_LOADER_DATA as u64, 2, p_out), &guest, &mut sink, &mut st, &clk, SLAB);
    assert_eq!(d.status, EFI_SUCCESS);
    let a2 = guest.u64_at(p_out);
    assert!(a2 >= POOL_BASE + 0x4000 && a2 + 0x2000 <= POOL_BASE + 0x8000);
    // Address: exact, must be free.
    guest.put_u64(p_out, POOL_BASE + 0x10000);
    let d = dispatch(ServiceId::AllocatePages, ServiceArgs::regs(ALLOCATE_ADDRESS as u64, EFI_LOADER_DATA as u64, 1, p_out), &guest, &mut sink, &mut st, &clk, SLAB);
    assert_eq!(d.status, EFI_SUCCESS);
    assert_eq!(guest.u64_at(p_out), POOL_BASE + 0x10000);
    let d = dispatch(ServiceId::AllocatePages, ServiceArgs::regs(ALLOCATE_ADDRESS as u64, EFI_LOADER_DATA as u64, 1, p_out), &guest, &mut sink, &mut st, &clk, SLAB);
    assert_eq!(d.status, EFI_NOT_FOUND); // already used
    // Conventional/Reserved are not allocatable types; huge request fails.
    let d = dispatch(ServiceId::AllocatePages, ServiceArgs::regs(0, EFI_CONVENTIONAL_MEMORY as u64, 1, p_out), &guest, &mut sink, &mut st, &clk, SLAB);
    assert_eq!(d.status, EFI_INVALID_PARAMETER);
    let d = dispatch(ServiceId::AllocatePages, ServiceArgs::regs(0, EFI_LOADER_DATA as u64, POOL_PAGES as u64, p_out), &guest, &mut sink, &mut st, &clk, SLAB);
    assert_eq!(d.status, EFI_OUT_OF_RESOURCES);
    // FreePages: valid, then double free fails, outside pool fails.
    assert_eq!(dispatch(ServiceId::FreePages, ServiceArgs::regs(a1, 4, 0, 0), &guest, &mut sink, &mut st, &clk, SLAB).status, EFI_SUCCESS);
    assert_eq!(dispatch(ServiceId::FreePages, ServiceArgs::regs(a1, 4, 0, 0), &guest, &mut sink, &mut st, &clk, SLAB).status, EFI_NOT_FOUND);
    assert_eq!(dispatch(ServiceId::FreePages, ServiceArgs::regs(0x1000, 1, 0, 0), &guest, &mut sink, &mut st, &clk, SLAB).status, EFI_NOT_FOUND);

    // --- AllocatePool / FreePool ------------------------------------------
    let d = dispatch(ServiceId::AllocatePool, ServiceArgs::regs(EFI_LOADER_DATA as u64, 100, p_out, 0), &guest, &mut sink, &mut st, &clk, SLAB);
    assert_eq!(d.status, EFI_SUCCESS);
    let pbuf = guest.u64_at(p_out);
    assert_eq!(pbuf & 0xfff, POOL_HEADER_BYTES);
    assert_eq!(guest.u64_at(pbuf - 16), POOL_MAGIC);
    assert_eq!(guest.u64_at(pbuf - 8), PagePool::pool_pages_for(100));
    assert_eq!(PagePool::pool_pages_for(100), 1);
    assert_eq!(PagePool::pool_pages_for(4096), 2);
    assert_eq!(dispatch(ServiceId::FreePool, ServiceArgs::regs(pbuf, 0, 0, 0), &guest, &mut sink, &mut st, &clk, SLAB).status, EFI_SUCCESS);
    // Header magic was cleared: second free is rejected.
    assert_eq!(dispatch(ServiceId::FreePool, ServiceArgs::regs(pbuf, 0, 0, 0), &guest, &mut sink, &mut st, &clk, SLAB).status, EFI_INVALID_PARAMETER);

    // --- GetMemoryMap / ExitBootServices ----------------------------------
    // 5th arg (DescriptorVersion*) lives on the fake stack at [rsp+0x28].
    guest.put_u64(stack + STACK_ARG5_OFF, p_dver);
    assert_eq!(stack_arg(&guest, stack, 5), Some(p_dver));
    assert_eq!(stack_arg(&guest, stack, 4), None);
    let args = ServiceArgs { a1: p_out, a2: 0, a3: p_out2, a4: p_out3, rsp: stack };
    guest.put_u64(p_out, 0);
    let d = dispatch(ServiceId::GetMemoryMap, args, &guest, &mut sink, &mut st, &clk, SLAB);
    assert_eq!(d.status, EFI_BUFFER_TOO_SMALL);
    let need = guest.u64_at(p_out);
    assert!(need >= 8 * MEMORY_DESCRIPTOR_SIZE && need % MEMORY_DESCRIPTOR_SIZE == 0);
    // Now with a big enough buffer at scratch+0x100.
    let map = scratch + 0x100;
    guest.put_u64(p_out, need);
    let args = ServiceArgs { a1: p_out, a2: map, a3: p_out2, a4: p_out3, rsp: stack };
    let d = dispatch(ServiceId::GetMemoryMap, args, &guest, &mut sink, &mut st, &clk, SLAB);
    assert_eq!(d.status, EFI_SUCCESS);
    assert_eq!(guest.u64_at(p_out), need);
    let key = guest.u64_at(p_out2);
    assert_eq!(key, 1);
    assert_eq!(guest.u64_at(p_out3), MEMORY_DESCRIPTOR_SIZE);
    let mut dv = [0u8; 4];
    guest.read(p_dver, &mut dv);
    assert_eq!(u32::from_le_bytes(dv), 1);
    // First descriptor: Reserved [0, 0x800000). Descriptors are contiguous
    // and cover [0, SLAB).
    let n = (need / MEMORY_DESCRIPTOR_SIZE) as usize;
    let mut cursor = 0u64;
    let mut saw_conv = false;
    for i in 0..n {
        let d0 = map + i as u64 * MEMORY_DESCRIPTOR_SIZE;
        let mut t = [0u8; 4];
        guest.read(d0, &mut t);
        let typ = u32::from_le_bytes(t);
        let start = guest.u64_at(d0 + 8);
        let pages = guest.u64_at(d0 + 24);
        assert_eq!(start, cursor, "descriptor {i} not contiguous");
        if i == 0 {
            assert_eq!(typ, EFI_RESERVED_MEMORY_TYPE);
            assert_eq!(pages * 4096, 0x0080_0000);
        }
        if typ == EFI_CONVENTIONAL_MEMORY {
            saw_conv = true;
        }
        cursor = start + pages * 4096;
    }
    assert_eq!(cursor, SLAB);
    assert!(saw_conv);
    // ExitBootServices: wrong key rejected, right key accepted (one-shot).
    assert_eq!(dispatch(ServiceId::ExitBootServices, ServiceArgs::regs(0x5246_0000_0000_0010, key + 7, 0, 0), &guest, &mut sink, &mut st, &clk, SLAB).status, EFI_INVALID_PARAMETER);
    assert!(!st.pool.exited());
    let d = dispatch(ServiceId::ExitBootServices, ServiceArgs::regs(0x5246_0000_0000_0010, key, 0, 0), &guest, &mut sink, &mut st, &clk, SLAB);
    assert_eq!(d.status, EFI_SUCCESS);
    assert!(d.exited_boot_services);
    assert!(st.pool.exited());
    let _ = POOL_END;

    // --- events / timers --------------------------------------------------
    // CreateEvent(EVT_TIMER, TPL_CALLBACK, 0, 0, &ev): 5th arg via stack.
    guest.put_u64(stack + STACK_ARG5_OFF, p_out4);
    let d = dispatch(ServiceId::CreateEvent, ServiceArgs { a1: EVT_TIMER as u64, a2: TPL_CALLBACK, a3: 0, a4: 0, rsp: stack }, &guest, &mut sink, &mut st, &clk, SLAB);
    assert_eq!(d.status, EFI_SUCCESS);
    let ev = guest.u64_at(p_out4);
    assert_eq!(ev & !0xFFF, super::events::EVENT_HANDLE_TAG);
    assert_ne!(ev, CONIN_WAIT_KEY_EVENT);
    // Notify-signal events need a notify fn.
    let d = dispatch(ServiceId::CreateEvent, ServiceArgs { a1: EVT_NOTIFY_SIGNAL as u64, a2: TPL_CALLBACK, a3: 0, a4: 0, rsp: stack }, &guest, &mut sink, &mut st, &clk, SLAB);
    assert_eq!(d.status, EFI_INVALID_PARAMETER);
    // CheckEvent: not ready until the timer fires.
    assert_eq!(dispatch(ServiceId::CheckEvent, ServiceArgs::regs(ev, 0, 0, 0), &guest, &mut sink, &mut st, &clk, SLAB).status, EFI_NOT_READY);
    // SetTimer relative 100 ms (1_000_000 x 100 ns) then WaitForEvent blocks
    // on the manual clock until it fires.
    assert_eq!(dispatch(ServiceId::SetTimer, ServiceArgs::regs(ev, TIMER_RELATIVE as u64, 1_000_000, 0), &guest, &mut sink, &mut st, &clk, SLAB).status, EFI_SUCCESS);
    guest.put_u64(p_out, ev);
    guest.put_u64(p_out2, 0xFFFF);
    let t0 = clk.now.get();
    let d = dispatch(ServiceId::WaitForEvent, ServiceArgs::regs(1, p_out, p_out2, 0), &guest, &mut sink, &mut st, &clk, SLAB);
    assert_eq!(d.status, EFI_SUCCESS);
    assert!(matches!(d.wait, Some(WaitOutcome::TimerFired(0))));
    assert_eq!(guest.u64_at(p_out2), 0); // *Index written
    assert!(clk.now.get() - t0 >= 1_000_000, "clock advanced past the deadline");
    assert_eq!(st.events.timers_fired, 1);
    // Relative timer disarmed after firing: CheckEvent not ready again.
    assert_eq!(dispatch(ServiceId::CheckEvent, ServiceArgs::regs(ev, 0, 0, 0), &guest, &mut sink, &mut st, &clk, SLAB).status, EFI_NOT_READY);
    // Periodic: fires repeatedly; CheckEvent clears; cancel stops.
    assert_eq!(dispatch(ServiceId::SetTimer, ServiceArgs::regs(ev, TIMER_PERIODIC as u64, 200_000, 0), &guest, &mut sink, &mut st, &clk, SLAB).status, EFI_SUCCESS);
    let mut fires = 0;
    for _ in 0..40 {
        if dispatch(ServiceId::CheckEvent, ServiceArgs::regs(ev, 0, 0, 0), &guest, &mut sink, &mut st, &clk, SLAB).status == EFI_SUCCESS {
            fires += 1;
        }
    }
    assert!(fires >= 3, "periodic fired {fires}");
    assert_eq!(dispatch(ServiceId::SetTimer, ServiceArgs::regs(ev, TIMER_CANCEL as u64, 0, 0), &guest, &mut sink, &mut st, &clk, SLAB).status, EFI_SUCCESS);
    // SignalEvent makes WaitForEvent return immediately.
    assert_eq!(dispatch(ServiceId::SignalEvent, ServiceArgs::regs(ev, 0, 0, 0), &guest, &mut sink, &mut st, &clk, SLAB).status, EFI_SUCCESS);
    let d = dispatch(ServiceId::WaitForEvent, ServiceArgs::regs(1, p_out, p_out2, 0), &guest, &mut sink, &mut st, &clk, SLAB);
    assert_eq!(d.status, EFI_SUCCESS);
    assert!(matches!(d.wait, Some(WaitOutcome::Immediate(0))));
    // Waiting on an unarmed, unsignaled timer can never progress: EFI_TIMEOUT,
    // not a host hang.
    let d = dispatch(ServiceId::WaitForEvent, ServiceArgs::regs(1, p_out, p_out2, 0), &guest, &mut sink, &mut st, &clk, SLAB);
    assert_eq!(d.status, EFI_TIMEOUT);
    assert!(matches!(d.wait, Some(WaitOutcome::Stuck)));
    // CloseEvent, then the handle is invalid.
    assert_eq!(dispatch(ServiceId::CloseEvent, ServiceArgs::regs(ev, 0, 0, 0), &guest, &mut sink, &mut st, &clk, SLAB).status, EFI_SUCCESS);
    assert_eq!(dispatch(ServiceId::CheckEvent, ServiceArgs::regs(ev, 0, 0, 0), &guest, &mut sink, &mut st, &clk, SLAB).status, EFI_INVALID_PARAMETER);
    // Garbage handle.
    assert_eq!(dispatch(ServiceId::SignalEvent, ServiceArgs::regs(0xDEAD, 0, 0, 0), &guest, &mut sink, &mut st, &clk, SLAB).status, EFI_INVALID_PARAMETER);

    // --- ConIn: WaitForKey + ReadKeyStroke from host input ------------------
    guest.put_u64(p_out, CONIN_WAIT_KEY_EVENT);
    assert_eq!(dispatch(ServiceId::CheckEvent, ServiceArgs::regs(CONIN_WAIT_KEY_EVENT, 0, 0, 0), &guest, &mut sink, &mut st, &clk, SLAB).status, EFI_NOT_READY);
    sink.1.push(b'y');
    // WaitForEvent on WaitForKey returns once input is pending.
    let d = dispatch(ServiceId::WaitForEvent, ServiceArgs::regs(1, p_out, p_out2, 0), &guest, &mut sink, &mut st, &clk, SLAB);
    assert_eq!(d.status, EFI_SUCCESS);
    let d = dispatch(ServiceId::ConInReadKeyStroke, ServiceArgs::regs(0, p_out3, 0, 0), &guest, &mut sink, &mut st, &clk, SLAB);
    assert_eq!(d.status, EFI_SUCCESS);
    let mut key = [0u8; 4];
    guest.read(p_out3, &mut key);
    assert_eq!(key, [0, 0, b'y', 0]); // ScanCode 0, UnicodeChar 'y'
    assert_eq!(dispatch(ServiceId::ConInReadKeyStroke, ServiceArgs::regs(0, p_out3, 0, 0), &guest, &mut sink, &mut st, &clk, SLAB).status, EFI_NOT_READY);

    // --- Stall / monotonic / watchdog / crc / copy / set ------------------
    let t0 = clk.now.get();
    assert_eq!(dispatch(ServiceId::Stall, ServiceArgs::regs(20_000, 0, 0, 0), &guest, &mut sink, &mut st, &clk, SLAB).status, EFI_SUCCESS);
    assert!(clk.now.get() - t0 >= 200_000); // 20 ms = 200_000 x 100 ns
    assert_eq!(dispatch(ServiceId::GetNextMonotonicCount, ServiceArgs::regs(p_out, 0, 0, 0), &guest, &mut sink, &mut st, &clk, SLAB).status, EFI_SUCCESS);
    let m1 = guest.u64_at(p_out);
    dispatch(ServiceId::GetNextMonotonicCount, ServiceArgs::regs(p_out, 0, 0, 0), &guest, &mut sink, &mut st, &clk, SLAB);
    assert_eq!(guest.u64_at(p_out), m1 + 1);
    assert_eq!(dispatch(ServiceId::SetWatchdogTimer, ServiceArgs::regs(300, 0, 0, 0), &guest, &mut sink, &mut st, &clk, SLAB).status, EFI_SUCCESS);
    assert_eq!(st.watchdog_sets, 1);
    // SetMem then CopyMem then CalculateCrc32 over guest memory.
    let buf_a = scratch + 0x800;
    let buf_b = scratch + 0x900;
    assert_eq!(dispatch(ServiceId::SetMem, ServiceArgs::regs(buf_a, 9, 0x31, 0), &guest, &mut sink, &mut st, &clk, SLAB).status, EFI_SUCCESS);
    let mut nine = [0u8; 9];
    guest.read(buf_a, &mut nine);
    assert_eq!(&nine, b"111111111");
    // Write "123456789" then copy and CRC it (known answer 0xCBF43926).
    guest.write(buf_a, b"123456789");
    assert_eq!(dispatch(ServiceId::CopyMem, ServiceArgs::regs(buf_b, buf_a, 9, 0), &guest, &mut sink, &mut st, &clk, SLAB).status, EFI_SUCCESS);
    guest.read(buf_b, &mut nine);
    assert_eq!(&nine, b"123456789");
    assert_eq!(dispatch(ServiceId::CalculateCrc32, ServiceArgs::regs(buf_b, 9, p_out, 0), &guest, &mut sink, &mut st, &clk, SLAB).status, EFI_SUCCESS);
    let mut c = [0u8; 4];
    guest.read(p_out, &mut c);
    assert_eq!(u32::from_le_bytes(c), 0xCBF4_3926);
    // Overlapping CopyMem (dst > src) preserves data (memmove semantics).
    guest.write(buf_a, b"ABCDEFGH");
    assert_eq!(dispatch(ServiceId::CopyMem, ServiceArgs::regs(buf_a + 2, buf_a, 6, 0), &guest, &mut sink, &mut st, &clk, SLAB).status, EFI_SUCCESS);
    let mut eight = [0u8; 8];
    guest.read(buf_a, &mut eight);
    assert_eq!(&eight, b"ABABCDEF");

    #[cfg(not(target_os = "uefi"))]
    println!("{RAYNU_F_SERVICES_OK_MARKER}");
}
