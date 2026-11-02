//! RayNu-F UEFI table images (UEFI 2.10, x64). Byte-exact, `#[repr(C)]`-free:
//! we serialize into a guest memory buffer so layout is explicit and
//! host-testable without relying on Rust struct padding.
//!
//! Pillar: [Z] · Proven Core: **outside** (ADR-016)
//!
//! Every function pointer in these tables points at a RayNu-F service
//! trampoline (see `services.rs`), so a guest call becomes an I/O exit we
//! service host-side. We author every structure — nothing here belongs to a
//! foreign firmware.

use super::services::{trampoline_slot_gpa, ServiceId};

/// `EFI_TABLE_HEADER` is 24 bytes: Signature u64, Revision u32, HeaderSize
/// u32, CRC32 u32, Reserved u32.
pub const EFI_TABLE_HEADER_SIZE: usize = 24;
/// UEFI 2.10 = `(2 << 16) | 100`.
pub const EFI_2_10_SYSTEM_TABLE_REVISION: u32 = (2 << 16) | 100;

/// `IBI SYST` little-endian.
pub const EFI_SYSTEM_TABLE_SIGNATURE: u64 = 0x5453_5953_2049_4249;
/// `BOOTSERV` little-endian.
pub const EFI_BOOT_SERVICES_SIGNATURE: u64 = 0x5652_4553_544f_4f42;
/// `RUNTSERV` little-endian.
pub const EFI_RUNTIME_SERVICES_SIGNATURE: u64 = 0x5652_4553_544e_5552;

/// x64 `EFI_SYSTEM_TABLE` field offsets and size (spec 4.3).
pub const SYSTEM_TABLE_FIRMWARE_VENDOR_OFF: usize = 24;
pub const SYSTEM_TABLE_FIRMWARE_REVISION_OFF: usize = 32;
pub const SYSTEM_TABLE_CONIN_HANDLE_OFF: usize = 40;
pub const SYSTEM_TABLE_CONIN_OFF: usize = 48;
pub const SYSTEM_TABLE_CONOUT_HANDLE_OFF: usize = 56;
pub const SYSTEM_TABLE_CONOUT_OFF: usize = 64;
pub const SYSTEM_TABLE_STDERR_HANDLE_OFF: usize = 72;
pub const SYSTEM_TABLE_STDERR_OFF: usize = 80;
pub const SYSTEM_TABLE_RUNTIME_SERVICES_OFF: usize = 88;
pub const SYSTEM_TABLE_BOOT_SERVICES_OFF: usize = 96;
pub const SYSTEM_TABLE_NUM_TABLE_ENTRIES_OFF: usize = 104;
pub const SYSTEM_TABLE_CONFIG_TABLE_OFF: usize = 112;
pub const SYSTEM_TABLE_SIZE: usize = 120;

/// `EFI_BOOT_SERVICES` has 44 function pointers after the header (spec 4.4).
pub const BOOT_SERVICES_FN_COUNT: usize = 44;
pub const BOOT_SERVICES_SIZE: usize = EFI_TABLE_HEADER_SIZE + BOOT_SERVICES_FN_COUNT * 8;

/// `EFI_RUNTIME_SERVICES` has 14 function pointers after the header (spec 4.5).
pub const RUNTIME_SERVICES_FN_COUNT: usize = 14;
pub const RUNTIME_SERVICES_SIZE: usize = EFI_TABLE_HEADER_SIZE + RUNTIME_SERVICES_FN_COUNT * 8;

/// `EFI_SIMPLE_TEXT_OUTPUT_PROTOCOL`: 9 function pointers + `Mode` pointer.
pub const SIMPLE_TEXT_OUTPUT_FN_COUNT: usize = 9;
pub const SIMPLE_TEXT_OUTPUT_MODE_PTR_OFF: usize = SIMPLE_TEXT_OUTPUT_FN_COUNT * 8;
pub const SIMPLE_TEXT_OUTPUT_SIZE: usize = SIMPLE_TEXT_OUTPUT_MODE_PTR_OFF + 8;

/// `SIMPLE_TEXT_OUTPUT_MODE`: MaxMode, Mode, Attribute, CursorColumn,
/// CursorRow (INT32 each) + CursorVisible (BOOLEAN) + padding → 24 bytes.
pub const SIMPLE_TEXT_OUTPUT_MODE_SIZE: usize = 24;

