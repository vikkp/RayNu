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

/// Handles RayNu-F publishes. Tagged like the console handles in `tables.rs`.
pub const HANDLE_CD: u64 = 0x5246_0000_0000_0020;
pub const HANDLE_DISK: u64 = 0x5246_0000_0000_0021;

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
}

impl Protocols {
    pub const fn new() -> Self {
        Protocols {
            slots: [EMPTY; PROTOCOL_SLOTS],
        }
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
