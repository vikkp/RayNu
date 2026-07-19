use super::*;

#[test]
fn marker_and_magic() {
    assert_eq!(M3_LOAD_OK_MARKER, "RAYNU-V-M3-LOAD-OK");
    assert_eq!(M3_BZIMAGE_OK_MARKER, "RAYNU-V-M3-BZIMAGE-OK");
    assert_eq!(SETUP_HEADER_MAGIC, 0x5372_6448);
    // "HdrS" little-endian bytes
    let bytes = SETUP_HEADER_MAGIC.to_le_bytes();
    assert_eq!(&bytes, b"HdrS");
    assert!(PROTO_EARLY_LINE.starts_with(b"Linux version"));
}

#[test]
fn minimal_bzimage_parses_and_entry_offset() {
    let mut buf = [0u8; MINIMAL_BZIMAGE_CAP];
    let n = build_minimal_bzimage(&mut buf);
    let img = &buf[..n];
    let info = parse_bzimage(img).expect("parse minimal bzImage");
    assert_eq!(info.setup_magic, SETUP_HEADER_MAGIC);
    assert_eq!(info.setup_sects, 4);
    assert_eq!(info.pm_offset, 5 * 512);
    assert_eq!(info.entry_file_off, info.pm_offset + BZIMAGE_ENTRY_OFFSET);
    assert!(info.pm_size >= BZIMAGE_ENTRY_OFFSET + 64);
    // Proto starts with test rsi,rsi at the 64-bit entry.
    assert_eq!(
        &img[info.entry_file_off..info.entry_file_off + 3],
        &[0x48, 0x85, 0xF6]
    );
}

/// Optional fixture writer: `RAYNU_WRITE_BZIMAGE=path cargo test …write_minimal…`
#[test]
fn write_minimal_bzimage_fixture() {
    let mut buf = [0u8; MINIMAL_BZIMAGE_CAP];
    let n = build_minimal_bzimage(&mut buf);
    assert!(parse_bzimage(&buf[..n]).is_ok());
    if let Ok(path) = std::env::var("RAYNU_WRITE_BZIMAGE") {
        std::fs::write(&path, &buf[..n]).unwrap_or_else(|e| panic!("write {path}: {e}"));
        eprintln!("wrote {path} ({n} bytes)");
    }
}

#[test]
fn proto_kernel_encodes_hdrs_check_and_outs() {
    let mut page = [0u8; 4096];
    let phys = page.as_mut_ptr() as u64;
    unsafe { write_synth_kernel(phys) };
    // Starts with test rsi,rsi
    assert_eq!(&page[0..3], &[0x48, 0x85, 0xF6]);
    // Contains out dx,al (0xEE) and mov edx,0x3f8
    assert!(page[..512].windows(5).any(|w| w == [0xBA, 0xF8, 0x03, 0x00, 0x00]));
    assert!(page[..512].contains(&0xEE));
    // First OUT byte of early line is 'L'
    let mut found_l = false;
    let mut i = 0;
    while i + 7 < 512 {
        if page[i] == 0xBA
            && page[i + 1] == 0xF8
            && page[i + 5] == 0xB0
            && page[i + 6] == b'L'
            && page[i + 7] == 0xEE
        {
            found_l = true;
            break;
        }
        i += 1;
    }
    assert!(found_l, "expected first COM1 OUT of 'Linux version…'");
}

#[test]
fn proto_init_encodes_shell_outs() {
    let mut page = [0u8; 4096];
    let phys = page.as_mut_ptr() as u64;
    unsafe { write_proto_init(phys) };
    assert!(PROTO_SHELL_LINE.starts_with(b"init:"));
    // First OUT is 'i' of init:
    let mut found = false;
    let mut i = 0;
    while i + 7 < 256 {
        if page[i] == 0xBA
            && page[i + 1] == 0xF8
            && page[i + 5] == 0xB0
            && page[i + 6] == b'i'
            && page[i + 7] == 0xEE
        {
            found = true;
            break;
        }
        i += 1;
    }
    assert!(found, "expected proto-init COM1 OUT of 'init:…'");
    assert!(page[..512].contains(&0xF4)); // hlt (OUTs are ~8 bytes each)
}