/// `EFI_SIMPLE_TEXT_INPUT_PROTOCOL`: Reset, ReadKeyStroke, WaitForKey.
pub const SIMPLE_TEXT_INPUT_SIZE: usize = 24;

/// Firmware vendor string `L"RayNu-F\0"` (8 CHAR16 = 16 bytes).
pub const FIRMWARE_VENDOR_UTF16: [u16; 8] = [
    b'R' as u16,
    b'a' as u16,
    b'y' as u16,
    b'N' as u16,
    b'u' as u16,
    b'-' as u16,
    b'F' as u16,
    0,
];
/// RayNu-F firmware revision reported in the system table (0.1).
pub const FIRMWARE_REVISION: u32 = 0x0000_0001;

/// Region layout (offsets from the image base GPA). Trampolines first so the
/// code page is separable from the data tables.
pub const IMAGE_TRAMPOLINE_OFF: usize = 0x0000;
/// `EFI_LOAD_FILE2_PROTOCOL` struct the Linux stub calls for the initrd.
/// First qword is the code GPA at [`IMAGE_INITRD_SHIM_CODE_OFF`].
pub const IMAGE_INITRD_SHIM_OFF: usize = 0x0800;
/// `out dx, eax` stub. The hypervisor then enters GRUB's real `LoadFile`.
pub const IMAGE_INITRD_SHIM_CODE_OFF: usize = 0x0810;
/// End-entire device path node. GRUB accepts only this as the LoadFile remainder.
pub const IMAGE_INITRD_END_OFF: usize = 0x0830;
pub const IMAGE_SYSTEM_TABLE_OFF: usize = 0x1000;

/// GPA of the initrd `LoadFile2` struct, or 0 when `system_table` is unset.
pub const fn initrd_loadfile_struct_gpa(system_table: u64) -> u64 {
    if system_table < IMAGE_SYSTEM_TABLE_OFF as u64 {
        0
    } else {
        system_table - IMAGE_SYSTEM_TABLE_OFF as u64 + IMAGE_INITRD_SHIM_OFF as u64
    }
}

