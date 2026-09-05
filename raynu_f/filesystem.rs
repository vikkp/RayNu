//! RayNu-F `EFI_SIMPLE_FILE_SYSTEM_PROTOCOL` + `EFI_FILE_PROTOCOL`
//! (UEFI 2.10 §13.4–13.5) over the FAT volume in the ISO's El Torito boot
//! image.
//!
//! Pillar: [Z] · Proven Core: **outside** (ADR-016)
//!
//! File handles are opaque tagged values indexing a fixed table, never guest
//! pointers. Each open file records its directory entry and a seek position;
//! reads go through [`super::fat`] to the CD backing store.
//!
//! Scope (honest): read-only. `Write`, `Delete`, `SetInfo` and directory
//! enumeration via `Read` on a directory return `EFI_UNSUPPORTED` /
//! `EFI_WRITE_PROTECTED` rather than pretending. A loader that only needs to
//! open and read `\EFI\BOOT\BOOTX64.EFI` is fully served.

use super::fat::{FatEntry, FatError, FatVolume};

/// `EFI_SIMPLE_FILE_SYSTEM_PROTOCOL`: Revision + OpenVolume.
pub const SFS_REVISION_OFF: usize = 0x00;
pub const SFS_OPEN_VOLUME_OFF: usize = 0x08;
pub const SFS_SIZE: usize = 0x10;
pub const SFS_REVISION: u64 = 0x0001_0000;

/// `EFI_FILE_PROTOCOL` field offsets (spec §13.5) and size.
pub const FILE_REVISION_OFF: usize = 0x00;
pub const FILE_OPEN_OFF: usize = 0x08;
pub const FILE_CLOSE_OFF: usize = 0x10;
pub const FILE_DELETE_OFF: usize = 0x18;
pub const FILE_READ_OFF: usize = 0x20;
pub const FILE_WRITE_OFF: usize = 0x28;
pub const FILE_GET_POSITION_OFF: usize = 0x30;
pub const FILE_SET_POSITION_OFF: usize = 0x38;
pub const FILE_GET_INFO_OFF: usize = 0x40;
pub const FILE_SET_INFO_OFF: usize = 0x48;
pub const FILE_FLUSH_OFF: usize = 0x50;
pub const FILE_SIZE: usize = 0x58;
/// Revision 1: no `OpenEx`/`ReadEx`/`WriteEx`/`FlushEx` (claiming rev 2 would
/// promise async entry points we do not publish).
pub const FILE_REVISION: u64 = 0x0001_0000;

/// `EFI_FILE_INFO_ID` {09576E92-6D3F-11D2-8E39-00A0C969723B}.
pub const GUID_FILE_INFO: [u8; 16] = [
    0x92, 0x6E, 0x57, 0x09, 0x3F, 0x6D, 0xD2, 0x11, 0x8E, 0x39, 0x00, 0xA0, 0xC9, 0x69, 0x72, 0x3B,
];

/// `EFI_FILE_INFO` fixed part: Size, FileSize, PhysicalSize, 3×EFI_TIME(16),
/// Attribute — then a NUL-terminated CHAR16 FileName.
pub const FILE_INFO_SIZE_OFF: usize = 0x00;
pub const FILE_INFO_FILE_SIZE_OFF: usize = 0x08;
pub const FILE_INFO_PHYSICAL_SIZE_OFF: usize = 0x10;
pub const FILE_INFO_CREATE_TIME_OFF: usize = 0x18;
pub const FILE_INFO_LAST_ACCESS_OFF: usize = 0x28;
pub const FILE_INFO_MODIFICATION_OFF: usize = 0x38;
pub const FILE_INFO_ATTRIBUTE_OFF: usize = 0x48;
pub const FILE_INFO_NAME_OFF: usize = 0x50;

