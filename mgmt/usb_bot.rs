//! USB BOT (Bulk-Only Transport) for iron DurableLun (outside Proven Core).
//!
//! Pillar: [Z] [A] [D]
//! Proven Core: **outside** (ADR-002 / ADR-018)
//! VERIFICATION: L1 host tests (CBW/CSW + mock bulk device).
//!
//! Virtio-blk / RayNu-F BlockIo read and write the LUN when NVMe is absent
//! and xHCI + BOT succeed after ExitBootServices. Nested File persist is
//! QEMU RAM. QEMU usb-storage ≠ R640 xHCI. Not `ISO-INSTALL-OK`. Not iron
//! `RAYNU-V-M8-DISK-PERSIST-OK`.
//!
//! ADR-004: persist backing is virtio-blk / BlockIo only.

use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// SCSI TEST UNIT READY.
pub const SCSI_TEST_UNIT_READY: u8 = 0x00;
/// SCSI REQUEST SENSE.
pub const SCSI_REQUEST_SENSE: u8 = 0x03;
/// SCSI INQUIRY.
pub const SCSI_INQUIRY: u8 = 0x12;
/// SCSI READ CAPACITY(10).
pub const SCSI_READ_CAPACITY_10: u8 = 0x25;
/// SCSI READ(10).
pub const SCSI_READ_10: u8 = 0x28;
/// SCSI WRITE(10).
pub const SCSI_WRITE_10: u8 = 0x2A;
/// SCSI START STOP UNIT (kept for CDB builders; bring-up does not send it).
pub const SCSI_START_STOP: u8 = 0x1B;
/// SCSI READ CAPACITY(16).
pub const SCSI_READ_CAPACITY_16: u8 = 0x9E;
/// Iron `6c278e85`: INQUIRY/TUR/CAPACITY succeeded; first 512-byte READ
/// timed out (`err=8` `cmpl=0xff` at `off=0x200`). Retry + `recover_pipes`.
pub const USB_BOT_RW_TRIES: u8 = 3;
/// BOT stage stamped into [`usb_bot_last_stage`] for COM2 `bot=`.
pub const BOT_STAGE_CBW: u8 = 1;
pub const BOT_STAGE_DATA: u8 = 2;
pub const BOT_STAGE_CSW: u8 = 3;
/// SCSI command stamped into [`usb_bot_last_scsi`] for COM2 `scsi=`.
pub const SCSI_TAG_INQUIRY: u8 = 1;
pub const SCSI_TAG_TUR: u8 = 2;
pub const SCSI_TAG_SENSE: u8 = 3;
pub const SCSI_TAG_SSTOP: u8 = 4;
pub const SCSI_TAG_CAPACITY: u8 = 5;
pub const SCSI_TAG_READ: u8 = 6;
pub const SCSI_TAG_WRITE: u8 = 7;

/// CBW signature `'USBC'`.
pub const CBW_SIG: u32 = 0x4342_5355;
/// CSW signature `'USBS'`.
pub const CSW_SIG: u32 = 0x5342_5355;
/// CBW is 31 bytes.
pub const CBW_LEN: usize = 31;
/// CSW is 13 bytes.
pub const CSW_LEN: usize = 13;

/// Honesty: QEMU usb-storage ≠ Force Off persist. Do not F11.
pub const USB_BOT_RESIDUAL_NOTE: &str =
    "residual: USB BOT I/O is not iron RAYNU-V-M8-DISK-PERSIST-OK; leftover DRAM remains the fallback when xHCI/BOT fail; do not format PERC; do not print ISO-INSTALL-OK; do not F11";

/// Why BOT / xHCI bring-up failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum UsbBotError {
    Cap = 1,
    Reset = 2,
    Enum = 3,
    Bot = 4,
    Capacity = 5,
    TooSmall = 6,
    Cruzer = 7,
    Xfer = 8,
    /// USB hub (class 09). Skip and keep scanning CCS ports.
    /// Iron `06ca0f95`: p14 `1604:10c0` class 09 became `err=4` and hid p11 DESC.
    Hub = 9,
}