/// GPA of the End-entire node the shim can hand to GRUB.
pub const fn initrd_loadfile_end_gpa(system_table: u64) -> u64 {
    if system_table < IMAGE_SYSTEM_TABLE_OFF as u64 {
        0
    } else {
        system_table - IMAGE_SYSTEM_TABLE_OFF as u64 + IMAGE_INITRD_END_OFF as u64
    }
}
pub const IMAGE_BOOT_SERVICES_OFF: usize = 0x1100;
pub const IMAGE_RUNTIME_SERVICES_OFF: usize = 0x1300;
pub const IMAGE_CONOUT_OFF: usize = 0x1400;
pub const IMAGE_CONOUT_MODE_OFF: usize = 0x1480;
pub const IMAGE_CONIN_OFF: usize = 0x14A0;
pub const IMAGE_VENDOR_OFF: usize = 0x1500;
/// F4: `EFI_BLOCK_IO_PROTOCOL` + `EFI_BLOCK_IO_MEDIA` for the CD and disk,
/// plus the GUID constants a guest passes to `HandleProtocol`.
pub const IMAGE_BLOCKIO_CD_OFF: usize = 0x1600;
pub const IMAGE_MEDIA_CD_OFF: usize = 0x1640;
pub const IMAGE_BLOCKIO_DISK_OFF: usize = 0x1680;
pub const IMAGE_MEDIA_DISK_OFF: usize = 0x16C0;
pub const IMAGE_GUID_BLOCK_IO_OFF: usize = 0x1700;
pub const IMAGE_GUID_SIMPLE_FS_OFF: usize = 0x1710;
pub const IMAGE_GUID_LOADED_IMAGE_OFF: usize = 0x1720;
/// F5: `EFI_SIMPLE_FILE_SYSTEM_PROTOCOL`, then one `EFI_FILE_PROTOCOL`
/// instance per open-file slot (a guest calls `This->Read(This, ...)`, so
/// each handle needs its own struct), then a `LoadedImage` for the started
/// image and its `EFI_FILE_INFO` GUID.
pub const IMAGE_SFS_OFF: usize = 0x1740;
pub const IMAGE_GUID_FILE_INFO_OFF: usize = 0x1760;
pub const IMAGE_LOADED_IMAGE_OFF: usize = 0x1780;
pub const IMAGE_DEVICE_PATH_OFF: usize = 0x17E0;
/// Room for the max of CD (0x1C) and HD (0x2E) device paths (F7).
pub const IMAGE_DEVICE_PATH_CAP: usize = 0x40;
pub const IMAGE_FILE_PROTO_OFF: usize = 0x1820;
/// Stride per file-protocol instance (0x58 rounded up for alignment).
pub const IMAGE_FILE_PROTO_STRIDE: usize = 0x60;
/// `EFI_CONFIGURATION_TABLE` array behind `SystemTable->ConfigurationTable`
/// (spec 4.6): `{GUID, VOID *VendorTable}` = 24 bytes each. Guests add to it
/// through `InstallConfigurationTable` (the Linux EFI stub registers the
/// initrd it loaded; nested `166377a` died on `EFI_UNSUPPORTED` there). We
/// will later publish ACPI here (F6a).
pub const IMAGE_CONFIG_TABLE_OFF: usize = 0x2000;
pub const CONFIG_TABLE_ENTRY_BYTES: usize = 24;
pub const CONFIG_TABLE_MAX_ENTRIES: usize = 16;
/// M8.9.5 / ADR-021 D7: one page for the SMBIOS 3.0 entry point and its
/// structure table (Type 11 systemd credentials, Type 127). Published
/// through the configuration table as `EFI_SMBIOS3_TABLE_GUID` when the
/// Start carries a saved NIC; left zero otherwise.
pub const IMAGE_SMBIOS_OFF: usize = 0x3000;
pub const IMAGE_SMBIOS_BYTES: usize = 0x1000;
/// B4: RSDP and the power-button DSDT. One page after SMBIOS. No MADT.
pub const IMAGE_ACPI_OFF: usize = 0x4000;
pub const IMAGE_ACPI_BYTES: usize = 0x1000;
/// Total bytes the image needs (tables + file protos + configuration table
/// + SMBIOS + the ACPI page).
pub const IMAGE_BYTES: usize = 0x5000;
const _: () = assert!(IMAGE_SMBIOS_OFF + IMAGE_SMBIOS_BYTES == IMAGE_ACPI_OFF);
const _: () = assert!(IMAGE_ACPI_OFF + IMAGE_ACPI_BYTES == IMAGE_BYTES);
const _: () = assert!(IMAGE_SMBIOS_OFF + IMAGE_SMBIOS_BYTES <= IMAGE_BYTES);
const _: () = assert!(
    IMAGE_CONFIG_TABLE_OFF + CONFIG_TABLE_MAX_ENTRIES * CONFIG_TABLE_ENTRY_BYTES <= IMAGE_SMBIOS_OFF
);
const _: () = assert!(IMAGE_DEVICE_PATH_OFF + IMAGE_DEVICE_PATH_CAP <= IMAGE_FILE_PROTO_OFF);
const _: () = assert!(IMAGE_INITRD_END_OFF + 4 <= IMAGE_SYSTEM_TABLE_OFF);
const _: () = assert!(IMAGE_DEVICE_PATH_OFF + super::protocol::DEVICE_PATH_BYTES <= IMAGE_FILE_PROTO_OFF);
const _: () = assert!(IMAGE_FILE_PROTO_OFF + 16 * IMAGE_FILE_PROTO_STRIDE <= IMAGE_CONFIG_TABLE_OFF);
const _: () = assert!(
    IMAGE_CONFIG_TABLE_OFF + CONFIG_TABLE_MAX_ENTRIES * CONFIG_TABLE_ENTRY_BYTES <= IMAGE_BYTES
);

/// Handles are opaque non-null values the guest hands back to us. We use
/// small tagged constants rather than real memory so a stray dereference
/// faults loudly instead of aliasing a table.
pub const HANDLE_CONIN: u64 = 0x5246_0000_0000_0001;
pub const HANDLE_CONOUT: u64 = 0x5246_0000_0000_0002;
pub const HANDLE_STDERR: u64 = 0x5246_0000_0000_0003;

