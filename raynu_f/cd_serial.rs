//! Library-CD `BlockIo` view of the Ubuntu `grub.cfg` (outside Proven Core).
//!
//! Pillar: [Z] · Proven Core: **outside** (ADR-016 / ADR-019)
//!
//! `4e9ebcdb` reached EBS with no `grub.cfg` line. GRUB 2.14 reads
//! `/boot/grub/grub.cfg` from the ISO9660 filesystem through `ReadBlocks`,
//! not through `File.Open` of the El Torito FAT. The `linux` line in that
//! file is the command line the handover jump gives the kernel.
//!
//! This view does two things, only for that CD, and only in the bytes
//! returned to the guest:
//!
//! * A directory record named `grub.cfg` / `loopback.cfg` (ISO9660 or
//!   Joliet) grows its Data Length when the file's last sector has room.
//! * A later read of that extent returns the file with `console=ttyS0` on
//!   each `linux` / `linuxefi` line.
//!
//! The backing store is not written. Alpine disk reads do not call this.

use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

use super::filesystem::amend_grub_cfg_serial;

/// ISO9660 logical sector. RayNu-F publishes the CD with this block size.
const ISO_SECTOR: usize = 2048;
/// One menu file. Ubuntu's live-server `grub.cfg` is a few kilobytes.
const VIEW_CAP: usize = 32 * 1024;
/// Room for several ` console=ttyS0` insertions inside [`VIEW_CAP`].
const VIEW_SLACK: usize = 1024;
const SLOTS: usize = 2;

/// What one chunk did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CdSerialTouch {
    pub amended: bool,
    pub skipped: bool,
}

struct Slot {
    on: AtomicBool,
    off: AtomicU64,
    orig: AtomicU32,
    new_len: AtomicU32,
    bytes: UnsafeCell<[u8; VIEW_CAP]>,
}

struct View {
    slots: [Slot; SLOTS],
    raw: UnsafeCell<[u8; VIEW_CAP]>,
    /// Sizes of the first amended file, for the COM2 line.
    noted_orig: AtomicU32,
    noted_new: AtomicU32,
}

// SAFETY: firmware `ReadBlocks` is BSP-only. Host tests take one thread.
// KANI-TARGET: library CD linux-line view (outside Proven Core).
unsafe impl Sync for View {}

static VIEW: View = View {
    slots: [
        Slot {
            on: AtomicBool::new(false),
            off: AtomicU64::new(0),
            orig: AtomicU32::new(0),
            new_len: AtomicU32::new(0),
            bytes: UnsafeCell::new([0; VIEW_CAP]),
        },
        Slot {
            on: AtomicBool::new(false),
            off: AtomicU64::new(0),
            orig: AtomicU32::new(0),
            new_len: AtomicU32::new(0),
            bytes: UnsafeCell::new([0; VIEW_CAP]),
        },
    ],
    raw: UnsafeCell::new([0; VIEW_CAP]),
    noted_orig: AtomicU32::new(0),
    noted_new: AtomicU32::new(0),
};

/// Drop remembered extents. `FirmwareState` reset does not clear this.
pub fn reset_cd_linux_view() {
    for slot in &VIEW.slots {
        slot.on.store(false, Ordering::Release);
        slot.off.store(0, Ordering::Release);
        slot.orig.store(0, Ordering::Release);
        slot.new_len.store(0, Ordering::Release);
    }
    VIEW.noted_orig.store(0, Ordering::Release);
    VIEW.noted_new.store(0, Ordering::Release);
}

/// Original and amended size of the first menu file this boot, if any.
pub fn cd_linux_lens() -> Option<(u32, u32)> {
    let orig = VIEW.noted_orig.load(Ordering::Acquire);
    let new_len = VIEW.noted_new.load(Ordering::Acquire);
    if orig == 0 || new_len == 0 {
        None
    } else {
        Some((orig, new_len))
    }
}

