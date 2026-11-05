//! ADR-021 A7. Two hypervisor images on the spare, and a pending slot.
//!
//! Pillar: [Z]
//! Proven Core: **outside** (ADR-002)
//!
//! The header sector and the two images sit 4 GiB below the NIC catalog,
//! under the Ubuntu library and above the guest tail. Guests are not
//! rewritten. UBUNTU0 is refused. LBA 0 is refused.
//!
//! The SPA stages the inactive image in chunks. Rollback drops a pending
//! slot. Confirm keeps it and arms a stick copy. The next boot, while UEFI
//! file protocols are still up, writes that image over `EFI/BOOT/BOOTX64.EFI`
//! on the loaded volume and keeps the previous file as `BOOTX64.OLD`.
//! The boot after that is the one that runs the new image. Host tests do
//! not print an iron marker.

use sha2::{Digest, Sha256};

use crate::mgmt::api::RestMethod;
use crate::mgmt::megaraid::{classify_ld_bytes, LdClass, SPARE_HIGH_RESERVE};

pub const MAGIC: &[u8; 8] = b"RAYNUFW7";
const VERSION: u8 = 1;
const PENDING_NONE: u8 = 0xff;
const CHECK_OFF: usize = 508;
/// One staged image. The release EFI is about 2.5 MiB.
pub const SLOT_BYTES: usize = 4 * 1024 * 1024;
const SLOT_SECTORS: u64 = (SLOT_BYTES as u64) / 512;
/// Distance from the catalog down to the header, past the stored ISO.
const BELOW_CATALOG: u64 = 4 * 1024 * 1024 * 1024;

/// Where the header and the two images sit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FwMap {
    pub header_lba: u64,
    pub slot_lba: [u64; 2],
}

/// Saved lengths. `pending == None` means the next boot keeps `active`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FwRecord {
    pub active: u8,
    pub pending: Option<u8>,
    /// Slot to copy onto the boot stick on the next pre-EBS pass.
    pub apply: Option<u8>,
    /// The stick file was replaced. Roll back restores `BOOTX64.OLD`.
    pub swapped: bool,
    /// Next pre-EBS pass copies `BOOTX64.OLD` over `BOOTX64.EFI`.
    pub revert: bool,
    pub len: [u32; 2],
    pub sha: [[u8; 32]; 2],
    pub durable: bool,
}

impl FwRecord {
    pub const fn empty() -> Self {
        Self {
            active: 0,
            pending: None,
            apply: None,
            swapped: false,
            revert: false,
            len: [0, 0],
            sha: [[0; 32], [0; 32]],
            durable: false,
        }
    }
}

/// Header LBA and the two image bases. `None` when the VD is not the spare
/// or the images would touch LBA 0.
pub fn fw_map(spare_bytes: u64) -> Option<FwMap> {
    if classify_ld_bytes(spare_bytes) != LdClass::Spare {
        return None;
    }
    let catalog = spare_bytes.checked_sub(SPARE_HIGH_RESERVE)?;
    if catalog % 512 != 0 || catalog == 0 {
        return None;
    }
    let header_off = catalog.checked_sub(BELOW_CATALOG)?;
    if header_off % 512 != 0 || header_off < 512 {
        return None;
    }
    let header_lba = header_off / 512;
    let slot1 = header_lba.checked_sub(SLOT_SECTORS)?;
    let slot0 = slot1.checked_sub(SLOT_SECTORS)?;
    if slot0 == 0 {
        return None;
    }
    Some(FwMap {
        header_lba,
        slot_lba: [slot0, slot1],
    })
}

/// LBA of one 512-byte piece of a slot. `None` when it would leave the slot.
pub fn chunk_lba(map: FwMap, slot: u8, off: u64) -> Option<u64> {
    if slot > 1 || off % 512 != 0 || off >= SLOT_BYTES as u64 {
        return None;
    }
    map.slot_lba[slot as usize].checked_add(off / 512)
}