/// Where every table landed, as guest-physical addresses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FirmwareImageLayout {
    pub base: u64,
    pub trampolines: u64,
    pub system_table: u64,
    pub boot_services: u64,
    pub runtime_services: u64,
    pub con_out: u64,
    pub con_out_mode: u64,
    pub con_in: u64,
    pub vendor: u64,
    pub blockio_cd: u64,
    pub media_cd: u64,
    pub blockio_disk: u64,
    pub media_disk: u64,
    pub guid_block_io: u64,
    pub guid_simple_fs: u64,
    pub guid_loaded_image: u64,
    pub sfs: u64,
    pub guid_file_info: u64,
    pub loaded_image: u64,
    pub device_path: u64,
    pub file_proto_base: u64,
    /// GPA of the `EFI_CONFIGURATION_TABLE` array (count lives in the system table).
    pub config_table: u64,
    /// GPA of the SMBIOS 3.0 entry point page (zero bytes until published).
    pub smbios: u64,
    /// GPA of the ACPI page (RSDP at offset 0). Published by [`publish_acpi`].
    pub acpi: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildError {
    BufferTooSmall,
    BaseUnaligned,
    /// SMBIOS strings do not fit the SMBIOS page, or a string is malformed.
    Smbios(super::smbios::SmbiosError),
}

/// Incremental CRC32 state (IEEE 802.3, reflected, poly `0xEDB88320`).
pub const fn crc32_start() -> u32 {
    0xFFFF_FFFF
}

/// Feed bytes into an incremental CRC32.
pub fn crc32_feed(mut crc: u32, data: &[u8]) -> u32 {
    for &b in data {
        crc ^= u32::from(b);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    crc
}

/// Finalize an incremental CRC32.
pub const fn crc32_finish(crc: u32) -> u32 {
    !crc
}

/// IEEE 802.3 CRC32 — the same CRC UEFI table headers and `CalculateCrc32` use.
pub fn crc32(data: &[u8]) -> u32 {
    crc32_finish(crc32_feed(crc32_start(), data))
}

/// `ConIn->WaitForKey` is event slot 0 (pre-created in `FirmwareState`).
pub const CONIN_WAIT_KEY_EVENT: u64 = super::events::EVENT_HANDLE_TAG;

fn put_u32(buf: &mut [u8], off: usize, v: u32) {
    buf[off..off + 4].copy_from_slice(&v.to_le_bytes());
}

fn put_u64(buf: &mut [u8], off: usize, v: u64) {
    buf[off..off + 8].copy_from_slice(&v.to_le_bytes());
}

fn get_u32(buf: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]])
}

pub fn get_u64(buf: &[u8], off: usize) -> u64 {
    let mut b = [0u8; 8];
    b.copy_from_slice(&buf[off..off + 8]);
    u64::from_le_bytes(b)
}

/// Write an `EFI_TABLE_HEADER` then fix up its CRC32 over `size` bytes with
/// the CRC field zeroed (spec 4.2: "CRC32 ... computed with this field zero").
fn write_header(buf: &mut [u8], off: usize, signature: u64, revision: u32, size: usize) {
    put_u64(buf, off, signature);
    put_u32(buf, off + 8, revision);
    put_u32(buf, off + 12, size as u32);
    put_u32(buf, off + 16, 0);
    put_u32(buf, off + 20, 0);
    let c = crc32(&buf[off..off + size]);
    put_u32(buf, off + 16, c);
}

/// Verify a table header's CRC the way a guest (or `CalculateCrc32`) would.
pub fn header_crc_valid(buf: &[u8], off: usize) -> bool {
    let size = get_u32(buf, off + 12) as usize;
    if size < EFI_TABLE_HEADER_SIZE || off + size > buf.len() {
        return false;
    }
    let stored = get_u32(buf, off + 16);
    let mut tmp = [0u8; 512];
    if size > tmp.len() {
        return false;
    }
    tmp[..size].copy_from_slice(&buf[off..off + size]);
    put_u32(&mut tmp, 16, 0);
    crc32(&tmp[..size]) == stored
}