#[test]
fn pack_boot_params_fields() {
    let mut buf = [0u8; BOOT_PARAMS_SIZE];
    pack_boot_params(&mut buf, 0x2000_0000, 0x1000, 0x1000_0000, 64 * 1024 * 1024);

    assert_eq!(setup_magic(&buf), SETUP_HEADER_MAGIC);
    assert_eq!(read_u32(&buf, OFF_RAMDISK_IMAGE), 0x2000_0000);
    assert_eq!(read_u32(&buf, OFF_RAMDISK_SIZE), 0x1000);
    assert_eq!(read_u32(&buf, OFF_CMD_LINE_PTR), 0x1000_0000);
    // Linux append_e820_table() rejects nr_entries < 2.
    assert_eq!(buf[OFF_E820_ENTRIES], 3);
    assert_eq!(buf[OFF_SENTINEL], 0);
    assert_eq!(buf[OFF_LOADFLAGS] & LOADFLAGS_LOADED_HIGH, LOADFLAGS_LOADED_HIGH);
    assert_eq!(read_u16(&buf, OFF_VERSION), 0x020C);
    assert_eq!(read_u16(&buf, OFF_BOOT_FLAG), 0xAA55);

    // e820[0] low RAM; e820[1] reserved hole; e820[2] extended above 1 MiB.
    assert_eq!(read_u64(&buf, OFF_E820_TABLE), 0);
    assert_eq!(read_u64(&buf, OFF_E820_TABLE + 8), E820_LOW_RAM_END);
    assert_eq!(read_u32(&buf, OFF_E820_TABLE + 16), E820_RAM);
    let e1 = OFF_E820_TABLE + E820_ENTRY_SIZE;
    assert_eq!(read_u64(&buf, e1), E820_LOW_RAM_END);
    assert_eq!(read_u64(&buf, e1 + 8), E820_HIGH_RAM_START - E820_LOW_RAM_END);
    assert_eq!(read_u32(&buf, e1 + 16), E820_RESERVED);
    let e2 = OFF_E820_TABLE + 2 * E820_ENTRY_SIZE;
    assert_eq!(read_u64(&buf, e2), E820_HIGH_RAM_START);
    assert_eq!(
        read_u64(&buf, e2 + 8),
        64 * 1024 * 1024 - E820_HIGH_RAM_START
    );
    assert_eq!(read_u32(&buf, e2 + 16), E820_RAM);
    assert_eq!(
        read_u32(&buf, OFF_ALT_MEM_K),
        ((64 * 1024 * 1024 - E820_HIGH_RAM_START) / 1024) as u32
    );
    assert!(read_u16(&buf, OFF_EXT_MEM_K) > 0);
}

#[test]
fn real_linux_cmdline_has_memmap_backup() {
    let s = core::str::from_utf8(REAL_LINUX_CMDLINE).unwrap();
    assert!(s.contains("rdinit=/init"));
    assert!(s.contains("memmap=640K@0"));
    assert!(s.contains("memmap=255M@1M"));
    assert!(s.contains("nolapic"));
    assert!(s.contains("lpj="));
    assert!(s.contains("clocksource=tsc"));
    assert!(s.contains("tsc=reliable"));
    assert!(s.contains("idle=poll"));
    assert!(!s.contains("notsc"));
}

#[test]
fn align_up_and_workspace_slack() {
    assert_eq!(align_up_u64(0x178b_000, 0x20_0000), 0x180_0000);
    assert_eq!(align_up_u64(0x180_0000, 0x20_0000), 0x180_0000);
    assert_eq!(align_up_u64(0x178b_000, 1), 0x178b_000);
    // Fixture path: no alignment slack.
    assert_eq!(bzimage_workspace_bytes(4096, 4096, 0x1000_0000, false), 4096);
    // Real Linux: init_size + 2MiB so align_up(load) + init_size fits.
    let init = 0x9b_2000;
    let align = 0x20_0000;
    assert_eq!(bzimage_workspace_bytes(930_816, init, align, true), init + align);
    // Latitude bug: unaligned load at 0x178b000 needed through 0x1800000+init.
    let load = 0x178b_000u64;
    let output = align_up_u64(load, align as u64);
    assert_eq!(output, 0x180_0000);
    let need_end = output + init as u64;
    let alloc_end = load + bzimage_workspace_bytes(930_816, init, align, true) as u64;
    assert!(alloc_end >= need_end, "0x{alloc_end:x} < 0x{need_end:x}");
}

#[test]
fn claim_load_pages_exclusive() {
    let pages = [
        (0x10_0000, PhysFrame::from_phys(0x10_0000)),
        (0x11_0000, PhysFrame::from_phys(0x11_0000)),
    ];
    assert!(claim_load_pages(&pages).is_ok());
}

#[test]
fn claim_rejects_hpa_alias() {
    let frame = PhysFrame::from_phys(0x20_0000);
    let pages = [(0x20_0000, frame), (0x21_0000, frame)];
    assert!(claim_load_pages(&pages).is_err());
}

fn read_u16(buf: &[u8], off: usize) -> u16 {
    buf[off] as u16 | ((buf[off + 1] as u16) << 8)
}

fn read_u64(buf: &[u8], off: usize) -> u64 {
    let mut v = 0u64;
    for i in 0..8 {
        v |= (buf[off + i] as u64) << (8 * i);
    }
    v
}