pub fn encode(rec: &FwRecord, buf: &mut [u8; 512]) -> bool {
    if rec.active > 1 {
        return false;
    }
    if let Some(p) = rec.pending {
        if p > 1 {
            return false;
        }
    }
    buf.fill(0);
    buf[..8].copy_from_slice(MAGIC);
    buf[8] = VERSION;
    buf[9] = rec.active;
    buf[10] = rec.pending.unwrap_or(PENDING_NONE);
    buf[11] = u8::from(rec.durable);
    buf[12] = rec.apply.unwrap_or(PENDING_NONE);
    buf[13] = u8::from(rec.swapped);
    buf[14] = u8::from(rec.revert);
    for i in 0..2 {
        let at = 16 + i * 36;
        buf[at..at + 4].copy_from_slice(&rec.len[i].to_le_bytes());
        buf[at + 4..at + 36].copy_from_slice(&rec.sha[i]);
    }
    let sum = checksum(&buf[..CHECK_OFF]);
    buf[CHECK_OFF..CHECK_OFF + 4].copy_from_slice(&sum.to_le_bytes());
    true
}

pub fn decode(buf: &[u8]) -> Option<FwRecord> {
    if buf.len() < 512 || &buf[..8] != MAGIC || buf[8] != VERSION {
        return None;
    }
    let sum = u32::from_le_bytes(buf[CHECK_OFF..CHECK_OFF + 4].try_into().ok()?);
    if sum != checksum(&buf[..CHECK_OFF]) {
        return None;
    }
    let active = buf[9];
    if active > 1 {
        return None;
    }
    let pending = match buf[10] {
        PENDING_NONE => None,
        p if p <= 1 => Some(p),
        _ => return None,
    };
    let apply = match buf[12] {
        PENDING_NONE => None,
        p if p <= 1 => Some(p),
        _ => return None,
    };
    let swapped = buf[13] != 0;
    let revert = buf[14] != 0;
    let mut len = [0u32; 2];
    let mut sha = [[0u8; 32]; 2];
    for i in 0..2 {
        let at = 16 + i * 36;
        len[i] = u32::from_le_bytes(buf[at..at + 4].try_into().ok()?);
        if len[i] as usize > SLOT_BYTES {
            return None;
        }
        sha[i].copy_from_slice(&buf[at + 4..at + 36]);
    }
    Some(FwRecord {
        active,
        pending,
        apply,
        swapped,
        revert,
        len,
        sha,
        durable: buf[11] != 0,
    })
}

fn checksum(buf: &[u8]) -> u32 {
    let mut sum = 0u32;
    for &b in buf {
        sum = sum.wrapping_add(u32::from(b));
    }
    sum
}

/// `MZ` and a `PE\0\0` signature inside the image.
pub fn is_pe(buf: &[u8]) -> bool {
    if buf.len() < 0x40 || &buf[..2] != b"MZ" {
        return false;
    }
    let e = u32::from_le_bytes(buf[0x3C..0x40].try_into().unwrap()) as usize;
    e + 4 <= buf.len() && &buf[e..e + 4] == b"PE\0\0"
}

fn sha256(buf: &[u8]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(buf);
    h.finalize().into()
}

struct State {
    rec: FwRecord,
    image: [u8; SLOT_BYTES],
    filled: usize,
    dirty: bool,
    /// A chunk upload is in progress. The spare image is not written yet.
    staging: bool,
    /// Next image byte not yet written to the spare.
    acked: usize,
}

impl State {
    const fn empty() -> Self {
        Self {
            rec: FwRecord::empty(),
            image: [0; SLOT_BYTES],
            filled: 0,
            dirty: false,
            staging: false,
            acked: 0,
        }
    }
}

static mut STATE: State = State::empty();

fn state() -> &'static mut State {
    unsafe { &mut *core::ptr::addr_of_mut!(STATE) }
}

/// Accept one chunk of the inactive slot. `off` must continue the image.
pub fn accept_chunk(off: usize, total: usize, body: &[u8]) -> Result<(), &'static str> {
    if total == 0 || total > SLOT_BYTES || body.is_empty() || off.checked_add(body.len()).is_none()
    {
        return Err("size");
    }
    if off + body.len() > total {
        return Err("size");
    }
    let st = state();
    if off == 0 {
        st.staging = true;
        st.filled = 0;
        st.acked = 0;
        if st.rec.pending.is_some() {
            st.rec.pending = None;
            st.rec.durable = false;
            st.dirty = true;
        }
        st.image[..body.len()].copy_from_slice(body);
        st.filled = body.len();
    } else if !st.staging || off != st.filled {
        return Err("order");
    } else {
        st.image[off..off + body.len()].copy_from_slice(body);
        st.filled = off + body.len();
    }
    if st.filled != total {
        return Ok(());
    }
    let image = &st.image[..st.filled];
    if !is_pe(image) {
        st.staging = false;
        st.filled = 0;
        return Err("pe");
    }
    let digest = sha256(image);
    let inactive = 1 - st.rec.active;
    st.rec.len[inactive as usize] = st.filled as u32;
    st.rec.sha[inactive as usize] = digest;
    st.rec.pending = Some(inactive);
    st.rec.durable = false;
    st.staging = false;
    st.dirty = true;
    st.acked = 0;
    Ok(())
}

