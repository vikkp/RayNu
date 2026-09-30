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
/// One library POST should not print the per-exchange SOL lines.
static LOG_QUIET: AtomicBool = AtomicBool::new(false);

/// COM2 progress step during a multi-gigabyte copy. Not every 4 KiB chunk.
const LIBRARY_PROGRESS_STEP: u64 = 64 * 1024 * 1024;

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

/// One POST body. Fits the 8 KiB coexist receive buffer with the headers.
/// One Guests POST. 64 KiB, 512-byte aligned, inside the TLS accumulator.
pub const LIBRARY_HTTP_CHUNK: usize = 64 * 1024;
const _: () = assert!(LIBRARY_HTTP_CHUNK + 8 * 1024 <= crate::mgmt::tls_coexist::COEXIST_RX_ACC_N);

static RECEIVED: AtomicU64 = AtomicU64::new(0);

/// Bytes of the file received so far.
pub fn received_bytes() -> u64 {
    RECEIVED.load(Ordering::Acquire)
}

/// Read `buf` from the stored file. Host and QEMU have no spare, so this
/// returns false there. Iron reads the library LBAs.
pub fn read_at(off: u64, buf: &mut [u8]) -> bool {
    let Some(place) = held() else {
        return false;
    };
    if buf.is_empty() || off % 512 != 0 || buf.len() % 512 != 0 {
        return false;
    }
    let end = match off.checked_add(buf.len() as u64) {
        Some(e) => e,
        None => return false,
    };
    if end > place.bytes {
        return false;
    }
    let mut done = 0usize;
    while done < buf.len() {
        let Some(lba) = library_chunk_lba(off + done as u64) else {
            return false;
        };
        if !crate::mgmt::megaraid::perc_spare_read(lba, &mut buf[done..done + 512]) {
            return false;
        }
        done += 512;
    }
    true
}

/// Accept one aligned chunk of the operator's ISO.
///
/// `off == 0` starts the library. Chunks must arrive in order. The last
/// chunk arms the CD boot and leaves the Alpine answerer quiet. The bytes
/// are not patched. On iron each sector is a PERC write. Host tests accept
/// the plan without a controller.
pub fn accept_upload(off: u64, total: u64, body: &[u8]) -> Result<(), &'static str> {
    if total == 0 || total % 512 != 0 || body.is_empty() || body.len() % 512 != 0 {
        return Err("align");
    }
    if body.len() > LIBRARY_HTTP_CHUNK {
        return Err("chunk");
    }
    if off == 0 {
        clear_held();
        RECEIVED.store(0, Ordering::Release);
        let spare = spare_for_upload();
        let occupied = crate::mgmt::guest_catalog::occupied_guest_end();
        if begin_library(spare, occupied, total).is_none() {
            return Err("place");
        }
    }
    if off != RECEIVED.load(Ordering::Acquire) {
        return Err("order");
    }
    let Some(place) = held() else {
        return Err("empty");
    };
    if place.bytes != total {
        return Err("size");
    }
    let mut done = 0usize;
    while done < body.len() {
        let at = off + done as u64;
        let Some(lba) = library_chunk_lba(at) else {
            return Err("lba");
        };
        let n = upload_write_len(lba, body.len() - done);
        if !write_upload_bytes(at, &body[done..done + n]) {
            return Err("write");
        }
        done += n;
    }
    let next = off + body.len() as u64;
    RECEIVED.store(next, Ordering::Release);
    if library_progress_due(off, next, total) {
        note_library_progress(next, total);
    }
    if next == total {
        let _ = arm_cd_boot();
        note_library_stored();
    }
    Ok(())
}

/// True when `next` crosses a 64 MiB mark and is not the final byte.
/// The final byte prints `library CD stored` instead.
pub fn library_progress_due(prev: u64, next: u64, total: u64) -> bool {
    if next == 0 || next >= total || LIBRARY_PROGRESS_STEP == 0 {
        return false;
    }
    prev / LIBRARY_PROGRESS_STEP != next / LIBRARY_PROGRESS_STEP
}

/// The listen loop calls this once per exchange. True only for a library POST.
pub fn take_exchange_quiet() -> bool {
    LOG_QUIET.swap(false, Ordering::AcqRel)
}

fn spare_for_upload() -> u64 {
    let live = crate::mgmt::megaraid::perc_spare_bytes();
    if live != 0 {
        live
    } else {
        crate::mgmt::megaraid::IRON_LD1_BYTES
    }
}

