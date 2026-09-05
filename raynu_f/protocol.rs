//! RayNu-F handle / protocol database (UEFI 2.10 §7.3).
//!
//! Pillar: [Z] · Proven Core: **outside** (ADR-016)
//!
//! A fixed table of `(handle, guid, interface)` triples. Handles are opaque
//! tagged values, never guest pointers, so a stray dereference faults loudly.
//! This is what lets a real loader find `BlockIo` / `SimpleFileSystem` the
//! architected way instead of us guessing at its internals.
//!
//! Not implemented (honest): driver binding / `ConnectController`, protocol
//! notify registration, and the by-child / by-driver `OpenProtocol`
//! bookkeeping (we accept and ignore agent/controller handles).

/// `EFI_GUID` as raw little-endian bytes (Data1 LE, Data2 LE, Data3 LE, Data4).
pub type Guid = [u8; 16];

/// `EFI_BLOCK_IO_PROTOCOL_GUID` {964E5B21-6459-11D2-8E39-00A0C969723B}.
pub const GUID_BLOCK_IO: Guid = [
    0x21, 0x5B, 0x4E, 0x96, 0x59, 0x64, 0xD2, 0x11, 0x8E, 0x39, 0x00, 0xA0, 0xC9, 0x69, 0x72, 0x3B,
];
/// `EFI_SIMPLE_FILE_SYSTEM_PROTOCOL_GUID` {964E5B22-6459-11D2-8E39-00A0C969723B}.
pub const GUID_SIMPLE_FILE_SYSTEM: Guid = [
    0x22, 0x5B, 0x4E, 0x96, 0x59, 0x64, 0xD2, 0x11, 0x8E, 0x39, 0x00, 0xA0, 0xC9, 0x69, 0x72, 0x3B,
];
/// `EFI_LOADED_IMAGE_PROTOCOL_GUID` {5B1B31A1-9562-11D2-8E3F-00A0C969723B}.
pub const GUID_LOADED_IMAGE: Guid = [
    0xA1, 0x31, 0x1B, 0x5B, 0x62, 0x95, 0xD2, 0x11, 0x8E, 0x3F, 0x00, 0xA0, 0xC9, 0x69, 0x72, 0x3B,
];
/// `EFI_DEVICE_PATH_PROTOCOL_GUID` {09576E91-6D3F-11D2-8E39-00A0C969723B}.
pub const GUID_DEVICE_PATH: Guid = [
    0x91, 0x6E, 0x57, 0x09, 0x3F, 0x6D, 0xD2, 0x11, 0x8E, 0x39, 0x00, 0xA0, 0xC9, 0x69, 0x72, 0x3B,
];

/// `EFI_LOAD_FILE2_PROTOCOL_GUID` {4006C0C1-FCB3-403E-996D-4A6C8724E06D}.
/// GRUB installs this (with a vendor device path) so the Linux EFI stub can
/// fetch the initrd; the kernel then calls GRUB's `LoadFile` directly,
/// guest-to-guest — we only need to hold the handle and answer
/// `LocateDevicePath`.
pub const GUID_LOAD_FILE2: Guid = [
    0xC1, 0xC0, 0x06, 0x40, 0xB3, 0xFC, 0x3E, 0x40, 0x99, 0x6D, 0x4A, 0x6C, 0x87, 0x24, 0xE0, 0x6D,
];

/// Handles RayNu-F publishes. Tagged like the console handles in `tables.rs`.
pub const HANDLE_CD: u64 = 0x5246_0000_0000_0020;
pub const HANDLE_DISK: u64 = 0x5246_0000_0000_0021;
/// Handle for the image `LoadImage` staged (F5).
pub const HANDLE_IMAGE: u64 = 0x5246_0000_0000_0022;
/// First handle minted for a guest `InstallMultipleProtocolInterfaces` with
/// `*Handle == NULL` (GRUB's initrd `LoadFile2` handle is the first).
pub const HANDLE_DYNAMIC_BASE: u64 = 0x5246_0000_0000_0100;
/// Dynamic handles we will mint before saying `EFI_OUT_OF_RESOURCES`.
pub const MAX_DYNAMIC_HANDLES: u64 = 0x100;

/// `EFI_LOADED_IMAGE_PROTOCOL` field offsets (x64) and size.
pub const LOADED_IMAGE_REVISION_OFF: usize = 0x00;
pub const LOADED_IMAGE_PARENT_OFF: usize = 0x08;
pub const LOADED_IMAGE_SYSTEM_TABLE_OFF: usize = 0x10;
pub const LOADED_IMAGE_DEVICE_HANDLE_OFF: usize = 0x18;
pub const LOADED_IMAGE_FILE_PATH_OFF: usize = 0x20;
pub const LOADED_IMAGE_RESERVED_OFF: usize = 0x28;
pub const LOADED_IMAGE_LOAD_OPTIONS_SIZE_OFF: usize = 0x30;
pub const LOADED_IMAGE_LOAD_OPTIONS_OFF: usize = 0x38;
pub const LOADED_IMAGE_IMAGE_BASE_OFF: usize = 0x40;
pub const LOADED_IMAGE_IMAGE_SIZE_OFF: usize = 0x48;
pub const LOADED_IMAGE_IMAGE_CODE_TYPE_OFF: usize = 0x50;
pub const LOADED_IMAGE_IMAGE_DATA_TYPE_OFF: usize = 0x54;
pub const LOADED_IMAGE_UNLOAD_OFF: usize = 0x58;
pub const LOADED_IMAGE_SIZE: usize = 0x60;
/// `EFI_LOADED_IMAGE_PROTOCOL_REVISION`.
pub const LOADED_IMAGE_REVISION: u32 = 0x1000;