/// Drop a pending slot, or arm restoring `BOOTX64.OLD`.
///
/// A pending upload or an armed copy is cancelled. Once the stick file has
/// been replaced, roll back asks the next boot to put `BOOTX64.OLD` back.
pub fn rollback() -> bool {
    let st = state();
    if st.rec.pending.is_some() || st.rec.apply.is_some() {
        st.rec.pending = None;
        st.rec.apply = None;
        st.rec.durable = false;
        st.staging = false;
        st.dirty = true;
        st.filled = 0;
        return true;
    }
    if st.rec.swapped && !st.rec.revert {
        st.rec.revert = true;
        st.dirty = true;
        return true;
    }
    false
}

/// The pending slot becomes the one to keep.
pub fn confirm() -> bool {
    let st = state();
    let Some(p) = st.rec.pending else {
        return false;
    };
    if st.staging || !st.rec.durable || st.rec.len[p as usize] == 0 {
        return false;
    }
    st.rec.active = p;
    st.rec.pending = None;
    st.rec.apply = Some(p);
    st.dirty = true;
    true
}

/// The slot a pre-EBS boot should copy onto `BOOTX64.EFI`.
pub fn stick_plan(rec: &FwRecord) -> Option<(u8, u32, [u8; 32])> {
    let slot = rec.apply?;
    if rec.revert || !rec.durable || slot > 1 {
        return None;
    }
    let len = rec.len[slot as usize];
    if (len as usize) < 0x44 || len as usize > SLOT_BYTES {
        return None;
    }
    Some((slot, len, rec.sha[slot as usize]))
}

/// Read `len` bytes of a slot image through `read_at` (byte offset, buffer).
/// Keeps the image only when it is a PE and the SHA-256 matches.
pub fn read_stick_image(
    len: u32,
    sha: [u8; 32],
    mut read_at: impl FnMut(u64, &mut [u8]) -> bool,
) -> bool {
    let len = len as usize;
    if len < 0x44 || len > SLOT_BYTES {
        return false;
    }
    let st = state();
    let mut off = 0usize;
    while off < len {
        let n = (len - off).min(512);
        let mut sector = [0u8; 512];
        if !read_at(off as u64, &mut sector[..n]) {
            st.filled = 0;
            return false;
        }
        st.image[off..off + n].copy_from_slice(&sector[..n]);
        off += n;
    }
    st.filled = len;
    if !is_pe(&st.image[..len]) || sha256(&st.image[..len]) != sha {
        st.filled = 0;
        return false;
    }
    true
}

/// Bytes accepted by [`read_stick_image`].
pub fn stick_image() -> Option<&'static [u8]> {
    let st = state();
    if st.filled < 0x44 {
        return None;
    }
    Some(&st.image[..st.filled])
}

static STICK_WRITTEN: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);

/// The pre-EBS copy finished. The post-EBS header clear uses this.
pub fn note_stick_written() {
    STICK_WRITTEN.store(true, core::sync::atomic::Ordering::Release);
}

/// Drop the stick-apply slot after a successful copy. The image bytes stay.
pub fn ack_stick_write() -> bool {
    if !STICK_WRITTEN.swap(false, core::sync::atomic::Ordering::AcqRel) {
        return false;
    }
    let st = state();
    if st.rec.apply.is_none() {
        return false;
    }
    st.rec.apply = None;
    st.rec.swapped = true;
    st.dirty = true;
    true
}

static STICK_RESTORED: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);

/// The pre-EBS restore of `BOOTX64.OLD` finished.
pub fn note_stick_restored() {
    STICK_RESTORED.store(true, core::sync::atomic::Ordering::Release);
}

