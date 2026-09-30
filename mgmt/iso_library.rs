//! M8.8 Ubuntu ISO library on RAYNU-SPARE (outside Proven Core).
//!
//! Pillar: [Z] [D]
//! Proven Core: **outside** (ADR-002 / ADR-019 decision 9).
//! VERIFICATION: L1 host tests. Host/CI never print an iron marker.
//!
//! M8.8.1 places the next guest disk after disks already on the spare, and
//! places the ISO at the high end, below the last-LBA mailbox probe.
//! M8.8.2 accepts aligned chunks into that range and does not patch them.
//! M8.8.3 presents the file as a CD and keeps the Alpine answerer quiet.
//! M8.8.4 deletes the library file only after the installed disk has booted.
//!
//! The firmware chunk writer and the CD device are the iron half. This
//! module is the placement and the policy. A failed install keeps the file.

use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use crate::mgmt::megaraid::{
    classify_ld_bytes, pack_ld_write16_tail, LdClass, TailPlacement, PERC_IMAGE_BYTES,
};

/// The file Guests will upload. Size is whatever the operator sends.
pub const UBUNTU_2604_LIVE_SERVER_NAME: &str = "ubuntu-26.04-live-server-amd64.iso";

/// Added on the kernel command line for this boot. The stored ISO is not rewritten.
pub const LIBRARY_SERIAL_ARG: &str = "console=ttyS0";

/// Last sector of RAYNU-SPARE stays the mailbox probe.
const PROBE_RESERVE: u64 = 512;

static LIB_OFF: AtomicU64 = AtomicU64::new(0);
static LIB_LEN: AtomicU64 = AtomicU64::new(0);
static CD_BOOT: AtomicBool = AtomicBool::new(false);
static DISK_BOOTED: AtomicBool = AtomicBool::new(false);

/// Where one library file sits on the spare.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LibraryPlacement {
    pub spare_off: u64,
    pub bytes: u64,
}

/// First byte a library file may not pass. `None` when `spare_bytes` is not
/// a spare-class VD or the probe sector does not fit.
pub fn library_floor(spare_bytes: u64, library_bytes: u64) -> Option<u64> {
    if classify_ld_bytes(spare_bytes) != LdClass::Spare {
        return None;
    }
    if library_bytes % 512 != 0 {
        return None;
    }
    let end = spare_bytes.checked_sub(PROBE_RESERVE)?;
    if end % 512 != 0 {
        return None;
    }
    end.checked_sub(library_bytes)
}

/// Place `iso_bytes` against the high end, growing downward.
///
/// `occupied_end` is the first free guest byte (window plus tails already
/// on the spare). The file must not meet that range, and it must not cover
/// the probe sector.
pub fn place_iso_library(
    spare_bytes: u64,
    occupied_end: u64,
    iso_bytes: u64,
) -> Option<LibraryPlacement> {
    if iso_bytes == 0 || iso_bytes % 512 != 0 || occupied_end % 512 != 0 {
        return None;
    }
    let spare_off = library_floor(spare_bytes, iso_bytes)?;
    if spare_off < occupied_end {
        return None;
    }
    Some(LibraryPlacement {
        spare_off,
        bytes: iso_bytes,
    })
}

/// Place a new guest disk at `occupied_end`, stopping before the library.
///
/// `disk_bytes` must be larger than the 8 GiB window. `occupied_end` is the
/// window when no tail exists yet, and the window plus that tail when one
/// does. `library_bytes == 0` still reserves the probe sector.
pub fn place_guest_after_occupied(
    spare_bytes: u64,
    window_bytes: u64,
    installed: bool,
    occupied_end: u64,
    library_bytes: u64,
    disk_bytes: u64,
) -> Option<TailPlacement> {
    if !installed || spare_bytes == 0 || window_bytes == 0 {
        return None;
    }
    if disk_bytes <= window_bytes || disk_bytes % 512 != 0 || window_bytes % 512 != 0 {
        return None;
    }
    if occupied_end < window_bytes || occupied_end % 512 != 0 {
        return None;
    }
    if classify_ld_bytes(spare_bytes) != LdClass::Spare {
        return None;
    }
    let limit = library_floor(spare_bytes, library_bytes)?;
    if occupied_end > limit {
        return None;
    }
    let end = occupied_end.checked_add(disk_bytes)?;
    if end > limit {
        return None;
    }
    Some(TailPlacement {
        spare_off: occupied_end,
        disk_bytes,
    })
}