/// Serialize the whole RayNu-F firmware image into `buf` for a region based
/// at `base` (page-aligned). Every function pointer targets its trampoline.
pub fn build_firmware_image(base: u64, buf: &mut [u8]) -> Result<FirmwareImageLayout, BuildError> {
    if base & 0xfff != 0 {
        return Err(BuildError::BaseUnaligned);
    }
    if buf.len() < IMAGE_BYTES {
        return Err(BuildError::BufferTooSmall);
    }
    for b in buf[..IMAGE_BYTES].iter_mut() {
        *b = 0;
    }

    let layout = FirmwareImageLayout {
        base,
        trampolines: base + IMAGE_TRAMPOLINE_OFF as u64,
        system_table: base + IMAGE_SYSTEM_TABLE_OFF as u64,
        boot_services: base + IMAGE_BOOT_SERVICES_OFF as u64,
        runtime_services: base + IMAGE_RUNTIME_SERVICES_OFF as u64,
        con_out: base + IMAGE_CONOUT_OFF as u64,
        con_out_mode: base + IMAGE_CONOUT_MODE_OFF as u64,
        con_in: base + IMAGE_CONIN_OFF as u64,
        vendor: base + IMAGE_VENDOR_OFF as u64,
        blockio_cd: base + IMAGE_BLOCKIO_CD_OFF as u64,
        media_cd: base + IMAGE_MEDIA_CD_OFF as u64,
        blockio_disk: base + IMAGE_BLOCKIO_DISK_OFF as u64,
        media_disk: base + IMAGE_MEDIA_DISK_OFF as u64,
        guid_block_io: base + IMAGE_GUID_BLOCK_IO_OFF as u64,
        guid_simple_fs: base + IMAGE_GUID_SIMPLE_FS_OFF as u64,
        guid_loaded_image: base + IMAGE_GUID_LOADED_IMAGE_OFF as u64,
        sfs: base + IMAGE_SFS_OFF as u64,
        guid_file_info: base + IMAGE_GUID_FILE_INFO_OFF as u64,
        loaded_image: base + IMAGE_LOADED_IMAGE_OFF as u64,
        device_path: base + IMAGE_DEVICE_PATH_OFF as u64,
        file_proto_base: base + IMAGE_FILE_PROTO_OFF as u64,
        config_table: base + IMAGE_CONFIG_TABLE_OFF as u64,
        smbios: base + IMAGE_SMBIOS_OFF as u64,
        acpi: base + IMAGE_ACPI_OFF as u64,
    };

    // Trampoline page: one stub per service slot.
    super::services::write_trampolines(&mut buf[IMAGE_TRAMPOLINE_OFF..IMAGE_SYSTEM_TABLE_OFF]);
    // Initrd LoadFile2 shim. The struct's function pointer is the stub below it.
    let code_gpa = base + IMAGE_INITRD_SHIM_CODE_OFF as u64;
    buf[IMAGE_INITRD_SHIM_OFF..IMAGE_INITRD_SHIM_OFF + 8]
        .copy_from_slice(&code_gpa.to_le_bytes());
    let stub = super::services::encode_trampoline(super::services::ServiceId::InitrdLoadFile);
    buf[IMAGE_INITRD_SHIM_CODE_OFF..IMAGE_INITRD_SHIM_CODE_OFF + stub.len()]
        .copy_from_slice(&stub);
    buf[IMAGE_INITRD_END_OFF] = super::protocol::DP_TYPE_END;
    buf[IMAGE_INITRD_END_OFF + 1] = super::protocol::DP_SUBTYPE_END_ENTIRE;
    buf[IMAGE_INITRD_END_OFF + 2..IMAGE_INITRD_END_OFF + 4]
        .copy_from_slice(&4u16.to_le_bytes());

    // Firmware vendor.
    for (i, ch) in FIRMWARE_VENDOR_UTF16.iter().enumerate() {
        buf[IMAGE_VENDOR_OFF + i * 2..IMAGE_VENDOR_OFF + i * 2 + 2]
            .copy_from_slice(&ch.to_le_bytes());
    }

    // SIMPLE_TEXT_OUTPUT_MODE: one 80x25 mode, attribute light-gray, cursor
    // at origin, visible.
    let m = IMAGE_CONOUT_MODE_OFF;
    put_u32(buf, m, 1); // MaxMode
    put_u32(buf, m + 4, 0); // Mode
    put_u32(buf, m + 8, 0x07); // Attribute (EFI_LIGHTGRAY)
    put_u32(buf, m + 12, 0); // CursorColumn
    put_u32(buf, m + 16, 0); // CursorRow
    buf[m + 20] = 1; // CursorVisible

    // SIMPLE_TEXT_OUTPUT_PROTOCOL.
    let co = IMAGE_CONOUT_OFF;
    let conout_ids = [
        ServiceId::ConOutReset,
        ServiceId::ConOutOutputString,
        ServiceId::ConOutTestString,
        ServiceId::ConOutQueryMode,
        ServiceId::ConOutSetMode,
        ServiceId::ConOutSetAttribute,
        ServiceId::ConOutClearScreen,
        ServiceId::ConOutSetCursorPosition,
        ServiceId::ConOutEnableCursor,
    ];
    for (i, id) in conout_ids.iter().enumerate() {
        put_u64(buf, co + i * 8, trampoline_slot_gpa(layout.trampolines, *id));
    }
    put_u64(buf, co + SIMPLE_TEXT_OUTPUT_MODE_PTR_OFF, layout.con_out_mode);

    // SIMPLE_TEXT_INPUT_PROTOCOL. WaitForKey is a real (opaque, tagged)
    // event handle: slot 0, pre-created in `FirmwareState::new()`, signaled
    // when host serial RX has a byte.
    let ci = IMAGE_CONIN_OFF;
    put_u64(buf, ci, trampoline_slot_gpa(layout.trampolines, ServiceId::ConInReset));
    put_u64(
        buf,
        ci + 8,
        trampoline_slot_gpa(layout.trampolines, ServiceId::ConInReadKeyStroke),
    );
    put_u64(buf, ci + 16, CONIN_WAIT_KEY_EVENT);

    // EFI_BOOT_SERVICES: every slot points at its own trampoline id so a
    // guest call is attributable even before the service is implemented.
    let bs = IMAGE_BOOT_SERVICES_OFF;
    for i in 0..BOOT_SERVICES_FN_COUNT {
        let id = ServiceId::boot_service(i);
        put_u64(
            buf,
            bs + EFI_TABLE_HEADER_SIZE + i * 8,
            trampoline_slot_gpa(layout.trampolines, id),
        );
    }
    write_header(
        buf,
        bs,
        EFI_BOOT_SERVICES_SIGNATURE,
        EFI_2_10_SYSTEM_TABLE_REVISION,
        BOOT_SERVICES_SIZE,
    );

    // EFI_RUNTIME_SERVICES: same pattern.
    let rt = IMAGE_RUNTIME_SERVICES_OFF;
    for i in 0..RUNTIME_SERVICES_FN_COUNT {
        let id = ServiceId::runtime_service(i);
        put_u64(
            buf,
            rt + EFI_TABLE_HEADER_SIZE + i * 8,
            trampoline_slot_gpa(layout.trampolines, id),
        );
    }
    write_header(
        buf,
        rt,
        EFI_RUNTIME_SERVICES_SIGNATURE,
        EFI_2_10_SYSTEM_TABLE_REVISION,
        RUNTIME_SERVICES_SIZE,
    );

    // EFI_SYSTEM_TABLE.
    let st = IMAGE_SYSTEM_TABLE_OFF;
    put_u64(buf, st + SYSTEM_TABLE_FIRMWARE_VENDOR_OFF, layout.vendor);
    put_u32(buf, st + SYSTEM_TABLE_FIRMWARE_REVISION_OFF, FIRMWARE_REVISION);
    put_u64(buf, st + SYSTEM_TABLE_CONIN_HANDLE_OFF, HANDLE_CONIN);
    put_u64(buf, st + SYSTEM_TABLE_CONIN_OFF, layout.con_in);
    put_u64(buf, st + SYSTEM_TABLE_CONOUT_HANDLE_OFF, HANDLE_CONOUT);
    put_u64(buf, st + SYSTEM_TABLE_CONOUT_OFF, layout.con_out);
    put_u64(buf, st + SYSTEM_TABLE_STDERR_HANDLE_OFF, HANDLE_STDERR);
    put_u64(buf, st + SYSTEM_TABLE_STDERR_OFF, layout.con_out);
    put_u64(buf, st + SYSTEM_TABLE_RUNTIME_SERVICES_OFF, layout.runtime_services);
    put_u64(buf, st + SYSTEM_TABLE_BOOT_SERVICES_OFF, layout.boot_services);
    put_u64(buf, st + SYSTEM_TABLE_NUM_TABLE_ENTRIES_OFF, 0);
    put_u64(buf, st + SYSTEM_TABLE_CONFIG_TABLE_OFF, layout.config_table);
    write_header(
        buf,
        st,
        EFI_SYSTEM_TABLE_SIGNATURE,
        EFI_2_10_SYSTEM_TABLE_REVISION,
        SYSTEM_TABLE_SIZE,
    );

    // F4: two EFI_BLOCK_IO_PROTOCOL instances sharing one set of trampolines
    // (`This`/`MediaId` select the device). Media contents are refreshed by
    // the launcher once the real ISO / disk sizes are known.
    for (proto, media) in [
        (IMAGE_BLOCKIO_CD_OFF, layout.media_cd),
        (IMAGE_BLOCKIO_DISK_OFF, layout.media_disk),
    ] {
        put_u64(buf, proto + super::blockio::BLOCKIO_REVISION_OFF, super::blockio::BLOCKIO_REVISION2);
        put_u64(buf, proto + super::blockio::BLOCKIO_MEDIA_OFF, media);
        put_u64(
            buf,
            proto + super::blockio::BLOCKIO_RESET_OFF,
            trampoline_slot_gpa(layout.trampolines, ServiceId::BlockIoReset),
        );
        put_u64(
            buf,
            proto + super::blockio::BLOCKIO_READ_OFF,
            trampoline_slot_gpa(layout.trampolines, ServiceId::BlockIoReadBlocks),
        );
        put_u64(
            buf,
            proto + super::blockio::BLOCKIO_WRITE_OFF,
            trampoline_slot_gpa(layout.trampolines, ServiceId::BlockIoWriteBlocks),
        );
        put_u64(
            buf,
            proto + super::blockio::BLOCKIO_FLUSH_OFF,
            trampoline_slot_gpa(layout.trampolines, ServiceId::BlockIoFlushBlocks),
        );
    }
    // GUID constants a guest can point `HandleProtocol` at.
    buf[IMAGE_GUID_BLOCK_IO_OFF..IMAGE_GUID_BLOCK_IO_OFF + 16]
        .copy_from_slice(&super::protocol::GUID_BLOCK_IO);
    buf[IMAGE_GUID_SIMPLE_FS_OFF..IMAGE_GUID_SIMPLE_FS_OFF + 16]
        .copy_from_slice(&super::protocol::GUID_SIMPLE_FILE_SYSTEM);
    buf[IMAGE_GUID_LOADED_IMAGE_OFF..IMAGE_GUID_LOADED_IMAGE_OFF + 16]
        .copy_from_slice(&super::protocol::GUID_LOADED_IMAGE);
    buf[IMAGE_GUID_FILE_INFO_OFF..IMAGE_GUID_FILE_INFO_OFF + 16]
        .copy_from_slice(&super::filesystem::GUID_FILE_INFO);

    // F5: EFI_SIMPLE_FILE_SYSTEM_PROTOCOL.
    put_u64(buf, IMAGE_SFS_OFF + super::filesystem::SFS_REVISION_OFF, super::filesystem::SFS_REVISION);
    put_u64(
        buf,
        IMAGE_SFS_OFF + super::filesystem::SFS_OPEN_VOLUME_OFF,
        trampoline_slot_gpa(layout.trampolines, ServiceId::SfsOpenVolume),
    );
    // One EFI_FILE_PROTOCOL per open-file slot; all share the trampolines and
    // are distinguished by `This`.
    for slot in 0..super::filesystem::FILE_SLOTS {
        let f = IMAGE_FILE_PROTO_OFF + slot * IMAGE_FILE_PROTO_STRIDE;
        put_u64(buf, f + super::filesystem::FILE_REVISION_OFF, super::filesystem::FILE_REVISION);
        for (off, id) in [
            (super::filesystem::FILE_OPEN_OFF, ServiceId::FileOpen),
            (super::filesystem::FILE_CLOSE_OFF, ServiceId::FileClose),
            (super::filesystem::FILE_DELETE_OFF, ServiceId::FileDelete),
            (super::filesystem::FILE_READ_OFF, ServiceId::FileRead),
            (super::filesystem::FILE_WRITE_OFF, ServiceId::FileWrite),
            (super::filesystem::FILE_GET_POSITION_OFF, ServiceId::FileGetPosition),
            (super::filesystem::FILE_SET_POSITION_OFF, ServiceId::FileSetPosition),
            (super::filesystem::FILE_GET_INFO_OFF, ServiceId::FileGetInfo),
            (super::filesystem::FILE_SET_INFO_OFF, ServiceId::FileSetInfo),
            (super::filesystem::FILE_FLUSH_OFF, ServiceId::FileFlush),
        ] {
            put_u64(buf, f + off, trampoline_slot_gpa(layout.trampolines, id));
        }
    }

    Ok(layout)
}