/// `EFI_FILE_*` attributes.
pub const EFI_FILE_READ_ONLY: u64 = 0x01;
pub const EFI_FILE_HIDDEN: u64 = 0x02;
pub const EFI_FILE_SYSTEM: u64 = 0x04;
pub const EFI_FILE_DIRECTORY: u64 = 0x10;
pub const EFI_FILE_ARCHIVE: u64 = 0x20;

/// `Open` modes.
pub const EFI_FILE_MODE_READ: u64 = 0x0000_0000_0000_0001;
pub const EFI_FILE_MODE_WRITE: u64 = 0x0000_0000_0000_0002;
pub const EFI_FILE_MODE_CREATE: u64 = 0x8000_0000_0000_0000;

/// Open-file table size.
pub const FILE_SLOTS: usize = 16;
/// Tag for file handles.
pub const FILE_HANDLE_TAG: u64 = 0x5246_0000_0000_2000;
/// Longest path component set we accept from a guest.
pub const MAX_PATH_BYTES: usize = 256;

pub const EFI_SUCCESS: u64 = 0;
pub const EFI_INVALID_PARAMETER: u64 = 0x8000_0000_0000_0002;
pub const EFI_UNSUPPORTED: u64 = 0x8000_0000_0000_0003;
pub const EFI_BUFFER_TOO_SMALL: u64 = 0x8000_0000_0000_0005;
pub const EFI_DEVICE_ERROR: u64 = 0x8000_0000_0000_0007;
pub const EFI_WRITE_PROTECTED: u64 = 0x8000_0000_0000_0008;
pub const EFI_NOT_FOUND: u64 = 0x8000_0000_0000_000E;
pub const EFI_OUT_OF_RESOURCES: u64 = 0x8000_0000_0000_0009;
pub const EFI_ACCESS_DENIED: u64 = 0x8000_0000_0000_000F;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct OpenFile {
    used: bool,
    /// Root directory handle (no backing entry).
    is_root: bool,
    entry: FatEntry,
    position: u64,
}

const NO_ENTRY: FatEntry = FatEntry {
    name: [0; 12],
    name_len: 0,
    attr: 0,
    first_cluster: 0,
    size: 0,
};

const EMPTY: OpenFile = OpenFile {
    used: false,
    is_root: false,
    entry: NO_ENTRY,
    position: 0,
};

/// Mounted FAT volume + open-file table.
#[derive(Clone)]
pub struct FileSystem {
    pub volume: Option<FatVolume>,
    slots: [OpenFile; FILE_SLOTS],
    /// Successful file reads (host bookkeeping for markers).
    pub file_reads: u32,
}

impl FileSystem {
    pub const fn new() -> Self {
        FileSystem {
            volume: None,
            slots: [EMPTY; FILE_SLOTS],
            file_reads: 0,
        }
    }

    pub const fn handle_for(slot: usize) -> u64 {
        FILE_HANDLE_TAG | slot as u64
    }

    fn slot_of(&self, handle: u64) -> Option<usize> {
        if handle & !0xFFF != FILE_HANDLE_TAG {
            return None;
        }
        let s = (handle & 0xFFF) as usize;
        if s < FILE_SLOTS && self.slots[s].used {
            Some(s)
        } else {
            None
        }
    }

    fn alloc_slot(&mut self) -> Option<usize> {
        self.slots.iter().position(|s| !s.used)
    }

    pub fn mounted(&self) -> bool {
        self.volume.is_some()
    }

    pub fn open_count(&self) -> usize {
        self.slots.iter().filter(|s| s.used).count()
    }

    /// `OpenVolume`: hand back a handle for the root directory.
    pub fn open_volume(&mut self) -> (u64, u64) {
        if self.volume.is_none() {
            return (EFI_NOT_FOUND, 0);
        }
        let Some(s) = self.alloc_slot() else {
            return (EFI_OUT_OF_RESOURCES, 0);
        };
        self.slots[s] = OpenFile {
            used: true,
            is_root: true,
            entry: NO_ENTRY,
            position: 0,
        };
        (EFI_SUCCESS, Self::handle_for(s))
    }