/// USB bulk pipes used after Configure Endpoint.
pub trait UsbBulk {
    fn bulk_out(&mut self, data: &[u8]) -> Result<(), UsbBotError>;
    fn bulk_in(&mut self, data: &mut [u8]) -> Result<usize, UsbBotError>;
    /// Reset both bulk pipes after a failed READ/WRITE. Not TUR/START STOP.
    /// Iron `73dc4d2e`: auto Reset Endpoint on ignored START STOP timed out
    /// the later CSW (`bot=csw` `err=8`).
    fn recover_pipes(&mut self) {}
    /// Beat after CAPACITY before the first 512-byte READ. Default no-op
    /// (host mocks). Live xHCI spins. Iron `96024edc`: CAPACITY (8 B)
    /// succeeded then READ CBW failed `cmpl=0`.
    fn settle(&mut self) {}
}

fn put_be_u32(b: &mut [u8], off: usize, v: u32) {
    b[off..off + 4].copy_from_slice(&v.to_be_bytes());
}

fn get_be_u32(b: &[u8], off: usize) -> u32 {
    u32::from_be_bytes(b[off..off + 4].try_into().unwrap_or([0; 4]))
}

fn get_le_u32(b: &[u8], off: usize) -> u32 {
    u32::from_le_bytes(b[off..off + 4].try_into().unwrap_or([0; 4]))
}

fn put_le_u32(b: &mut [u8], off: usize, v: u32) {
    b[off..off + 4].copy_from_slice(&v.to_le_bytes());
}

/// Command Block Wrapper (USB MSC BOT 5.1).
#[derive(Clone, Copy)]
pub struct Cbw {
    pub bytes: [u8; CBW_LEN],
}

impl Cbw {
    pub fn scsi(tag: u32, data_len: u32, dir_in: bool, lun: u8, cdb: &[u8]) -> Self {
        let mut b = [0u8; CBW_LEN];
        put_le_u32(&mut b, 0, CBW_SIG);
        put_le_u32(&mut b, 4, tag);
        put_le_u32(&mut b, 8, data_len);
        b[12] = if dir_in { 0x80 } else { 0 };
        b[13] = lun & 0xF;
        let n = cdb.len().min(16);
        b[14] = n as u8;
        b[15..15 + n].copy_from_slice(&cdb[..n]);
        Self { bytes: b }
    }
}

/// Decode CSW status (0 = passed).
pub fn csw_ok(csw: &[u8]) -> bool {
    csw.len() >= CSW_LEN && get_le_u32(csw, 0) == CSW_SIG && csw[12] == 0
}

/// SCSI READ/WRITE(10) CDB.
pub fn cdb_rw10(write: bool, lba: u32, nlb: u16) -> [u8; 16] {
    let mut c = [0u8; 16];
    c[0] = if write { SCSI_WRITE_10 } else { SCSI_READ_10 };
    put_be_u32(&mut c, 2, lba);
    c[7] = (nlb >> 8) as u8;
    c[8] = nlb as u8;
    c
}

/// SCSI READ CAPACITY(10).
pub fn cdb_read_capacity10() -> [u8; 16] {
    let mut c = [0u8; 16];
    c[0] = SCSI_READ_CAPACITY_10;
    c
}

/// SCSI INQUIRY (36-byte standard).
pub fn cdb_inquiry() -> [u8; 16] {
    let mut c = [0u8; 16];
    c[0] = SCSI_INQUIRY;
    c[4] = 36;
    c
}

/// SCSI TEST UNIT READY (no data).
pub fn cdb_tur() -> [u8; 16] {
    [0u8; 16]
}

/// SCSI REQUEST SENSE (18-byte fixed).
pub fn cdb_request_sense() -> [u8; 16] {
    let mut c = [0u8; 16];
    c[0] = SCSI_REQUEST_SENSE;
    c[4] = 18;
    c
}