/// Drop the restore request after `BOOTX64.OLD` is back on the stick.
pub fn ack_stick_restore() -> bool {
    if !STICK_RESTORED.swap(false, core::sync::atomic::Ordering::AcqRel) {
        return false;
    }
    let st = state();
    if !st.rec.revert {
        return false;
    }
    st.rec.revert = false;
    st.rec.swapped = false;
    st.dirty = true;
    true
}

pub fn record() -> FwRecord {
    state().rec
}

/// JSON object for `GET /host/update` and the overview poll.
pub fn status_body(buf: &mut [u8]) -> usize {
    let rec = record();
    let pend = match rec.pending {
        Some(p) => p as i32,
        None => -1,
    };
    let apply = match rec.apply {
        Some(p) => p as i32,
        None => -1,
    };
    let body = format_record(
        rec.active,
        pend,
        rec.len[0],
        rec.len[1],
        rec.durable,
        apply,
        rec.revert,
        rec.swapped,
    );
    if body.n > buf.len() {
        return 0;
    }
    buf[..body.n].copy_from_slice(&body.buf[..body.n]);
    body.n
}

pub fn restore(buf: &[u8]) -> bool {
    let Some(rec) = decode(buf) else {
        return false;
    };
    let st = state();
    st.rec = rec;
    st.dirty = false;
    st.filled = 0;
    st.staging = false;
    st.acked = rec.len[rec.pending.unwrap_or(0) as usize] as usize;
    true
}

pub fn copy_header() -> Option<[u8; 512]> {
    let st = state();
    if !st.dirty {
        return None;
    }
    if let Some(slot) = st.rec.pending {
        if st.acked < st.rec.len[slot as usize] as usize {
            return None;
        }
    }
    let mut buf = [0u8; 512];
    if !encode(&st.rec, &mut buf) {
        return None;
    }
    Some(buf)
}

pub fn ack_header() {
    state().dirty = false;
}

/// Next 512 bytes of the pending image that still need a spare write.
pub fn next_image_sector(spare_bytes: u64) -> Option<(u64, [u8; 512])> {
    let st = state();
    if st.staging {
        return None;
    }
    let slot = st.rec.pending?;
    let len = st.rec.len[slot as usize] as usize;
    if st.rec.durable || st.acked >= len || len == 0 {
        return None;
    }
    let map = fw_map(spare_bytes)?;
    let lba = chunk_lba(map, slot, st.acked as u64)?;
    let mut sector = [0u8; 512];
    let n = (len - st.acked).min(512);
    sector[..n].copy_from_slice(&st.image[st.acked..st.acked + n]);
    Some((lba, sector))
}

pub fn ack_image_sector() {
    let st = state();
    let Some(slot) = st.rec.pending else {
        return;
    };
    let len = st.rec.len[slot as usize] as usize;
    st.acked = (st.acked + 512).min(len);
    if st.acked >= len {
        st.rec.durable = true;
        st.dirty = true;
    }
}

pub enum UpdateHttp {
    NotMine,
    Unauthorized,
    Bad,
    Json {
        status: u16,
        len: usize,
        body: [u8; 256],
    },
}

pub fn host_update_rest(method: RestMethod, path: &str, authed: bool) -> UpdateHttp {
    if path != "/host/update" && path != "/host/update/rollback" && path != "/host/update/confirm" {
        return UpdateHttp::NotMine;
    }
    if !authed {
        return UpdateHttp::Unauthorized;
    }
    if path == "/host/update" && matches!(method, RestMethod::Get) {
        let mut body = [0u8; 160];
        let n = status_body(&mut body);
        return json_status(200, &body[..n]);
    }
    if !matches!(method, RestMethod::Post) {
        return UpdateHttp::Bad;
    }
    if path == "/host/update/rollback" {
        return if rollback() {
            let rec = record();
            let apply = match rec.apply {
                Some(p) => p as i32,
                None => -1,
            };
            let body = format_record(
                rec.active,
                -1,
                rec.len[0],
                rec.len[1],
                rec.durable,
                apply,
                rec.revert,
                rec.swapped,
            );
            json_status(200, &body.buf[..body.n])
        } else {
            json_status(409, b"{\"ok\":false,\"reason\":\"none\"}")
        };
    }
    if confirm() {
        let rec = record();
        let pend = match rec.pending {
            Some(p) => p as i32,
            None => -1,
        };
        let apply = match rec.apply {
            Some(p) => p as i32,
            None => -1,
        };
        let body = format_record(
            rec.active,
            pend,
            rec.len[0],
            rec.len[1],
            rec.durable,
            apply,
            rec.revert,
            rec.swapped,
        );
        json_status(200, &body.buf[..body.n])
    } else {
        json_status(409, b"{\"ok\":false,\"reason\":\"none\"}")
    }
}