/// Amend one library-CD `ReadBlocks` buffer in place.
///
/// `read_at` reads the original ISO at a byte offset. It must not recurse
/// into this function. `abs_off` is the byte offset of `chunk` in the ISO.
pub fn amend_library_cd_chunk(
    chunk: &mut [u8],
    abs_off: u64,
    read_at: &mut dyn FnMut(u64, &mut [u8]) -> bool,
) -> CdSerialTouch {
    let mut touch = CdSerialTouch {
        amended: false,
        skipped: false,
    };
    let mut sector = 0usize;
    while sector + ISO_SECTOR <= chunk.len() {
        let sec_off = abs_off + sector as u64;
        match consider_sector(&mut chunk[sector..sector + ISO_SECTOR], sec_off, read_at) {
            SectorHit::Amended => touch.amended = true,
            SectorHit::Skipped => touch.skipped = true,
            SectorHit::None => {}
        }
        sector += ISO_SECTOR;
    }
    if overlay(chunk, abs_off) {
        touch.amended = true;
    }
    touch
}

enum SectorHit {
    None,
    Amended,
    Skipped,
}

fn consider_sector(
    sector: &mut [u8],
    _sec_off: u64,
    read_at: &mut dyn FnMut(u64, &mut [u8]) -> bool,
) -> SectorHit {
    if sector_is_stashed_file(_sec_off) {
        return SectorHit::None;
    }
    let mut hit = SectorHit::None;
    let mut p = 0usize;
    while p + 33 < sector.len() {
        let rec_len = sector[p] as usize;
        if rec_len == 0 {
            break;
        }
        if rec_len < 34 || p + rec_len > sector.len() {
            break;
        }
        let namelen = sector[p + 32] as usize;
        if namelen > 0 && p + 33 + namelen <= sector.len() && rec_len >= 33 + namelen {
            let name = &sector[p + 33..p + 33 + namelen];
            if name_is_menu_cfg(name) {
                match arm_record(sector, p, read_at) {
                    SectorHit::Amended => hit = SectorHit::Amended,
                    SectorHit::Skipped => {
                        if !matches!(hit, SectorHit::Amended) {
                            hit = SectorHit::Skipped;
                        }
                    }
                    SectorHit::None => {}
                }
            }
        }
        p += rec_len;
    }
    hit
}

fn arm_record(
    sector: &mut [u8],
    rec: usize,
    read_at: &mut dyn FnMut(u64, &mut [u8]) -> bool,
) -> SectorHit {
    let lba = u32::from_le_bytes(sector[rec + 2..rec + 6].try_into().unwrap());
    let lba_be = u32::from_be_bytes(sector[rec + 6..rec + 10].try_into().unwrap());
    let orig = u32::from_le_bytes(sector[rec + 10..rec + 14].try_into().unwrap());
    let orig_be = u32::from_be_bytes(sector[rec + 14..rec + 18].try_into().unwrap());
    // Both endian fields match on a real directory record. That rejects
    // a `grub.cfg` byte string sitting inside vmlinuz.
    if lba == 0 || lba != lba_be || orig != orig_be {
        return SectorHit::None;
    }
    if orig == 0 || orig as usize > VIEW_CAP - VIEW_SLACK {
        return if orig as usize > VIEW_CAP - VIEW_SLACK {
            SectorHit::Skipped
        } else {
            SectorHit::None
        };
    }
    let file_off = u64::from(lba).saturating_mul(ISO_SECTOR as u64);
    if slot_for(file_off).is_some() {
        // Already armed. Still rewrite the length so a re-read of the
        // directory stays consistent with the stashed file.
        let new_len = VIEW.slots.iter().find_map(|s| {
            if s.on.load(Ordering::Acquire) && s.off.load(Ordering::Acquire) == file_off {
                Some(s.new_len.load(Ordering::Acquire))
            } else {
                None
            }
        });
        if let Some(new_len) = new_len {
            write_len(sector, rec, new_len);
            return SectorHit::Amended;
        }
    }
    // SAFETY: BSP-only, or one host test. `raw` is filled before amend.
    // KANI-TARGET: library CD grub.cfg peek (outside Proven Core).
    let raw = unsafe { &mut *VIEW.raw.get() };
    let orig_us = orig as usize;
    if !read_at(file_off, &mut raw[..orig_us]) {
        return SectorHit::None;
    }
    let pad = ISO_SECTOR - (orig_us % ISO_SECTOR);
    let pad = if orig_us % ISO_SECTOR == 0 { 0 } else { pad };
    // SAFETY: distinct from `raw`. Published only after the length is stored.
    // KANI-TARGET: library CD grub.cfg amend (outside Proven Core).
    let slot_idx = match free_slot() {
        Some(i) => i,
        None => return SectorHit::Skipped,
    };
    let amended = unsafe { &mut *VIEW.slots[slot_idx].bytes.get() };
    let Some(n) = amend_grub_cfg_serial(&raw[..orig_us], amended) else {
        return SectorHit::None;
    };
    if n < orig_us || n - orig_us > pad {
        return SectorHit::Skipped;
    }
    let new_len = n as u32;
    VIEW.slots[slot_idx].off.store(file_off, Ordering::Release);
    VIEW.slots[slot_idx].orig.store(orig, Ordering::Release);
    VIEW.slots[slot_idx].new_len.store(new_len, Ordering::Release);
    VIEW.slots[slot_idx].on.store(true, Ordering::Release);
    if VIEW.noted_orig.load(Ordering::Acquire) == 0 {
        VIEW.noted_orig.store(orig, Ordering::Release);
        VIEW.noted_new.store(new_len, Ordering::Release);
    }
    write_len(sector, rec, new_len);
    SectorHit::Amended
}