/// SCSI START STOP UNIT. `start=true` spins the platter (Immed=0).
/// Not sent during [`usb_bot_bring_up`] — iron `73dc4d2e` timed out CSW after
/// START STOP + auto EP-reset. Keep the CDB for host tests.
pub fn cdb_start_stop(start: bool) -> [u8; 16] {
    let mut c = [0u8; 16];
    c[0] = SCSI_START_STOP;
    c[4] = if start { 1 } else { 0 };
    c
}

/// COM2 `bot=` tag for a BOT stage byte.
pub fn usb_bot_stage_name(stage: u8) -> &'static str {
    match stage {
        BOT_STAGE_CBW => "cbw",
        BOT_STAGE_DATA => "data",
        BOT_STAGE_CSW => "csw",
        _ => "?",
    }
}

/// COM2 `scsi=` tag for a SCSI command byte.
pub fn usb_bot_scsi_name(tag: u8) -> &'static str {
    match tag {
        SCSI_TAG_INQUIRY => "inquiry",
        SCSI_TAG_TUR => "tur",
        SCSI_TAG_SENSE => "sense",
        SCSI_TAG_SSTOP => "sstop",
        SCSI_TAG_CAPACITY => "capacity",
        SCSI_TAG_READ => "read",
        SCSI_TAG_WRITE => "write",
        _ => "?",
    }
}

/// Later-port Reset/Hub must not overwrite Enum/Xfer/Bot/Capacity LAST_*.
/// Iron `73dc4d2e`: p14 hub stamped `cmpl=0` over p11 CSW.
/// Iron `96024edc`: p11 SET_CONFIG `cmd=5 err=8` then p14 `reset_port`
/// wrote packed `portsc=…0e00000e03` `cmpl=0x0e801a40` (`bot=? scsi=?`).
pub fn usb_bot_keep_xfer_diag(err: u8) -> bool {
    err == UsbBotError::Xfer as u8
        || err == UsbBotError::Bot as u8
        || err == UsbBotError::Capacity as u8
        || err == UsbBotError::Enum as u8
}

/// Stamp unless a prior Enum/Xfer/Bot/Capacity fail is already latched.
pub fn store_usb_bot_diag_unless_kept(err: UsbBotError, bar: u64, portsc: u64, cmpl: u64) {
    if usb_bot_keep_xfer_diag(usb_bot_last_err()) {
        return;
    }
    store_usb_bot_diag(err, bar, portsc, cmpl);
}

/// xHCI software timeout (`consume_transfer` spun out). Not a real CC.
pub const USB_BOT_CMPL_TIMEOUT: u8 = 0xFF;

/// Reset Endpoint after DATA/CSW fail (pending IN TRB). Do **not** reset
/// after a CBW fail with a real completion — iron `96024edc` READ
/// `bot=cbw cmpl=0` plus `73dc4d2e` showed Reset Endpoint on a live OUT
/// pipe kills the next command. Iron `6ba076cc`: SET_CONFIG + CAPACITY
/// held, then READ CBW **timeout** `cmpl=0xff` — retry stacked a second
/// CBW behind a still-pending TRB. Timeout (not leftover `cmpl=0`)
/// resets pipes so the retry has a clean OUT ring.
pub fn usb_bot_recover_after_fail(stage: u8) -> bool {
    if stage != BOT_STAGE_CBW {
        return true;
    }
    usb_bot_last_cmpl() as u8 == USB_BOT_CMPL_TIMEOUT
}

fn stamp_scsi_cdb(cdb: &[u8]) {
    let tag = match cdb.first().copied().unwrap_or(0) {
        SCSI_INQUIRY => SCSI_TAG_INQUIRY,
        SCSI_TEST_UNIT_READY => SCSI_TAG_TUR,
        SCSI_REQUEST_SENSE => SCSI_TAG_SENSE,
        SCSI_START_STOP => SCSI_TAG_SSTOP,
        SCSI_READ_CAPACITY_10 | SCSI_READ_CAPACITY_16 => SCSI_TAG_CAPACITY,
        SCSI_READ_10 => SCSI_TAG_READ,
        SCSI_WRITE_10 => SCSI_TAG_WRITE,
        _ => 0,
    };
    LAST_SCSI.store(tag, Ordering::Release);
}