/// 4096 when the spare LBA is 8-sector aligned and that many bytes remain.
fn upload_write_len(lba: u64, remain: usize) -> usize {
    let chunk = crate::mgmt::megaraid::PERC_IMAGE_CHUNK_BYTES as usize;
    if remain >= chunk && lba % u64::from(crate::mgmt::megaraid::PERC_IMAGE_CHUNK_SECTORS) == 0 {
        chunk
    } else {
        512
    }
}

fn write_upload_bytes(off: u64, bytes: &[u8]) -> bool {
    #[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
    {
        crate::mgmt::megaraid::perc_library_write(off, bytes)
    }
    #[cfg(not(all(target_os = "uefi", feature = "uefi-bin")))]
    {
        let _ = (off, bytes);
        true
    }
}

fn note_library_progress(got: u64, total: u64) {
    #[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
    {
        let mut line = [0u8; 72];
        let prefix = b"boot: M8.8 library ";
        line[..prefix.len()].copy_from_slice(prefix);
        let mut n = prefix.len();
        n += write_u64_dec(&mut line[n..], got);
        line[n] = b'/';
        n += 1;
        n += write_u64_dec(&mut line[n..], total);
        if let Ok(text) = core::str::from_utf8(&line[..n]) {
            crate::boot::serial::write_line_nowait(text);
        }
    }
    #[cfg(not(all(target_os = "uefi", feature = "uefi-bin")))]
    {
        let _ = (got, total);
    }
}

fn note_library_stored() {
    #[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
    {
        crate::boot::serial::write_line_nowait(
            "boot: M8.8 library CD stored (ubuntu-26.04-live-server-amd64.iso; bytes unchanged)",
        );
    }
}

/// `POST /perc/library/{off}/{total}` with a raw body. `None` when this is
/// not that request. Binary bodies never go through the UTF-8 HTTP parser.
pub fn library_http_response(raw: &[u8], out: &mut [u8]) -> Option<usize> {
    if !raw.starts_with(b"POST /perc/library/") {
        return None;
    }
    LOG_QUIET.store(true, Ordering::Release);
    let header_end = raw.windows(4).position(|w| w == b"\r\n\r\n")?;
    let head = core::str::from_utf8(&raw[..header_end]).ok()?;
    let body = &raw[header_end + 4..];
    if !library_authorized(head) {
        return Some(library_status(401, b"{\"ok\":false,\"reason\":\"auth\"}", out));
    }
    let (off, total) = match library_path_off(head) {
        Some(v) => v,
        None => return Some(library_status(400, b"{\"ok\":false,\"reason\":\"path\"}", out)),
    };
    match accept_upload(off, total, body) {
        Ok(()) => {
            let armed: &[u8] = if answerer_quiet() { b"true" } else { b"false" };
            let mut msg = [0u8; 80];
            let prefix = b"{\"ok\":true,\"received\":";
            msg[..prefix.len()].copy_from_slice(prefix);
            let mut n = prefix.len();
            n += write_u64_dec(&mut msg[n..], received_bytes());
            let mid = b",\"cd\":";
            msg[n..n + mid.len()].copy_from_slice(mid);
            n += mid.len();
            msg[n..n + armed.len()].copy_from_slice(armed);
            n += armed.len();
            msg[n] = b'}';
            n += 1;
            Some(library_status(200, &msg[..n], out))
        }
        Err(reason) => {
            let mut msg = [0u8; 64];
            let prefix = b"{\"ok\":false,\"reason\":\"";
            msg[..prefix.len()].copy_from_slice(prefix);
            let mut n = prefix.len();
            let rb = reason.as_bytes();
            msg[n..n + rb.len()].copy_from_slice(rb);
            n += rb.len();
            msg[n..n + 2].copy_from_slice(b"\"}");
            n += 2;
            Some(library_status(409, &msg[..n], out))
        }
    }
}

fn library_authorized(head: &str) -> bool {
    let token = head.lines().find_map(|line| {
        let rest = line
            .strip_prefix("Authorization:")
            .or_else(|| line.strip_prefix("authorization:"))?;
        let rest = rest.trim();
        crate::mgmt::http::extract_bearer_token(rest)
    });
    crate::mgmt::api::auth_allows(token)
}