    /// `Open(This, *New, FileName, OpenMode, Attributes)` — read-only.
    pub fn open<R: super::fat::VolumeRead>(
        &mut self,
        this: u64,
        path: &[u8],
        mode: u64,
        r: &R,
    ) -> (u64, u64) {
        let Some(vol) = self.volume else {
            return (EFI_NOT_FOUND, 0);
        };
        if self.slot_of(this).is_none() {
            return (EFI_INVALID_PARAMETER, 0);
        }
        if mode & (EFI_FILE_MODE_WRITE | EFI_FILE_MODE_CREATE) != 0 {
            // Honest: the CD volume is read-only.
            return (EFI_WRITE_PROTECTED, 0);
        }
        if mode & EFI_FILE_MODE_READ == 0 || path.is_empty() {
            return (EFI_INVALID_PARAMETER, 0);
        }
        match super::fat::resolve_path(&vol, r, path) {
            Ok(entry) => {
                let Some(s) = self.alloc_slot() else {
                    return (EFI_OUT_OF_RESOURCES, 0);
                };
                self.slots[s] = OpenFile {
                    used: true,
                    is_root: false,
                    entry,
                    position: 0,
                };
                (EFI_SUCCESS, Self::handle_for(s))
            }
            Err(FatError::NotFound) => (EFI_NOT_FOUND, 0),
            Err(FatError::NotADirectory) => (EFI_NOT_FOUND, 0),
            Err(_) => (EFI_DEVICE_ERROR, 0),
        }
    }

    /// `Close`.
    pub fn close(&mut self, handle: u64) -> u64 {
        match self.slot_of(handle) {
            Some(s) => {
                self.slots[s] = EMPTY;
                EFI_SUCCESS
            }
            None => EFI_INVALID_PARAMETER,
        }
    }

    /// Size of an open file (0 for the root).
    pub fn size_of(&self, handle: u64) -> Option<u64> {
        let s = self.slot_of(handle)?;
        Some(if self.slots[s].is_root {
            0
        } else {
            u64::from(self.slots[s].entry.size)
        })
    }

    pub fn is_directory(&self, handle: u64) -> Option<bool> {
        let s = self.slot_of(handle)?;
        Some(self.slots[s].is_root || self.slots[s].entry.is_dir())
    }

    pub fn position(&self, handle: u64) -> Option<u64> {
        let s = self.slot_of(handle)?;
        Some(self.slots[s].position)
    }

    /// `SetPosition`. `u64::MAX` seeks to end-of-file (spec).
    pub fn set_position(&mut self, handle: u64, pos: u64) -> u64 {
        let Some(s) = self.slot_of(handle) else {
            return EFI_INVALID_PARAMETER;
        };
        if self.slots[s].is_root || self.slots[s].entry.is_dir() {
            // Directories may only be rewound to 0.
            return if pos == 0 {
                self.slots[s].position = 0;
                EFI_SUCCESS
            } else {
                EFI_UNSUPPORTED
            };
        }
        let size = u64::from(self.slots[s].entry.size);
        self.slots[s].position = if pos == u64::MAX { size } else { pos };
        EFI_SUCCESS
    }

    /// `Read(This, *BufferSize, Buffer)` into `buf`; returns
    /// `(status, bytes_read)`. Clamps at EOF and advances the position.
    pub fn read<R: super::fat::VolumeRead>(
        &mut self,
        handle: u64,
        buf: &mut [u8],
        r: &R,
    ) -> (u64, usize) {
        let Some(vol) = self.volume else {
            return (EFI_NOT_FOUND, 0);
        };
        let Some(s) = self.slot_of(handle) else {
            return (EFI_INVALID_PARAMETER, 0);
        };
        if self.slots[s].is_root || self.slots[s].entry.is_dir() {
            // Directory enumeration is not implemented (honest).
            return (EFI_UNSUPPORTED, 0);
        }
        let size = u64::from(self.slots[s].entry.size);
        let pos = self.slots[s].position;
        if pos >= size {
            return (EFI_SUCCESS, 0); // EOF: zero bytes, not an error
        }
        let want = buf.len().min((size - pos) as usize);
        if want == 0 {
            return (EFI_SUCCESS, 0);
        }
        match super::fat::read_chain(
            &vol,
            r,
            self.slots[s].entry.first_cluster,
            pos,
            &mut buf[..want],
        ) {
            Ok(n) => {
                self.slots[s].position = pos + n as u64;
                self.file_reads = self.file_reads.saturating_add(1);
                (EFI_SUCCESS, n)
            }
            Err(_) => (EFI_DEVICE_ERROR, 0),
        }
    }