/// `POST /host/update/chunk/{off}/{total}` with a raw body.
pub fn update_http_response(raw: &[u8], out: &mut [u8]) -> Option<usize> {
    if !raw.starts_with(b"POST /host/update/chunk/") {
        return None;
    }
    let header_end = raw.windows(4).position(|w| w == b"\r\n\r\n")?;
    let head = core::str::from_utf8(&raw[..header_end]).ok()?;
    let body = &raw[header_end + 4..];
    if !chunk_authorized(head) {
        return Some(status_line(401, b"{\"ok\":false,\"reason\":\"auth\"}", out));
    }
    let line = head.lines().next()?;
    let path = line.split_whitespace().nth(1)?;
    let rest = path.strip_prefix("/host/update/chunk/")?;
    let (off_s, total_s) = rest.split_once('/')?;
    let off: usize = parse_usize(off_s)?;
    let total: usize = parse_usize(total_s)?;
    match accept_chunk(off, total, body) {
        Ok(()) => Some(status_line(200, b"{\"ok\":true}", out)),
        Err(_) => Some(status_line(
            409,
            b"{\"ok\":false,\"reason\":\"chunk\"}",
            out,
        )),
    }
}

fn chunk_authorized(head: &str) -> bool {
    let token = head.lines().find_map(|line| {
        let rest = line
            .strip_prefix("Authorization:")
            .or_else(|| line.strip_prefix("authorization:"))?;
        crate::mgmt::http::extract_bearer_token(rest.trim())
    });
    crate::mgmt::api::auth_allows(token)
}

fn parse_usize(s: &str) -> Option<usize> {
    if s.is_empty() || s.len() > 10 || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse().ok()
}

fn format_record(
    active: u8,
    pending: i32,
    len0: u32,
    len1: u32,
    durable: bool,
    apply: i32,
    revert: bool,
    swapped: bool,
) -> RecordJson {
    let mut s = RecordJson {
        buf: [0; 160],
        n: 0,
    };
    s.push(b"{\"active\":");
    s.push_dec(u32::from(active));
    s.push(b",\"pending\":");
    if pending < 0 {
        s.push(b"null");
    } else {
        s.push_dec(pending as u32);
    }
    s.push(b",\"len0\":");
    s.push_dec(len0);
    s.push(b",\"len1\":");
    s.push_dec(len1);
    s.push(if durable {
        b",\"durable\":true"
    } else {
        b",\"durable\":false"
    });
    s.push(b",\"apply\":");
    if apply < 0 {
        s.push(b"null");
    } else {
        s.push_dec(apply as u32);
    }
    s.push(if revert {
        b",\"revert\":true"
    } else {
        b",\"revert\":false"
    });
    s.push(if swapped {
        b",\"swapped\":true}"
    } else {
        b",\"swapped\":false}"
    });
    s
}

struct RecordJson {
    buf: [u8; 160],
    n: usize,
}

impl RecordJson {
    fn push(&mut self, bytes: &[u8]) {
        for &b in bytes {
            if self.n >= self.buf.len() {
                break;
            }
            self.buf[self.n] = b;
            self.n += 1;
        }
    }
    fn push_dec(&mut self, mut n: u32) {
        let mut tmp = [0u8; 10];
        let mut i = 10;
        loop {
            i -= 1;
            tmp[i] = b'0' + (n % 10) as u8;
            n /= 10;
            if n == 0 {
                break;
            }
        }
        self.push(&tmp[i..]);
    }
}

fn json_status(status: u16, body: &[u8]) -> UpdateHttp {
    let mut out = [0u8; 256];
    let n = body.len().min(out.len());
    out[..n].copy_from_slice(&body[..n]);
    UpdateHttp::Json {
        status,
        len: n,
        body: out,
    }
}

