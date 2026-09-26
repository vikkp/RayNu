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
//! This slice packs frames, chooses the spare logical disk, and on the EFI
//! loads the Fusion scratch pads for the one H740P Mini. `outbound_msg_0` is
//! printed because iron already showed it as zero; it is not the Harpoon
//! status word. It does not ring a doorbell, post an MFA descriptor, issue a
//! DCMD, or change [`crate::mgmt::durable_lun::pick_durable_lun`].
//! The guest disk stays the Toshiba. Iron `RAYNU-V-M8-PERC-LUN-OK` is not this close.

/// Iron COM2 close: a READ of RAYNU-SPARE lived, and the LD was attached.
/// Host/CI/nested must never print this.
pub const M8_PERC_LUN_OK_MARKER: &str = "RAYNU-V-M8-PERC-LUN-OK";

/// Host/CI: frames pack, the size fence keeps UBUNTU0, and reset stays forbidden.
pub const M8_PERC_HOST_OK_MARKER: &str = "RAYNU-V-M8-PERC-HOST-OK";

/// Honesty: a firmware-state load is not a doorbell and not the iron marker.
pub const PERC_HOST_RESIDUAL_NOTE: &str =
    "residual: M8.7 Harpoon status is scratch_pad_0; outbound_msg_0 is the xscale register; not iron RAYNU-V-M8-PERC-LUN-OK; no doorbell; durable_lun still skip PERC; QEMU has no H740P; do not format UBUNTU0; do not partition RAYNU-SPARE";

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

/// Xscale / classic MFI status word (BAR0 + 0x18).
///
/// Iron `2dd2b412` loaded this on the H740P Mini and got `0`. Harpoon is
/// Ventura/Fusion. Linux `megasas_read_fw_status_reg_fusion` does not use
/// this offset. A zero here is not `MFI_STATE_UNDEFINED` and is not a reset.
pub const MFI_OUTBOUND_MSG_0: u32 = 0x18;

/// Fusion doorbell. Linux writes `MFI_RESET_FLAGS` here to leave OPERATIONAL.
/// This slice never stores it.
pub const MFI_DOORBELL: u32 = 0x00;
/// Fusion request-descriptor port. IOC init and DCMDs post here later.
/// This slice never stores it.
pub const MFI_INBOUND_QUEUE_PORT: u32 = 0x40;
/// `ABORT | READY | MFIMODE`. Drops the previous owner's queues. Not OCR.
pub const MFI_RESET_FLAGS: u32 = 0x0000_0007;

/// Fusion firmware-status word. Upper nibble is `MFI_STATE_*`. Low 16 bits
/// are the max command count. Linux reads this for Harpoon.
pub const MFI_SCRATCH_PAD_0: u32 = 0xB0;
/// Reply-queue count and RDPQ live here. Ventura uses the extended field.
pub const MFI_SCRATCH_PAD_1: u32 = 0xB4;
/// Raid-map size hint. Loaded with the status word so IOC init is sized once.
pub const MFI_SCRATCH_PAD_2: u32 = 0xB8;
/// NVMe page-size hint on Ventura. Same snapshot, still a load.
pub const MFI_SCRATCH_PAD_3: u32 = 0xBC;

/// Bits 21:14 of scratch pad 1, plus one. Thunderbolt's 5-bit field is not this.
pub const MR_MAX_REPLY_QUEUES_EXT_OFFSET: u32 = 0x003F_C000;
pub const MR_MAX_REPLY_QUEUES_EXT_SHIFT: u32 = 14;
/// Scratch pad 1. Set means the firmware wants an RDPQ array.
pub const MR_RDPQ_MODE_OFFSET: u32 = 0x0080_0000;

/// PCI command register. The only config dword the fwstate slice may write.
pub const PCI_CFG_COMMAND: u8 = 0x04;
pub const PCI_CFG_HEADER: u8 = 0x0C;
pub const PCI_CFG_BAR0: u8 = 0x10;
pub const PCI_CFG_BAR1: u8 = 0x14;
/// Subsystem vendor (low 16) and subsystem device (high 16). Header type 0.
pub const PCI_CFG_SUBSYS: u8 = 0x2C;
/// Memory Space Enable. Bus master is a different bit and stays as firmware left it.
pub const PCI_CMD_MEMORY: u16 = 1 << 1;
pub const PCI_CMD_BUS_MASTER: u16 = 1 << 2;