fn write_len(sector: &mut [u8], rec: usize, len: u32) {
    sector[rec + 10..rec + 14].copy_from_slice(&len.to_le_bytes());
    sector[rec + 14..rec + 18].copy_from_slice(&len.to_be_bytes());
}

fn slot_for(off: u64) -> Option<usize> {
    VIEW.slots.iter().position(|s| {
        s.on.load(Ordering::Acquire) && s.off.load(Ordering::Acquire) == off
    })
}

fn free_slot() -> Option<usize> {
    VIEW.slots.iter().position(|s| !s.on.load(Ordering::Acquire))
}

fn sector_is_stashed_file(sec_off: u64) -> bool {
    VIEW.slots.iter().any(|slot| {
        if !slot.on.load(Ordering::Acquire) {
            return false;
        }
        let off = slot.off.load(Ordering::Acquire);
        let orig = u64::from(slot.orig.load(Ordering::Acquire));
        let span = orig.saturating_add(ISO_SECTOR as u64 - 1) / ISO_SECTOR as u64 * ISO_SECTOR as u64;
        sec_off < off + span && sec_off + ISO_SECTOR as u64 > off
    })
}

fn overlay(chunk: &mut [u8], abs_off: u64) -> bool {
    let end = abs_off + chunk.len() as u64;
    let mut changed = false;
    for slot in &VIEW.slots {
        if !slot.on.load(Ordering::Acquire) {
            continue;
        }
        let off = slot.off.load(Ordering::Acquire);
        let new_len = slot.new_len.load(Ordering::Acquire) as u64;
        let file_end = off + new_len;
        if end <= off || abs_off >= file_end {
            continue;
        }
        let lo = abs_off.max(off);
        let hi = end.min(file_end);
        // SAFETY: slot bytes were published before `on`.
        // KANI-TARGET: library CD grub.cfg overlay (outside Proven Core).
        let src = unsafe { &*slot.bytes.get() };
        for abs in lo..hi {
            let i = (abs - abs_off) as usize;
            let rel = (abs - off) as usize;
            if chunk[i] != src[rel] {
                chunk[i] = src[rel];
                changed = true;
            }
        }
    }
    changed
}

fn name_is_menu_cfg(name: &[u8]) -> bool {
    iso_name_is(name, b"GRUB.CFG")
        || iso_name_is(name, b"LOOPBACK.CFG")
        || joliet_name_is(name, b"grub.cfg")
        || joliet_name_is(name, b"loopback.cfg")
}

fn iso_name_is(name: &[u8], stem: &[u8]) -> bool {
    let end = name.iter().position(|&b| b == b';').unwrap_or(name.len());
    name[..end].eq_ignore_ascii_case(stem)
}

fn joliet_name_is(name: &[u8], ascii: &[u8]) -> bool {
    if name.len() < 2 || name.len() % 2 != 0 {
        return false;
    }
    let mut got = [0u8; 32];
    let mut n = 0usize;
    for pair in name.chunks_exact(2) {
        if pair[0] != 0 || pair[1] > 0x7f {
            return false;
        }
        if pair[1] == b';' {
            break;
        }
        if n == got.len() {
            return false;
        }
        got[n] = pair[1];
        n += 1;
    }
    got[..n].eq_ignore_ascii_case(ascii)
}