fn status_line(code: u16, body: &[u8], out: &mut [u8]) -> usize {
    let prefix: &[u8] = if code == 200 {
        b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: keep-alive\r\nContent-Length: "
    } else if code == 401 {
        b"HTTP/1.1 401 Unauthorized\r\nContent-Type: application/json\r\nConnection: keep-alive\r\nContent-Length: "
    } else {
        b"HTTP/1.1 409 Conflict\r\nContent-Type: application/json\r\nConnection: keep-alive\r\nContent-Length: "
    };
    if prefix.len() + 8 + body.len() > out.len() {
        return 0;
    }
    out[..prefix.len()].copy_from_slice(prefix);
    let mut n = prefix.len();
    let mut len = body.len();
    let mut tmp = [0u8; 8];
    let mut i = 8;
    loop {
        i -= 1;
        tmp[i] = b'0' + (len % 10) as u8;
        len /= 10;
        if len == 0 {
            break;
        }
    }
    out[n..n + (8 - i)].copy_from_slice(&tmp[i..]);
    n += 8 - i;
    out[n..n + 4].copy_from_slice(b"\r\n\r\n");
    n += 4;
    out[n..n + body.len()].copy_from_slice(body);
    n + body.len()
}

/// Boot line after a spare read. Empty when the sector was not ours.
pub fn boot_line(buf: &[u8]) -> Option<[u8; 64]> {
    let rec = decode(buf)?;
    let mut line = [0u8; 64];
    let prefix = b"boot: A7 slot=";
    line[..prefix.len()].copy_from_slice(prefix);
    let mut n = prefix.len();
    line[n] = b'0' + rec.active;
    n += 1;
    let mid = b" pending=";
    line[n..n + mid.len()].copy_from_slice(mid);
    n += mid.len();
    match rec.pending {
        None => {
            line[n..n + 4].copy_from_slice(b"none");
            n += 4;
        }
        Some(p) => {
            line[n] = b'0' + p;
            n += 1;
        }
    }
    let _ = n;
    Some(line)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mgmt::api::auth_allows;
    use crate::mgmt::iso_library::library_floor;
    use crate::mgmt::megaraid::LAB_SPARE_BYTES;

    fn reset(st: &mut State) {
        st.rec = FwRecord::empty();
        st.filled = 0;
        st.dirty = false;
        st.staging = false;
        st.acked = 0;
    }

    fn hold() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        LOCK.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn pe(n: usize) -> Vec<u8> {
        let mut b = vec![0u8; n];
        b[0] = b'M';
        b[1] = b'Z';
        let e = 0x40u32;
        b[0x3C..0x40].copy_from_slice(&e.to_le_bytes());
        b[0x40..0x44].copy_from_slice(b"PE\0\0");
        b
    }

    #[test]
    fn map_sits_under_the_library_and_off_the_guest_tail() {
        let map = fw_map(LAB_SPARE_BYTES).unwrap();
        let catalog = LAB_SPARE_BYTES - SPARE_HIGH_RESERVE;
        let iso = 2_918_598_656u64;
        let iso_off = library_floor(LAB_SPARE_BYTES, iso).unwrap();
        let slot_end = (map.header_lba + 1) * 512;
        let slot_start = map.slot_lba[0] * 512;
        assert!(slot_end <= iso_off);
        assert!(catalog > slot_end);
        let tail = crate::mgmt::megaraid::PERC_IMAGE_BYTES + 10 * 1024 * 1024 * 1024;
        let tail_end = tail + 10 * 1024 * 1024 * 1024;
        assert!(slot_start > tail_end);
        assert!(map.slot_lba[0] < map.slot_lba[1]);
        assert!(map.slot_lba[1] + SLOT_SECTORS == map.header_lba);
        assert!(fw_map(400 * 1024 * 1024 * 1024).is_none());
        assert!(chunk_lba(map, 0, 0) == Some(map.slot_lba[0]));
        assert!(chunk_lba(map, 0, SLOT_BYTES as u64).is_none());
    }

    #[test]
    fn stage_then_rollback_keeps_the_active_slot() {
        let _guard = hold();
        let st = state();
        reset(st);
        let image = pe(256);
        assert!(accept_chunk(0, image.len(), &image).is_ok());
        let rec = record();
        assert_eq!(rec.active, 0);
        assert_eq!(rec.pending, Some(1));
        assert!(!rec.durable);
        assert!(rec.len[1] == image.len() as u32);
        assert!(rollback());
        assert!(record().pending.is_none());
        assert!(!rollback());
    }

    #[test]
    fn confirm_promotes_the_pending_slot_and_the_next_stage_uses_the_other() {
        let _guard = hold();
        let st = state();
        reset(st);
        let image = pe(256);
        accept_chunk(0, image.len(), &image).unwrap();
        while super::next_image_sector(LAB_SPARE_BYTES).is_some() {
            super::ack_image_sector();
        }
        assert!(record().durable);
        assert!(confirm());
        assert_eq!(record().active, 1);
        assert!(record().pending.is_none());
        assert_eq!(record().apply, Some(1));
        assert!(rollback());
        assert!(record().apply.is_none());
        let second = pe(320);
        accept_chunk(0, second.len(), &second[..100]).unwrap();
        accept_chunk(100, second.len(), &second[100..]).unwrap();
        assert_eq!(record().pending, Some(0));
        assert_eq!(record().active, 1);
    }

    #[test]
    fn stick_copy_keeps_only_a_matching_image() {
        let _guard = hold();
        let st = state();
        reset(st);
        let image = pe(128);
        accept_chunk(0, image.len(), &image).unwrap();
        while super::next_image_sector(LAB_SPARE_BYTES).is_some() {
            super::ack_image_sector();
        }
        assert!(confirm());
        let (slot, len, sha) = stick_plan(&record()).unwrap();
        assert_eq!(slot, 1);
        assert!(read_stick_image(len, sha, |off, buf| {
            let start = off as usize;
            buf.copy_from_slice(&image[start..start + buf.len()]);
            true
        }));
        assert_eq!(stick_image().unwrap(), image.as_slice());
        let mut bad = sha;
        bad[0] ^= 1;
        assert!(!read_stick_image(len, bad, |off, buf| {
            let start = off as usize;
            buf.copy_from_slice(&image[start..start + buf.len()]);
            true
        }));
        assert!(stick_image().is_none());
        note_stick_written();
        assert!(ack_stick_write());
        assert!(record().apply.is_none());
        assert!(record().swapped);
        assert!(rollback());
        assert!(record().revert);
        note_stick_restored();
        assert!(ack_stick_restore());
        assert!(!record().revert);
        assert!(!record().swapped);
    }

    #[test]
    fn a_gap_or_a_non_pe_is_refused() {
        let _guard = hold();
        let st = state();
        reset(st);
        let image = pe(256);
        assert!(accept_chunk(10, image.len(), &image[10..]).is_err());
        reset(st);
        let mut bad = image.clone();
        bad[0] = 0;
        assert!(accept_chunk(0, bad.len(), &bad).is_err());
        assert!(record().pending.is_none());
    }

    #[test]
    fn header_round_trip_and_http() {
        let _guard = hold();
        let st = state();
        reset(st);
        let image = pe(128);
        accept_chunk(0, image.len(), &image).unwrap();
        let mut sector = [0u8; 512];
        assert!(encode(&record(), &mut sector));
        let line = boot_line(&sector).unwrap();
        let text = core::str::from_utf8(&line).unwrap();
        assert!(text.starts_with("boot: A7 slot=0 pending=1"));
        let back = decode(&sector).unwrap();
        assert_eq!(back.pending, Some(1));
        assert!(!auth_allows(None));
        match host_update_rest(RestMethod::Post, "/host/update/rollback", false) {
            UpdateHttp::Unauthorized => {}
            _ => panic!("auth"),
        }
        match host_update_rest(RestMethod::Post, "/host/update/rollback", true) {
            UpdateHttp::Json { status, .. } => assert_eq!(status, 200),
            _ => panic!("rollback"),
        }
        let raw = b"POST /host/update/chunk/0/128 HTTP/1.1\r\nAuthorization: Bearer raynu-v-bringup\r\n\r\n";
        let mut req = raw.to_vec();
        req.extend_from_slice(&image);
        let mut out = [0u8; 256];
        let n = update_http_response(&req, &mut out).unwrap();
        let text = core::str::from_utf8(&out[..n]).unwrap();
        assert!(text.contains("200"));
        assert!(include_str!("../assets/webui.html").contains("/host/update/chunk/"));
    }
}