/// Write one device's `EFI_BLOCK_IO_MEDIA` into the image buffer.
pub fn write_block_media(buf: &mut [u8], off: usize, media: &super::blockio::BlockMedia) {
    let mut m = [0u8; super::blockio::MEDIA_SIZE];
    media.encode(&mut m);
    buf[off..off + super::blockio::MEDIA_SIZE].copy_from_slice(&m);
}

/// Publish an SMBIOS 3.0 table whose Type 11 strings are `strings`, and
/// make it the first `EFI_CONFIGURATION_TABLE` entry (`EFI_SMBIOS3_TABLE_GUID`).
///
/// Call once, right after [`build_firmware_image`] and before launch: the
/// configuration table must still be empty, because a guest's later
/// `InstallConfigurationTable` reads the count from the system table and
/// appends behind this entry. Sets `NumberOfTableEntries = 1` and
/// recomputes the system table header CRC. Returns the structure table
/// byte length the guest will walk.
///
/// `buf` is the whole image (`IMAGE_BYTES`) based at `layout.base`.
pub fn publish_smbios(
    buf: &mut [u8],
    layout: &FirmwareImageLayout,
    strings: &[&[u8]],
) -> Result<usize, BuildError> {
    if buf.len() < IMAGE_BYTES {
        return Err(BuildError::BufferTooSmall);
    }
    let img = super::smbios::write_smbios(
        &mut buf[IMAGE_SMBIOS_OFF..IMAGE_SMBIOS_OFF + IMAGE_SMBIOS_BYTES],
        layout.smbios,
        strings,
    )
    .map_err(BuildError::Smbios)?;
    let e = IMAGE_CONFIG_TABLE_OFF;
    buf[e..e + 16].copy_from_slice(&super::smbios::SMBIOS3_TABLE_GUID);
    put_u64(buf, e + 16, layout.smbios);
    let st = IMAGE_SYSTEM_TABLE_OFF;
    put_u64(buf, st + SYSTEM_TABLE_NUM_TABLE_ENTRIES_OFF, 1);
    write_header(
        buf,
        st,
        EFI_SYSTEM_TABLE_SIGNATURE,
        EFI_2_10_SYSTEM_TABLE_REVISION,
        SYSTEM_TABLE_SIZE,
    );
    Ok(img.table_len)
}