/// A well-formed device path for the CD: a Media/CD-ROM node followed by the
/// End node. A loader reads `LoadedImage->DeviceHandle` and its device path to
/// find the volume it booted from, so a NULL here would strand it.
/// Media Device Path (type 4) / CD-ROM (subtype 2), length 0x18:
/// header(4) + BootEntry u32 + PartitionStart u64 + PartitionSize u64.
pub const DP_TYPE_MEDIA: u8 = 0x04;
pub const DP_SUBTYPE_CDROM: u8 = 0x02;
/// Media / HardDrive (subtype 1), length 0x2A (UEFI 2.10 Table 10-12):
/// header(4) + PartitionNumber u32 + PartitionStart u64 + PartitionSize u64
/// + Signature[16] + MBRType u8 + SignatureType u8.
pub const DP_SUBTYPE_HD: u8 = 0x01;
pub const DP_TYPE_END: u8 = 0x7F;
pub const DP_SUBTYPE_END_ENTIRE: u8 = 0xFF;
pub const DP_CDROM_LEN: usize = 0x18;
pub const DP_HD_LEN: usize = 0x2A;
pub const DP_END_LEN: usize = 0x04;
pub const CD_DEVICE_PATH_BYTES: usize = DP_CDROM_LEN + DP_END_LEN;
pub const HD_DEVICE_PATH_BYTES: usize = DP_HD_LEN + DP_END_LEN;
/// Firmware image slot is the max of CD (0x1C) and HD (0x2E) paths.
pub const DEVICE_PATH_BYTES: usize = HD_DEVICE_PATH_BYTES;
/// GPT (UEFI `MBRType`).
pub const DP_MBR_TYPE_GPT: u8 = 0x02;
/// GUID signature (UEFI `SignatureType`).
pub const DP_SIG_TYPE_GUID: u8 = 0x02;

/// Serialize the CD device path (`boot_entry`, and the El Torito extent in
/// 2048-byte CD blocks). The End node stays at [`DP_CDROM_LEN`]; extra bytes
/// in the (HD-sized) slot are left zero so existing CD encodings stay
/// byte-exact through the End node.
pub fn encode_cd_device_path(
    boot_entry: u32,
    partition_start: u64,
    partition_size: u64,
    out: &mut [u8; DEVICE_PATH_BYTES],
) {
    for b in out.iter_mut() {
        *b = 0;
    }
    out[0] = DP_TYPE_MEDIA;
    out[1] = DP_SUBTYPE_CDROM;
    out[2..4].copy_from_slice(&(DP_CDROM_LEN as u16).to_le_bytes());
    out[4..8].copy_from_slice(&boot_entry.to_le_bytes());
    out[8..16].copy_from_slice(&partition_start.to_le_bytes());
    out[16..24].copy_from_slice(&partition_size.to_le_bytes());
    out[DP_CDROM_LEN] = DP_TYPE_END;
    out[DP_CDROM_LEN + 1] = DP_SUBTYPE_END_ENTIRE;
    out[DP_CDROM_LEN + 2..DP_CDROM_LEN + 4]
        .copy_from_slice(&(DP_END_LEN as u16).to_le_bytes());
}

/// Serialize a GPT HardDrive device path (Media/HardDrive + End).
/// `signature` is the partition unique GUID. Byte-exact vs UEFI 2.10.
pub fn encode_hd_device_path(
    partition_number: u32,
    start_lba: u64,
    size_lba: u64,
    signature: [u8; 16],
    out: &mut [u8; DEVICE_PATH_BYTES],
) {
    for b in out.iter_mut() {
        *b = 0;
    }
    out[0] = DP_TYPE_MEDIA;
    out[1] = DP_SUBTYPE_HD;
    out[2..4].copy_from_slice(&(DP_HD_LEN as u16).to_le_bytes());
    out[4..8].copy_from_slice(&partition_number.to_le_bytes());
    out[8..16].copy_from_slice(&start_lba.to_le_bytes());
    out[16..24].copy_from_slice(&size_lba.to_le_bytes());
    out[24..40].copy_from_slice(&signature);
    out[40] = DP_MBR_TYPE_GPT;
    out[41] = DP_SIG_TYPE_GUID;
    out[DP_HD_LEN] = DP_TYPE_END;
    out[DP_HD_LEN + 1] = DP_SUBTYPE_END_ENTIRE;
    out[DP_HD_LEN + 2..DP_HD_LEN + 4].copy_from_slice(&(DP_END_LEN as u16).to_le_bytes());
}

