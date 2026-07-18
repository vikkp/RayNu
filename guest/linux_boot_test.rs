use super::*;

#[test]
fn marker_and_magic() {
    assert_eq!(M3_LOAD_OK_MARKER, "RAYNU-V-M3-LOAD-OK");
    assert_eq!(SETUP_HEADER_MAGIC, 0x5372_6448);
    // "HdrS" little-endian bytes
    let bytes = SETUP_HEADER_MAGIC.to_le_bytes();
    assert_eq!(&bytes, b"HdrS");
}

#[test]
fn pack_boot_params_fields() {
    let mut buf = [0u8; BOOT_PARAMS_SIZE];
    pack_boot_params(&mut buf, 0x2000_0000, 0x1000, 0x1000_0000, 64 * 1024 * 1024);

    assert_eq!(setup_magic(&buf), SETUP_HEADER_MAGIC);
    assert_eq!(read_u32(&buf, OFF_RAMDISK_IMAGE), 0x2000_0000);
    assert_eq!(read_u32(&buf, OFF_RAMDISK_SIZE), 0x1000);
    assert_eq!(read_u32(&buf, OFF_CMD_LINE_PTR), 0x1000_0000);
    assert_eq!(buf[OFF_E820_ENTRIES], 1);
    assert_eq!(buf[OFF_LOADFLAGS] & LOADFLAGS_LOADED_HIGH, LOADFLAGS_LOADED_HIGH);
    assert_eq!(read_u16(&buf, OFF_VERSION), 0x020C);
    assert_eq!(read_u16(&buf, OFF_BOOT_FLAG), 0xAA55);

    // e820[0] = { addr=0, size=64MiB, type=RAM }
    assert_eq!(read_u64(&buf, OFF_E820_TABLE), 0);
    assert_eq!(read_u64(&buf, OFF_E820_TABLE + 8), 64 * 1024 * 1024);
    assert_eq!(read_u32(&buf, OFF_E820_TABLE + 16), E820_RAM);
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