/// Record one library file. Replaces a previous file only when none is held.
pub fn begin_library(spare_bytes: u64, occupied_end: u64, iso_bytes: u64) -> Option<LibraryPlacement> {
    if held().is_some() {
        return None;
    }
    let place = place_iso_library(spare_bytes, occupied_end, iso_bytes)?;
    LIB_OFF.store(place.spare_off, Ordering::Release);
    LIB_LEN.store(place.bytes, Ordering::Release);
    CD_BOOT.store(false, Ordering::Release);
    DISK_BOOTED.store(false, Ordering::Release);
    Some(place)
}

/// Bytes currently in the library. Zero when the slot is empty.
pub fn held_bytes() -> u64 {
    if LIB_LEN.load(Ordering::Acquire) == 0 {
        0
    } else {
        LIB_LEN.load(Ordering::Acquire)
    }
}

/// The stored file, if M8.8.2 has begun.
pub fn held() -> Option<LibraryPlacement> {
    let bytes = LIB_LEN.load(Ordering::Acquire);
    if bytes == 0 {
        return None;
    }
    Some(LibraryPlacement {
        spare_off: LIB_OFF.load(Ordering::Acquire),
        bytes,
    })
}

/// Spare LBA for one 512-byte chunk of the stored file.
///
/// `off` is the offset within the ISO. The chunk is not patched. A range
/// outside the file, or inside the guest disks, is `None`.
pub fn library_chunk_lba(off: u64) -> Option<u64> {
    let place = held()?;
    if off % 512 != 0 {
        return None;
    }
    let end = off.checked_add(512)?;
    if end > place.bytes {
        return None;
    }
    let spare = place.spare_off.checked_add(off)?;
    if spare < PERC_IMAGE_BYTES {
        return None;
    }
    Some(spare / 512)
}

/// Pack one WRITE(16) of a library chunk. The window and the probe sector
/// are not this write. UBUNTU0 sizes return `None`.
pub fn pack_library_chunk(
    spare_bytes: u64,
    target_id: u8,
    off: u64,
    data_phys: u64,
) -> Option<[u8; 64]> {
    let place = held()?;
    let lba = library_chunk_lba(off)?;
    pack_ld_write16_tail(
        spare_bytes,
        target_id,
        lba,
        1,
        data_phys,
        place.spare_off,
        place.bytes,
    )
}

/// M8.8.3. The next guest boot uses the library file as a CD.
/// The Alpine answerer does not type `root` or `setup-disk`.
pub fn arm_cd_boot() -> bool {
    if held().is_none() {
        return false;
    }
    CD_BOOT.store(true, Ordering::Release);
    true
}

/// True while the Ubuntu CD boot is the installer.
pub fn answerer_quiet() -> bool {
    CD_BOOT.load(Ordering::Acquire) && held().is_some()
}

/// The library file is the CD for this boot.
pub fn present_as_cd() -> bool {
    answerer_quiet()
}

/// M8.8.4. The installed disk has booted, so the ISO can leave the spare.
/// Returns true when a held file was deleted.
pub fn note_installed_disk_booted() -> bool {
    if !CD_BOOT.load(Ordering::Acquire) || held().is_none() {
        return false;
    }
    DISK_BOOTED.store(true, Ordering::Release);
    clear_held();
    true
}

/// A failed install keeps the file so the operator does not upload it again.
pub fn note_install_failed() {
    DISK_BOOTED.store(false, Ordering::Release);
}

