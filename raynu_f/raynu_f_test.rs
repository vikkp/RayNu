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
    assert_eq!(
        super::RAYNU_F_DISK_BOOT_OK_MARKER,
        "RAYNU-V-RAYNU-F-DISK-BOOT-OK"
    );
    assert_ne!(
        super::RAYNU_F_DISK_BOOT_OK_MARKER,
        "RAYNU-V-M7-ISO-INSTALL-OK"
    );
    assert_ne!(
        super::RAYNU_F_DISK_BOOT_OK_MARKER,
        "RAYNU-V-M7-ISO-BOOTED-FROM-DISK"
    );
    assert_ne!(
        super::RAYNU_F_DISK_BOOT_OK_MARKER,
        super::RAYNU_F_START_IMAGE_OK_MARKER
    );

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
    // Protocol notify and most runtime services remain unimplemented; the
    // variable services answer "no such variable" (no store), never a fake.
    assert_eq!(
        dispatch(ServiceId::RegisterProtocolNotify, args, &guest, &mut sink3, &mut st, &clk, SLAB).status,
        EFI_UNSUPPORTED
    );
    assert_eq!(
        dispatch(ServiceId::runtime_service(0), args, &guest, &mut sink3, &mut st, &clk, SLAB).status,
        EFI_UNSUPPORTED
    );
    assert_eq!(ServiceId::GetVariable, ServiceId::runtime_service(6));
    assert_eq!(
        dispatch(ServiceId::GetVariable, ServiceArgs::regs(base + 0x1900, base + 0x1910, 0, base + 0x1920), &guest, &mut sink3, &mut st, &clk, SLAB).status,
        0x8000_0000_0000_000E
    );
    assert_eq!(
        dispatch(ServiceId::GetVariable, args, &guest, &mut sink3, &mut st, &clk, SLAB).status,
        EFI_INVALID_PARAMETER
    );
    assert_eq!(
        dispatch(ServiceId::GetNextVariableName, ServiceArgs::regs(base + 0x1900, base + 0x1910, base + 0x1920, 0), &guest, &mut sink3, &mut st, &clk, SLAB).status,
        0x8000_0000_0000_000E
    );
    // UnloadImage with nothing loaded is a parameter error, not a success.
    assert_eq!(
        dispatch(ServiceId::UnloadImage, args, &guest, &mut sink3, &mut st, &clk, SLAB).status,
        EFI_INVALID_PARAMETER
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
        BELOW1M_BASE, BELOW1M_PAGES, EFI_BUFFER_TOO_SMALL, EFI_CONVENTIONAL_MEMORY,
        EFI_LOADER_DATA, EFI_NOT_FOUND, EFI_OUT_OF_RESOURCES, EFI_RESERVED_MEMORY_TYPE,
        MEMORY_DESCRIPTOR_SIZE, POOL_BASE, POOL_END, POOL_HEADER_BYTES, POOL_MAGIC, POOL_PAGES,
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
    assert_eq!(BELOW1M_PAGES, 158);
    assert_eq!(st.pool.free_pages(), POOL_PAGES + BELOW1M_PAGES);
    guest.put_u64(p_out, 0);
    let d = dispatch(ServiceId::AllocatePages, ServiceArgs::regs(ALLOCATE_ANY_PAGES as u64, EFI_LOADER_DATA as u64, 4, p_out), &guest, &mut sink, &mut st, &clk, SLAB);
    assert_eq!(d.status, EFI_SUCCESS);
    assert!(d.alloc_ok);
    let a1 = guest.u64_at(p_out);
    assert_eq!(a1, POOL_BASE);
    assert_eq!(st.pool.free_pages(), POOL_PAGES + BELOW1M_PAGES - 4);
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
    guest.put_u64(p_out3, 0);
    let d = dispatch(ServiceId::GetMemoryMap, args, &guest, &mut sink, &mut st, &clk, SLAB);
    assert_eq!(d.status, EFI_BUFFER_TOO_SMALL);
    let need = guest.u64_at(p_out);
    assert!(need >= 12 * MEMORY_DESCRIPTOR_SIZE && need % MEMORY_DESCRIPTOR_SIZE == 0);
    // The sizing call also reports DescriptorSize/Version (Linux computes its
    // pool size from desc_size here; nested d7e755d got garbage).
    assert_eq!(guest.u64_at(p_out3), MEMORY_DESCRIPTOR_SIZE);
    let mut dv0 = [0u8; 4];
    guest.read(p_dver, &mut dv0);
    assert_eq!(u32::from_le_bytes(dv0), 1);
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
    // First descriptor: Reserved [0, 0x1000) (IVT/BDA). The firmware image
    // at 0x800000 is RuntimeServicesCode. A conventional run exists below
    // 1 MiB (Linux real-mode trampoline). Descriptors are contiguous and
    // cover [0, SLAB).
    let n = (need / MEMORY_DESCRIPTOR_SIZE) as usize;
    let mut cursor = 0u64;
    let mut saw_conv = false;
    let mut saw_rt = false;
    let mut saw_below1m = false;
    for i in 0..n {
        let d0 = map + i as u64 * MEMORY_DESCRIPTOR_SIZE;
        let mut t = [0u8; 4];
        guest.read(d0, &mut t);
        let typ = u32::from_le_bytes(t);
        let start = guest.u64_at(d0 + 8);
        let pages = guest.u64_at(d0 + 24);
        assert_eq!(start, cursor, "descriptor {i} not contiguous");
        let attr = guest.u64_at(d0 + 32);
        if i == 0 {
            assert_eq!(typ, EFI_RESERVED_MEMORY_TYPE);
            assert_eq!(pages * 4096, BELOW1M_BASE);
        }
        if start == 0x0080_0000 {
            // The firmware image (trampolines + tables) is runtime code the
            // OS keeps mapped and executable after EBS (nested 0ab0f9d: Linux
            // executed our SetVirtualAddressMap trampoline from an NX page).
            assert_eq!(pages * 4096, super::tables::IMAGE_BYTES as u64);
            assert_eq!(typ, super::memory::EFI_RUNTIME_SERVICES_CODE);
            assert_ne!(attr & super::memory::EFI_MEMORY_RUNTIME, 0);
            saw_rt = true;
        } else {
            assert_eq!(attr & super::memory::EFI_MEMORY_RUNTIME, 0, "only runtime regions carry RUNTIME");
        }
        assert_ne!(attr & super::memory::EFI_MEMORY_WB, 0);
        if typ == EFI_CONVENTIONAL_MEMORY {
            saw_conv = true;
            if start < 0x10_0000 {
                saw_below1m = true;
                assert_eq!(start, BELOW1M_BASE);
                assert_eq!(pages, BELOW1M_PAGES as u64);
            }
        }
        cursor = start + pages * 4096;
    }
    assert_eq!(cursor, SLAB);
    assert!(saw_conv);
    assert!(saw_rt);
    assert!(saw_below1m, "Linux reserve_real_mode needs conventional below 1MiB");
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

/// Backing store for the F4 BlockIo tests: CD image + install disk.
static F4_CD: std::sync::OnceLock<std::sync::Mutex<Vec<u8>>> = std::sync::OnceLock::new();
static F4_DISK: std::sync::OnceLock<std::sync::Mutex<Vec<u8>>> = std::sync::OnceLock::new();

fn f4_read(media_id: u32, off: u64, buf: &mut [u8]) -> bool {
    let cell = match media_id {
        super::MEDIA_ID_CD => &F4_CD,
        super::MEDIA_ID_DISK => &F4_DISK,
        _ => return false,
    };
    let g = cell.get().unwrap().lock().unwrap();
    let start = off as usize;
    if start + buf.len() > g.len() {
        return false;
    }
    buf.copy_from_slice(&g[start..start + buf.len()]);
    true
}

fn f4_write(media_id: u32, off: u64, buf: &[u8]) -> bool {
    if media_id != super::MEDIA_ID_DISK {
        return false;
    }
    let mut g = F4_DISK.get().unwrap().lock().unwrap();
    let start = off as usize;
    if start + buf.len() > g.len() {
        return false;
    }
    g[start..start + buf.len()].copy_from_slice(buf);
    true
}

#[test]
fn raynu_f_protocols_and_blockio() {
    use super::blockio::{
        validate_transfer, BlockMedia, BLOCKIO_MEDIA_OFF, BLOCKIO_READ_OFF, BLOCKIO_REVISION2,
        BLOCKIO_REVISION_OFF, BLOCKIO_WRITE_OFF, CD_BLOCK_SIZE, DISK_BLOCK_SIZE,
        EFI_BAD_BUFFER_SIZE, EFI_MEDIA_CHANGED, EFI_NO_MEDIA, EFI_WRITE_PROTECTED,
        MEDIA_BLOCK_SIZE_OFF, MEDIA_LAST_BLOCK_OFF, MEDIA_MEDIA_ID_OFF, MEDIA_PRESENT_OFF,
        MEDIA_READ_ONLY_OFF, MEDIA_REMOVABLE_OFF, MEDIA_SIZE,
    };
    use super::memory::POOL_HEADER_BYTES;
    use super::protocol::{
        Protocols, ALL_HANDLES, BY_PROTOCOL, BY_REGISTER_NOTIFY, GUID_BLOCK_IO,
        GUID_LOADED_IMAGE, GUID_SIMPLE_FILE_SYSTEM, HANDLE_CD, HANDLE_DISK, MAX_HANDLES,
    };
    use super::services::{
        dispatch, FirmwareState, ServiceArgs, ServiceId, EFI_INVALID_PARAMETER, EFI_SUCCESS,
        EFI_UNSUPPORTED, STACK_ARG5_OFF,
    };
    use super::tables::{
        build_firmware_image, get_u64, write_block_media, IMAGE_BLOCKIO_CD_OFF,
        IMAGE_BLOCKIO_DISK_OFF, IMAGE_BYTES, IMAGE_GUID_BLOCK_IO_OFF, IMAGE_MEDIA_CD_OFF,
        IMAGE_MEDIA_DISK_OFF,
    };
    use super::RAYNU_F_BLOCKIO_GATE_MARKER;

    const EFI_NOT_FOUND: u64 = 0x8000_0000_0000_000E;
    const EFI_BUFFER_TOO_SMALL: u64 = 0x8000_0000_0000_0005;

    // --- GUIDs are the real UEFI values ----------------------------------
    // BlockIo {964E5B21-6459-11D2-8E39-00A0C969723B}
    assert_eq!(u32::from_le_bytes(GUID_BLOCK_IO[0..4].try_into().unwrap()), 0x964E_5B21);
    assert_eq!(u16::from_le_bytes(GUID_BLOCK_IO[4..6].try_into().unwrap()), 0x6459);
    assert_eq!(u16::from_le_bytes(GUID_BLOCK_IO[6..8].try_into().unwrap()), 0x11D2);
    assert_eq!(&GUID_BLOCK_IO[8..], &[0x8E, 0x39, 0x00, 0xA0, 0xC9, 0x69, 0x72, 0x3B]);
    assert_eq!(u32::from_le_bytes(GUID_SIMPLE_FILE_SYSTEM[0..4].try_into().unwrap()), 0x964E_5B22);
    assert_eq!(u32::from_le_bytes(GUID_LOADED_IMAGE[0..4].try_into().unwrap()), 0x5B1B_31A1);
    assert_ne!(GUID_BLOCK_IO, GUID_SIMPLE_FILE_SYSTEM);

    // --- media encoding matches EFI_BLOCK_IO_MEDIA ------------------------
    assert_eq!(MEDIA_SIZE, 0x30);
    let iso_bytes = 64u64 * 2048; // 64 CD blocks
    let disk_bytes = 256u64 * 512; // 256 disk blocks
    let media_cd = BlockMedia::cd(iso_bytes);
    let media_disk = BlockMedia::disk(disk_bytes);
    assert_eq!(media_cd.block_size, CD_BLOCK_SIZE);
    assert_eq!(media_cd.last_block, 63);
    assert!(media_cd.read_only && media_cd.removable && media_cd.present);
    assert_eq!(media_disk.block_size, DISK_BLOCK_SIZE);
    assert_eq!(media_disk.last_block, 255);
    assert!(!media_disk.read_only && !media_disk.removable && media_disk.present);
    // No media when there are no blocks.
    assert!(!BlockMedia::cd(0).present);
    assert!(!BlockMedia::disk(511).present);
    let mut enc = [0u8; MEDIA_SIZE];
    media_cd.encode(&mut enc);
    assert_eq!(u32::from_le_bytes(enc[MEDIA_MEDIA_ID_OFF..MEDIA_MEDIA_ID_OFF + 4].try_into().unwrap()), super::MEDIA_ID_CD);
    assert_eq!(enc[MEDIA_REMOVABLE_OFF], 1);
    assert_eq!(enc[MEDIA_PRESENT_OFF], 1);
    assert_eq!(enc[MEDIA_READ_ONLY_OFF], 1);
    assert_eq!(u32::from_le_bytes(enc[MEDIA_BLOCK_SIZE_OFF..MEDIA_BLOCK_SIZE_OFF + 4].try_into().unwrap()), 2048);
    assert_eq!(u64::from_le_bytes(enc[MEDIA_LAST_BLOCK_OFF..MEDIA_LAST_BLOCK_OFF + 8].try_into().unwrap()), 63);

    // --- transfer validation (spec 13.9) ---------------------------------
    assert_eq!(validate_transfer(&media_cd, 1, 0, 2048, 0x1000, false), EFI_SUCCESS);
    assert_eq!(validate_transfer(&media_cd, 1, 0, 0, 0, false), EFI_SUCCESS); // zero-length ok
    assert_eq!(validate_transfer(&media_cd, 9, 0, 2048, 0x1000, false), EFI_MEDIA_CHANGED);
    assert_eq!(validate_transfer(&media_cd, 1, 0, 2048, 0x1000, true), EFI_WRITE_PROTECTED);
    assert_eq!(validate_transfer(&media_cd, 1, 0, 100, 0x1000, false), EFI_BAD_BUFFER_SIZE);
    assert_eq!(validate_transfer(&media_cd, 1, 64, 2048, 0x1000, false), EFI_INVALID_PARAMETER);
    assert_eq!(validate_transfer(&media_cd, 1, 63, 4096, 0x1000, false), EFI_INVALID_PARAMETER);
    assert_eq!(validate_transfer(&media_cd, 1, 63, 2048, 0x1000, false), EFI_SUCCESS);
    assert_eq!(validate_transfer(&media_cd, 1, u64::MAX, 2048, 0x1000, false), EFI_INVALID_PARAMETER);
    assert_eq!(validate_transfer(&media_cd, 1, 0, 2048, 0, false), EFI_INVALID_PARAMETER);
    assert_eq!(validate_transfer(&BlockMedia::cd(0), 1, 0, 2048, 0x1000, false), EFI_NO_MEDIA);
    assert_eq!(validate_transfer(&media_disk, 2, 0, 512, 0x1000, true), EFI_SUCCESS);

    // --- protocol database ------------------------------------------------
    let mut db = Protocols::new();
    assert_eq!(db.install(HANDLE_CD, GUID_BLOCK_IO, 0x9000), EFI_SUCCESS);
    assert_eq!(db.install(HANDLE_DISK, GUID_BLOCK_IO, 0x9100), EFI_SUCCESS);
    assert_eq!(db.interface_for(HANDLE_CD, &GUID_BLOCK_IO), Some(0x9000));
    assert_eq!(db.interface_for(HANDLE_DISK, &GUID_BLOCK_IO), Some(0x9100));
    assert_eq!(db.interface_for(HANDLE_CD, &GUID_SIMPLE_FILE_SYSTEM), None);
    assert_eq!(db.interface_for(0xDEAD, &GUID_BLOCK_IO), None);
    // Re-install replaces rather than duplicating.
    assert_eq!(db.install(HANDLE_CD, GUID_BLOCK_IO, 0x9200), EFI_SUCCESS);
    assert_eq!(db.interface_for(HANDLE_CD, &GUID_BLOCK_IO), Some(0x9200));
    assert_eq!(db.count_on_handle(HANDLE_CD), 1);
    assert_eq!(db.install(0, GUID_BLOCK_IO, 0x1), EFI_INVALID_PARAMETER);
    let mut hs = [0u64; MAX_HANDLES];
    assert_eq!(db.locate(BY_PROTOCOL, Some(&GUID_BLOCK_IO), &mut hs), 2);
    assert!(hs[..2].contains(&HANDLE_CD) && hs[..2].contains(&HANDLE_DISK));
    assert_eq!(db.locate(BY_PROTOCOL, Some(&GUID_SIMPLE_FILE_SYSTEM), &mut hs), 0);
    assert_eq!(db.locate(ALL_HANDLES, None, &mut hs), 2);
    assert_eq!(db.first_interface(&GUID_BLOCK_IO), Some(0x9200));

    // --- table layout: two BlockIo instances sharing trampolines ----------
    let base = 0u64;
    let mut mem = vec![0u8; SLAB as usize];
    let tb = 0x0080_0000usize;
    let layout = build_firmware_image(tb as u64, &mut mem[tb..tb + IMAGE_BYTES]).unwrap();
    write_block_media(&mut mem[tb..tb + IMAGE_BYTES], IMAGE_MEDIA_CD_OFF, &media_cd);
    write_block_media(&mut mem[tb..tb + IMAGE_BYTES], IMAGE_MEDIA_DISK_OFF, &media_disk);
    assert_eq!(layout.blockio_cd, tb as u64 + IMAGE_BLOCKIO_CD_OFF as u64);
    assert_eq!(layout.media_cd, tb as u64 + IMAGE_MEDIA_CD_OFF as u64);
    assert_ne!(layout.blockio_cd, layout.blockio_disk);
    let cd = tb + IMAGE_BLOCKIO_CD_OFF;
    let dk = tb + IMAGE_BLOCKIO_DISK_OFF;
    assert_eq!(get_u64(&mem, cd + BLOCKIO_REVISION_OFF), BLOCKIO_REVISION2);
    assert_eq!(get_u64(&mem, cd + BLOCKIO_MEDIA_OFF), layout.media_cd);
    assert_eq!(get_u64(&mem, dk + BLOCKIO_MEDIA_OFF), layout.media_disk);
    // Both instances point at the same shared trampolines.
    assert_eq!(get_u64(&mem, cd + BLOCKIO_READ_OFF), get_u64(&mem, dk + BLOCKIO_READ_OFF));
    assert_ne!(get_u64(&mem, cd + BLOCKIO_READ_OFF), get_u64(&mem, cd + BLOCKIO_WRITE_OFF));
    assert_eq!(
        get_u64(&mem, cd + BLOCKIO_READ_OFF),
        super::trampoline_slot_gpa(layout.trampolines, ServiceId::BlockIoReadBlocks)
    );
    // The GUID constant is in the image for a guest to point HandleProtocol at.
    assert_eq!(&mem[tb + IMAGE_GUID_BLOCK_IO_OFF..tb + IMAGE_GUID_BLOCK_IO_OFF + 16], &GUID_BLOCK_IO);

    // --- backing stores ---------------------------------------------------
    // CD: a plausible ISO9660 — "CD001" at LBA 16 offset 1.
    let mut iso = vec![0u8; iso_bytes as usize];
    iso[16 * 2048] = 1;
    iso[16 * 2048 + 1..16 * 2048 + 6].copy_from_slice(b"CD001");
    F4_CD.set(std::sync::Mutex::new(iso)).ok();
    F4_DISK.set(std::sync::Mutex::new(vec![0u8; disk_bytes as usize])).ok();

    let guest = MockGuest::new(base, mem);
    let mut sink = CaptureSink::default();
    let mut st = FirmwareState::new();
    let clk = ManualClock { now: Cell::new(1_000), step: 1_000 };
    st.media_cd = media_cd;
    st.media_disk = media_disk;
    st.blockio_cd = layout.blockio_cd;
    st.blockio_disk = layout.blockio_disk;
    st.read_blocks = Some(f4_read);
    st.write_blocks = Some(f4_write);
    assert_eq!(st.protocols.install(HANDLE_CD, GUID_BLOCK_IO, layout.blockio_cd), EFI_SUCCESS);
    assert_eq!(st.protocols.install(HANDLE_DISK, GUID_BLOCK_IO, layout.blockio_disk), EFI_SUCCESS);
    assert_eq!(st.media_for(layout.blockio_cd).map(|m| m.media_id), Some(super::MEDIA_ID_CD));
    assert_eq!(st.media_for(0xDEAD), None);

    let scratch = 0x00AF_0000u64;
    let stack = scratch + 0x2000;
    let p_iface = scratch;
    let p_guid = scratch + 0x40;
    let p_size = scratch + 0x50;
    let p_count = scratch + 0x58;
    let buf = scratch + 0x1000;
    guest.write(p_guid, &GUID_BLOCK_IO);

    // --- HandleProtocol / OpenProtocol / LocateProtocol -------------------
    guest.put_u64(p_iface, 0);
    let d = dispatch(ServiceId::HandleProtocol, ServiceArgs::regs(HANDLE_CD, p_guid, p_iface, 0), &guest, &mut sink, &mut st, &clk, SLAB);
    assert_eq!(d.status, EFI_SUCCESS);
    let bio_cd = guest.u64_at(p_iface);
    assert_eq!(bio_cd, layout.blockio_cd);
    // A guest can now follow This->Media and read geometry itself.
    assert_eq!(guest.u64_at(bio_cd + BLOCKIO_MEDIA_OFF as u64), layout.media_cd);
    let mut mid = [0u8; 4];
    guest.read(layout.media_cd + MEDIA_MEDIA_ID_OFF as u64, &mut mid);
    assert_eq!(u32::from_le_bytes(mid), super::MEDIA_ID_CD);
    // Unknown handle / unknown GUID.
    assert_eq!(dispatch(ServiceId::HandleProtocol, ServiceArgs::regs(0xDEAD, p_guid, p_iface, 0), &guest, &mut sink, &mut st, &clk, SLAB).status, EFI_NOT_FOUND);
    guest.write(p_guid + 0x20, &GUID_SIMPLE_FILE_SYSTEM);
    assert_eq!(dispatch(ServiceId::HandleProtocol, ServiceArgs::regs(HANDLE_CD, p_guid + 0x20, p_iface, 0), &guest, &mut sink, &mut st, &clk, SLAB).status, EFI_NOT_FOUND);
    // OpenProtocol yields the same interface; CloseProtocol succeeds.
    guest.put_u64(p_iface, 0);
    assert_eq!(dispatch(ServiceId::OpenProtocol, ServiceArgs::regs(HANDLE_DISK, p_guid, p_iface, 0), &guest, &mut sink, &mut st, &clk, SLAB).status, EFI_SUCCESS);
    assert_eq!(guest.u64_at(p_iface), layout.blockio_disk);
    assert_eq!(dispatch(ServiceId::CloseProtocol, ServiceArgs::regs(HANDLE_DISK, p_guid, 0, 0), &guest, &mut sink, &mut st, &clk, SLAB).status, EFI_SUCCESS);
    // LocateProtocol returns the first publisher.
    guest.put_u64(p_iface, 0);
    assert_eq!(dispatch(ServiceId::LocateProtocol, ServiceArgs::regs(p_guid, 0, p_iface, 0), &guest, &mut sink, &mut st, &clk, SLAB).status, EFI_SUCCESS);
    assert_ne!(guest.u64_at(p_iface), 0);

    // --- LocateHandle: sizing then fill -----------------------------------
    guest.put_u64(stack + STACK_ARG5_OFF, 0); // Buffer = NULL
    guest.put_u64(p_size, 0);
    let args = ServiceArgs { a1: BY_PROTOCOL as u64, a2: p_guid, a3: 0, a4: p_size, rsp: stack };
    assert_eq!(dispatch(ServiceId::LocateHandle, args, &guest, &mut sink, &mut st, &clk, SLAB).status, EFI_BUFFER_TOO_SMALL);
    assert_eq!(guest.u64_at(p_size), 16); // two handles
    guest.put_u64(stack + STACK_ARG5_OFF, buf);
    guest.put_u64(p_size, 16);
    let d = dispatch(ServiceId::LocateHandle, args, &guest, &mut sink, &mut st, &clk, SLAB);
    assert_eq!(d.status, EFI_SUCCESS);
    let h0 = guest.u64_at(buf);
    let h1 = guest.u64_at(buf + 8);
    assert!([h0, h1].contains(&HANDLE_CD) && [h0, h1].contains(&HANDLE_DISK));
    // ByRegisterNotify is honestly unsupported.
    let args_n = ServiceArgs { a1: BY_REGISTER_NOTIFY as u64, a2: p_guid, a3: 0, a4: p_size, rsp: stack };
    assert_eq!(dispatch(ServiceId::LocateHandle, args_n, &guest, &mut sink, &mut st, &clk, SLAB).status, EFI_UNSUPPORTED);

    // --- LocateHandleBuffer allocates from our pool -----------------------
    guest.put_u64(stack + STACK_ARG5_OFF, p_iface);
    let free_before = st.pool.free_pages();
    let args = ServiceArgs { a1: BY_PROTOCOL as u64, a2: p_guid, a3: 0, a4: p_count, rsp: stack };
    let d = dispatch(ServiceId::LocateHandleBuffer, args, &guest, &mut sink, &mut st, &clk, SLAB);
    assert_eq!(d.status, EFI_SUCCESS);
    assert_eq!(guest.u64_at(p_count), 2);
    let arr = guest.u64_at(p_iface);
    assert_eq!(arr & 0xfff, POOL_HEADER_BYTES);
    assert!(st.pool.free_pages() < free_before);
    let a0 = guest.u64_at(arr);
    assert!([HANDLE_CD, HANDLE_DISK].contains(&a0));
    // FreePool releases it.
    assert_eq!(dispatch(ServiceId::FreePool, ServiceArgs::regs(arr, 0, 0, 0), &guest, &mut sink, &mut st, &clk, SLAB).status, EFI_SUCCESS);
    assert_eq!(st.pool.free_pages(), free_before);

    // --- InstallProtocolInterface (a loader publishing its own) -----------
    let p_handle = scratch + 0x80; // must not collide with p_guid+0x20
    guest.put_u64(p_handle, HANDLE_CD);
    guest.write(p_guid + 0x20, &GUID_SIMPLE_FILE_SYSTEM);
    let d = dispatch(ServiceId::InstallProtocolInterface, ServiceArgs::regs(p_handle, p_guid + 0x20, 0, 0xABCD_0000), &guest, &mut sink, &mut st, &clk, SLAB);
    assert_eq!(d.status, EFI_SUCCESS);
    guest.put_u64(p_iface, 0);
    assert_eq!(dispatch(ServiceId::HandleProtocol, ServiceArgs::regs(HANDLE_CD, p_guid + 0x20, p_iface, 0), &guest, &mut sink, &mut st, &clk, SLAB).status, EFI_SUCCESS);
    assert_eq!(guest.u64_at(p_iface), 0xABCD_0000);
    // Non-native interface types are rejected.
    assert_eq!(dispatch(ServiceId::InstallProtocolInterface, ServiceArgs::regs(p_handle, p_guid + 0x20, 1, 0x1), &guest, &mut sink, &mut st, &clk, SLAB).status, EFI_INVALID_PARAMETER);

    // --- BlockIo.ReadBlocks: the ISO9660 PVD at LBA 16 --------------------
    guest.put_u64(stack + STACK_ARG5_OFF, buf);
    let args = ServiceArgs { a1: bio_cd, a2: super::MEDIA_ID_CD as u64, a3: 16, a4: 2048, rsp: stack };
    let d = dispatch(ServiceId::BlockIoReadBlocks, args, &guest, &mut sink, &mut st, &clk, SLAB);
    assert_eq!(d.status, EFI_SUCCESS);
    assert!(d.block_io_ok);
    assert_eq!(st.block_reads, 1);
    let mut sig = [0u8; 6];
    guest.read(buf, &mut sig);
    assert_eq!(&sig, b"\x01CD001"); // volume descriptor type 1 + magic
    // Writing the CD is refused by media, not by the backend.
    let d = dispatch(ServiceId::BlockIoWriteBlocks, args, &guest, &mut sink, &mut st, &clk, SLAB);
    assert_eq!(d.status, EFI_WRITE_PROTECTED);
    assert!(!d.block_io_ok);
    // Wrong media id / past the end / bad size.
    let bad = ServiceArgs { a1: bio_cd, a2: 99, a3: 16, a4: 2048, rsp: stack };
    assert_eq!(dispatch(ServiceId::BlockIoReadBlocks, bad, &guest, &mut sink, &mut st, &clk, SLAB).status, EFI_MEDIA_CHANGED);
    let bad = ServiceArgs { a1: bio_cd, a2: super::MEDIA_ID_CD as u64, a3: 64, a4: 2048, rsp: stack };
    assert_eq!(dispatch(ServiceId::BlockIoReadBlocks, bad, &guest, &mut sink, &mut st, &clk, SLAB).status, EFI_INVALID_PARAMETER);
    let bad = ServiceArgs { a1: bio_cd, a2: super::MEDIA_ID_CD as u64, a3: 0, a4: 999, rsp: stack };
    assert_eq!(dispatch(ServiceId::BlockIoReadBlocks, bad, &guest, &mut sink, &mut st, &clk, SLAB).status, EFI_BAD_BUFFER_SIZE);
    // A bogus `This` is rejected.
    let bad = ServiceArgs { a1: 0xDEAD, a2: 1, a3: 0, a4: 512, rsp: stack };
    assert_eq!(dispatch(ServiceId::BlockIoReadBlocks, bad, &guest, &mut sink, &mut st, &clk, SLAB).status, EFI_INVALID_PARAMETER);

    // --- BlockIo write/read round-trip on the install disk ----------------
    // This is the path a real installer's GPT write will take.
    let gpt = b"EFI PART";
    guest.write(buf, gpt);
    let wargs = ServiceArgs { a1: layout.blockio_disk, a2: super::MEDIA_ID_DISK as u64, a3: 1, a4: 512, rsp: stack };
    let d = dispatch(ServiceId::BlockIoWriteBlocks, wargs, &guest, &mut sink, &mut st, &clk, SLAB);
    assert_eq!(d.status, EFI_SUCCESS);
    assert!(d.block_io_ok);
    assert_eq!(st.block_writes, 1);
    // It really landed in the backing store at LBA 1 = byte 512.
    assert_eq!(&F4_DISK.get().unwrap().lock().unwrap()[512..520], gpt);
    // Read it back through the protocol into a different buffer.
    guest.put_u64(stack + STACK_ARG5_OFF, buf + 0x800);
    let rargs = ServiceArgs { a1: layout.blockio_disk, a2: super::MEDIA_ID_DISK as u64, a3: 1, a4: 512, rsp: stack };
    assert_eq!(dispatch(ServiceId::BlockIoReadBlocks, rargs, &guest, &mut sink, &mut st, &clk, SLAB).status, EFI_SUCCESS);
    let mut back = [0u8; 8];
    guest.read(buf + 0x800, &mut back);
    assert_eq!(&back, gpt);
    // Multi-block transfer spanning our 4096-byte chunking.
    guest.put_u64(stack + STACK_ARG5_OFF, buf);
    let many = ServiceArgs { a1: layout.blockio_disk, a2: super::MEDIA_ID_DISK as u64, a3: 0, a4: 512 * 20, rsp: stack };
    assert_eq!(dispatch(ServiceId::BlockIoReadBlocks, many, &guest, &mut sink, &mut st, &clk, SLAB).status, EFI_SUCCESS);
    // Reset / Flush.
    assert_eq!(dispatch(ServiceId::BlockIoReset, ServiceArgs::regs(bio_cd, 0, 0, 0), &guest, &mut sink, &mut st, &clk, SLAB).status, EFI_SUCCESS);
    assert_eq!(dispatch(ServiceId::BlockIoFlushBlocks, ServiceArgs::regs(layout.blockio_disk, 0, 0, 0), &guest, &mut sink, &mut st, &clk, SLAB).status, EFI_SUCCESS);
    assert_eq!(dispatch(ServiceId::BlockIoFlushBlocks, ServiceArgs::regs(0xDEAD, 0, 0, 0), &guest, &mut sink, &mut st, &clk, SLAB).status, EFI_INVALID_PARAMETER);

    // --- still honestly unsupported ---------------------------------------
    let a0 = ServiceArgs::regs(0, 0, 0, 0);
    for id in [ServiceId::RegisterProtocolNotify, ServiceId::ConnectController, ServiceId::ProtocolsPerHandle] {
        assert_eq!(dispatch(id, a0, &guest, &mut sink, &mut st, &clk, SLAB).status, EFI_UNSUPPORTED, "{}", id.name());
    }
    // Exit is implemented (F6-prep c) but with no started image it is a
    // parameter error, never a silent success.
    assert_eq!(dispatch(ServiceId::Exit, a0, &guest, &mut sink, &mut st, &clk, SLAB).status, EFI_INVALID_PARAMETER);

    #[cfg(not(target_os = "uefi"))]
    println!("{RAYNU_F_BLOCKIO_GATE_MARKER}");
}

/// Build a real FAT12 volume containing `\EFI\BOOT\BOOTX64.EFI`.
/// Geometry: 512 B sectors, 1 sector/cluster, 1 FAT, 64 root entries.
fn build_fat12_esp(payload: &[u8]) -> Vec<u8> {
    const SEC: usize = 512;
    const TOTAL: usize = 512; // 512 sectors = 256 KiB
    const RESERVED: usize = 1;
    const FAT_SECS: usize = 2;
    const ROOT_ENTS: usize = 64;
    let root_secs = ROOT_ENTS * 32 / SEC; // 4
    let mut v = vec![0u8; TOTAL * SEC];
    // BPB
    v[0] = 0xEB;
    v[1] = 0x3C;
    v[2] = 0x90;
    v[3..11].copy_from_slice(b"RAYNUF  ");
    v[11..13].copy_from_slice(&(SEC as u16).to_le_bytes());
    v[13] = 1; // sectors per cluster
    v[14..16].copy_from_slice(&(RESERVED as u16).to_le_bytes());
    v[16] = 1; // num FATs
    v[17..19].copy_from_slice(&(ROOT_ENTS as u16).to_le_bytes());
    v[19..21].copy_from_slice(&(TOTAL as u16).to_le_bytes());
    v[21] = 0xF8;
    v[22..24].copy_from_slice(&(FAT_SECS as u16).to_le_bytes());
    v[510] = 0x55;
    v[511] = 0xAA;

    let fat_start = RESERVED;
    let root_start = fat_start + FAT_SECS;
    let data_start = root_start + root_secs;

    // FAT12 entry writer.
    let set_fat = |v: &mut Vec<u8>, cl: usize, val: u16| {
        let off = fat_start * SEC + cl * 3 / 2;
        let cur = u16::from_le_bytes([v[off], v[off + 1]]);
        let new = if cl & 1 == 0 {
            (cur & 0xF000) | (val & 0x0FFF)
        } else {
            (cur & 0x000F) | (val << 4)
        };
        v[off..off + 2].copy_from_slice(&new.to_le_bytes());
    };
    set_fat(&mut v, 0, 0xFF8);
    set_fat(&mut v, 1, 0xFFF);

    // 8.3 directory entry writer.
    let mk_ent = |name: &[u8; 11], attr: u8, cluster: u16, size: u32| {
        let mut e = [0u8; 32];
        e[..11].copy_from_slice(name);
        e[11] = attr;
        e[26..28].copy_from_slice(&cluster.to_le_bytes());
        e[28..32].copy_from_slice(&size.to_le_bytes());
        e
    };

    // Cluster 2 = \EFI dir, cluster 3 = \EFI\BOOT dir, clusters 4.. = file.
    let cl_efi = 2usize;
    let cl_boot = 3usize;
    let cl_file = 4usize;
    // Root: EFI <DIR> -> cluster 2
    let root_off = root_start * SEC;
    v[root_off..root_off + 32].copy_from_slice(&mk_ent(b"EFI        ", 0x10, cl_efi as u16, 0));
    set_fat(&mut v, cl_efi, 0xFFF);
    // \EFI: ".", "..", BOOT <DIR> -> cluster 3
    let efi_off = (data_start + cl_efi - 2) * SEC;
    v[efi_off..efi_off + 32].copy_from_slice(&mk_ent(b".          ", 0x10, cl_efi as u16, 0));
    v[efi_off + 32..efi_off + 64].copy_from_slice(&mk_ent(b"..         ", 0x10, 0, 0));
    v[efi_off + 64..efi_off + 96].copy_from_slice(&mk_ent(b"BOOT       ", 0x10, cl_boot as u16, 0));
    set_fat(&mut v, cl_boot, 0xFFF);
    // \EFI\BOOT: ".", "..", BOOTX64.EFI -> cluster 4, size = payload
    let boot_off = (data_start + cl_boot - 2) * SEC;
    v[boot_off..boot_off + 32].copy_from_slice(&mk_ent(b".          ", 0x10, cl_boot as u16, 0));
    v[boot_off + 32..boot_off + 64].copy_from_slice(&mk_ent(b"..         ", 0x10, cl_efi as u16, 0));
    v[boot_off + 64..boot_off + 96].copy_from_slice(&mk_ent(
        b"BOOTX64 EFI",
        0x20,
        cl_file as u16,
        payload.len() as u32,
    ));
    // Payload across a multi-cluster chain (512 B clusters exercise it).
    let nclusters = (payload.len() + SEC - 1) / SEC;
    for i in 0..nclusters {
        let cl = cl_file + i;
        let off = (data_start + cl - 2) * SEC;
        let start = i * SEC;
        let end = (start + SEC).min(payload.len());
        v[off..off + (end - start)].copy_from_slice(&payload[start..end]);
        let next = if i + 1 == nclusters { 0xFFF } else { (cl + 1) as u16 };
        set_fat(&mut v, cl, next);
    }
    v
}

struct VecVol(Vec<u8>);
impl super::fat::VolumeRead for VecVol {
    fn read_at(&self, off: u64, buf: &mut [u8]) -> bool {
        let s = off as usize;
        if s + buf.len() > self.0.len() {
            return false;
        }
        buf.copy_from_slice(&self.0[s..s + buf.len()]);
        true
    }
}

#[test]
fn raynu_f_fat_and_simple_filesystem() {
    use super::fat::{
        parse_bpb, resolve_path, FatError, FatKind, ATTR_DIRECTORY, ATTR_LONG_NAME,
        DIR_ENTRY_FREE, DIR_ENTRY_SIZE,
    };
    use super::filesystem::{
        utf16_path_to_ascii, FileSystem, EFI_FILE_DIRECTORY, EFI_FILE_MODE_CREATE,
        EFI_FILE_MODE_READ, EFI_FILE_MODE_WRITE, FILE_INFO_ATTRIBUTE_OFF,
        FILE_INFO_FILE_SIZE_OFF, FILE_INFO_NAME_OFF, FILE_INFO_SIZE_OFF, FILE_REVISION,
        FILE_SIZE, GUID_FILE_INFO, MAX_PATH_BYTES, SFS_REVISION, SFS_SIZE,
    };
    use super::testapp::{build_test_app, TESTAPP_FILE_BYTES};

    const EFI_SUCCESS: u64 = 0;
    const EFI_NOT_FOUND: u64 = 0x8000_0000_0000_000E;
    const EFI_UNSUPPORTED: u64 = 0x8000_0000_0000_0003;
    const EFI_WRITE_PROTECTED: u64 = 0x8000_0000_0000_0008;
    const EFI_INVALID_PARAMETER: u64 = 0x8000_0000_0000_0002;
    const EFI_BUFFER_TOO_SMALL: u64 = 0x8000_0000_0000_0005;

    // Spec-shape constants.
    assert_eq!(SFS_SIZE, 0x10);
    assert_eq!(FILE_SIZE, 0x58);
    assert_eq!(SFS_REVISION, 0x0001_0000);
    assert_eq!(FILE_REVISION, 0x0001_0000);
    assert_eq!(FILE_INFO_NAME_OFF, 0x50);
    // EFI_FILE_INFO_ID {09576E92-6D3F-11D2-8E39-00A0C969723B}
    assert_eq!(u32::from_le_bytes(GUID_FILE_INFO[0..4].try_into().unwrap()), 0x0957_6E92);
    assert_eq!(&GUID_FILE_INFO[8..], &[0x8E, 0x39, 0x00, 0xA0, 0xC9, 0x69, 0x72, 0x3B]);

    // The payload is a real PE32+ so F5b can LoadImage it straight off the FS.
    let mut app = vec![0u8; TESTAPP_FILE_BYTES];
    build_test_app(&mut app).unwrap();
    let vol_bytes = build_fat12_esp(&app);
    let vol = VecVol(vol_bytes);

    // --- BPB ------------------------------------------------------------
    let mut boot = [0u8; 512];
    assert!(super::fat::VolumeRead::read_at(&vol, 0, &mut boot));
    let fv = parse_bpb(&boot).expect("BPB parses");
    assert_eq!(fv.kind, FatKind::Fat12); // 512-sector volume => FAT12
    assert_eq!(fv.bytes_per_sector, 512);
    assert_eq!(fv.sectors_per_cluster, 1);
    assert_eq!(fv.num_fats, 1);
    assert_eq!(fv.root_entries, 64);
    assert_eq!(fv.fat_start, 1);
    assert_eq!(fv.root_start, 3);
    assert_eq!(fv.root_sectors, 4);
    assert_eq!(fv.data_start, 7);
    assert_eq!(fv.cluster_bytes(), 512);
    assert!(fv.cluster_is_valid(2) && !fv.cluster_is_valid(1));
    assert!(fv.is_end_of_chain(0xFFF) && !fv.is_end_of_chain(3));
    // Rejects junk and impossible geometry.
    assert_eq!(parse_bpb(&[0u8; 16]), Err(FatError::ShortRead));
    assert_eq!(parse_bpb(&[0u8; 512]), Err(FatError::BadBpb));

    // --- short-name rendering + entry decode -----------------------------
    let (n, l) = super::fat::short_name(b"BOOTX64 EFI");
    assert_eq!(&n[..l], b"BOOTX64.EFI");
    let (n, l) = super::fat::short_name(b"EFI        ");
    assert_eq!(&n[..l], b"EFI");
    // LFN slots and deleted entries are skipped, not misread as files.
    let mut lfn = [0u8; DIR_ENTRY_SIZE];
    lfn[11] = ATTR_LONG_NAME;
    assert!(super::fat::parse_dir_entry(&lfn).is_none());
    let mut del = [0u8; DIR_ENTRY_SIZE];
    del[0] = DIR_ENTRY_FREE;
    assert!(super::fat::parse_dir_entry(&del).is_none());
    assert!(super::fat::parse_dir_entry(&[0u8; DIR_ENTRY_SIZE]).is_none());

    // --- path resolution -------------------------------------------------
    let e = resolve_path(&fv, &vol, b"\\EFI\\BOOT\\BOOTX64.EFI").expect("resolves");
    assert_eq!(e.name_bytes(), b"BOOTX64.EFI");
    assert_eq!(e.size as usize, app.len());
    assert!(!e.is_dir());
    // Forward slashes, duplicate and missing leading separators all work.
    assert_eq!(resolve_path(&fv, &vol, b"/EFI/BOOT/BOOTX64.EFI").unwrap().size, e.size);
    assert_eq!(resolve_path(&fv, &vol, b"EFI\\\\BOOT\\BOOTX64.EFI").unwrap().size, e.size);
    // Case-insensitive (FAT semantics).
    assert_eq!(resolve_path(&fv, &vol, b"\\efi\\boot\\bootx64.efi").unwrap().size, e.size);
    // Directories resolve as directories.
    let d = resolve_path(&fv, &vol, b"\\EFI").unwrap();
    assert!(d.is_dir() && d.attr & ATTR_DIRECTORY != 0);
    // Honest failures.
    assert_eq!(resolve_path(&fv, &vol, b"\\EFI\\BOOT\\NOPE.EFI"), Err(FatError::NotFound));
    assert_eq!(resolve_path(&fv, &vol, b"\\NOPE\\X"), Err(FatError::NotFound));
    // A file used as a directory component is not silently accepted.
    assert_eq!(
        resolve_path(&fv, &vol, b"\\EFI\\BOOT\\BOOTX64.EFI\\X"),
        Err(FatError::NotADirectory)
    );
    assert_eq!(resolve_path(&fv, &vol, b""), Err(FatError::NotFound));

    // --- cluster-chain read matches the payload byte for byte -------------
    let mut whole = vec![0u8; app.len()];
    let got = super::fat::read_chain(&fv, &vol, e.first_cluster, 0, &mut whole).unwrap();
    assert_eq!(got, app.len());
    assert_eq!(whole, app);
    // The chain really spans multiple clusters (512 B each).
    assert!(app.len() > 512, "payload must exceed one cluster to prove chaining");
    // Read at an offset that starts mid-chain.
    let mut mid = [0u8; 64];
    super::fat::read_chain(&fv, &vol, e.first_cluster, 600, &mut mid).unwrap();
    assert_eq!(&mid[..], &app[600..664]);
    // Offset past EOF yields nothing rather than garbage.
    let mut none = [0u8; 16];
    assert_eq!(
        super::fat::read_chain(&fv, &vol, e.first_cluster, 1 << 20, &mut none).unwrap(),
        0
    );

    // --- SimpleFileSystem / EFI_FILE_PROTOCOL ----------------------------
    let mut fs = FileSystem::new();
    assert!(!fs.mounted());
    // Unmounted volume refuses OpenVolume.
    assert_eq!(fs.open_volume().0, EFI_NOT_FOUND);
    fs.volume = Some(fv);
    assert!(fs.mounted());
    let (st, root) = fs.open_volume();
    assert_eq!(st, EFI_SUCCESS);
    assert_ne!(root, 0);
    assert_eq!(fs.is_directory(root), Some(true));
    assert_eq!(fs.open_count(), 1);

    // Open the bootloader read-only.
    let (st, fh) = fs.open(root, b"\\EFI\\BOOT\\BOOTX64.EFI", EFI_FILE_MODE_READ, &vol);
    assert_eq!(st, EFI_SUCCESS);
    assert_eq!(fs.size_of(fh), Some(app.len() as u64));
    assert_eq!(fs.is_directory(fh), Some(false));
    assert_eq!(fs.position(fh), Some(0));
    // Write / create modes are refused honestly on a read-only volume.
    assert_eq!(fs.open(root, b"\\X", EFI_FILE_MODE_READ | EFI_FILE_MODE_WRITE, &vol).0, EFI_WRITE_PROTECTED);
    assert_eq!(fs.open(root, b"\\X", EFI_FILE_MODE_CREATE | EFI_FILE_MODE_READ, &vol).0, EFI_WRITE_PROTECTED);
    assert_eq!(fs.open(root, b"\\EFI", 0, &vol).0, EFI_INVALID_PARAMETER);
    assert_eq!(fs.open(0xDEAD, b"\\EFI", EFI_FILE_MODE_READ, &vol).0, EFI_INVALID_PARAMETER);
    assert_eq!(fs.open(root, b"\\NOPE", EFI_FILE_MODE_READ, &vol).0, EFI_NOT_FOUND);

    // Sequential reads reassemble the whole PE, and the position advances.
    let mut acc = Vec::new();
    let mut chunk = [0u8; 100];
    loop {
        let (st, n) = fs.read(fh, &mut chunk, &vol);
        assert_eq!(st, EFI_SUCCESS);
        if n == 0 {
            break;
        }
        acc.extend_from_slice(&chunk[..n]);
    }
    assert_eq!(acc, app);
    assert_eq!(fs.position(fh), Some(app.len() as u64));
    // It is a genuine PE32+ that our F2a loader accepts.
    assert!(super::parse_pe32plus(&acc).is_ok());

    // SetPosition: rewind, seek to EOF (u64::MAX), and re-read.
    assert_eq!(fs.set_position(fh, 0), EFI_SUCCESS);
    assert_eq!(fs.position(fh), Some(0));
    let mut two = [0u8; 2];
    let (st, n) = fs.read(fh, &mut two, &vol);
    assert_eq!((st, n), (EFI_SUCCESS, 2));
    assert_eq!(&two, b"MZ");
    assert_eq!(fs.set_position(fh, u64::MAX), EFI_SUCCESS);
    assert_eq!(fs.position(fh), Some(app.len() as u64));
    assert_eq!(fs.read(fh, &mut two, &vol), (EFI_SUCCESS, 0)); // EOF, not an error
    // Directories: rewind only, and Read is honestly unsupported.
    assert_eq!(fs.set_position(root, 0), EFI_SUCCESS);
    assert_eq!(fs.set_position(root, 8), EFI_UNSUPPORTED);
    assert_eq!(fs.read(root, &mut two, &vol).0, EFI_UNSUPPORTED);

    // GetInfo(EFI_FILE_INFO): sizing then fill.
    let mut small = [0u8; 8];
    let (st, need) = fs.file_info(fh, &mut small);
    assert_eq!(st, EFI_BUFFER_TOO_SMALL);
    assert_eq!(need, FILE_INFO_NAME_OFF as u64 + (b"BOOTX64.EFI".len() as u64 + 1) * 2);
    let mut info = vec![0u8; need as usize];
    let (st, wrote) = fs.file_info(fh, &mut info);
    assert_eq!((st, wrote), (EFI_SUCCESS, need));
    assert_eq!(u64::from_le_bytes(info[FILE_INFO_SIZE_OFF..8].try_into().unwrap()), need);
    assert_eq!(
        u64::from_le_bytes(info[FILE_INFO_FILE_SIZE_OFF..FILE_INFO_FILE_SIZE_OFF + 8].try_into().unwrap()),
        app.len() as u64
    );
    assert_eq!(
        u64::from_le_bytes(info[FILE_INFO_ATTRIBUTE_OFF..FILE_INFO_ATTRIBUTE_OFF + 8].try_into().unwrap())
            & EFI_FILE_DIRECTORY,
        0
    );
    // FileName is CHAR16 "BOOTX64.EFI\0".
    let name: Vec<u8> = info[FILE_INFO_NAME_OFF..]
        .chunks(2)
        .take_while(|c| c[0] != 0 || c[1] != 0)
        .map(|c| c[0])
        .collect();
    assert_eq!(name, b"BOOTX64.EFI");
    // Root reports itself as a directory.
    let mut rinfo = [0u8; 128];
    let (st, _) = fs.file_info(root, &mut rinfo);
    assert_eq!(st, EFI_SUCCESS);
    assert_ne!(
        u64::from_le_bytes(rinfo[FILE_INFO_ATTRIBUTE_OFF..FILE_INFO_ATTRIBUTE_OFF + 8].try_into().unwrap())
            & EFI_FILE_DIRECTORY,
        0
    );

    // Close releases the slot; the handle then becomes invalid.
    let open_before = fs.open_count();
    assert_eq!(fs.close(fh), EFI_SUCCESS);
    assert_eq!(fs.open_count(), open_before - 1);
    assert_eq!(fs.close(fh), EFI_INVALID_PARAMETER);
    assert_eq!(fs.size_of(fh), None);
    assert!(fs.file_reads > 0);

    // --- guest CHAR16 path conversion -------------------------------------
    let mut buf = [0u8; MAX_PATH_BYTES];
    let units: Vec<u16> = "\\EFI\\BOOT\\BOOTX64.EFI\0".encode_utf16().collect();
    let n = utf16_path_to_ascii(&units, &mut buf).unwrap();
    assert_eq!(&buf[..n], b"\\EFI\\BOOT\\BOOTX64.EFI");
    // Non-ASCII is rejected rather than silently mangled.
    assert!(utf16_path_to_ascii(&[0x2603, 0], &mut buf).is_none());
    let long: Vec<u16> = core::iter::repeat(b'A' as u16).take(MAX_PATH_BYTES + 1).collect();
    assert!(utf16_path_to_ascii(&long, &mut buf).is_none());

    #[cfg(not(target_os = "uefi"))]
    println!("{}", super::RAYNU_F_FS_GATE_MARKER);
}

/// F5b: the CD backing store is an ISO whose El Torito boot image is the FAT
/// ESP built by `build_fat12_esp`. `F5_CD_FAT_OFF` is that extent's offset.
const F5_CD_FAT_OFF: u64 = 64 * 2048; // LBA 64
static F5_CD: std::sync::OnceLock<std::sync::Mutex<Vec<u8>>> = std::sync::OnceLock::new();

fn f5_read(media_id: u32, off: u64, buf: &mut [u8]) -> bool {
    if media_id != super::MEDIA_ID_CD {
        return false;
    }
    let g = F5_CD.get().unwrap().lock().unwrap();
    let s = off as usize;
    if s + buf.len() > g.len() {
        return false;
    }
    buf.copy_from_slice(&g[s..s + buf.len()]);
    true
}

#[test]
fn raynu_f_filesystem_loadimage_startimage() {
    use super::filesystem::{
        EFI_FILE_MODE_READ, EFI_FILE_MODE_WRITE, FILE_INFO_FILE_SIZE_OFF, FILE_SLOTS,
        GUID_FILE_INFO, SFS_OPEN_VOLUME_OFF, SFS_REVISION, SFS_REVISION_OFF,
    };
    use super::protocol::{
        GUID_LOADED_IMAGE, HANDLE_IMAGE, LOADED_IMAGE_IMAGE_BASE_OFF,
        LOADED_IMAGE_IMAGE_SIZE_OFF, LOADED_IMAGE_REVISION, LOADED_IMAGE_REVISION_OFF,
        LOADED_IMAGE_SYSTEM_TABLE_OFF,
    };
    use super::services::{
        dispatch, FirmwareState, ServiceArgs, ServiceId, EFI_INVALID_PARAMETER, EFI_SUCCESS,
        EFI_UNSUPPORTED, STACK_ARG5_OFF,
    };
    use super::tables::{
        build_firmware_image, get_u64, IMAGE_BYTES, IMAGE_FILE_PROTO_OFF,
        IMAGE_FILE_PROTO_STRIDE, IMAGE_GUID_FILE_INFO_OFF, IMAGE_SFS_OFF,
        IMAGE_SYSTEM_TABLE_OFF,
    };
    use super::testapp::{build_test_app, TESTAPP_FILE_BYTES};

    const EFI_WRITE_PROTECTED: u64 = 0x8000_0000_0000_0008;
    const EFI_NOT_FOUND: u64 = 0x8000_0000_0000_000E;

    // Build an "ISO" whose El Torito boot image is our FAT12 ESP.
    let mut app = vec![0u8; TESTAPP_FILE_BYTES];
    build_test_app(&mut app).unwrap();
    let esp = build_fat12_esp(&app);
    let mut iso = vec![0u8; F5_CD_FAT_OFF as usize + esp.len() + 2048];
    iso[16 * 2048] = 1;
    iso[16 * 2048 + 1..16 * 2048 + 6].copy_from_slice(b"CD001");
    iso[F5_CD_FAT_OFF as usize..F5_CD_FAT_OFF as usize + esp.len()].copy_from_slice(&esp);
    let iso_len = iso.len() as u64;
    F5_CD.set(std::sync::Mutex::new(iso)).ok();

    // Guest slab with our tables.
    let base = 0u64;
    let mut mem = vec![0u8; SLAB as usize];
    let tb = 0x0080_0000usize;
    let layout = build_firmware_image(tb as u64, &mut mem[tb..tb + IMAGE_BYTES]).unwrap();
    let guest = MockGuest::new(base, mem);
    let mut sink = CaptureSink::default();
    let clk = ManualClock { now: Cell::new(1_000), step: 1_000 };
    let mut st = FirmwareState::new();

    // Mount: CD BlockIo backing + FAT volume at the El Torito extent.
    st.read_blocks = Some(f5_read);
    st.media_cd = super::BlockMedia::cd(iso_len);
    st.blockio_cd = layout.blockio_cd;
    st.fat_volume_off = F5_CD_FAT_OFF;
    st.file_proto_base = layout.file_proto_base;
    st.sfs = layout.sfs;
    st.loaded_image_proto = layout.loaded_image;
    st.system_table = layout.system_table;
    // Parse the BPB off the CD the way the launcher will.
    let mut boot = [0u8; 512];
    assert!(f5_read(super::MEDIA_ID_CD, F5_CD_FAT_OFF, &mut boot));
    st.fs.volume = Some(super::fat::parse_bpb(&boot).expect("BPB off the CD"));
    let _ = st.protocols.install(super::HANDLE_CD, super::protocol::GUID_SIMPLE_FILE_SYSTEM, layout.sfs);

    // --- table shapes -----------------------------------------------------
    assert_eq!(layout.sfs, tb as u64 + IMAGE_SFS_OFF as u64);
    assert_eq!(get_u64(&guest.mem.borrow(), tb + IMAGE_SFS_OFF + SFS_REVISION_OFF), SFS_REVISION);
    assert_eq!(
        get_u64(&guest.mem.borrow(), tb + IMAGE_SFS_OFF + SFS_OPEN_VOLUME_OFF),
        super::trampoline_slot_gpa(layout.trampolines, ServiceId::SfsOpenVolume)
    );
    assert_eq!(&guest.mem.borrow()[tb + IMAGE_GUID_FILE_INFO_OFF..tb + IMAGE_GUID_FILE_INFO_OFF + 16], &GUID_FILE_INFO);
    // Every file slot has its own EFI_FILE_PROTOCOL, all sharing trampolines.
    let f0 = tb + IMAGE_FILE_PROTO_OFF;
    let f1 = f0 + IMAGE_FILE_PROTO_STRIDE;
    assert_eq!(
        get_u64(&guest.mem.borrow(), f0 + super::filesystem::FILE_READ_OFF),
        get_u64(&guest.mem.borrow(), f1 + super::filesystem::FILE_READ_OFF)
    );
    assert_eq!(layout.file_proto_base, f0 as u64);
    // The array fits inside the image.
    assert!(IMAGE_FILE_PROTO_OFF + FILE_SLOTS * IMAGE_FILE_PROTO_STRIDE <= IMAGE_BYTES);

    let scratch = 0x00AF_0000u64;
    let stack = scratch + 0x4000;
    let p_root = scratch;
    let p_file = scratch + 8;
    let p_size = scratch + 16;
    let p_pos = scratch + 24;
    let p_handle = scratch + 32;
    let p_guid = scratch + 0x100;
    let path_gpa = scratch + 0x200;
    let buf = scratch + 0x1000;

    // Guest writes the CHAR16 path, as a real loader would.
    let units: Vec<u16> = "\\EFI\\BOOT\\BOOTX64.EFI\0".encode_utf16().collect();
    for (i, u) in units.iter().enumerate() {
        guest.write(path_gpa + i as u64 * 2, &u.to_le_bytes());
    }

    // --- OpenVolume -------------------------------------------------------
    let d = dispatch(ServiceId::SfsOpenVolume, ServiceArgs::regs(layout.sfs, p_root, 0, 0), &guest, &mut sink, &mut st, &clk, SLAB);
    assert_eq!(d.status, EFI_SUCCESS);
    let root = guest.u64_at(p_root);
    assert_eq!(root, layout.file_proto_base); // first slot
    // A bogus `This` is rejected.
    assert_eq!(dispatch(ServiceId::SfsOpenVolume, ServiceArgs::regs(0xDEAD, p_root, 0, 0), &guest, &mut sink, &mut st, &clk, SLAB).status, EFI_INVALID_PARAMETER);

    // --- Open the bootloader ---------------------------------------------
    guest.put_u64(stack + STACK_ARG5_OFF, 0); // Attributes
    let args = ServiceArgs { a1: root, a2: p_file, a3: path_gpa, a4: EFI_FILE_MODE_READ, rsp: stack };
    let d = dispatch(ServiceId::FileOpen, args, &guest, &mut sink, &mut st, &clk, SLAB);
    assert_eq!(d.status, EFI_SUCCESS);
    let fh = guest.u64_at(p_file);
    assert_ne!(fh, root);
    assert_eq!((fh - layout.file_proto_base) % IMAGE_FILE_PROTO_STRIDE as u64, 0);
    // Write mode on the read-only CD is refused honestly.
    let wargs = ServiceArgs { a1: root, a2: p_file, a3: path_gpa, a4: EFI_FILE_MODE_READ | EFI_FILE_MODE_WRITE, rsp: stack };
    assert_eq!(dispatch(ServiceId::FileOpen, wargs, &guest, &mut sink, &mut st, &clk, SLAB).status, EFI_WRITE_PROTECTED);

    // --- GetInfo(EFI_FILE_INFO) reports the real size ---------------------
    guest.write(p_guid, &GUID_FILE_INFO);
    guest.put_u64(p_size, 512);
    let d = dispatch(ServiceId::FileGetInfo, ServiceArgs::regs(fh, p_guid, p_size, buf), &guest, &mut sink, &mut st, &clk, SLAB);
    assert_eq!(d.status, EFI_SUCCESS);
    let mut fs8 = [0u8; 8];
    guest.read(buf + FILE_INFO_FILE_SIZE_OFF as u64, &mut fs8);
    assert_eq!(u64::from_le_bytes(fs8), app.len() as u64);
    // An unknown info GUID is unsupported, not faked.
    guest.write(p_guid + 0x20, &GUID_LOADED_IMAGE);
    assert_eq!(dispatch(ServiceId::FileGetInfo, ServiceArgs::regs(fh, p_guid + 0x20, p_size, buf), &guest, &mut sink, &mut st, &clk, SLAB).status, EFI_UNSUPPORTED);

    // --- Read the whole PE into guest memory ------------------------------
    guest.put_u64(p_size, app.len() as u64);
    let d = dispatch(ServiceId::FileRead, ServiceArgs::regs(fh, p_size, buf, 0), &guest, &mut sink, &mut st, &clk, SLAB);
    assert_eq!(d.status, EFI_SUCCESS);
    assert!(d.file_read_ok);
    assert_eq!(guest.u64_at(p_size), app.len() as u64);
    let mut got = vec![0u8; app.len()];
    guest.read(buf, &mut got);
    assert_eq!(got, app, "file read through EFI_FILE_PROTOCOL matches the ESP payload");
    // Position advanced; a second read hits EOF with zero bytes.
    assert_eq!(dispatch(ServiceId::FileGetPosition, ServiceArgs::regs(fh, p_pos, 0, 0), &guest, &mut sink, &mut st, &clk, SLAB).status, EFI_SUCCESS);
    assert_eq!(guest.u64_at(p_pos), app.len() as u64);
    guest.put_u64(p_size, 64);
    let d = dispatch(ServiceId::FileRead, ServiceArgs::regs(fh, p_size, buf + 0x8000, 0), &guest, &mut sink, &mut st, &clk, SLAB);
    assert_eq!((d.status, guest.u64_at(p_size)), (EFI_SUCCESS, 0));
    // Rewind and re-read the first two bytes.
    assert_eq!(dispatch(ServiceId::FileSetPosition, ServiceArgs::regs(fh, 0, 0, 0), &guest, &mut sink, &mut st, &clk, SLAB).status, EFI_SUCCESS);
    guest.put_u64(p_size, 2);
    dispatch(ServiceId::FileRead, ServiceArgs::regs(fh, p_size, buf + 0x8000, 0), &guest, &mut sink, &mut st, &clk, SLAB);
    let mut mz = [0u8; 2];
    guest.read(buf + 0x8000, &mut mz);
    assert_eq!(&mz, b"MZ");
    // Write/Delete/SetInfo are refused; Flush is a no-op success.
    for id in [ServiceId::FileWrite, ServiceId::FileDelete, ServiceId::FileSetInfo] {
        assert_eq!(dispatch(id, ServiceArgs::regs(fh, 0, 0, 0), &guest, &mut sink, &mut st, &clk, SLAB).status, EFI_WRITE_PROTECTED, "{}", id.name());
    }
    assert_eq!(dispatch(ServiceId::FileFlush, ServiceArgs::regs(fh, 0, 0, 0), &guest, &mut sink, &mut st, &clk, SLAB).status, EFI_SUCCESS);
    // Bogus `This` values are rejected everywhere.
    for id in [ServiceId::FileRead, ServiceId::FileClose, ServiceId::FileGetPosition] {
        assert_eq!(dispatch(id, ServiceArgs::regs(0xDEAD, p_size, buf, 0), &guest, &mut sink, &mut st, &clk, SLAB).status, EFI_INVALID_PARAMETER, "{}", id.name());
    }

    // --- LoadImage from the bytes we just read ----------------------------
    guest.put_u64(stack + STACK_ARG5_OFF, app.len() as u64); // SourceSize
    guest.put_u64(stack + STACK_ARG5_OFF + 8, p_handle); // *ImageHandle
    let li_args = ServiceArgs { a1: 0, a2: 0, a3: 0, a4: buf, rsp: stack };
    let free_before = st.pool.free_pages();
    let d = dispatch(ServiceId::LoadImage, li_args, &guest, &mut sink, &mut st, &clk, SLAB);
    assert_eq!(d.status, EFI_SUCCESS);
    assert!(d.image_loaded);
    assert_eq!(guest.u64_at(p_handle), HANDLE_IMAGE);
    assert!(st.pool.free_pages() < free_before, "image came from our pool");
    assert_ne!(st.image_base, 0);
    assert_eq!(st.image_size, super::testapp::TESTAPP_SIZE_OF_IMAGE as u64);
    assert_eq!(st.image_entry, st.image_base + super::testapp::TESTAPP_ENTRY_RVA as u64);
    // The image really landed: entry bytes match the app's code, and the
    // DIR64 relocation was applied against the new base.
    let mut code = [0u8; 8];
    guest.read(st.image_entry, &mut code);
    assert_eq!(&code, &super::testapp::TESTAPP_CODE[..8]);
    let msg_ptr = guest.u64_at(st.image_base + super::testapp::TESTAPP_MSG_PTR_RVA as u64);
    assert_eq!(msg_ptr, st.image_base + super::testapp::TESTAPP_MSG_RVA as u64);
    // LoadedImage was published on the image handle with honest fields.
    let li = st.loaded_image_proto;
    let mut rev = [0u8; 4];
    guest.read(li + LOADED_IMAGE_REVISION_OFF as u64, &mut rev);
    assert_eq!(u32::from_le_bytes(rev), LOADED_IMAGE_REVISION);
    assert_eq!(guest.u64_at(li + LOADED_IMAGE_IMAGE_BASE_OFF as u64), st.image_base);
    assert_eq!(guest.u64_at(li + LOADED_IMAGE_IMAGE_SIZE_OFF as u64), st.image_size);
    assert_eq!(guest.u64_at(li + LOADED_IMAGE_SYSTEM_TABLE_OFF as u64), tb as u64 + IMAGE_SYSTEM_TABLE_OFF as u64);
    guest.write(p_guid + 0x40, &GUID_LOADED_IMAGE);
    guest.put_u64(p_root, 0);
    assert_eq!(dispatch(ServiceId::HandleProtocol, ServiceArgs::regs(HANDLE_IMAGE, p_guid + 0x40, p_root, 0), &guest, &mut sink, &mut st, &clk, SLAB).status, EFI_SUCCESS);
    assert_eq!(guest.u64_at(p_root), li);
    // A DevicePath-only load (no SourceBuffer) is honestly unsupported.
    let dp = ServiceArgs { a1: 0, a2: 0, a3: 0x1234, a4: 0, rsp: stack };
    assert_eq!(dispatch(ServiceId::LoadImage, dp, &guest, &mut sink, &mut st, &clk, SLAB).status, EFI_UNSUPPORTED);
    // Non-PE source is EFI_LOAD_ERROR, not a crash.
    guest.write(buf + 0x9000, b"not a PE at all");
    let bad = ServiceArgs { a1: 0, a2: 0, a3: 0, a4: buf + 0x9000, rsp: stack };
    assert_eq!(dispatch(ServiceId::LoadImage, bad, &guest, &mut sink, &mut st, &clk, SLAB).status, 0x8000_0000_0000_0012);

    // --- device path + LoadedImage.DeviceHandle ---------------------------
    // A loader reads DeviceHandle/FilePath to find its own volume; a NULL
    // there would strand it, so both must be populated and well-formed.
    {
        use super::protocol::{
            encode_cd_device_path, CD_DEVICE_PATH_BYTES, DEVICE_PATH_BYTES, DP_CDROM_LEN,
            DP_SUBTYPE_CDROM, DP_SUBTYPE_END_ENTIRE, DP_TYPE_END, DP_TYPE_MEDIA, GUID_DEVICE_PATH,
            LOADED_IMAGE_DEVICE_HANDLE_OFF, LOADED_IMAGE_FILE_PATH_OFF,
        };
        use super::tables::IMAGE_DEVICE_PATH_OFF;
        let mut dp = [0u8; DEVICE_PATH_BYTES];
        encode_cd_device_path(1, 64, 512, &mut dp);
        assert_eq!(dp[0], DP_TYPE_MEDIA);
        assert_eq!(dp[1], DP_SUBTYPE_CDROM);
        assert_eq!(u16::from_le_bytes([dp[2], dp[3]]) as usize, DP_CDROM_LEN);
        assert_eq!(u32::from_le_bytes(dp[4..8].try_into().unwrap()), 1);
        assert_eq!(u64::from_le_bytes(dp[8..16].try_into().unwrap()), 64);
        assert_eq!(u64::from_le_bytes(dp[16..24].try_into().unwrap()), 512);
        // End-of-device-path node terminates it.
        assert_eq!(dp[DP_CDROM_LEN], DP_TYPE_END);
        assert_eq!(dp[DP_CDROM_LEN + 1], DP_SUBTYPE_END_ENTIRE);
        assert_eq!(u16::from_le_bytes([dp[DP_CDROM_LEN + 2], dp[DP_CDROM_LEN + 3]]), 4);
        // Slot is HD-sized; bytes after the CD End node stay zero.
        assert!(dp[CD_DEVICE_PATH_BYTES..].iter().all(|&b| b == 0));
        {
            use super::protocol::{
                device_path_is_grub_partition_child, encode_hd_device_path, DP_HD_LEN,
                DP_MBR_TYPE_GPT, DP_SIG_TYPE_GUID, DP_SUBTYPE_HD, HD_DEVICE_PATH_BYTES,
            };
            let sig = [
                0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE,
                0xFF, 0x00,
            ];
            let mut hd = [0u8; DEVICE_PATH_BYTES];
            encode_hd_device_path(1, 34, 128, sig, &mut hd);
            assert_eq!(HD_DEVICE_PATH_BYTES, 0x2E);
            assert_eq!(dp.len(), HD_DEVICE_PATH_BYTES);
            assert_eq!(hd[0], DP_TYPE_MEDIA);
            assert_eq!(hd[1], DP_SUBTYPE_HD);
            assert_eq!(u16::from_le_bytes([hd[2], hd[3]]) as usize, DP_HD_LEN);
            assert_eq!(u32::from_le_bytes(hd[4..8].try_into().unwrap()), 1);
            assert_eq!(u64::from_le_bytes(hd[8..16].try_into().unwrap()), 34);
            assert_eq!(u64::from_le_bytes(hd[16..24].try_into().unwrap()), 128);
            assert_eq!(&hd[24..40], &sig);
            assert_eq!(hd[40], DP_MBR_TYPE_GPT);
            assert_eq!(hd[41], DP_SIG_TYPE_GUID);
            assert_eq!(hd[DP_HD_LEN], DP_TYPE_END);
            assert_eq!(hd[DP_HD_LEN + 1], DP_SUBTYPE_END_ENTIRE);
            assert_eq!(
                u16::from_le_bytes([hd[DP_HD_LEN + 2], hd[DP_HD_LEN + 3]]),
                4
            );
            assert!(device_path_is_grub_partition_child(&hd));
        }
        {
            use super::protocol::{
                device_path_is_grub_partition_child, encode_whole_disk_device_path,
                DP_SUBTYPE_VENDOR, DP_TYPE_HARDWARE, DP_VENDOR_LEN, GUID_RAYNU_F_DISK,
                WHOLE_DISK_DEVICE_PATH_BYTES,
            };
            let mut whole = [0u8; DEVICE_PATH_BYTES];
            encode_whole_disk_device_path(&mut whole);
            assert_eq!(WHOLE_DISK_DEVICE_PATH_BYTES, 24);
            assert_eq!(whole[0], DP_TYPE_HARDWARE);
            assert_eq!(whole[1], DP_SUBTYPE_VENDOR);
            assert_eq!(
                u16::from_le_bytes([whole[2], whole[3]]) as usize,
                DP_VENDOR_LEN
            );
            assert_eq!(&whole[4..20], &GUID_RAYNU_F_DISK);
            assert_eq!(whole[DP_VENDOR_LEN], DP_TYPE_END);
            assert_eq!(whole[DP_VENDOR_LEN + 1], DP_SUBTYPE_END_ENTIRE);
            assert_eq!(
                u16::from_le_bytes([whole[DP_VENDOR_LEN + 2], whole[DP_VENDOR_LEN + 3]]),
                4
            );
            assert!(whole[WHOLE_DISK_DEVICE_PATH_BYTES..].iter().all(|&b| b == 0));
            assert!(!device_path_is_grub_partition_child(&whole));
        }
        assert!(super::protocol::device_path_is_grub_partition_child(&dp));
        assert_eq!(layout.device_path, tb as u64 + IMAGE_DEVICE_PATH_OFF as u64);
        // Publish it the way the launcher does, then re-run LoadImage so
        // LoadedImage picks the fields up.
        guest.write(layout.device_path, &dp);
        st.device_path = layout.device_path;
        st.device_handle = super::HANDLE_CD;
        assert_eq!(st.protocols.install(super::HANDLE_CD, GUID_DEVICE_PATH, layout.device_path), EFI_SUCCESS);
        guest.put_u64(stack + STACK_ARG5_OFF, app.len() as u64);
        guest.put_u64(stack + STACK_ARG5_OFF + 8, p_handle);
        let again = ServiceArgs { a1: 0, a2: 0, a3: 0, a4: buf, rsp: stack };
        assert_eq!(dispatch(ServiceId::LoadImage, again, &guest, &mut sink, &mut st, &clk, SLAB).status, EFI_SUCCESS);
        assert_eq!(guest.u64_at(li + LOADED_IMAGE_DEVICE_HANDLE_OFF as u64), super::HANDLE_CD);
        assert_eq!(guest.u64_at(li + LOADED_IMAGE_FILE_PATH_OFF as u64), layout.device_path);
        // And the loader can look the device path up on that handle.
        guest.write(p_guid + 0x60, &GUID_DEVICE_PATH);
        guest.put_u64(p_root, 0);
        assert_eq!(dispatch(ServiceId::HandleProtocol, ServiceArgs::regs(super::HANDLE_CD, p_guid + 0x60, p_root, 0), &guest, &mut sink, &mut st, &clk, SLAB).status, EFI_SUCCESS);
        assert_eq!(guest.u64_at(p_root), layout.device_path);
    }

    // --- StartImage asks the hypervisor to redirect the guest -------------
    let d = dispatch(ServiceId::StartImage, ServiceArgs::regs(HANDLE_IMAGE, 0, 0, 0), &guest, &mut sink, &mut st, &clk, SLAB);
    assert_eq!(d.status, EFI_SUCCESS);
    assert_eq!(d.start_image, Some((st.image_entry, HANDLE_IMAGE)));
    // A wrong handle does not redirect.
    let d = dispatch(ServiceId::StartImage, ServiceArgs::regs(0xDEAD, 0, 0, 0), &guest, &mut sink, &mut st, &clk, SLAB);
    assert_eq!(d.status, EFI_INVALID_PARAMETER);
    assert_eq!(d.start_image, None);
    // Exit from the started image asks the host to unwind to the caller
    // with the image's status; a second Exit (nothing started) is refused.
    assert!(st.image_started);
    let d = dispatch(ServiceId::Exit, ServiceArgs::regs(HANDLE_IMAGE, 0x8000_0000_0000_0003, 0, 0), &guest, &mut sink, &mut st, &clk, SLAB);
    assert_eq!(d.status, EFI_SUCCESS);
    assert_eq!(d.exit_image, Some(0x8000_0000_0000_0003));
    assert!(!st.image_started);
    let d = dispatch(ServiceId::Exit, ServiceArgs::regs(HANDLE_IMAGE, 0, 0, 0), &guest, &mut sink, &mut st, &clk, SLAB);
    assert_eq!(d.status, EFI_INVALID_PARAMETER);
    assert_eq!(d.exit_image, None);
    assert_eq!(dispatch(ServiceId::Exit, ServiceArgs::regs(0xDEAD, 0, 0, 0), &guest, &mut sink, &mut st, &clk, SLAB).status, EFI_INVALID_PARAMETER);
    // UnloadImage after Exit frees the image pages and its LoadedImage.
    let free_before = st.pool.free_pages();
    let img_pages = (st.image_size + 4095) / 4096;
    assert_eq!(dispatch(ServiceId::UnloadImage, ServiceArgs::regs(HANDLE_IMAGE, 0, 0, 0), &guest, &mut sink, &mut st, &clk, SLAB).status, EFI_SUCCESS);
    assert_eq!(st.pool.free_pages() as u64, free_before as u64 + img_pages);
    assert_eq!(st.image_base, 0);
    assert_eq!(st.protocols.interface_for(HANDLE_IMAGE, &GUID_LOADED_IMAGE), None);
    assert_eq!(dispatch(ServiceId::UnloadImage, ServiceArgs::regs(HANDLE_IMAGE, 0, 0, 0), &guest, &mut sink, &mut st, &clk, SLAB).status, EFI_INVALID_PARAMETER);

    // --- Close releases the slot -------------------------------------------
    let open_before = st.fs.open_count();
    assert_eq!(dispatch(ServiceId::FileClose, ServiceArgs::regs(fh, 0, 0, 0), &guest, &mut sink, &mut st, &clk, SLAB).status, EFI_SUCCESS);
    assert_eq!(st.fs.open_count(), open_before - 1);
    // Opening a path that is not there.
    let units2: Vec<u16> = "\\EFI\\BOOT\\NOPE.EFI\0".encode_utf16().collect();
    for (i, u) in units2.iter().enumerate() {
        guest.write(path_gpa + 0x100 + i as u64 * 2, &u.to_le_bytes());
    }
    let a2 = ServiceArgs { a1: root, a2: p_file, a3: path_gpa + 0x100, a4: EFI_FILE_MODE_READ, rsp: stack };
    assert_eq!(dispatch(ServiceId::FileOpen, a2, &guest, &mut sink, &mut st, &clk, SLAB).status, EFI_NOT_FOUND);

    #[cfg(not(target_os = "uefi"))]
    println!("{}", super::RAYNU_F_IMAGE_GATE_MARKER);
}

/// F6-prep: what GRUB taught us on nested `7ee3a3b`. The allocator spans the
/// slab pool plus the pre-mapped report-RAM; the memory map advertises as
/// conventional exactly what `AllocatePages(AllocateAddress)` can hand out;
/// and a directly-staged image gets `LoadedImage` published without
/// `LoadImage`.
#[test]
fn raynu_f_high_ram_and_launch_loaded_image() {
    use super::memory::{
        MemRun, PagePool, ALLOCATE_ADDRESS, ALLOCATE_ANY_PAGES, ALLOCATE_MAX_ADDRESS,
        BELOW1M_BASE, BELOW1M_END, BELOW1M_PAGES, EFI_BOOT_SERVICES_DATA, EFI_CONVENTIONAL_MEMORY,
        EFI_INVALID_PARAMETER, EFI_LOADER_CODE, EFI_LOADER_DATA, EFI_NOT_FOUND,
        EFI_OUT_OF_RESOURCES, EFI_RESERVED_MEMORY_TYPE, EFI_SUCCESS, HIGH_MAX_PAGES,
        MAX_DESCRIPTORS, POOL_BASE, POOL_END, POOL_PAGES,
    };
    use super::protocol::{
        GUID_LOADED_IMAGE, HANDLE_IMAGE, LOADED_IMAGE_DEVICE_HANDLE_OFF,
        LOADED_IMAGE_IMAGE_BASE_OFF, LOADED_IMAGE_PARENT_OFF, LOADED_IMAGE_REVISION,
        LOADED_IMAGE_REVISION_OFF, LOADED_IMAGE_SYSTEM_TABLE_OFF,
    };
    use super::services::{dispatch, publish_loaded_image, FirmwareState, ServiceArgs, ServiceId};
    use super::tables::{build_firmware_image, IMAGE_BYTES};

    const HIGH_BASE: u64 = 0x0200_0000; // 32 MiB: PLATFORM_RAM_BYTES
    let mut runs = [MemRun { typ: 0, start: 0, pages: 0 }; MAX_DESCRIPTORS];

    // --- No high region: the map still never lies ---------------------------
    let pool = PagePool::new();
    assert_eq!(pool.high_region(), None);
    let n = pool.memory_map(SLAB, &mut runs);
    let mut cursor = 0;
    for r in &runs[..n] {
        assert_eq!(r.start, cursor);
        cursor = r.start + r.pages * 4096;
        if r.typ == EFI_CONVENTIONAL_MEMORY {
            let in_pool = r.start >= POOL_BASE && cursor <= POOL_END;
            let in_below1m = r.start >= BELOW1M_BASE && cursor <= BELOW1M_END;
            assert!(in_pool || in_below1m, "conventional outside managed windows: {r:?}");
        }
    }
    assert_eq!(cursor, SLAB);
    // The slab slack between our fixed windows is firmware-owned, not a lie.
    assert!(runs[..n].iter().any(|r| r.typ == EFI_BOOT_SERVICES_DATA && r.start < POOL_BASE));
    assert_eq!(runs[0].typ, EFI_RESERVED_MEMORY_TYPE);

    // --- Configure a high region ---------------------------------------------
    let mut pool = PagePool::new();
    // Rejected: below the pool, unaligned, zero.
    assert_eq!(pool.set_high_region(0x0100_0000, 16), 0);
    assert_eq!(pool.set_high_region(HIGH_BASE + 1, 16), 0);
    assert_eq!(pool.set_high_region(HIGH_BASE, 0), 0);
    assert_eq!(pool.high_region(), None);
    // 2 GiB pre-mapped is clamped to the 256 MiB we manage.
    assert_eq!(pool.set_high_region(HIGH_BASE, (0x8000_0000 - 0x0200_0000) / 4096), HIGH_MAX_PAGES);
    // Nested-size: 64 MiB of report-RAM.
    let high_pages = (64 * 1024 * 1024) / 4096;
    assert_eq!(pool.set_high_region(HIGH_BASE, high_pages), high_pages);
    assert_eq!(pool.high_region(), Some((HIGH_BASE, high_pages)));
    assert_eq!(pool.free_pages(), POOL_PAGES + BELOW1M_PAGES + high_pages);
    assert_eq!(pool.free_low_pages(), POOL_PAGES);

    // Map: slab, reserved tail, then the high region as one conventional run.
    let n = pool.memory_map(SLAB, &mut runs);
    let mut cursor = 0;
    let mut high_conv = 0u64;
    for r in &runs[..n] {
        assert_eq!(r.start, cursor, "gap before {r:?}");
        cursor = r.start + r.pages * 4096;
        if r.typ == EFI_CONVENTIONAL_MEMORY && r.start >= HIGH_BASE {
            high_conv += r.pages;
        }
    }
    assert_eq!(cursor, HIGH_BASE + high_pages as u64 * 4096);
    assert_eq!(high_conv, high_pages as u64);

    // --- GRUB mm_init pattern: AllocateAddress at every conventional run ----
    // Every conventional descriptor must be allocatable in full at its own
    // address; that is the contract GRUB relies on (else "too little memory"
    // / AllocateAddress NOT_FOUND, nested 7ee3a3b lines 240/259).
    // GRUB efi/mm.c discards regions below 1 MiB for the heap; Linux then
    // needs that window still conventional at EBS (nested 033bc0d trampoline
    // panic). Walk the same way.
    let mut grabbed = 0u64;
    let want = 8192u64; // GRUB DEFAULT_HEAP_SIZE 32 MiB in pages
    for r in runs[..n].iter().filter(|r| r.typ == EFI_CONVENTIONAL_MEMORY) {
        if r.start < 0x10_0000 {
            continue;
        }
        if grabbed >= want {
            break;
        }
        let take = r.pages.min(want - grabbed);
        let (s, at) = pool.allocate_pages(ALLOCATE_ADDRESS, EFI_LOADER_DATA, take, r.start);
        assert_eq!(s, EFI_SUCCESS, "AllocateAddress refused its own conventional run {r:?}");
        assert_eq!(at, r.start);
        grabbed += take;
    }
    assert_eq!(grabbed, want, "a 32 MiB heap fits");
    // Below-1M window must still be conventional after the loader's 32 MiB grab.
    let n_after = pool.memory_map(SLAB, &mut runs);
    assert!(
        runs[..n_after].iter().any(|r| r.typ == EFI_CONVENTIONAL_MEMORY
            && r.start == BELOW1M_BASE
            && r.pages == BELOW1M_PAGES as u64),
        "GRUB heap stole the Linux trampoline window"
    );
    // Firmware-internal pool allocation still works after the loader's grab
    // (nested 7ee3a3b line 240 was OUT_OF_RESOURCES here).
    let (s, p) = pool.allocate_pages(ALLOCATE_ANY_PAGES, EFI_LOADER_DATA, 3, 0);
    assert_eq!(s, EFI_SUCCESS);
    assert!(p >= HIGH_BASE, "slab pool drained, fell through to high: {p:#x}");
    // Freeing in the high region and re-allocating exactly there.
    assert_eq!(pool.free_pages_at(p, 3), EFI_SUCCESS);
    assert_eq!(pool.free_pages_at(p, 3), EFI_NOT_FOUND);
    let (s, p2) = pool.allocate_pages(ALLOCATE_ADDRESS, EFI_LOADER_CODE, 3, p);
    assert_eq!((s, p2), (EFI_SUCCESS, p));
    // AllocateAddress beyond the region end / outside both regions.
    let end = HIGH_BASE + high_pages as u64 * 4096;
    assert_eq!(pool.allocate_pages(ALLOCATE_ADDRESS, EFI_LOADER_DATA, 1, end).0, EFI_NOT_FOUND);
    assert_eq!(pool.allocate_pages(ALLOCATE_ADDRESS, EFI_LOADER_DATA, 2, end - 4096).0, EFI_NOT_FOUND);
    assert_eq!(pool.allocate_pages(ALLOCATE_ADDRESS, EFI_LOADER_DATA, 1, 0x0080_4000).0, EFI_NOT_FOUND);
    // MaxAddress: a cap below 32 MiB never lands in the high region; a cap
    // above it may. A cap under the pool finds nothing.
    let (s, q) = pool.allocate_pages(ALLOCATE_MAX_ADDRESS, EFI_LOADER_DATA, 1, HIGH_BASE - 1);
    assert!(s == EFI_SUCCESS && q < HIGH_BASE || s == EFI_NOT_FOUND);
    let (s, q) = pool.allocate_pages(ALLOCATE_MAX_ADDRESS, EFI_LOADER_DATA, 1, end - 1);
    assert_eq!(s, EFI_SUCCESS);
    assert!(q + 4096 <= end);
    assert_eq!(pool.allocate_pages(ALLOCATE_MAX_ADDRESS, EFI_LOADER_DATA, 1, POOL_BASE - 1).0, EFI_NOT_FOUND);
    // Bigger than any region: invalid; bigger than what is left: exhausted.
    assert_eq!(pool.allocate_pages(ALLOCATE_ANY_PAGES, EFI_LOADER_DATA, high_pages as u64 + 1, 0).0, EFI_INVALID_PARAMETER);
    assert_eq!(pool.allocate_pages(ALLOCATE_ANY_PAGES, EFI_LOADER_DATA, high_pages as u64, 0).0, EFI_OUT_OF_RESOURCES);
    // The map now shows the loader's heap as LoaderData and stays contiguous.
    let n = pool.memory_map(SLAB, &mut runs);
    let mut cursor = 0;
    let mut loader_pages = 0u64;
    for r in &runs[..n] {
        assert_eq!(r.start, cursor);
        cursor = r.start + r.pages * 4096;
        if r.typ == EFI_LOADER_DATA {
            loader_pages += r.pages;
        }
    }
    assert_eq!(cursor, end);
    assert!(loader_pages >= want);

    // --- Launcher-side LoadedImage publish ----------------------------------
    let mut mem = vec![0u8; SLAB as usize];
    let tb = 0x0080_0000usize;
    let layout = build_firmware_image(tb as u64, &mut mem[tb..tb + IMAGE_BYTES]).unwrap();
    let guest = MockGuest::new(0, mem);
    let mut sink = CaptureSink::default();
    let clk = ManualClock { now: Cell::new(1_000), step: 1_000 };
    let mut st = FirmwareState::new();
    // Not published yet: the protocol lookup GRUB makes first fails.
    let p_guid = 0x00AF_0000u64;
    let p_out = p_guid + 0x20;
    guest.write(p_guid, &GUID_LOADED_IMAGE);
    assert_eq!(
        dispatch(ServiceId::OpenProtocol, ServiceArgs::regs(HANDLE_IMAGE, p_guid, p_out, 0), &guest, &mut sink, &mut st, &clk, SLAB).status,
        EFI_NOT_FOUND
    );
    // Nothing to publish into / no image: honest false.
    assert!(!publish_loaded_image(&mut st, &guest, 0));
    st.loaded_image_proto = layout.loaded_image;
    assert!(!publish_loaded_image(&mut st, &guest, 0)); // image_handle == 0
    // What the F5 launcher sets before VMLAUNCH.
    st.system_table = layout.system_table;
    st.device_handle = super::HANDLE_CD;
    st.device_path = layout.device_path;
    st.image_handle = HANDLE_IMAGE;
    st.image_base = 0x00BB_1000;
    st.image_size = 724_992;
    st.image_entry = 0x00BB_2000;
    assert!(publish_loaded_image(&mut st, &guest, 0));
    let li = layout.loaded_image;
    let mut rev = [0u8; 4];
    guest.read(li + LOADED_IMAGE_REVISION_OFF as u64, &mut rev);
    assert_eq!(u32::from_le_bytes(rev), LOADED_IMAGE_REVISION);
    assert_eq!(guest.u64_at(li + LOADED_IMAGE_PARENT_OFF as u64), 0);
    assert_eq!(guest.u64_at(li + LOADED_IMAGE_SYSTEM_TABLE_OFF as u64), layout.system_table);
    assert_eq!(guest.u64_at(li + LOADED_IMAGE_IMAGE_BASE_OFF as u64), 0x00BB_1000);
    assert_eq!(guest.u64_at(li + LOADED_IMAGE_DEVICE_HANDLE_OFF as u64), super::HANDLE_CD);
    // Now GRUB's first call succeeds and returns our struct.
    guest.put_u64(p_out, 0);
    assert_eq!(
        dispatch(ServiceId::OpenProtocol, ServiceArgs::regs(HANDLE_IMAGE, p_guid, p_out, 0), &guest, &mut sink, &mut st, &clk, SLAB).status,
        EFI_SUCCESS
    );
    assert_eq!(guest.u64_at(p_out), li);
}

/// Nested `033bc0d`: Linux 6.12.13-virt reached `start_kernel` on RayNu-F
/// then panicked in `init_real_mode` because the map reserved `[0, 8 MiB)`
/// as one blob — no conventional page below 1 MiB for the real-mode trampoline.
/// The window must be `EfiConventionalMemory`, `AllocateAddress`-honest, and
/// still free after `AllocateAnyPages` / a GRUB-sized heap grab.
#[test]
fn raynu_f_below_1m_trampoline_window() {
    use super::memory::{
        MemRun, PagePool, ALLOCATE_ADDRESS, ALLOCATE_ANY_PAGES, ALLOCATE_MAX_ADDRESS,
        BELOW1M_BASE, BELOW1M_END, BELOW1M_PAGES, EFI_CONVENTIONAL_MEMORY, EFI_LOADER_DATA,
        EFI_SUCCESS, MAX_DESCRIPTORS, POOL_BASE,
    };

    const SLAB: u64 = 0x0200_0000;
    let mut pool = PagePool::new();
    let mut runs = [MemRun { typ: 0, start: 0, pages: 0 }; MAX_DESCRIPTORS];
    let n = pool.memory_map(SLAB, &mut runs);
    let mut cursor = 0u64;
    let mut below = None;
    for r in &runs[..n] {
        assert_eq!(r.start, cursor);
        cursor = r.start + r.pages * 4096;
        if r.typ == EFI_CONVENTIONAL_MEMORY && r.start < 0x10_0000 {
            below = Some(*r);
        }
    }
    assert_eq!(cursor, SLAB);
    let below = below.expect("no conventional run below 1MiB");
    assert_eq!(below.start, BELOW1M_BASE);
    assert_eq!(below.start + below.pages * 4096, BELOW1M_END);
    assert_eq!(below.pages, BELOW1M_PAGES as u64);
    assert!(below.pages >= 2, "Linux trampoline needs at least a couple of pages");

    // Honesty: AllocateAddress of the whole window succeeds.
    let (s, at) = pool.allocate_pages(ALLOCATE_ADDRESS, EFI_LOADER_DATA, below.pages, below.start);
    assert_eq!((s, at), (EFI_SUCCESS, BELOW1M_BASE));
    assert_eq!(pool.free_pages_at(BELOW1M_BASE, below.pages), EFI_SUCCESS);

    // AnyPages must not steal it (firmware/loader heap stays in the slab).
    let (s, p) = pool.allocate_pages(ALLOCATE_ANY_PAGES, EFI_LOADER_DATA, 1, 0);
    assert_eq!(s, EFI_SUCCESS);
    assert!(p >= POOL_BASE, "AnyPages consumed the trampoline window: {p:#x}");
    assert_eq!(pool.free_pages_at(p, 1), EFI_SUCCESS);

    // Explicit sub-1M MaxAddress (what a firmware that *wants* low RAM would do).
    let (s, q) = pool.allocate_pages(ALLOCATE_MAX_ADDRESS, EFI_LOADER_DATA, 1, BELOW1M_END - 1);
    assert_eq!(s, EFI_SUCCESS);
    assert!(q >= BELOW1M_BASE && q + 4096 <= BELOW1M_END, "MaxAddress sub-1M {q:#x}");
    assert_eq!(pool.free_pages_at(q, 1), EFI_SUCCESS);

    // After AnyPages has taken a bite of the slab, the window is still
    // conventional — the e820 Linux `reserve_real_mode` will see.
    let (s, _) = pool.allocate_pages(ALLOCATE_ANY_PAGES, EFI_LOADER_DATA, 16, 0);
    assert_eq!(s, EFI_SUCCESS);
    let n = pool.memory_map(SLAB, &mut runs);
    assert!(
        runs[..n].iter().any(|r| r.typ == EFI_CONVENTIONAL_MEMORY
            && r.start < 0x10_0000
            && r.pages >= 1),
        "AnyPages drained the trampoline window out of the map"
    );
}

/// F6-prep b: the GRUB → Linux EFI stub initrd handshake, nested `2d34fff`.
/// GRUB's last act before the kernel is
/// `InstallMultipleProtocolInterfaces(&h, &LoadFile2, &lf2, &DevicePath, &dp, NULL)`
/// with `h == NULL`; the kernel then `LocateDevicePath(&LoadFile2, &dp, &h)`
/// and calls GRUB's `LoadFile` directly. We only hold the handle.
#[test]
fn raynu_f_install_multiple_and_locate_device_path() {
    use super::protocol::{
        GUID_BLOCK_IO, GUID_DEVICE_PATH, GUID_LOAD_FILE2, HANDLE_CD, HANDLE_DYNAMIC_BASE,
        EFI_NOT_FOUND, EFI_OUT_OF_RESOURCES,
    };
    use super::services::{
        dispatch, FirmwareState, ServiceArgs, ServiceId, EFI_INVALID_PARAMETER, EFI_SUCCESS,
        EFI_UNSUPPORTED as EFI_UNSUPPORTED_SVC, STACK_ARG5_OFF,
    };
    use super::tables::{build_firmware_image, IMAGE_BYTES};

    let mut mem = vec![0u8; SLAB as usize];
    let tb = 0x0080_0000usize;
    let layout = build_firmware_image(tb as u64, &mut mem[tb..tb + IMAGE_BYTES]).unwrap();
    let guest = MockGuest::new(0, mem);
    let mut sink = CaptureSink::default();
    let clk = ManualClock { now: Cell::new(1_000), step: 1_000 };
    let mut st = FirmwareState::new();

    let scratch = 0x00AF_0000u64;
    let stack = scratch + 0x4000;
    let p_handle = scratch; // EFI_HANDLE cell
    let g_lf2 = scratch + 0x100;
    let g_dp = scratch + 0x110;
    let g_blk = scratch + 0x120;
    guest.write(g_lf2, &GUID_LOAD_FILE2);
    guest.write(g_dp, &GUID_DEVICE_PATH);
    guest.write(g_blk, &GUID_BLOCK_IO);
    // GRUB's LoadFile2 "interface" is a struct in its own memory; any pointer.
    let lf2_iface = scratch + 0x200;
    // Linux initrd vendor media device path: Media(4)/Vendor(3), len 0x14
    // = hdr(4) + GUID(16), then End-Entire.
    let dp_initrd = scratch + 0x300;
    let mut dp = [0u8; 0x18];
    dp[0] = 0x04;
    dp[1] = 0x03;
    dp[2..4].copy_from_slice(&0x14u16.to_le_bytes());
    dp[4..20].copy_from_slice(&[
        0x27, 0xE4, 0x68, 0x55, 0xFC, 0x68, 0x3D, 0x4F, 0xAC, 0x74, 0xCA, 0x55, 0x52, 0x31, 0xCC, 0x68,
    ]);
    dp[0x14] = 0x7F;
    dp[0x15] = 0xFF;
    dp[0x16..0x18].copy_from_slice(&4u16.to_le_bytes());
    guest.write(dp_initrd, &dp);

    // --- InstallMultiple with *Handle == NULL mints a handle ---------------
    // args: a1=&h, a2=&LoadFile2, a3=lf2, a4=&DevicePath, [rsp+0x28]=dp, [rsp+0x30]=NULL
    guest.put_u64(p_handle, 0);
    guest.put_u64(stack + STACK_ARG5_OFF, dp_initrd);
    guest.put_u64(stack + STACK_ARG5_OFF + 8, 0);
    let a = ServiceArgs { a1: p_handle, a2: g_lf2, a3: lf2_iface, a4: g_dp, rsp: stack };
    let d = dispatch(ServiceId::InstallMultipleProtocolInterfaces, a, &guest, &mut sink, &mut st, &clk, SLAB);
    assert_eq!(d.status, EFI_SUCCESS);
    let h = guest.u64_at(p_handle);
    assert_eq!(h, HANDLE_DYNAMIC_BASE);
    assert_eq!(st.protocols.interface_for(h, &GUID_LOAD_FILE2), Some(lf2_iface));
    assert_eq!(st.protocols.interface_for(h, &GUID_DEVICE_PATH), Some(dp_initrd));
    // A second NULL-handle install mints a different handle.
    guest.put_u64(p_handle, 0);
    guest.put_u64(stack + STACK_ARG5_OFF, 0); // (&BlockIo, iface) then NULL
    let a2 = ServiceArgs { a1: p_handle, a2: g_blk, a3: scratch + 0x400, a4: 0, rsp: stack };
    assert_eq!(dispatch(ServiceId::InstallMultipleProtocolInterfaces, a2, &guest, &mut sink, &mut st, &clk, SLAB).status, EFI_SUCCESS);
    assert_eq!(guest.u64_at(p_handle), HANDLE_DYNAMIC_BASE + 1);
    // Installing onto an existing handle keeps it.
    guest.put_u64(p_handle, HANDLE_CD);
    assert_eq!(dispatch(ServiceId::InstallMultipleProtocolInterfaces, a2, &guest, &mut sink, &mut st, &clk, SLAB).status, EFI_SUCCESS);
    assert_eq!(guest.u64_at(p_handle), HANDLE_CD);
    assert_eq!(st.protocols.interface_for(HANDLE_CD, &GUID_BLOCK_IO), Some(scratch + 0x400));
    // Malformed: NULL *Handle pointer; empty list; NULL interface.
    assert_eq!(dispatch(ServiceId::InstallMultipleProtocolInterfaces, ServiceArgs { a1: 0, ..a }, &guest, &mut sink, &mut st, &clk, SLAB).status, EFI_INVALID_PARAMETER);
    assert_eq!(dispatch(ServiceId::InstallMultipleProtocolInterfaces, ServiceArgs { a2: 0, ..a }, &guest, &mut sink, &mut st, &clk, SLAB).status, EFI_INVALID_PARAMETER);
    guest.put_u64(p_handle, 0);
    guest.put_u64(stack + STACK_ARG5_OFF, dp_initrd);
    let bad = ServiceArgs { a3: 0, ..a };
    assert_eq!(dispatch(ServiceId::InstallMultipleProtocolInterfaces, bad, &guest, &mut sink, &mut st, &clk, SLAB).status, EFI_INVALID_PARAMETER);
    assert_eq!(guest.u64_at(p_handle), 0, "no handle minted on failure");
    let _ = EFI_OUT_OF_RESOURCES;

    // --- LocateDevicePath finds GRUB's handle for the kernel ----------------
    // Publish the CD's own device path too (launcher does); it must not match.
    let dp_cd = scratch + 0x500;
    let mut cd = [0u8; super::protocol::DEVICE_PATH_BYTES];
    super::protocol::encode_cd_device_path(1, 77, 2880, &mut cd);
    guest.write(dp_cd, &cd);
    assert_eq!(st.protocols.install(HANDLE_CD, GUID_DEVICE_PATH, dp_cd), EFI_SUCCESS);
    // Kernel: dp = &initrd path; (&LoadFile2, &dp, &h)
    let p_dp = scratch + 0x600;
    let p_h = scratch + 0x608;
    guest.put_u64(p_dp, dp_initrd);
    guest.put_u64(p_h, 0);
    let d = dispatch(ServiceId::LocateDevicePath, ServiceArgs::regs(g_lf2, p_dp, p_h, 0), &guest, &mut sink, &mut st, &clk, SLAB);
    assert_eq!(d.status, EFI_SUCCESS);
    assert_eq!(guest.u64_at(p_h), h);
    // *DevicePath advanced to the End node (whole path matched).
    assert_eq!(guest.u64_at(p_dp), dp_initrd + 0x14);
    // A path nobody published, or a protocol nobody has on a matching path.
    let dp_other = scratch + 0x700;
    let mut other = dp;
    other[4] ^= 0xFF;
    guest.write(dp_other, &other);
    guest.put_u64(p_dp, dp_other);
    assert_eq!(dispatch(ServiceId::LocateDevicePath, ServiceArgs::regs(g_lf2, p_dp, p_h, 0), &guest, &mut sink, &mut st, &clk, SLAB).status, EFI_NOT_FOUND);
    guest.put_u64(p_dp, dp_cd);
    assert_eq!(dispatch(ServiceId::LocateDevicePath, ServiceArgs::regs(g_lf2, p_dp, p_h, 0), &guest, &mut sink, &mut st, &clk, SLAB).status, EFI_NOT_FOUND);
    // The CD path does resolve for BlockIo, and a longer request
    // (CD path + extra node) still resolves to the CD with the tail returned.
    guest.put_u64(p_dp, dp_cd);
    assert_eq!(dispatch(ServiceId::LocateDevicePath, ServiceArgs::regs(g_blk, p_dp, p_h, 0), &guest, &mut sink, &mut st, &clk, SLAB).status, EFI_SUCCESS);
    assert_eq!(guest.u64_at(p_h), HANDLE_CD);
    let dp_long = scratch + 0x800;
    let mut long = [0u8; 0x18 + 0x14 + 4];
    long[..0x18].copy_from_slice(&cd[..0x18]); // CD node without its End
    long[0x18..0x18 + 0x14].copy_from_slice(&dp[..0x14]); // vendor node
    long[0x2C] = 0x7F;
    long[0x2D] = 0xFF;
    long[0x2E..0x30].copy_from_slice(&4u16.to_le_bytes());
    guest.write(dp_long, &long);
    guest.put_u64(p_dp, dp_long);
    assert_eq!(dispatch(ServiceId::LocateDevicePath, ServiceArgs::regs(g_blk, p_dp, p_h, 0), &guest, &mut sink, &mut st, &clk, SLAB).status, EFI_SUCCESS);
    assert_eq!(guest.u64_at(p_h), HANDLE_CD);
    assert_eq!(guest.u64_at(p_dp), dp_long + 0x18, "remainder starts at the vendor node");
    // Unterminated path is rejected, not walked off the end.
    let dp_bad = scratch + 0x900;
    guest.write(dp_bad, &[0x04, 0x03, 0x00, 0x00]); // len 0
    guest.put_u64(p_dp, dp_bad);
    assert_eq!(dispatch(ServiceId::LocateDevicePath, ServiceArgs::regs(g_blk, p_dp, p_h, 0), &guest, &mut sink, &mut st, &clk, SLAB).status, EFI_INVALID_PARAMETER);

    // --- UninstallMultiple ------------------------------------------------
    guest.put_u64(stack + STACK_ARG5_OFF, dp_initrd);
    guest.put_u64(stack + STACK_ARG5_OFF + 8, 0);
    // Wrong interface for one pair: nothing removed.
    let u_bad = ServiceArgs { a1: h, a2: g_lf2, a3: lf2_iface + 8, a4: g_dp, rsp: stack };
    assert_eq!(dispatch(ServiceId::UninstallMultipleProtocolInterfaces, u_bad, &guest, &mut sink, &mut st, &clk, SLAB).status, EFI_INVALID_PARAMETER);
    assert!(st.protocols.handle_exists(h));
    let u = ServiceArgs { a1: h, a2: g_lf2, a3: lf2_iface, a4: g_dp, rsp: stack };
    assert_eq!(dispatch(ServiceId::UninstallMultipleProtocolInterfaces, u, &guest, &mut sink, &mut st, &clk, SLAB).status, EFI_SUCCESS);
    assert!(!st.protocols.handle_exists(h));
    guest.put_u64(p_dp, dp_initrd);
    assert_eq!(dispatch(ServiceId::LocateDevicePath, ServiceArgs::regs(g_lf2, p_dp, p_h, 0), &guest, &mut sink, &mut st, &clk, SLAB).status, EFI_NOT_FOUND);

    // --- InstallConfigurationTable (the stub's initrd record) ---------------
    use super::tables::{
        header_crc_valid, CONFIG_TABLE_ENTRY_BYTES, CONFIG_TABLE_MAX_ENTRIES,
        IMAGE_CONFIG_TABLE_OFF, IMAGE_SYSTEM_TABLE_OFF, SYSTEM_TABLE_CONFIG_TABLE_OFF,
        SYSTEM_TABLE_NUM_TABLE_ENTRIES_OFF,
    };
    let sys = layout.system_table;
    assert_eq!(layout.config_table, tb as u64 + IMAGE_CONFIG_TABLE_OFF as u64);
    assert_eq!(guest.u64_at(sys + SYSTEM_TABLE_CONFIG_TABLE_OFF as u64), layout.config_table);
    assert_eq!(guest.u64_at(sys + SYSTEM_TABLE_NUM_TABLE_ENTRIES_OFF as u64), 0);
    // Not wired (state has no table): honest UNSUPPORTED, as before.
    let g_initrd = scratch + 0xA00; // LINUX_EFI_INITRD_MEDIA_GUID
    guest.write(g_initrd, &dp[4..20]);
    assert_eq!(dispatch(ServiceId::InstallConfigurationTable, ServiceArgs::regs(g_initrd, 0x1234_0000, 0, 0), &guest, &mut sink, &mut st, &clk, SLAB).status, EFI_UNSUPPORTED_SVC);
    st.system_table = sys;
    st.config_table = layout.config_table;
    // Add.
    assert_eq!(dispatch(ServiceId::InstallConfigurationTable, ServiceArgs::regs(g_initrd, 0x1234_0000, 0, 0), &guest, &mut sink, &mut st, &clk, SLAB).status, EFI_SUCCESS);
    assert_eq!(guest.u64_at(sys + SYSTEM_TABLE_NUM_TABLE_ENTRIES_OFF as u64), 1);
    let mut e = [0u8; 16];
    guest.read(layout.config_table, &mut e);
    assert_eq!(e, dp[4..20]);
    assert_eq!(guest.u64_at(layout.config_table + 16), 0x1234_0000);
    // The system table header CRC was recomputed and still verifies.
    assert!(header_crc_valid(&guest.mem.borrow(), tb + IMAGE_SYSTEM_TABLE_OFF));
    // Replace (same GUID) keeps the count.
    assert_eq!(dispatch(ServiceId::InstallConfigurationTable, ServiceArgs::regs(g_initrd, 0x5678_0000, 0, 0), &guest, &mut sink, &mut st, &clk, SLAB).status, EFI_SUCCESS);
    assert_eq!(guest.u64_at(sys + SYSTEM_TABLE_NUM_TABLE_ENTRIES_OFF as u64), 1);
    assert_eq!(guest.u64_at(layout.config_table + 16), 0x5678_0000);
    // A second GUID appends; removing the first shifts it down.
    assert_eq!(dispatch(ServiceId::InstallConfigurationTable, ServiceArgs::regs(g_blk, 0x9ABC_0000, 0, 0), &guest, &mut sink, &mut st, &clk, SLAB).status, EFI_SUCCESS);
    assert_eq!(guest.u64_at(sys + SYSTEM_TABLE_NUM_TABLE_ENTRIES_OFF as u64), 2);
    assert_eq!(dispatch(ServiceId::InstallConfigurationTable, ServiceArgs::regs(g_initrd, 0, 0, 0), &guest, &mut sink, &mut st, &clk, SLAB).status, EFI_SUCCESS);
    assert_eq!(guest.u64_at(sys + SYSTEM_TABLE_NUM_TABLE_ENTRIES_OFF as u64), 1);
    guest.read(layout.config_table, &mut e);
    assert_eq!(e, GUID_BLOCK_IO);
    assert_eq!(guest.u64_at(layout.config_table + 16), 0x9ABC_0000);
    assert!(header_crc_valid(&guest.mem.borrow(), tb + IMAGE_SYSTEM_TABLE_OFF));
    // Removing an absent GUID is NOT_FOUND; a NULL GUID pointer is invalid.
    assert_eq!(dispatch(ServiceId::InstallConfigurationTable, ServiceArgs::regs(g_initrd, 0, 0, 0), &guest, &mut sink, &mut st, &clk, SLAB).status, EFI_NOT_FOUND);
    assert_eq!(dispatch(ServiceId::InstallConfigurationTable, ServiceArgs::regs(0, 1, 0, 0), &guest, &mut sink, &mut st, &clk, SLAB).status, EFI_INVALID_PARAMETER);
    // Fill to capacity, then one more is OUT_OF_RESOURCES.
    for i in 1..CONFIG_TABLE_MAX_ENTRIES {
        let g = scratch + 0xB00 + i as u64 * 16;
        let mut gb = [0u8; 16];
        gb[0] = i as u8;
        gb[15] = 0xEE;
        guest.write(g, &gb);
        assert_eq!(dispatch(ServiceId::InstallConfigurationTable, ServiceArgs::regs(g, 0x1000 + i as u64, 0, 0), &guest, &mut sink, &mut st, &clk, SLAB).status, EFI_SUCCESS);
    }
    assert_eq!(guest.u64_at(sys + SYSTEM_TABLE_NUM_TABLE_ENTRIES_OFF as u64), CONFIG_TABLE_MAX_ENTRIES as u64);
    assert_eq!(dispatch(ServiceId::InstallConfigurationTable, ServiceArgs::regs(g_initrd, 0x1, 0, 0), &guest, &mut sink, &mut st, &clk, SLAB).status, EFI_OUT_OF_RESOURCES);
    let _ = CONFIG_TABLE_ENTRY_BYTES;
}

/// Opt-in integration test against a real distro ISO: set `RAYNU_F_REAL_ISO`
/// to its path and run with `--ignored`. Walks exactly what the launcher's F5c
/// path does: El Torito -> EFI FAT ESP -> \EFI\BOOT\BOOTX64.EFI -> PE32+ load.
#[test]
#[ignore]
fn raynu_f_real_iso_bootloader_path() {
    let Ok(path) = std::env::var("RAYNU_F_REAL_ISO") else {
        eprintln!("RAYNU_F_REAL_ISO not set; skipping");
        return;
    };
    let iso = std::fs::read(&path).expect("read ISO");
    let et = crate::mgmt::el_torito::parse_el_torito(&iso).expect("El Torito catalog");
    eprintln!("el torito: efi={} load_lba={} sectors={}", et.efi, et.load_lba, et.sector_count);
    assert!(et.efi, "must select the EFI (0xEF) section entry, not the BIOS default");
    let fat_off = et.load_lba as u64 * 2048;

    struct Vol<'a>(&'a [u8], u64);
    impl super::fat::VolumeRead for Vol<'_> {
        fn read_at(&self, off: u64, buf: &mut [u8]) -> bool {
            let s = (self.1 + off) as usize;
            if s + buf.len() > self.0.len() {
                return false;
            }
            buf.copy_from_slice(&self.0[s..s + buf.len()]);
            true
        }
    }
    let vol = Vol(&iso, fat_off);
    let mut boot = [0u8; 512];
    assert!(super::fat::VolumeRead::read_at(&vol, 0, &mut boot));
    let fv = super::fat::parse_bpb(&boot).expect("EFI image is FAT");
    eprintln!("fat: {:?} bps={} spc={} clusters={}", fv.kind, fv.bytes_per_sector, fv.sectors_per_cluster, fv.cluster_count);

    let e = super::fat::resolve_path(&fv, &vol, b"\\EFI\\BOOT\\BOOTX64.EFI").expect("BOOTX64.EFI present");
    eprintln!("BOOTX64.EFI: size={} first_cluster={}", e.size, e.first_cluster);
    assert!(e.size > 0 && !e.is_dir());

    let mut file = vec![0u8; e.size as usize];
    let n = super::fat::read_chain(&fv, &vol, e.first_cluster, 0, &mut file).expect("read chain");
    assert_eq!(n, file.len(), "read the whole bootloader");
    assert_eq!(&file[0..2], b"MZ");

    let pe = super::parse_pe32plus(&file).expect("bootloader is PE32+ x64");
    eprintln!("pe: entry_rva=0x{:x} image_base=0x{:x} size_of_image=0x{:x} sections={} subsystem={}",
        pe.entry_rva, pe.image_base, pe.size_of_image, pe.num_sections, pe.subsystem);
    assert_eq!(pe.subsystem, super::pe::SUBSYSTEM_EFI_APPLICATION);

    // Load it at a base the launcher's pool would hand out (relocation path).
    let mut img = vec![0u8; pe.size_of_image as usize];
    let l = super::load_pe32plus(&file, 0x00B0_0000, &mut img).expect("loads with relocations");
    eprintln!("loaded: entry=0x{:x} relocs={} sections={}", l.entry, l.relocs_applied, l.sections_loaded);
    assert_eq!(l.sections_loaded, pe.num_sections);
    // The launcher uses the GuestMem loader; prove it produces identical bytes.
    let guest = MockGuest::new(0, {
        let mut m = vec![0u8; 0x0200_0000];
        m[0x0090_0000..0x0090_0000 + file.len()].copy_from_slice(&file);
        m
    });
    let l2 = super::pe::load_pe32plus_guest(&guest, 0x0090_0000, file.len() as u64, 0x00B0_0000, pe.size_of_image as u64 + 4096)
        .expect("guest-memory loader");
    assert_eq!(l2.entry, l.entry);
    assert_eq!(l2.relocs_applied, l.relocs_applied);
    let mut back = vec![0u8; pe.size_of_image as usize];
    guest.read(0x00B0_0000, &mut back);
    assert_eq!(back, img, "slice loader and guest-memory loader agree byte for byte");
    println!("RAYNU-V-RAYNU-F-REAL-ISO-PATH-OK");
}