/// Last LBA (inclusive) and block size from READ CAPACITY(10).
pub fn capacity10_bytes(data: &[u8]) -> Option<(u64, u32)> {
    if data.len() < 8 {
        return None;
    }
    let last = u64::from(get_be_u32(data, 0));
    let lba = get_be_u32(data, 4);
    if lba == 0 || lba > 4096 {
        return None;
    }
    last.checked_add(1)?
        .checked_mul(u64::from(lba))
        .map(|n| (n, lba))
}

fn bot_cmd(
    hw: &mut impl UsbBulk,
    tag: u32,
    dir_in: bool,
    cdb: &[u8],
    buf: &mut [u8],
) -> Result<(), UsbBotError> {
    let data_len = buf.len() as u32;
    let cbw = Cbw::scsi(tag, data_len, dir_in, 0, cdb);
    stamp_scsi_cdb(cdb);
    store_usb_bot_stage(BOT_STAGE_CBW);
    hw.bulk_out(&cbw.bytes)?;
    if data_len != 0 {
        store_usb_bot_stage(BOT_STAGE_DATA);
        if dir_in {
            let n = hw.bulk_in(buf)?;
            if n < buf.len() {
                for b in buf.iter_mut().skip(n) {
                    *b = 0;
                }
            }
        } else {
            hw.bulk_out(buf)?;
        }
    }
    store_usb_bot_stage(BOT_STAGE_CSW);
    let mut csw = [0u8; CSW_LEN];
    let n = hw.bulk_in(&mut csw)?;
    if n < CSW_LEN || !csw_ok(&csw) {
        return Err(UsbBotError::Bot);
    }
    Ok(())
}

fn bot_cmd_retry(
    hw: &mut impl UsbBulk,
    tag: &mut u32,
    dir_in: bool,
    cdb: &[u8],
    buf: &mut [u8],
) -> Result<(), UsbBotError> {
    let mut last = UsbBotError::Xfer;
    for _ in 0..USB_BOT_RW_TRIES {
        *tag = tag.wrapping_add(1);
        if *tag == 0 {
            *tag = 1;
        }
        match bot_cmd(hw, *tag, dir_in, cdb, buf) {
            Ok(()) => return Ok(()),
            Err(e) => {
                last = e;
                if usb_bot_recover_after_fail(usb_bot_last_stage()) {
                    hw.recover_pipes();
                }
            }
        }
    }
    Err(last)
}

/// Waited INQUIRY + READ CAPACITY(10) + one native READ. Do not send START STOP or
/// TUR. Iron `6c278e85`: CAPACITY printed `usb I/O ready` then peek READ
/// timed out. Iron `73dc4d2e`: START STOP (Immed=0) + ignored timeout +
/// auto EP-reset failed CSW before ready. Iron `68e16633`: `scsi=tur bot=cbw`
/// was the READ-probe recovery TUR (stamped over the READ fail) plus p14
/// hub `cmpl=0` clobber — not "LUN stayed on p10". Iron `6ba076cc`:
/// CAPACITY held, READ CBW `cmpl=0xff`. Iron `0780df21`: `let _ = INQUIRY`
/// then `bot=csw scsi=capacity cmpl=0xff` (pending IN). Iron cap-csw:
/// skip INQUIRY then `bot=cbw scsi=capacity cmpl=0xff` — first bulk OUT
/// after SET_CONFIG with no settle/INQUIRY. Toshiba is a spinning HDD:
/// settle + waited INQUIRY (recover_pipes on fail, never ignore). Retry
/// CAPACITY with `recover_pipes` on DATA/CSW timeout. Do not claim ready
/// until a data-stage READ completes.
pub fn usb_bot_bring_up(hw: &mut impl UsbBulk, min_bytes: u64) -> Result<(u64, u32), UsbBotError> {
    hw.settle();
    let mut tag = 1u32;
    let mut inq = [0u8; 36];
    if bot_cmd_retry(hw, &mut tag, true, &cdb_inquiry(), &mut inq).is_err() {
        hw.settle();
    }
    let mut cap = [0u8; 8];
    bot_cmd_retry(hw, &mut tag, true, &cdb_read_capacity10(), &mut cap)?;
    hw.settle();
    let (bytes, lba) = capacity10_bytes(&cap).ok_or(UsbBotError::Capacity)?;
    if bytes < min_bytes {
        return Err(UsbBotError::TooSmall);
    }
    let n = lba as usize;
    if n == 0 || n > 4096 {
        return Err(UsbBotError::Capacity);
    }
    let mut probe = [0u8; 4096];
    usb_bot_rw(hw, &mut tag, lba, 0, &mut probe[..n], false)?;
    Ok((bytes, lba))
}

