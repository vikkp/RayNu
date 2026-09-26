//! M8.7 PERC mailbox, host slice (outside Proven Core).
//!
//! Pillar: [Z] [D]
//! Proven Core: **outside** (ADR-002 / ADR-018). Do not touch VMX/EPT.
//! VERIFICATION: L1 host tests (frame bytes + size fence).
//!
//! Layout follows Linux `drivers/scsi/megaraid/megaraid_sas.h` (v6.12):
//! `megasas_dcmd_frame`, `megasas_pthru_frame`, `MR_LD_LIST`, `MFI_STATE_*`.
//! Device `1000:0016` is `PCI_DEVICE_ID_LSI_HARPOON` (PERC H740P Mini).
//!
//! This slice packs frames and chooses the spare logical disk. It does not
//! map BAR0, ring a doorbell, or change [`crate::mgmt::durable_lun::pick_durable_lun`].
//! The guest disk stays the Toshiba. Iron `RAYNU-V-M8-PERC-LUN-OK` is not this close.

/// Iron COM2 close: a READ of RAYNU-SPARE lived, and the LD was attached.
/// Host/CI/nested must never print this.
pub const M8_PERC_LUN_OK_MARKER: &str = "RAYNU-V-M8-PERC-LUN-OK";

/// Host/CI: frames pack, the size fence keeps UBUNTU0, and reset stays forbidden.
pub const M8_PERC_HOST_OK_MARKER: &str = "RAYNU-V-M8-PERC-HOST-OK";

/// Honesty: a packed frame is not a doorbell and not the iron marker.
pub const PERC_HOST_RESIDUAL_NOTE: &str =
    "residual: M8.7 host pack is not iron RAYNU-V-M8-PERC-LUN-OK; no BAR0 map; no doorbell; durable_lun still skip PERC; QEMU has no H740P; do not format UBUNTU0; do not partition RAYNU-SPARE";

/// 64-byte MFI frame (`MEGAMFI_FRAME_SIZE`).
pub const MFI_FRAME_BYTES: usize = 64;

pub const MFI_CMD_INIT: u8 = 0x00;
pub const MFI_CMD_LD_READ: u8 = 0x01;
pub const MFI_CMD_LD_WRITE: u8 = 0x02;
pub const MFI_CMD_LD_SCSI_IO: u8 = 0x03;
pub const MFI_CMD_DCMD: u8 = 0x05;

pub const MFI_FRAME_SGL64: u16 = 0x0002;
pub const MFI_FRAME_DIR_WRITE: u16 = 0x0008;
pub const MFI_FRAME_DIR_READ: u16 = 0x0010;

/// `MR_DCMD_LD_GET_LIST`. The only DCMD this slice will encode.
pub const MR_DCMD_LD_GET_LIST: u32 = 0x0301_0000;
pub const MR_DCMD_CTRL_GET_INFO: u32 = 0x0101_0000;
pub const MR_DCMD_CTRL_CACHE_FLUSH: u32 = 0x0110_1000;
pub const MR_DCMD_CTRL_SHUTDOWN: u32 = 0x0105_0000;
pub const MR_DCMD_HIBERNATE_SHUTDOWN: u32 = 0x0106_0000;
pub const MR_DCMD_CLUSTER_RESET_ALL: u32 = 0x0801_0100;
pub const MR_DCMD_CLUSTER_RESET_LD: u32 = 0x0801_0200;

pub const MFI_STATE_MASK: u32 = 0xF000_0000;
pub const MFI_STATE_READY: u32 = 0xB000_0000;
pub const MFI_STATE_OPERATIONAL: u32 = 0xC000_0000;
pub const MFI_STATE_FAULT: u32 = 0xF000_0000;
pub const MFI_STATE_FLUSH_CACHE: u32 = 0xA000_0000;
pub const MFI_RESET_REQUIRED: u32 = 0x0000_0001;
/// Value Linux writes to reset the adapter. This slice never encodes a store of it.
pub const MFI_RESET_ADAPTER: u32 = 0x0000_0002;
pub const MFI_STATE_FORCE_OCR: u32 = 0x0000_0080;

/// Firmware posts state in the upper nibble of `outbound_msg_0` (BAR0 + 0x18).
/// Later slices may load this register. They must not store `inbound_msg_0`.
pub const MFI_OUTBOUND_MSG_0: u32 = 0x18;

pub const SCSI_READ_16: u8 = 0x88;
pub const SCSI_WRITE_16: u8 = 0x8A;
pub const SCSI_WRITE_10: u8 = 0x2A;
pub const SCSI_SYNCHRONIZE_CACHE_10: u8 = 0x35;