/// `EFI_LOCATE_SEARCH_TYPE`.
pub const ALL_HANDLES: u32 = 0;
pub const BY_REGISTER_NOTIFY: u32 = 1;
pub const BY_PROTOCOL: u32 = 2;

/// Table capacity (2 devices × a few protocols, plus loader installs).
pub const PROTOCOL_SLOTS: usize = 32;
/// Max handles a `LocateHandle` result can hold.
pub const MAX_HANDLES: usize = 16;

pub const EFI_SUCCESS: u64 = 0;
pub const EFI_INVALID_PARAMETER: u64 = 0x8000_0000_0000_0002;
pub const EFI_UNSUPPORTED: u64 = 0x8000_0000_0000_0003;
pub const EFI_BUFFER_TOO_SMALL: u64 = 0x8000_0000_0000_0005;
pub const EFI_NOT_FOUND: u64 = 0x8000_0000_0000_000E;
pub const EFI_OUT_OF_RESOURCES: u64 = 0x8000_0000_0000_0009;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Entry {
    used: bool,
    handle: u64,
    guid: Guid,
    interface: u64,
}

const EMPTY: Entry = Entry {
    used: false,
    handle: 0,
    guid: [0; 16],
    interface: 0,
};

/// Fixed protocol database.
#[derive(Clone)]
pub struct Protocols {
    slots: [Entry; PROTOCOL_SLOTS],
    /// Dynamic handles minted so far (see [`HANDLE_DYNAMIC_BASE`]).
    minted: u64,
}

impl Protocols {
    pub const fn new() -> Self {
        Protocols {
            slots: [EMPTY; PROTOCOL_SLOTS],
            minted: 0,
        }
    }

    /// Mint a fresh, never-before-used handle for a guest install with
    /// `*Handle == NULL`. Tagged and never a guest pointer.
    pub fn new_handle(&mut self) -> Option<u64> {
        if self.minted >= MAX_DYNAMIC_HANDLES {
            return None;
        }
        let h = HANDLE_DYNAMIC_BASE + self.minted;
        self.minted += 1;
        Some(h)
    }

    /// Remove `guid` from `handle` if `interface` matches what is installed
    /// (UEFI `UninstallProtocolInterface`). `EFI_NOT_FOUND` otherwise.
    pub fn uninstall(&mut self, handle: u64, guid: &Guid, interface: u64) -> u64 {
        for s in self.slots.iter_mut() {
            if s.used && s.handle == handle && &s.guid == guid {
                if s.interface != interface {
                    return EFI_NOT_FOUND;
                }
                *s = EMPTY;
                return EFI_SUCCESS;
            }
        }
        EFI_NOT_FOUND
    }

    /// Whether `handle` has at least one protocol (i.e. still exists).
    pub fn handle_exists(&self, handle: u64) -> bool {
        self.slots.iter().any(|s| s.used && s.handle == handle)
    }

    /// Publish `guid` on `handle` pointing at `interface`. Replaces an
    /// existing (handle, guid) pair.
    pub fn install(&mut self, handle: u64, guid: Guid, interface: u64) -> u64 {
        if handle == 0 || interface == 0 {
            return EFI_INVALID_PARAMETER;
        }
        for s in self.slots.iter_mut() {
            if s.used && s.handle == handle && s.guid == guid {
                s.interface = interface;
                return EFI_SUCCESS;
            }
        }
        for s in self.slots.iter_mut() {
            if !s.used {
                *s = Entry {
                    used: true,
                    handle,
                    guid,
                    interface,
                };
                return EFI_SUCCESS;
            }
        }
        EFI_OUT_OF_RESOURCES
    }

    /// `HandleProtocol` / `OpenProtocol` lookup.
    pub fn interface_for(&self, handle: u64, guid: &Guid) -> Option<u64> {
        self.slots
            .iter()
            .find(|s| s.used && s.handle == handle && &s.guid == guid)
            .map(|s| s.interface)
    }

    /// First interface published for `guid` on any handle (`LocateProtocol`).
    pub fn first_interface(&self, guid: &Guid) -> Option<u64> {
        self.slots
            .iter()
            .find(|s| s.used && &s.guid == guid)
            .map(|s| s.interface)
    }

    /// Handles matching a search. Returns how many were written to `out`.
    pub fn locate(&self, search: u32, guid: Option<&Guid>, out: &mut [u64; MAX_HANDLES]) -> usize {
        let mut n = 0usize;
        for s in self.slots.iter().filter(|s| s.used) {
            let hit = match search {
                ALL_HANDLES => true,
                BY_PROTOCOL => guid.map_or(false, |g| &s.guid == g),
                _ => false,
            };
            if !hit {
                continue;
            }
            if out[..n].contains(&s.handle) {
                continue; // one entry per handle
            }
            if n == MAX_HANDLES {
                break;
            }
            out[n] = s.handle;
            n += 1;
        }
        n
    }

    /// Protocols published on `handle`.
    pub fn count_on_handle(&self, handle: u64) -> usize {
        self.slots
            .iter()
            .filter(|s| s.used && s.handle == handle)
            .count()
    }
}