/// Read or write `buf` at byte `off`. Length must be a multiple of LBA
/// and not exceed 4 KiB (one BOT data stage).
pub fn usb_bot_rw(
    hw: &mut impl UsbBulk,
    tag: &mut u32,
    lba_bytes: u32,
    off: u64,
    buf: &mut [u8],
    write: bool,
) -> Result<(), UsbBotError> {
    let lba = u64::from(lba_bytes);
    if lba == 0 || buf.is_empty() || off % lba != 0 || (buf.len() as u64) % lba != 0 {
        return Err(UsbBotError::Xfer);
    }
    if buf.len() > 4096 {
        return Err(UsbBotError::Xfer);
    }
    let slba = off / lba;
    let nlb = (buf.len() as u64) / lba;
    if nlb == 0 || nlb > 0xFFFF || slba > u64::from(u32::MAX) {
        return Err(UsbBotError::Xfer);
    }
    let cdb = cdb_rw10(write, slba as u32, nlb as u16);
    bot_cmd_retry(hw, tag, !write, &cdb, buf)
}

static IO_READY: AtomicBool = AtomicBool::new(false);
static NS_BYTES: AtomicU64 = AtomicU64::new(0);
static LBA_BYTES: AtomicU64 = AtomicU64::new(0);
static LUN_RESERVED: AtomicBool = AtomicBool::new(false);
static LAST_ERR: core::sync::atomic::AtomicU8 = core::sync::atomic::AtomicU8::new(0);
static LAST_BAR: AtomicU64 = AtomicU64::new(0);
static LAST_PORTSC: AtomicU64 = AtomicU64::new(0);
static LAST_CMPL: AtomicU64 = AtomicU64::new(0);
static LAST_STAGE: core::sync::atomic::AtomicU8 = core::sync::atomic::AtomicU8::new(0);
static LAST_SCSI: core::sync::atomic::AtomicU8 = core::sync::atomic::AtomicU8::new(0);
static BOT_TAG: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(10);

pub fn usb_bot_io_ready() -> bool {
    IO_READY.load(Ordering::Acquire)
}

pub fn usb_bot_ns_bytes() -> u64 {
    NS_BYTES.load(Ordering::Acquire)
}

pub fn usb_bot_lba_bytes() -> u32 {
    LBA_BYTES.load(Ordering::Acquire) as u32
}

pub fn store_usb_bot_ready(bytes: u64, lba: u32) {
    NS_BYTES.store(bytes, Ordering::Release);
    LBA_BYTES.store(u64::from(lba), Ordering::Release);
    LAST_ERR.store(0, Ordering::Release);
    IO_READY.store(true, Ordering::Release);
}

pub fn clear_usb_bot_ready() {
    IO_READY.store(false, Ordering::Release);
    NS_BYTES.store(0, Ordering::Release);
    LBA_BYTES.store(0, Ordering::Release);
    LUN_RESERVED.store(false, Ordering::Release);
}