    /// Serialize `EFI_FILE_INFO` for an open handle into `out`.
    /// Returns `(status, bytes_needed)`; `EFI_BUFFER_TOO_SMALL` when short.
    pub fn file_info(&self, handle: u64, out: &mut [u8]) -> (u64, u64) {
        let Some(s) = self.slot_of(handle) else {
            return (EFI_INVALID_PARAMETER, 0);
        };
        let f = &self.slots[s];
        let name = if f.is_root { b"\\".as_slice() } else { f.entry.name_bytes() };
        // FileName is CHAR16 + NUL.
        let need = FILE_INFO_NAME_OFF as u64 + (name.len() as u64 + 1) * 2;
        if (out.len() as u64) < need {
            return (EFI_BUFFER_TOO_SMALL, need);
        }
        for b in out[..need as usize].iter_mut() {
            *b = 0;
        }
        let size = if f.is_root { 0 } else { u64::from(f.entry.size) };
        out[FILE_INFO_SIZE_OFF..FILE_INFO_SIZE_OFF + 8].copy_from_slice(&need.to_le_bytes());
        out[FILE_INFO_FILE_SIZE_OFF..FILE_INFO_FILE_SIZE_OFF + 8]
            .copy_from_slice(&size.to_le_bytes());
        out[FILE_INFO_PHYSICAL_SIZE_OFF..FILE_INFO_PHYSICAL_SIZE_OFF + 8]
            .copy_from_slice(&size.to_le_bytes());
        // EFI_TIME fields stay zero: the FAT timestamps are not plumbed
        // through and inventing them would be a lie.
        let mut attr = 0u64;
        if f.is_root || f.entry.is_dir() {
            attr |= EFI_FILE_DIRECTORY;
        }
        if f.entry.attr & super::fat::ATTR_READ_ONLY != 0 {
            attr |= EFI_FILE_READ_ONLY;
        }
        if f.entry.attr & super::fat::ATTR_HIDDEN != 0 {
            attr |= EFI_FILE_HIDDEN;
        }
        if f.entry.attr & super::fat::ATTR_SYSTEM != 0 {
            attr |= EFI_FILE_SYSTEM;
        }
        if f.entry.attr & super::fat::ATTR_ARCHIVE != 0 {
            attr |= EFI_FILE_ARCHIVE;
        }
        out[FILE_INFO_ATTRIBUTE_OFF..FILE_INFO_ATTRIBUTE_OFF + 8]
            .copy_from_slice(&attr.to_le_bytes());
        for (i, &c) in name.iter().enumerate() {
            let at = FILE_INFO_NAME_OFF + i * 2;
            out[at..at + 2].copy_from_slice(&u16::from(c).to_le_bytes());
        }
        (EFI_SUCCESS, need)
    }
}

/// Convert a guest CHAR16 path to ASCII bytes for the FAT lookup.
/// Returns `None` on a non-ASCII code unit or an over-long path.
pub fn utf16_path_to_ascii(units: &[u16], out: &mut [u8; MAX_PATH_BYTES]) -> Option<usize> {
    let mut n = 0usize;
    for &u in units {
        if u == 0 {
            break;
        }
        if n == MAX_PATH_BYTES || u > 0x7f {
            return None;
        }
        out[n] = u as u8;
        n += 1;
    }
    Some(n)
}