/// Append the ACPI 1.0 and 2.0 configuration-table entries for the power
/// button. Call after [`publish_smbios`] when that ran, so the SMBIOS entry
/// stays first. Returns the RSDP guest-physical address.
pub fn publish_acpi(buf: &mut [u8], layout: &FirmwareImageLayout) -> Result<u64, BuildError> {
    use super::acpi_publish::{write_acpi_page, ACPI10_GUID, ACPI20_GUID, ACPI_PAGE};
    if buf.len() < IMAGE_BYTES {
        return Err(BuildError::BufferTooSmall);
    }
    let page = &mut buf[IMAGE_ACPI_OFF..IMAGE_ACPI_OFF + ACPI_PAGE];
    if !write_acpi_page(page, layout.acpi) {
        return Err(BuildError::BufferTooSmall);
    }
    let st = IMAGE_SYSTEM_TABLE_OFF;
    let n = get_u64(buf, st + SYSTEM_TABLE_NUM_TABLE_ENTRIES_OFF) as usize;
    if n + 2 > CONFIG_TABLE_MAX_ENTRIES {
        return Err(BuildError::BufferTooSmall);
    }
    let gpa = layout.acpi;
    for (i, guid) in [ACPI10_GUID, ACPI20_GUID].into_iter().enumerate() {
        let e = IMAGE_CONFIG_TABLE_OFF + (n + i) * CONFIG_TABLE_ENTRY_BYTES;
        buf[e..e + 16].copy_from_slice(&guid);
        put_u64(buf, e + 16, gpa);
    }
    put_u64(buf, st + SYSTEM_TABLE_NUM_TABLE_ENTRIES_OFF, (n + 2) as u64);
    write_header(
        buf,
        st,
        EFI_SYSTEM_TABLE_SIGNATURE,
        EFI_2_10_SYSTEM_TABLE_REVISION,
        SYSTEM_TABLE_SIZE,
    );
    Ok(gpa)
}