/// Inclusive last bus of the H740P walk. Lab BDF is `0000:18:00.0`.
/// `0x3F` covers that bus. This is not a 256-bus scan.
pub const PERC_SCAN_BUS_LAST: u8 = 0x3F;

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

/// Harpoon (`1000:0016`, Ventura) status offset. Not [`MFI_OUTBOUND_MSG_0`].
pub fn harpoon_fw_status_offset() -> u32 {
    MFI_SCRATCH_PAD_0
}

/// Ventura reply-queue count from scratch pad 1. Meaningful only when the
/// four scratch pads are not all zero.
pub fn fusion_reply_queues_ventura(scratch_pad_1: u32) -> u32 {
    ((scratch_pad_1 & MR_MAX_REPLY_QUEUES_EXT_OFFSET) >> MR_MAX_REPLY_QUEUES_EXT_SHIFT) + 1
}

pub fn fusion_rdpq(scratch_pad_1: u32) -> bool {
    scratch_pad_1 & MR_RDPQ_MODE_OFFSET != 0
}

/// Low 16 bits of the fusion status word.
pub fn fusion_max_cmds(status: u32) -> u16 {
    status as u16
}

/// All four scratch pads zero: the BAR did not present a fusion register file.
/// The next step is to stop. It is not a walk of other offsets.
pub fn fusion_regs_look_unmapped(s0: u32, s1: u32, s2: u32, s3: u32) -> bool {
    s0 == 0 && s1 == 0 && s2 == 0 && s3 == 0
}

/// MFA descriptor post (inbound queue port) stays off in this image.
pub fn fusion_post_is_allowed() -> bool {
    false
}

/// Doorbell write that moves OPERATIONAL → READY stays off in this image.
pub fn doorbell_transition_is_allowed() -> bool {
    false
}

/// True for the one management opcode we encode.
pub fn dcmd_is_allowed(opcode: u32) -> bool {
    opcode == MR_DCMD_LD_GET_LIST
}

/// One H740P Mini by device id alone. Iron `cc03d01b` found two `1000:0016`
/// (the Mini and the H840). Use [`h740p_mini_subsys`] to tell them apart.
pub fn h740p_mini_singleton(vendor: u16, device: u16, count: u32) -> bool {
    count == 1 && vendor == PCI_VENDOR_LSI && device == PCI_DEVICE_H740P_HARPOON
}

pub const PCI_VENDOR_DELL: u16 = 0x1028;
/// pci.ids: PERC H840 Adapter. Config-space id only. Never map its BAR.
pub const PCI_SUBSYS_H840: u16 = 0x1fc9;
/// pci.ids: full-height PERC H740P Adapter. Not the integrated Mini.
pub const PCI_SUBSYS_H740P_ADAPTER: u16 = 0x1fcb;
/// pci.ids: PERC H740P Mini.
pub const PCI_SUBSYS_H740P_MINI_1FCD: u16 = 0x1fcd;
/// pci.ids: PERC H740P Mini (second Dell id).
pub const PCI_SUBSYS_H740P_MINI_1FCF: u16 = 0x1fcf;

/// Dell H740P Mini only. H840 (`1028:1fc9`) and the full-height adapter are false.
pub fn h740p_mini_subsys(subvendor: u16, subdevice: u16) -> bool {
    subvendor == PCI_VENDOR_DELL
        && matches!(
            subdevice,
            PCI_SUBSYS_H740P_MINI_1FCD | PCI_SUBSYS_H740P_MINI_1FCF
        )
}

/// OR in memory space. Leave every other command bit, including bus master.
pub fn pci_cmd_for_fwstate_load(cmd: u16) -> u16 {
    cmd | PCI_CMD_MEMORY
}

/// True when `after` turns bus master on and `before` had it off.
pub fn pci_cmd_newly_bus_master(before: u16, after: u16) -> bool {
    before & PCI_CMD_BUS_MASTER == 0 && after & PCI_CMD_BUS_MASTER != 0
}

/// 64-bit memory BAR. `None` for I/O space, a zero address, or anything below 1 MiB.
pub fn memory_bar64(bar0: u32, bar1: u32) -> Option<u64> {
    if bar0 & 1 != 0 {
        return None;
    }
    let lo = u64::from(bar0 & !0xF);
    let addr = if bar0 & 0x4 != 0 {
        lo | (u64::from(bar1) << 32)
    } else {
        lo
    };
    if addr < 0x10_0000 || addr & 0xF != 0 {
        None
    } else {
        Some(addr)
    }
}