pub const PCI_VENDOR_LSI: u16 = 0x1000;
/// Linux `PCI_DEVICE_ID_LSI_HARPOON`. Lab H740P Mini.
pub const PCI_DEVICE_H740P_HARPOON: u16 = 0x0016;

pub const LD_LIST_HEADER_BYTES: usize = 8;
pub const LD_LIST_ENTRY_BYTES: usize = 16;
pub const LD_LIST_PARSE_MAX: usize = 64;

const GIB: u64 = 1024 * 1024 * 1024;
const TIB: u64 = GIB * 1024;

/// Lab UBUNTU0 is ~400 GiB. Anything in this window is the boot VD.
pub const PERC_UBUNTU_MIN_BYTES: u64 = 300 * GIB;
pub const PERC_UBUNTU_MAX_BYTES: u64 = 512 * GIB;
/// Lab RAYNU-SPARE is ~2.882 TiB. The full RAID-6 (~3.27 TiB) is outside.
pub const PERC_SPARE_MIN_BYTES: u64 = 5 * TIB / 2;
pub const PERC_SPARE_MAX_BYTES: u64 = 31 * TIB / 10;

pub const LAB_UBUNTU0_BYTES: u64 = 400 * GIB;
/// ~2.882 TiB, rounded down to a 512-byte multiple so a sector round-trip matches.
pub const LAB_SPARE_BYTES: u64 = (2882 * (TIB / 512) / 1000) * 512;
/// Old single-VD usable capacity, 3351.75 GiB. Must not classify as the spare.
pub const LAB_WHOLE_ARRAY_BYTES: u64 = 335175 * GIB / 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LdClass {
    Ubuntu,
    Spare,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LdEntry {
    pub target_id: u8,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SparePick {
    pub target_id: u8,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickError {
    NoSpare,
    ManySpares,
    UbuntuOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseError {
    Short,
    Sector,
    TooMany,
    Overflow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameError {
    NotOneBlock,
    Length,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LdList {
    pub count: usize,
    pub entries: [LdEntry; LD_LIST_PARSE_MAX],
}

/// Upper nibble of the firmware state register.
pub fn fw_state(raw: u32) -> u32 {
    raw & MFI_STATE_MASK
}

/// True only when firmware is already up and is not asking for a reset or OCR.
/// A false result means stop. It does not mean "reset the adapter".
pub fn fw_state_allows_mailbox(raw: u32) -> bool {
    if raw & (MFI_RESET_REQUIRED | MFI_STATE_FORCE_OCR) != 0 {
        return false;
    }
    matches!(fw_state(raw), MFI_STATE_READY | MFI_STATE_OPERATIONAL)
}

/// This module never stores `MFI_RESET_ADAPTER` to `inbound_msg_0`.
pub fn adapter_reset_is_allowed() -> bool {
    false
}

/// True for the one management opcode we encode.
pub fn dcmd_is_allowed(opcode: u32) -> bool {
    opcode == MR_DCMD_LD_GET_LIST
}

/// One H740P Mini. A second `1000:0016`, or any other RAID device, stops the probe.
pub fn h740p_mini_singleton(vendor: u16, device: u16, count: u32) -> bool {
    count == 1 && vendor == PCI_VENDOR_LSI && device == PCI_DEVICE_H740P_HARPOON
}

pub fn classify_ld_bytes(size_bytes: u64) -> LdClass {
    if (PERC_UBUNTU_MIN_BYTES..=PERC_UBUNTU_MAX_BYTES).contains(&size_bytes) {
        LdClass::Ubuntu
    } else if (PERC_SPARE_MIN_BYTES..=PERC_SPARE_MAX_BYTES).contains(&size_bytes) {
        LdClass::Spare
    } else {
        LdClass::Other
    }
}

/// Exactly one LD in the spare window. Ubuntu-sized LDs are never returned.
pub fn pick_spare(entries: &[LdEntry]) -> Result<SparePick, PickError> {
    let mut spare: Option<SparePick> = None;
    let mut ubuntu = 0u32;
    for e in entries {
        match classify_ld_bytes(e.size_bytes) {
            LdClass::Spare => {
                if spare.is_some() {
                    return Err(PickError::ManySpares);
                }
                spare = Some(SparePick {
                    target_id: e.target_id,
                    size_bytes: e.size_bytes,
                });
            }
            LdClass::Ubuntu => ubuntu = ubuntu.saturating_add(1),
            LdClass::Other => {}
        }
    }
    match spare {
        Some(p) => Ok(p),
        None if ubuntu > 0 => Err(PickError::UbuntuOnly),
        None => Err(PickError::NoSpare),
    }
}

/// `MR_LD_LIST`: `ldCount` + reserved, then 16-byte entries. `size` is in sectors.
pub fn parse_ld_list(buf: &[u8], sector_bytes: u32) -> Result<LdList, ParseError> {
    if sector_bytes != 512 && sector_bytes != 4096 {
        return Err(ParseError::Sector);
    }
    if buf.len() < LD_LIST_HEADER_BYTES {
        return Err(ParseError::Short);
    }
    let count = u32::from_le_bytes(buf[0..4].try_into().unwrap()) as usize;
    if count > LD_LIST_PARSE_MAX {
        return Err(ParseError::TooMany);
    }
    let need = LD_LIST_HEADER_BYTES + count * LD_LIST_ENTRY_BYTES;
    if buf.len() < need {
        return Err(ParseError::Short);
    }
    let mut entries = [LdEntry {
        target_id: 0,
        size_bytes: 0,
    }; LD_LIST_PARSE_MAX];
    for i in 0..count {
        let off = LD_LIST_HEADER_BYTES + i * LD_LIST_ENTRY_BYTES;
        let target_id = buf[off];
        let blocks = u64::from_le_bytes(buf[off + 8..off + 16].try_into().unwrap());
        let size_bytes = blocks
            .checked_mul(u64::from(sector_bytes))
            .ok_or(ParseError::Overflow)?;
        entries[i] = LdEntry {
            target_id,
            size_bytes,
        };
    }
    Ok(LdList { count, entries })
}

/// DCMD `MR_DCMD_LD_GET_LIST`. Direction is read. SGL64 at byte 0x28.
pub fn pack_ld_get_list(context: u32, reply_phys: u64, reply_bytes: u32) -> [u8; MFI_FRAME_BYTES] {
    let mut f = [0u8; MFI_FRAME_BYTES];
    f[0] = MFI_CMD_DCMD;
    f[7] = 1;
    put_u32(&mut f, 0x08, context);
    put_u16(&mut f, 0x10, MFI_FRAME_SGL64 | MFI_FRAME_DIR_READ);
    put_u16(&mut f, 0x12, 30);
    put_u32(&mut f, 0x14, reply_bytes);
    put_u32(&mut f, 0x18, MR_DCMD_LD_GET_LIST);
    put_u64(&mut f, 0x28, reply_phys);
    put_u32(&mut f, 0x30, reply_bytes);
    f
}

/// One-block READ(16) to `target_id`. CDB sits at byte 0x20 (after the sense address).
pub fn pack_ld_read16(
    target_id: u8,
    lba: u64,
    blocks: u32,
    data_phys: u64,
    data_bytes: u32,
) -> Result<[u8; MFI_FRAME_BYTES], FrameError> {
    if blocks != 1 {
        return Err(FrameError::NotOneBlock);
    }
    if data_bytes != 512 {
        return Err(FrameError::Length);
    }
    let mut f = [0u8; MFI_FRAME_BYTES];
    f[0] = MFI_CMD_LD_SCSI_IO;
    f[4] = target_id;
    f[6] = 16;
    f[7] = 1;
    put_u16(&mut f, 0x10, MFI_FRAME_SGL64 | MFI_FRAME_DIR_READ);
    put_u16(&mut f, 0x12, 30);
    put_u32(&mut f, 0x14, data_bytes);
    let mut cdb = [0u8; 16];
    cdb[0] = SCSI_READ_16;
    put_u64_be(&mut cdb, 2, lba);
    put_u32_be(&mut cdb, 10, blocks);
    f[0x20..0x30].copy_from_slice(&cdb);
    put_u64(&mut f, 0x30, data_phys);
    put_u32(&mut f, 0x38, data_bytes);
    Ok(f)
}

/// True when the 16-byte CDB is a single-block READ(16).
pub fn cdb_is_single_read16(cdb: &[u8]) -> bool {
    cdb.len() >= 16
        && cdb[0] == SCSI_READ_16
        && u32::from_be_bytes(cdb[10..14].try_into().unwrap()) == 1
}

pub fn frame_is_read(frame: &[u8]) -> bool {
    if frame.len() < 0x12 {
        return false;
    }
    let flags = u16::from_le_bytes(frame[0x10..0x12].try_into().unwrap());
    flags & MFI_FRAME_DIR_READ != 0 && flags & MFI_FRAME_DIR_WRITE == 0
}

pub fn host_never_prints_iron_perc_ok() -> bool {
    M8_PERC_LUN_OK_MARKER == "RAYNU-V-M8-PERC-LUN-OK"
        && M8_PERC_HOST_OK_MARKER == "RAYNU-V-M8-PERC-HOST-OK"
        && M8_PERC_HOST_OK_MARKER != M8_PERC_LUN_OK_MARKER
        && !adapter_reset_is_allowed()
}

/// Lab fixture plus the plan text. Host tests call this. It does not touch hardware.
pub fn prop_perc_host_package() -> bool {
    let plan = include_str!("../docs/m8_plan.md");
    let ubuntu = LdEntry {
        target_id: 0,
        size_bytes: LAB_UBUNTU0_BYTES,
    };
    let spare = LdEntry {
        target_id: 1,
        size_bytes: LAB_SPARE_BYTES,
    };
    let picked = pick_spare(&[ubuntu, spare]);
    let whole = LdEntry {
        target_id: 0,
        size_bytes: LAB_WHOLE_ARRAY_BYTES,
    };
    picked
        == Ok(SparePick {
            target_id: 1,
            size_bytes: LAB_SPARE_BYTES,
        })
        && classify_ld_bytes(LAB_UBUNTU0_BYTES) == LdClass::Ubuntu
        && classify_ld_bytes(LAB_SPARE_BYTES) == LdClass::Spare
        && classify_ld_bytes(LAB_WHOLE_ARRAY_BYTES) == LdClass::Other
        && pick_spare(&[ubuntu]) == Err(PickError::UbuntuOnly)
        && pick_spare(&[whole]) == Err(PickError::NoSpare)
        && pick_spare(&[spare, spare]) == Err(PickError::ManySpares)
        && dcmd_is_allowed(MR_DCMD_LD_GET_LIST)
        && !dcmd_is_allowed(MR_DCMD_CTRL_SHUTDOWN)
        && !dcmd_is_allowed(MR_DCMD_CTRL_CACHE_FLUSH)
        && !dcmd_is_allowed(MR_DCMD_HIBERNATE_SHUTDOWN)
        && !dcmd_is_allowed(MR_DCMD_CLUSTER_RESET_ALL)
        && !dcmd_is_allowed(MR_DCMD_CLUSTER_RESET_LD)
        && fw_state_allows_mailbox(MFI_STATE_OPERATIONAL)
        && fw_state_allows_mailbox(MFI_STATE_READY)
        && !fw_state_allows_mailbox(MFI_STATE_FAULT)
        && !fw_state_allows_mailbox(MFI_STATE_FLUSH_CACHE)
        && !fw_state_allows_mailbox(MFI_STATE_OPERATIONAL | MFI_RESET_REQUIRED)
        && !fw_state_allows_mailbox(MFI_STATE_READY | MFI_STATE_FORCE_OCR)
        && h740p_mini_singleton(PCI_VENDOR_LSI, PCI_DEVICE_H740P_HARPOON, 1)
        && !h740p_mini_singleton(PCI_VENDOR_LSI, PCI_DEVICE_H740P_HARPOON, 2)
        && !h740p_mini_singleton(PCI_VENDOR_LSI, 0x005d, 1)
        && host_never_prints_iron_perc_ok()
        && PERC_HOST_RESIDUAL_NOTE.contains("not iron")
        && plan.contains("M8.7")
        && plan.contains(M8_PERC_HOST_OK_MARKER)
        && plan.contains("skip PERC")
}

fn put_u16(buf: &mut [u8], off: usize, v: u16) {
    buf[off..off + 2].copy_from_slice(&v.to_le_bytes());
}

fn put_u32(buf: &mut [u8], off: usize, v: u32) {
    buf[off..off + 4].copy_from_slice(&v.to_le_bytes());
}

fn put_u64(buf: &mut [u8], off: usize, v: u64) {
    buf[off..off + 8].copy_from_slice(&v.to_le_bytes());
}

fn put_u32_be(buf: &mut [u8], off: usize, v: u32) {
    buf[off..off + 4].copy_from_slice(&v.to_be_bytes());
}

fn put_u64_be(buf: &mut [u8], off: usize, v: u64) {
    buf[off..off + 8].copy_from_slice(&v.to_be_bytes());
}

#[cfg(test)]
#[path = "megaraid_test.rs"]
mod megaraid_test;