fn library_path_off(head: &str) -> Option<(u64, u64)> {
    let line = head.lines().next()?;
    let path = line.split_whitespace().nth(1)?;
    let rest = path.strip_prefix("/perc/library/")?;
    let (off_s, total_s) = rest.split_once('/')?;
    let off = parse_u64(off_s)?;
    let total = parse_u64(total_s)?;
    Some((off, total))
}

fn parse_u64(s: &str) -> Option<u64> {
    if s.is_empty() || s.len() > 20 || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse().ok()
}

fn write_u64_dec(buf: &mut [u8], mut n: u64) -> usize {
    let mut tmp = [0u8; 20];
    let mut i = 20;
    if n == 0 {
        buf[0] = b'0';
        return 1;
    }
    while n > 0 {
        i -= 1;
        tmp[i] = b'0' + (n % 10) as u8;
        n /= 10;
    }
    let digits = &tmp[i..];
    buf[..digits.len()].copy_from_slice(digits);
    digits.len()
}

fn library_status(status: u16, body: &[u8], out: &mut [u8]) -> usize {
    let reason = if status == 200 {
        "OK"
    } else if status == 401 {
        "Unauthorized"
    } else if status == 409 {
        "Conflict"
    } else {
        "Bad Request"
    };
    let mut head = [0u8; 160];
    let mut n = 0;
    let p = b"HTTP/1.1 ";
    head[n..n + p.len()].copy_from_slice(p);
    n += p.len();
    n += write_u64_dec(&mut head[n..], u64::from(status));
    head[n] = b' ';
    n += 1;
    head[n..n + reason.len()].copy_from_slice(reason.as_bytes());
    n += reason.len();
    let mid = b"\r\nContent-Type: application/json\r\nContent-Length: ";
    head[n..n + mid.len()].copy_from_slice(mid);
    n += mid.len();
    n += write_u64_dec(&mut head[n..], body.len() as u64);
    let end = b"\r\nConnection: keep-alive\r\n\r\n";
    head[n..n + end.len()].copy_from_slice(end);
    n += end.len();
    if n + body.len() > out.len() {
        return 0;
    }
    out[..n].copy_from_slice(&head[..n]);
    out[n..n + body.len()].copy_from_slice(body);
    n + body.len()
}

/// Host tests only.
#[cfg(test)]
pub fn clear_library_for_test() {
    clear_held();
    DISK_BOOTED.store(false, Ordering::Release);
    RECEIVED.store(0, Ordering::Release);
    LOG_QUIET.store(false, Ordering::Release);
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

    #[test]
    fn m88_http_chunk_arms_the_cd_without_utf8() {
        clear_library_for_test();
        let total = 1024u64;
        let mut sector = [0xA5u8; 512];
        sector[0] = 0xFF;
        let mut raw = Vec::new();
        raw.extend_from_slice(
            b"POST /perc/library/0/1024 HTTP/1.1\r\nAuthorization: Bearer raynu-v-bringup\r\nContent-Length: 512\r\n\r\n",
        );
        raw.extend_from_slice(&sector);
        let mut out = [0u8; 512];
        let n = library_http_response(&raw, &mut out).unwrap();
        let text = core::str::from_utf8(&out[..n]).unwrap_or("");
        assert!(text.contains("200"), "{text}");
        assert!(take_exchange_quiet());
        assert!(!take_exchange_quiet());
        assert_eq!(received_bytes(), 512);
        assert!(!library_progress_due(0, 4096, 2_918_598_656));
        assert!(library_progress_due(64 * 1024 * 1024 - 4096, 64 * 1024 * 1024, 2_918_598_656));
        let html = include_str!("../assets/webui.html");
        assert!(html.contains("if(listBusy||up)return"));
        assert!(html.contains("if(up)return"));
        assert!(html.contains("ISO dropped"));
        assert!(html.contains("n=65536"));
        assert_eq!(LIBRARY_HTTP_CHUNK, 65536);
        assert!(!answerer_quiet());
        raw.clear();
        raw.extend_from_slice(
            b"POST /perc/library/512/1024 HTTP/1.1\r\nAuthorization: Bearer raynu-v-bringup\r\n\r\n",
        );
        raw.extend_from_slice(&sector);
        let n = library_http_response(&raw, &mut out).unwrap();
        let text = core::str::from_utf8(&out[..n]).unwrap_or("");
        assert!(text.contains("\"cd\":true"), "{text}");
        assert!(answerer_quiet());
        assert!(present_as_cd());
        let mut got = [0u8; 512];
        assert!(!read_at(0, &mut got));
        clear_library_for_test();
    }
}