/// True only for one H740P and a BAR [`memory_bar64`] already accepted.
/// A false result means skip the load. It does not mean "reset the adapter".
pub fn fwstate_may_load(count: u32, bar: Option<u64>) -> bool {
    bar.is_some() && h740p_mini_singleton(PCI_VENDOR_LSI, PCI_DEVICE_H740P_HARPOON, count)
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
        && fwstate_may_load(1, Some(0xF000_0000))
        && !fwstate_may_load(2, Some(0xF000_0000))
        && !fwstate_may_load(1, None)
        && !fwstate_may_load(0, None)
        && memory_bar64(0xF000_0000, 0) == Some(0xF000_0000)
        && memory_bar64(1, 0).is_none()
        && pci_cmd_for_fwstate_load(0) == PCI_CMD_MEMORY
        && !pci_cmd_newly_bus_master(0, pci_cmd_for_fwstate_load(0))
        && pci_cmd_for_fwstate_load(PCI_CMD_BUS_MASTER) & PCI_CMD_BUS_MASTER != 0
        && PERC_SCAN_BUS_LAST >= 0x18
        && PERC_SCAN_BUS_LAST < 0xFF
        && h740p_mini_subsys(PCI_VENDOR_DELL, PCI_SUBSYS_H740P_MINI_1FCD)
        && h740p_mini_subsys(PCI_VENDOR_DELL, PCI_SUBSYS_H740P_MINI_1FCF)
        && !h740p_mini_subsys(PCI_VENDOR_DELL, PCI_SUBSYS_H840)
        && !h740p_mini_subsys(PCI_VENDOR_DELL, PCI_SUBSYS_H740P_ADAPTER)
        && !h740p_mini_subsys(PCI_VENDOR_LSI, PCI_SUBSYS_H740P_MINI_1FCD)
        && h740p_mini_singleton(PCI_VENDOR_LSI, PCI_DEVICE_H740P_HARPOON, 1)
        && !h740p_mini_singleton(PCI_VENDOR_LSI, PCI_DEVICE_H740P_HARPOON, 2)
        && !h740p_mini_singleton(PCI_VENDOR_LSI, 0x005d, 1)
        && harpoon_fw_status_offset() == MFI_SCRATCH_PAD_0
        && harpoon_fw_status_offset() != MFI_OUTBOUND_MSG_0
        && fusion_reply_queues_ventura(0) == 1
        && fusion_reply_queues_ventura(1 << MR_MAX_REPLY_QUEUES_EXT_SHIFT) == 2
        && fusion_rdpq(MR_RDPQ_MODE_OFFSET)
        && !fusion_rdpq(0)
        && fusion_max_cmds(MFI_STATE_OPERATIONAL | 0x03F8) == 0x03F8
        && fusion_regs_look_unmapped(0, 0, 0, 0)
        && !fusion_regs_look_unmapped(MFI_STATE_OPERATIONAL, 0, 0, 0)
        && !fusion_post_is_allowed()
        && !doorbell_transition_is_allowed()
        && MFI_RESET_FLAGS == 0x7
        && MFI_DOORBELL == 0
        && MFI_INBOUND_QUEUE_PORT == 0x40
        && host_never_prints_iron_perc_ok()
        && PERC_HOST_RESIDUAL_NOTE.contains("not iron")
        && PERC_HOST_RESIDUAL_NOTE.contains("scratch_pad_0")
        && plan.contains("M8.7")
        && plan.contains(M8_PERC_HOST_OK_MARKER)
        && plan.contains("skip PERC")
        && plan.contains("scratch_pad_0")
        && plan.contains("IOC init")
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

/// Post-EBS read of the H740P Mini register file. Host and QEMU take the empty stub.
///
/// Walks PCI config for `1000:0016` through [`PERC_SCAN_BUS_LAST`] (lab bus
/// `0x18` is inside that window). Iron `cc03d01b` printed `refuse count=2`.
/// Iron `2dd2b412` mapped only the Mini (`18:00.0`, `1028:1fcd`, BAR
/// `0x9d800000`) and read `outbound_msg_0` as zero. That offset is the xscale
/// status word. This function still prints it, then loads scratch pads
/// `0xB0`–`0xBC`. `allow` on the fusion line is [`fw_state_allows_mailbox`] of
/// scratch pad 0. All-zero scratch pads print `mapped=0` and stop the design
/// there. The H840 BAR is never mapped. Does not set bus master, does not
/// store the doorbell or the inbound queue port, and does not change the
/// durable-LUN pick.
#[cfg(not(all(target_os = "uefi", feature = "uefi-bin")))]
pub fn perc_fwstate_probe() {}

#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
pub fn perc_fwstate_probe() {
    use crate::boot::serial;
    use crate::mgmt::e1000_mmio::{pci_read32, pci_write32};

    serial::write_str("boot: perc fwstate scan bus_last=");
    write_hex8(PERC_SCAN_BUS_LAST);
    serial::write_line(" (not PERC-LUN-OK)");

    let mut harpoon = 0u32;
    let mut mini_count = 0u32;
    let mut found = (0u8, 0u8, 0u8);
    for bus in 0u8..=PERC_SCAN_BUS_LAST {
        for dev in 0u8..32 {
            for func in 0u8..8 {
                let id = pci_read32(bus, dev, func, 0);
                if id == 0xFFFF_FFFF {
                    if func == 0 {
                        break;
                    }
                    continue;
                }
                let vendor = id as u16;
                let device = (id >> 16) as u16;
                if vendor == PCI_VENDOR_LSI && device == PCI_DEVICE_H740P_HARPOON {
                    let ss = pci_read32(bus, dev, func, PCI_CFG_SUBSYS);
                    let subvendor = ss as u16;
                    let subdevice = (ss >> 16) as u16;
                    let mini = h740p_mini_subsys(subvendor, subdevice);
                    harpoon = harpoon.saturating_add(1);
                    if mini {
                        if mini_count == 0 {
                            found = (bus, dev, func);
                        }
                        mini_count = mini_count.saturating_add(1);
                    }
                    serial::write_str("boot: perc fwstate cand bdf=");
                    write_bdf(bus, dev, func);
                    serial::write_str(" sub=");
                    write_hex16(subvendor);
                    serial::write_byte(b':');
                    write_hex16(subdevice);
                    serial::write_str(" mini=");
                    serial::write_byte(if mini { b'1' } else { b'0' });
                    serial::write_line(" (not PERC-LUN-OK)");
                }
                if func == 0 {
                    let ht = (pci_read32(bus, dev, func, PCI_CFG_HEADER) >> 16) as u8;
                    if ht & 0x80 == 0 {
                        break;
                    }
                }
            }
        }
    }

    // Dummy BAR: this only checks mini_count == 1. The real BAR is read below.
    if !fwstate_may_load(mini_count, Some(0x10_0000)) {
        serial::write_str("boot: perc fwstate refuse mini=");
        write_dec(mini_count);
        serial::write_str(" harpoon=");
        write_dec(harpoon);
        serial::write_line(" (not PERC-LUN-OK)");
        return;
    }

    let (bus, dev, func) = found;
    let bar0 = pci_read32(bus, dev, func, PCI_CFG_BAR0);
    let bar1 = if bar0 & 0x4 != 0 {
        pci_read32(bus, dev, func, PCI_CFG_BAR1)
    } else {
        0
    };
    let Some(bar) = memory_bar64(bar0, bar1) else {
        serial::write_str("boot: perc fwstate refuse bdf=");
        write_bdf(bus, dev, func);
        serial::write_line(" bar=0 (not PERC-LUN-OK)");
        return;
    };
    if !fwstate_may_load(mini_count, Some(bar)) {
        serial::write_str("boot: perc fwstate refuse bdf=");
        write_bdf(bus, dev, func);
        serial::write_line(" bar=0 (not PERC-LUN-OK)");
        return;
    }

    let cmd_dw = pci_read32(bus, dev, func, PCI_CFG_COMMAND);
    let cmd = cmd_dw as u16;
    let next = pci_cmd_for_fwstate_load(cmd);
    let mse = next != cmd;
    if mse {
        // High half stays 0 so RW1C status bits are not cleared.
        // Bus master is unchanged: `next` only ORs memory space.
        debug_assert!(!pci_cmd_newly_bus_master(cmd, next));
        pci_write32(bus, dev, func, PCI_CFG_COMMAND, u32::from(next));
        core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
    }

    // SAFETY: firmware-assigned H740P BAR; offset is outbound_msg_0.
    // UEFI identity map is still installed. One load. No doorbell.
    // KANI-TARGET: host tests cover the allow gate, not this load.
    let raw =
        unsafe { core::ptr::read_volatile((bar + u64::from(MFI_OUTBOUND_MSG_0)) as *const u32) };
    let allow = fw_state_allows_mailbox(raw);

    serial::write_str("boot: perc fwstate bdf=");
    write_bdf(bus, dev, func);
    serial::write_str(" bar=0x");
    write_hex64(bar);
    serial::write_str(" raw=0x");
    write_hex32(raw);
    serial::write_str(" allow=");
    serial::write_byte(if allow { b'1' } else { b'0' });
    serial::write_str(" mse=");
    serial::write_byte(if mse { b'1' } else { b'0' });
    serial::write_line(" (not PERC-LUN-OK)");

    // SAFETY: same firmware-assigned Mini BAR. Linux maps 8 KiB of it and
    // reads outbound_scratch_pad_0 as the Harpoon status word. Four aligned
    // loads. No doorbell, no inbound queue port, no H840.
    // KANI-TARGET: host tests cover the allow gate and the queue decoder.
    let load = |off: u32| unsafe { core::ptr::read_volatile((bar + u64::from(off)) as *const u32) };
    let s0 = load(MFI_SCRATCH_PAD_0);
    let s1 = load(MFI_SCRATCH_PAD_1);
    let s2 = load(MFI_SCRATCH_PAD_2);
    let s3 = load(MFI_SCRATCH_PAD_3);
    debug_assert!(!fusion_post_is_allowed());
    debug_assert!(!doorbell_transition_is_allowed());
    let fallow = fw_state_allows_mailbox(s0);
    let unmapped = fusion_regs_look_unmapped(s0, s1, s2, s3);

    serial::write_str("boot: perc fusion bdf=");
    write_bdf(bus, dev, func);
    serial::write_str(" s0=0x");
    write_hex32(s0);
    serial::write_str(" s1=0x");
    write_hex32(s1);
    serial::write_str(" s2=0x");
    write_hex32(s2);
    serial::write_str(" s3=0x");
    write_hex32(s3);
    serial::write_str(" allow=");
    serial::write_byte(if fallow { b'1' } else { b'0' });
    if unmapped {
        serial::write_str(" queues=na");
    } else {
        serial::write_str(" queues=");
        write_dec(fusion_reply_queues_ventura(s1));
    }
    serial::write_str(" rdpq=");
    serial::write_byte(if !unmapped && fusion_rdpq(s1) {
        b'1'
    } else {
        b'0'
    });
    serial::write_str(" mapped=");
    serial::write_byte(if unmapped { b'0' } else { b'1' });
    serial::write_str(" post=0");
    serial::write_line(" (not PERC-LUN-OK)");
}

#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
fn write_dec(mut n: u32) {
    use crate::boot::serial;
    let mut buf = [0u8; 10];
    let mut i = buf.len();
    if n == 0 {
        serial::write_byte(b'0');
        return;
    }
    while n > 0 {
        i -= 1;
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
    }
    for b in &buf[i..] {
        serial::write_byte(*b);
    }
}

#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
fn write_hex8(v: u8) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    crate::boot::serial::write_byte(HEX[(v >> 4) as usize]);
    crate::boot::serial::write_byte(HEX[(v & 0xF) as usize]);
}

#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
fn write_hex16(v: u16) {
    write_hex8((v >> 8) as u8);
    write_hex8(v as u8);
}

#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
fn write_hex32(v: u32) {
    write_hex8((v >> 24) as u8);
    write_hex8((v >> 16) as u8);
    write_hex8((v >> 8) as u8);
    write_hex8(v as u8);
}

#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
fn write_hex64(v: u64) {
    write_hex32((v >> 32) as u32);
    write_hex32(v as u32);
}

#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
fn write_bdf(bus: u8, dev: u8, func: u8) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    write_hex8(bus);
    crate::boot::serial::write_byte(b':');
    write_hex8(dev);
    crate::boot::serial::write_byte(b'.');
    crate::boot::serial::write_byte(HEX[(func & 0xF) as usize]);
}

#[cfg(test)]
#[path = "megaraid_test.rs"]
mod megaraid_test;
