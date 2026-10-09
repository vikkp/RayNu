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
//! open and read `\EFI\BOOT\BOOTX64.EFI` is fully served. A relative
//! `File.Open` is resolved against the directory in `This`. An empty name
//! duplicates that handle.

use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicU32, Ordering};

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
    /// Non-zero: this handle reads the library `grub.cfg` serial view of
    /// this length. The FAT bytes stay the original file.
    view_len: u32,
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
    view_len: 0,
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
            view_len: 0,
        };
        (EFI_SUCCESS, Self::handle_for(s))
    }

    fn duplicate(&mut self, slot: usize) -> (u64, u64) {
        let Some(s) = self.alloc_slot() else {
            return (EFI_OUT_OF_RESOURCES, 0);
        };
        let mut copy = self.slots[slot];
        copy.position = 0;
        self.slots[s] = copy;
        (EFI_SUCCESS, Self::handle_for(s))
    }

    /// `Open(This, *New, FileName, OpenMode, Attributes)` — read-only.
    ///
    /// An empty `FileName` duplicates `This` (UEFI 2.10 §13.5). A path that
    /// starts with `\` or `/` is from the volume root. Any other path is
    /// relative to the directory `This`.
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
        let Some(this_slot) = self.slot_of(this) else {
            return (EFI_INVALID_PARAMETER, 0);
        };
        if mode & (EFI_FILE_MODE_WRITE | EFI_FILE_MODE_CREATE) != 0 {
            // Honest: the CD volume is read-only.
            return (EFI_WRITE_PROTECTED, 0);
        }
        if mode & EFI_FILE_MODE_READ == 0 {
            return (EFI_INVALID_PARAMETER, 0);
        }
        if path.is_empty() {
            return self.duplicate(this_slot);
        }
        let start = if path[0] == b'\\' || path[0] == b'/' {
            0
        } else if self.slots[this_slot].is_root {
            0
        } else if self.slots[this_slot].entry.is_dir() {
            self.slots[this_slot].entry.first_cluster
        } else {
            return (EFI_INVALID_PARAMETER, 0);
        };
        match super::fat::resolve_in(&vol, r, start, path) {
            Ok(entry) => {
                let Some(s) = self.alloc_slot() else {
                    return (EFI_OUT_OF_RESOURCES, 0);
                };
                self.slots[s] = OpenFile {
                    used: true,
                    is_root: false,
                    entry,
                    position: 0,
                    view_len: 0,
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

    /// Size of an open file (0 for the root). A library serial view reports
    /// the amended length. The FAT directory entry stays the original size.
    pub fn size_of(&self, handle: u64) -> Option<u64> {
        let s = self.slot_of(handle)?;
        Some(self.logical_size(s))
    }

    fn logical_size(&self, slot: usize) -> u64 {
        let f = &self.slots[slot];
        if f.is_root {
            0
        } else if f.view_len != 0 {
            u64::from(f.view_len)
        } else {
            u64::from(f.entry.size)
        }
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
        let size = self.logical_size(s);
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
        if self.slots[s].view_len != 0 {
            return self.read_serial_view(s, buf);
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
        let mut name_buf = [0u8; 12];
        let name_len;
        let is_root;
        let is_dir;
        let attr_bits;
        {
            let f = &self.slots[s];
            is_root = f.is_root;
            is_dir = f.entry.is_dir();
            attr_bits = f.entry.attr;
            if is_root {
                name_buf[0] = b'\\';
                name_len = 1;
            } else {
                let raw = f.entry.name_bytes();
                name_len = raw.len().min(name_buf.len());
                name_buf[..name_len].copy_from_slice(&raw[..name_len]);
            }
        }
        let name = &name_buf[..name_len];
        // FileName is CHAR16 + NUL.
        let need = FILE_INFO_NAME_OFF as u64 + (name.len() as u64 + 1) * 2;
        if (out.len() as u64) < need {
            return (EFI_BUFFER_TOO_SMALL, need);
        }
        for b in out[..need as usize].iter_mut() {
            *b = 0;
        }
        let size = self.logical_size(s);
        out[FILE_INFO_SIZE_OFF..FILE_INFO_SIZE_OFF + 8].copy_from_slice(&need.to_le_bytes());
        out[FILE_INFO_FILE_SIZE_OFF..FILE_INFO_FILE_SIZE_OFF + 8]
            .copy_from_slice(&size.to_le_bytes());
        out[FILE_INFO_PHYSICAL_SIZE_OFF..FILE_INFO_PHYSICAL_SIZE_OFF + 8]
            .copy_from_slice(&size.to_le_bytes());
        // EFI_TIME fields stay zero: the FAT timestamps are not plumbed
        // through and inventing them would be a lie.
        let mut attr = 0u64;
        if is_root || is_dir {
            attr |= EFI_FILE_DIRECTORY;
        }
        if attr_bits & super::fat::ATTR_READ_ONLY != 0 {
            attr |= EFI_FILE_READ_ONLY;
        }
        if attr_bits & super::fat::ATTR_HIDDEN != 0 {
            attr |= EFI_FILE_HIDDEN;
        }
        if attr_bits & super::fat::ATTR_SYSTEM != 0 {
            attr |= EFI_FILE_SYSTEM;
        }
        if attr_bits & super::fat::ATTR_ARCHIVE != 0 {
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

    fn read_serial_view(&mut self, slot: usize, buf: &mut [u8]) -> (u64, usize) {
        let view_len = self.slots[slot].view_len;
        let cluster = self.slots[slot].entry.first_cluster;
        let pos = self.slots[slot].position;
        let size = u64::from(view_len);
        if pos >= size {
            return (EFI_SUCCESS, 0);
        }
        if LIBRARY_GRUB_CFG.cluster.load(Ordering::Acquire) != cluster
            || LIBRARY_GRUB_CFG.len.load(Ordering::Acquire) != view_len
        {
            return (EFI_DEVICE_ERROR, 0);
        }
        let want = buf.len().min((size - pos) as usize);
        if want == 0 {
            return (EFI_SUCCESS, 0);
        }
        // SAFETY: BSP-only firmware dispatch, or one host test thread. The
        // view bytes are published before `len`/`cluster`, and this read
        // copies them out before any later arm replaces the buffer.
        // KANI-TARGET: library grub.cfg serial view read (outside Proven Core).
        unsafe {
            let src = &*LIBRARY_GRUB_CFG.bytes.get();
            let start = pos as usize;
            buf[..want].copy_from_slice(&src[start..start + want]);
        }
        self.slots[slot].position = pos + want as u64;
        self.file_reads = self.file_reads.saturating_add(1);
        (EFI_SUCCESS, want)
    }

    /// Build the library-CD `grub.cfg` view for this handle.
    ///
    /// `linux` / `linuxefi` lines gain `console=ttyS0` in the bytes later
    /// `Read` calls return. The FAT chain is not written. A file that does
    /// not need the argument stays on the original size. A file that does
    /// not fit in the view is left unchanged (`TooBig`).
    pub fn arm_library_serial_view<R: super::fat::VolumeRead>(
        &mut self,
        handle: u64,
        r: &R,
    ) -> SerialView {
        let Some(s) = self.slot_of(handle) else {
            return SerialView::Unchanged;
        };
        if self.slots[s].is_root || self.slots[s].entry.is_dir() {
            return SerialView::Unchanged;
        }
        let orig = self.slots[s].entry.size as usize;
        if orig == 0 {
            return SerialView::Unchanged;
        }
        if orig > GRUB_CFG_VIEW_CAP - GRUB_CFG_SERIAL_SLACK {
            return SerialView::TooBig;
        }
        let cluster = self.slots[s].entry.first_cluster;
        let vol = match self.volume {
            Some(v) => v,
            None => return SerialView::Unchanged,
        };
        // SAFETY: same BSP / single-thread owner as `read_serial_view`. The
        // raw buffer is private until `amend_grub_cfg_serial` finishes, and
        // the view is published only after it is complete.
        // KANI-TARGET: library grub.cfg serial view build (outside Proven Core).
        let raw = unsafe { &mut *LIBRARY_GRUB_CFG.raw.get() };
        let mut filled = 0usize;
        while filled < orig {
            let chunk = (orig - filled).min(4096);
            match super::fat::read_chain(&vol, r, cluster, filled as u64, &mut raw[filled..filled + chunk])
            {
                Ok(n) if n == chunk => filled += n,
                _ => return SerialView::Unchanged,
            }
        }
        let view = unsafe { &mut *LIBRARY_GRUB_CFG.bytes.get() };
        let needs = cfg_line_needs_serial(&raw[..orig]);
        let Some(n) = amend_grub_cfg_serial(&raw[..orig], view) else {
            return if needs {
                SerialView::TooBig
            } else {
                SerialView::Unchanged
            };
        };
        LIBRARY_GRUB_CFG.len.store(n as u32, Ordering::Release);
        LIBRARY_GRUB_CFG.cluster.store(cluster, Ordering::Release);
        self.slots[s].view_len = n as u32;
        self.slots[s].position = 0;
        SerialView::Amended
    }
}

/// How `arm_library_serial_view` left the open `grub.cfg`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SerialView {
    /// No `linux` line needed the argument, or the handle was not a file.
    Unchanged,
    /// Later reads return the amended bytes. The FAT file was not written.
    Amended,
    /// The file does not fit in the view. Reads stay on the original bytes.
    TooBig,
}

/// Bytes reserved so a handful of `linux` lines can grow inside the view.
pub const GRUB_CFG_SERIAL_SLACK: usize = 512;
/// One amended `grub.cfg`. The El Torito menu file is a few kilobytes.
pub const GRUB_CFG_VIEW_CAP: usize = 16 * 1024;

struct LibraryGrubCfg {
    cluster: AtomicU32,
    len: AtomicU32,
    bytes: UnsafeCell<[u8; GRUB_CFG_VIEW_CAP]>,
    raw: UnsafeCell<[u8; GRUB_CFG_VIEW_CAP]>,
}

// SAFETY: the firmware dispatcher is BSP-only. Host tests take the suite
// one thread at a time. The buffer is not shared with the guest.
// KANI-TARGET: library grub.cfg view static (outside Proven Core).
unsafe impl Sync for LibraryGrubCfg {}

static LIBRARY_GRUB_CFG: LibraryGrubCfg = LibraryGrubCfg {
    cluster: AtomicU32::new(0),
    len: AtomicU32::new(0),
    bytes: UnsafeCell::new([0; GRUB_CFG_VIEW_CAP]),
    raw: UnsafeCell::new([0; GRUB_CFG_VIEW_CAP]),
};

/// Drop the amended view. File handles are cleared with `FirmwareState`.
pub fn reset_library_grub_cfg_view() {
    LIBRARY_GRUB_CFG.cluster.store(0, Ordering::Release);
    LIBRARY_GRUB_CFG.len.store(0, Ordering::Release);
}

/// True when the last path component is `grub.cfg` (any case).
pub fn path_is_grub_cfg(path: &[u8]) -> bool {
    let name = path_tail(path);
    name.eq_ignore_ascii_case(b"grub.cfg")
}

/// True when the last path component is `grubx64.efi` (any case).
pub fn path_is_grubx64(path: &[u8]) -> bool {
    let name = match path.iter().rposition(|&c| c == b'\\' || c == b'/') {
        Some(i) => &path[i + 1..],
        None => path,
    };
    name.eq_ignore_ascii_case(b"grubx64.efi")
}

fn path_tail(path: &[u8]) -> &[u8] {
    match path.iter().rposition(|&c| c == b'\\' || c == b'/') {
        Some(i) => &path[i + 1..],
        None => path,
    }
}

fn trim_ascii_start(s: &[u8]) -> &[u8] {
    let mut i = 0;
    while i < s.len() && (s[i] == b' ' || s[i] == b'\t') {
        i += 1;
    }
    &s[i..]
}

fn bytes_contains(hay: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty() && hay.windows(needle.len()).any(|w| w == needle)
}

/// First token is `linux` or `linuxefi`, and the line does not already name
/// the serial console. `linux16` and comments stay as they are.
fn line_needs_serial(line: &[u8]) -> bool {
    let s = trim_ascii_start(line);
    if !has_token(s, b"linuxefi") && !has_token(s, b"linux") {
        return false;
    }
    !bytes_contains(s, crate::mgmt::iso_library::LIBRARY_SERIAL_ARG.as_bytes())
}

fn has_token(s: &[u8], token: &[u8]) -> bool {
    s.len() >= token.len()
        && s.starts_with(token)
        && (s.len() == token.len()
            || s[token.len()] == b' '
            || s[token.len()] == b'\t')
}

fn cfg_line_needs_serial(src: &[u8]) -> bool {
    let mut i = 0;
    while i < src.len() {
        let start = i;
        while i < src.len() && src[i] != b'\n' {
            i += 1;
        }
        let mut end = i;
        if end > start && src[end - 1] == b'\r' {
            end -= 1;
        }
        if line_needs_serial(&src[start..end]) {
            return true;
        }
        if i < src.len() {
            i += 1;
        }
    }
    false
}

/// Append `console=ttyS0` to each `linux` / `linuxefi` line that lacks it.
///
/// Returns the amended length, including the original newlines. `None` when
/// no line changes, or when the result does not fit in `out` (nothing is
/// written in that case).
pub fn amend_grub_cfg_serial(src: &[u8], out: &mut [u8]) -> Option<usize> {
    let arg = crate::mgmt::iso_library::LIBRARY_SERIAL_ARG.as_bytes();
    let cloud = crate::mgmt::iso_library::LIBRARY_CLOUD_INIT_ARG.as_bytes();
    let extra_len = 1 + arg.len() + 1 + cloud.len();
    let mut n = 0usize;
    let mut changed = false;
    let mut i = 0usize;
    while i < src.len() {
        let start = i;
        while i < src.len() && src[i] != b'\n' {
            i += 1;
        }
        let mut end = i;
        let has_nl = i < src.len();
        if end > start && src[end - 1] == b'\r' {
            end -= 1;
        }
        let extra = if line_needs_serial(&src[start..end]) {
            changed = true;
            extra_len
        } else {
            0
        };
        n = n.saturating_add(end - start).saturating_add(extra);
        if end < i {
            n = n.saturating_add(1); // CR
        }
        if has_nl {
            n = n.saturating_add(1);
            i += 1;
        }
    }
    if !changed || n > out.len() {
        return None;
    }
    let mut w = 0usize;
    i = 0;
    while i < src.len() {
        let start = i;
        while i < src.len() && src[i] != b'\n' {
            i += 1;
        }
        let mut end = i;
        let has_nl = i < src.len();
        if end > start && src[end - 1] == b'\r' {
            end -= 1;
        }
        out[w..w + (end - start)].copy_from_slice(&src[start..end]);
        w += end - start;
        if line_needs_serial(&src[start..end]) {
            out[w] = b' ';
            w += 1;
            out[w..w + arg.len()].copy_from_slice(arg);
            w += arg.len();
            out[w] = b' ';
            w += 1;
            out[w..w + cloud.len()].copy_from_slice(cloud);
            w += cloud.len();
        }
        if end < i {
            out[w] = b'\r';
            w += 1;
        }
        if has_nl {
            out[w] = b'\n';
            w += 1;
            i += 1;
        }
    }
    Some(w)
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