pub fn reserve_durable_lun_usb(bytes: u64) {
    NS_BYTES.store(bytes, Ordering::Release);
    LUN_RESERVED.store(true, Ordering::Release);
}

pub fn durable_lun_usb_reserved() -> bool {
    LUN_RESERVED.load(Ordering::Acquire)
        && IO_READY.load(Ordering::Acquire)
        && NS_BYTES.load(Ordering::Acquire) != 0
}

pub fn take_durable_lun_usb() -> Option<u64> {
    if !LUN_RESERVED.swap(false, Ordering::AcqRel) {
        return None;
    }
    let bytes = NS_BYTES.load(Ordering::Acquire);
    if !IO_READY.load(Ordering::Acquire) || bytes == 0 {
        None
    } else {
        Some(bytes)
    }
}

pub fn usb_bot_last_err() -> u8 {
    LAST_ERR.load(Ordering::Acquire)
}

pub fn usb_bot_last_bar() -> u64 {
    LAST_BAR.load(Ordering::Acquire)
}

pub fn usb_bot_last_portsc() -> u64 {
    LAST_PORTSC.load(Ordering::Acquire)
}

pub fn usb_bot_last_cmpl() -> u64 {
    LAST_CMPL.load(Ordering::Acquire)
}

pub fn usb_bot_last_stage() -> u8 {
    LAST_STAGE.load(Ordering::Acquire)
}

pub fn usb_bot_last_scsi() -> u8 {
    LAST_SCSI.load(Ordering::Acquire)
}

pub fn store_usb_bot_stage(stage: u8) {
    LAST_STAGE.store(stage, Ordering::Release);
}

pub fn store_usb_bot_diag(err: UsbBotError, bar: u64, portsc: u64, cmpl: u64) {
    LAST_ERR.store(err as u8, Ordering::Release);
    LAST_BAR.store(bar, Ordering::Release);
    LAST_PORTSC.store(portsc, Ordering::Release);
    LAST_CMPL.store(cmpl, Ordering::Release);
}

pub fn next_bot_tag() -> u32 {
    let t = BOT_TAG.fetch_add(1, Ordering::AcqRel);
    if t == 0 {
        BOT_TAG.store(1, Ordering::Release);
        1
    } else {
        t
    }
}

/// Host tests attach a Vec as the USB LUN (no xHCI).
#[cfg(test)]
pub fn host_usb_attach(ns: &'static mut [u8], lba: u32) {
    HOST_NS.store(ns.as_mut_ptr() as u64, Ordering::Release);
    HOST_NS_LEN.store(ns.len() as u64, Ordering::Release);
    store_usb_bot_ready(ns.len() as u64, lba);
}

#[cfg(test)]
static HOST_NS: AtomicU64 = AtomicU64::new(0);
#[cfg(test)]
static HOST_NS_LEN: AtomicU64 = AtomicU64::new(0);

#[cfg(test)]
pub fn host_usb_rw(off: u64, buf: &mut [u8], write: bool) -> bool {
    let ptr = HOST_NS.load(Ordering::Acquire);
    let len = HOST_NS_LEN.load(Ordering::Acquire);
    if ptr == 0 || buf.is_empty() {
        return false;
    }
    let Some(end) = off.checked_add(buf.len() as u64) else {
        return false;
    };
    if end > len {
        return false;
    }
    // SAFETY: host_usb_attach promised a live test allocation.
    let ns = unsafe { core::slice::from_raw_parts_mut(ptr as *mut u8, len as usize) };
    let start = off as usize;
    if write {
        ns[start..start + buf.len()].copy_from_slice(buf);
    } else {
        buf.copy_from_slice(&ns[start..start + buf.len()]);
    }
    true
}

#[cfg(not(test))]
pub fn host_usb_rw(_off: u64, _buf: &mut [u8], _write: bool) -> bool {
    false
}

#[cfg(test)]
#[path = "usb_bot_test.rs"]
mod usb_bot_test;