fn clear_held() {
    LIB_OFF.store(0, Ordering::Release);
    LIB_LEN.store(0, Ordering::Release);
    CD_BOOT.store(false, Ordering::Release);
}

/// Host tests only.
#[cfg(test)]
pub fn clear_library_for_test() {
    clear_held();
    DISK_BOOTED.store(false, Ordering::Release);
}

#[cfg(test)]
mod iso_library_test {
    use super::*;
    use crate::mgmt::megaraid::{IRON_LD1_BYTES, LAB_UBUNTU0_BYTES, PERC_IMAGE_BYTES};

    #[test]
    fn m88_places_the_next_disk_after_the_tail_and_the_iso_at_the_high_end() {
        clear_library_for_test();
        let spare = IRON_LD1_BYTES;
        let window = PERC_IMAGE_BYTES;
        let ten = 10_240u64 * 1024 * 1024;
        let first = place_guest_after_occupied(spare, window, true, window, 0, ten).unwrap();
        assert_eq!(first.spare_off, window);
        let occupied = window + ten;
        let second = place_guest_after_occupied(spare, window, true, occupied, 0, ten).unwrap();
        assert_eq!(second.spare_off, occupied);
        assert!(second.spare_off + second.disk_bytes <= spare - PROBE_RESERVE);
        let iso = 2u64 * 1024 * 1024 * 1024;
        let lib = place_iso_library(spare, occupied, iso).unwrap();
        assert_eq!(lib.bytes, iso);
        assert_eq!(lib.spare_off + lib.bytes, spare - PROBE_RESERVE);
        assert!(lib.spare_off >= occupied);
        assert!(place_guest_after_occupied(spare, window, true, occupied, iso, ten).is_some());
        let huge = lib.spare_off - occupied;
        assert!(place_guest_after_occupied(spare, window, true, occupied, iso, huge + 512).is_none());
        assert!(place_iso_library(spare, lib.spare_off + 512, iso).is_none());
        assert!(place_iso_library(LAB_UBUNTU0_BYTES, window, iso).is_none());
        assert!(place_guest_after_occupied(spare, window, false, window, 0, ten).is_none());
        assert!(place_guest_after_occupied(spare, window, true, window, 0, window).is_none());
    }

    #[test]
    fn m88_upload_boot_and_reclaim_keep_a_failed_install() {
        clear_library_for_test();
        let spare = IRON_LD1_BYTES;
        let window = PERC_IMAGE_BYTES;
        let ten = 10_240u64 * 1024 * 1024;
        let iso = 2u64 * 1024 * 1024 * 1024;
        let occupied = window + ten;
        let place = begin_library(spare, occupied, iso).unwrap();
        assert!(begin_library(spare, occupied, iso).is_none());
        assert_eq!(held_bytes(), iso);
        assert_eq!(
            library_chunk_lba(0),
            Some(place.spare_off / 512)
        );
        assert!(library_chunk_lba(1).is_none());
        assert!(library_chunk_lba(iso).is_none());
        assert!(pack_library_chunk(spare, 1, 0, 0x1006000).is_some());
        assert!(pack_library_chunk(LAB_UBUNTU0_BYTES, 1, 0, 0x1006000).is_none());
        assert!(!answerer_quiet());
        assert!(arm_cd_boot());
        assert!(present_as_cd());
        assert!(answerer_quiet());
        assert_eq!(LIBRARY_SERIAL_ARG, "console=ttyS0");
        assert_eq!(
            UBUNTU_2604_LIVE_SERVER_NAME,
            "ubuntu-26.04-live-server-amd64.iso"
        );
        note_install_failed();
        assert_eq!(held_bytes(), iso);
        assert!(note_installed_disk_booted());
        assert_eq!(held_bytes(), 0);
        assert!(!answerer_quiet());
        assert!(!note_installed_disk_booted());
        clear_library_for_test();
    }
}
