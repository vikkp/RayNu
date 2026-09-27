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
//! printed because iron `2dd2b412` showed it as zero; it is not the Harpoon
//! status word. Iron `22ce3728` read scratch pad 0 as `0xb73c0fed` (READY,
//! max commands 4077). Iron `83ae471e` printed the same READY line and then
//! `perc ioc skip above4g`: the BSS static was not a 4 KiB address below
//! 4 GiB, and scratch pad 1 still has 64-bit DMA clear. This EFI takes eight
//! pages from the post-EBS frame pool (iron `phys=0x1000000`) and posts from
//! there. When the state nibble is READY and scratch pad 1 asks for RDPQ, the
//! EFI posts one IOC init and, only if that status is 0, one
//! `MR_DCMD_LD_GET_LIST`. Iron `7577f934` returned both statuses `0x00`:
//! target 0 is UBUNTU0 (`429496467456` bytes) and target 1 is RAYNU-SPARE
//! (`3169417691136` bytes, `pick=1`). This EFI then posts one READ(16) of
//! LBA 0 and, only if that status is 0, one READ(16) of the last LBA.
//! Both are one 512-byte block, spare only. If both return 0, the spare
//! is offered as a read-only virtio-blk at `00:04.0`. Every post is a
//! single 64-bit store to the low inbound queue port (`0xC0`). It does
//! not ring the doorbell, write a block, or change
//! [`crate::mgmt::durable_lun::pick_durable_lun`].
//! The boot disk stays the Toshiba. `RAYNU-V-M8-PERC-LUN-OK` prints only
//! after a guest read of that virtio device succeeds, and that line uses
//! `write_line_nowait` because Linux earlycon hushes `write_line`.

/// Iron COM2 close: a READ of RAYNU-SPARE lived, and the LD was attached.
/// Host/CI/nested must never print this.
pub const M8_PERC_LUN_OK_MARKER: &str = "RAYNU-V-M8-PERC-LUN-OK";

/// Host/CI: frames pack, the size fence keeps UBUNTU0, and reset stays forbidden.
pub const M8_PERC_HOST_OK_MARKER: &str = "RAYNU-V-M8-PERC-HOST-OK";

/// Spare virtio backing. Zero until both host READs return status 0.
/// Host tests leave these at zero. The guest read path is UEFI-only.
static PERC_SPARE_BAR: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
static PERC_SPARE_BYTES: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
static PERC_SPARE_DATA: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
static PERC_SPARE_FRAME: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
static PERC_SPARE_TARGET: core::sync::atomic::AtomicU8 = core::sync::atomic::AtomicU8::new(0);
static PERC_SPARE_GUEST_OK: core::sync::atomic::AtomicBool =
    core::sync::atomic::AtomicBool::new(false);

/// Bytes of the armed read-only spare. Zero when the host READs did not both
/// return status 0, and always zero in host tests.
pub fn perc_spare_bytes() -> u64 {
    PERC_SPARE_BYTES.load(core::sync::atomic::Ordering::Acquire)
}

/// One or more 512-byte sectors from the armed spare. Never a WRITE.
/// Host and QEMU return false. The iron marker prints on the first success.
pub fn perc_spare_read(lba: u64, buf: &mut [u8]) -> bool {
    #[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
    {
        perc_spare_read_uefi(lba, buf)
    }
    #[cfg(not(all(target_os = "uefi", feature = "uefi-bin")))]
    {
        let _ = (lba, buf);
        false
    }
}

/// Honesty: a firmware-state load is not a doorbell and not the iron marker.
pub const PERC_HOST_RESIDUAL_NOTE: &str =
    "residual: M8.7 Harpoon status is scratch_pad_0; outbound_msg_0 is the xscale register; READY nibble posts one IOC init and one LD list to inbound_low_queue_port; DMA is eight frame-pool pages below 4GiB; READ(16) of LBA 0 then the last LBA on the spare only; read-only virtio 00:04.0 after both status 0; not iron RAYNU-V-M8-PERC-LUN-OK until a guest read; no doorbell; no WRITE; durable_lun still skip PERC; QEMU has no H740P; do not format UBUNTU0; do not partition RAYNU-SPARE";

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
/// This slice never stores it. Iron is already READY, so the transition is skipped.
pub const MFI_DOORBELL: u32 = 0x00;
/// Legacy MFI inbound port. Fusion does not post here.
/// This slice never stores it.
pub const MFI_INBOUND_QUEUE_PORT: u32 = 0x40;
/// Fusion MFA port. One 64-bit store covers `0xC0` and `0xC4`.
pub const MFI_INBOUND_LOW_QUEUE_PORT: u32 = 0xC0;
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

/// `MEGASAS_REQ_DESCRIPT_FLAGS_MFA << TYPE_SHIFT`. Low byte of an MFA descriptor.
pub const MFA_REQUEST_FLAGS: u64 = 0x02;
pub const MPI2_FUNCTION_IOC_INIT: u8 = 0x02;
pub const MPI2_WHOINIT_HOST_DRIVER: u8 = 0x04;
pub const MPI2_VERSION: u16 = 0x0200;
pub const MPI2_HEADER_VERSION: u16 = 0x1000;
pub const MPI2_IOCINIT_MSGFLAG_RDPQ_ARRAY_MODE: u8 = 0x01;
/// `sizeof(MPI2_IOC_INIT_REQUEST)` in Linux v6.12.
pub const MPI2_IOC_INIT_BYTES: usize = 0x48;
pub const MEGA_MPI2_RAID_DEFAULT_IO_FRAME_SIZE: u16 = 256;
pub const MFI_CMD_STATUS_POLL: u8 = 0xFF;
pub const MFI_FRAME_DONT_POST_IN_REPLY_QUEUE: u16 = 0x0001;
pub const MFI_STAT_OK: u8 = 0x00;
/// One reply queue. Poll mode. Not the 128 the firmware can offer.
pub const FUSION_REPLY_Q_DEPTH: u16 = 16;
pub const FUSION_HOST_PAGE_SHIFT: u8 = 12;
pub const FUSION_HOST_MSIX_VECTORS: u8 = 1;
/// `sizeof(MR_LD_LIST)` with `MAX_LOGICAL_DRIVES_EXT` 256. A 64-entry buffer is too small.
pub const LD_LIST_FW_ENTRIES: usize = 256;
pub const LD_LIST_FW_BYTES: u32 =
    (LD_LIST_HEADER_BYTES + LD_LIST_FW_ENTRIES * LD_LIST_ENTRY_BYTES) as u32;
/// Bounded poll. Not Linux's 180s `MFI_IO_TIMEOUT_SECS`.
pub const FUSION_POLL_SECS: u64 = 4;
/// Init frame, IOC message, request frames, RDPQ, reply queue, DCMD, LD list.
/// Eight pages, taken from the frame pool. Not a BSS static: iron `83ae471e`
/// placed that static outside the 32-bit DMA window.
pub const FUSION_DMA_PAGES: u64 = 8;
pub const FUSION_DOORBELL_WAIT_SECS: u64 = 2;
/// Iron `22ce3728` scratch pads. Host tests pin the decode. Not a second load.
pub const IRON_FUSION_S0: u32 = 0xb73c_0fed;
pub const IRON_FUSION_S1: u32 = 0xb1df_c50f;

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
/// Iron `7577f934` LD 0. 400 GiB minus 256 KiB. Classifies as UBUNTU0.
pub const IRON_LD0_BYTES: u64 = 429_496_467_456;
/// Iron `7577f934` LD 1. ~2.882 TiB. Classifies as RAYNU-SPARE. `pick=1`.
pub const IRON_LD1_BYTES: u64 = 3_169_417_691_136;
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

/// True only for the READY state nibble.
///
/// Low 16 bits of scratch pad 0 are `max_fw_cmds`. On iron they are `0x0fed`
/// (4077), which sets bit 0 and bit 7. Those bits are not `MFI_RESET_REQUIRED`
/// or `MFI_STATE_FORCE_OCR` on this register. OPERATIONAL does not post in
/// this image: that transition would store the doorbell, and this image never
/// does. FAULT and an all-zero pad do not post.
pub fn fusion_post_is_allowed(status: u32) -> bool {
    fw_state(status) == MFI_STATE_READY
}

/// True when `base` is a non-zero 4 KiB address whose eight pages end at or
/// below 4 GiB. Scratch pad 1 on iron `83ae471e` (`0xb1dfc50f`) has bit 25
/// clear, so a pointer at or above 4 GiB is not a DMA address for this Mini.
pub fn fusion_dma_base_ok(base: u64) -> bool {
    let bytes = FUSION_DMA_PAGES.saturating_mul(4096);
    base != 0 && bytes == 32768 && base & 0xFFF == 0 && base.saturating_add(bytes) <= 0x1_0000_0000
}

/// Doorbell write that moves OPERATIONAL → READY stays off in this image.
pub fn doorbell_transition_is_allowed() -> bool {
    false
}

/// Cycles to spin at `tsc_hz` (0 or a tiny calibration → 2.1 GHz).
pub fn fusion_poll_cycles(tsc_hz: u64, secs: u64) -> u64 {
    let hz = if tsc_hz < 1_000_000 {
        2_100_000_000
    } else {
        tsc_hz
    };
    hz.saturating_mul(secs)
}

/// MFA descriptor for a 256-byte-aligned frame below 4 GiB.
///
/// Low byte is [`MFA_REQUEST_FLAGS`]. An address that would occupy that byte,
/// or any address at or above 4 GiB, is rejected. Scratch pad 1 bit 25 was
/// clear on iron, so this image does not advertise 64-bit DMA.
pub fn pack_mfa_descriptor(frame_phys: u64) -> Option<u64> {
    if frame_phys & 0xFF != 0 || frame_phys >= 0x1_0000_0000 {
        return None;
    }
    Some(frame_phys | MFA_REQUEST_FLAGS)
}

/// MPI2 IOC INIT, RDPQ array mode, one MSI-X vector, poll depth 16.
///
/// `ChainOffset` stays 0. `driver_operations` is not this message.
/// Both physical addresses must sit below 4 GiB.
pub fn pack_ioc_init_request(
    io_frame_phys: u64,
    rdpq_array_phys: u64,
) -> Option<[u8; MPI2_IOC_INIT_BYTES]> {
    if io_frame_phys >= 0x1_0000_0000
        || rdpq_array_phys >= 0x1_0000_0000
        || io_frame_phys & 0xFF != 0
        || rdpq_array_phys & 0xF != 0
    {
        return None;
    }
    let mut m = [0u8; MPI2_IOC_INIT_BYTES];
    m[0x00] = MPI2_WHOINIT_HOST_DRIVER;
    m[0x03] = MPI2_FUNCTION_IOC_INIT;
    m[0x07] = MPI2_IOCINIT_MSGFLAG_RDPQ_ARRAY_MODE;
    put_u16(&mut m, 0x0C, MPI2_VERSION);
    put_u16(&mut m, 0x0E, MPI2_HEADER_VERSION);
    m[0x16] = FUSION_HOST_PAGE_SHIFT;
    m[0x17] = FUSION_HOST_MSIX_VECTORS;
    put_u16(&mut m, 0x1A, MEGA_MPI2_RAID_DEFAULT_IO_FRAME_SIZE / 4);
    put_u16(&mut m, 0x1C, FUSION_REPLY_Q_DEPTH);
    put_u64(&mut m, 0x28, io_frame_phys);
    put_u64(&mut m, 0x30, rdpq_array_phys);
    Some(m)
}

/// One RDPQ array entry: reply-queue physical address, then 8 reserved bytes.
pub fn pack_rdpq_entry(queue_phys: u64) -> Option<[u8; 16]> {
    if queue_phys >= 0x1_0000_0000 || queue_phys & 0xF != 0 {
        return None;
    }
    let mut e = [0u8; 16];
    put_u64(&mut e, 0, queue_phys);
    Some(e)
}

/// `MFI_CMD_INIT` header. `driver_operations` stays 0. Polled (`cmd_status` `0xFF`).
pub fn pack_mfi_init_frame(ioc_msg_phys: u64) -> Option<[u8; MFI_FRAME_BYTES]> {
    if ioc_msg_phys >= 0x1_0000_0000 || ioc_msg_phys & 0xF != 0 {
        return None;
    }
    let mut f = [0u8; MFI_FRAME_BYTES];
    f[0] = MFI_CMD_INIT;
    f[2] = MFI_CMD_STATUS_POLL;
    put_u16(&mut f, 0x10, MFI_FRAME_DONT_POST_IN_REPLY_QUEUE);
    put_u32(&mut f, 0x14, MPI2_IOC_INIT_BYTES as u32);
    put_u32(&mut f, 0x18, ioc_msg_phys as u32);
    put_u32(&mut f, 0x1C, (ioc_msg_phys >> 32) as u32);
    Some(f)
}

/// Polled `MR_DCMD_LD_GET_LIST`. Buffer length is the full 256-entry firmware struct.
pub fn pack_ld_get_list_polled(
    context: u32,
    reply_phys: u64,
    reply_bytes: u32,
) -> Option<[u8; MFI_FRAME_BYTES]> {
    if reply_bytes != LD_LIST_FW_BYTES || reply_phys >= 0x1_0000_0000 || reply_phys & 0xF != 0 {
        return None;
    }
    let mut f = pack_ld_get_list(context, reply_phys, reply_bytes);
    f[2] = MFI_CMD_STATUS_POLL;
    let flags = u16::from_le_bytes(f[0x10..0x12].try_into().unwrap());
    put_u16(&mut f, 0x10, flags | MFI_FRAME_DONT_POST_IN_REPLY_QUEUE);
    if !frame_is_read(&f) || f[0] != MFI_CMD_DCMD {
        return None;
    }
    Some(f)
}

/// Target id for one LBA-0 READ(16), only when [`pick_spare`] returned the spare.
pub fn spare_read_target(pick: Result<SparePick, PickError>) -> Option<u8> {
    match pick {
        Ok(p) if classify_ld_bytes(p.size_bytes) == LdClass::Spare => Some(p.target_id),
        _ => None,
    }
}

/// Last 512-byte LBA of a spare-sized LD. `None` when the size is not a
/// whole number of sectors, or there is no sector to read.
pub fn spare_last_lba(size_bytes: u64) -> Option<u64> {
    if size_bytes < 512 || size_bytes % 512 != 0 {
        return None;
    }
    Some(size_bytes / 512 - 1)
}

/// Polled one-block READ(16). Sense stays inside the data page.
/// `data_phys` is the 512-byte payload. Sense is the next 32 bytes.
pub fn pack_ld_read16_polled(
    target_id: u8,
    lba: u64,
    data_phys: u64,
) -> Option<[u8; MFI_FRAME_BYTES]> {
    if data_phys == 0
        || data_phys >= 0x1_0000_0000
        || data_phys & 0x1FF != 0
        || data_phys.saturating_add(512 + 32) > 0x1_0000_0000
    {
        return None;
    }
    let mut f = pack_ld_read16(target_id, lba, 1, data_phys, 512).ok()?;
    f[1] = 32;
    f[2] = MFI_CMD_STATUS_POLL;
    put_u64(&mut f, 0x18, data_phys + 512);
    let flags = u16::from_le_bytes(f[0x10..0x12].try_into().unwrap());
    put_u16(&mut f, 0x10, flags | MFI_FRAME_DONT_POST_IN_REPLY_QUEUE);
    if !frame_is_read(&f) || f[0] != MFI_CMD_LD_SCSI_IO || !cdb_is_single_read16(&f[0x20..0x30]) {
        return None;
    }
    if flags & MFI_FRAME_DIR_WRITE != 0 {
        return None;
    }
    Some(f)
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

/// Memory space plus bus master. Used only on the H740P Mini, and only when
/// an MFA descriptor is about to be posted. Never applied to the H840.
pub fn pci_cmd_for_fusion_post(cmd: u16) -> u16 {
    cmd | PCI_CMD_MEMORY | PCI_CMD_BUS_MASTER
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
        && classify_ld_bytes(IRON_LD0_BYTES) == LdClass::Ubuntu
        && classify_ld_bytes(LAB_SPARE_BYTES) == LdClass::Spare
        && classify_ld_bytes(IRON_LD1_BYTES) == LdClass::Spare
        && spare_read_target(pick_spare(&[
            LdEntry {
                target_id: 0,
                size_bytes: IRON_LD0_BYTES,
            },
            LdEntry {
                target_id: 1,
                size_bytes: IRON_LD1_BYTES,
            },
        ]))
            == Some(1)
        && spare_read_target(Err(PickError::UbuntuOnly)).is_none()
        && spare_last_lba(IRON_LD1_BYTES).is_some()
        && pack_ld_read16_polled(1, 0, 0x1006000).is_some()
        && pack_ld_read16_polled(1, spare_last_lba(IRON_LD1_BYTES).unwrap_or(0), 0x1006000)
            .is_some()
        && pack_ld_read16_polled(1, 0, 0).is_none()
        && pack_ld_read16_polled(1, 0, 0x1_0000_0000).is_none()
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
        && fw_state(IRON_FUSION_S0) == MFI_STATE_READY
        && !fw_state_allows_mailbox(IRON_FUSION_S0)
        && fusion_max_cmds(IRON_FUSION_S0) == 0x0fed
        && fusion_reply_queues_ventura(IRON_FUSION_S1) == 128
        && fusion_rdpq(IRON_FUSION_S1)
        && fusion_post_is_allowed(IRON_FUSION_S0)
        && fusion_post_is_allowed(MFI_STATE_READY | MFI_RESET_REQUIRED)
        && !fusion_post_is_allowed(MFI_STATE_OPERATIONAL)
        && !fusion_post_is_allowed(MFI_STATE_OPERATIONAL | 0x0fed)
        && !fusion_post_is_allowed(MFI_STATE_FAULT)
        && !fusion_post_is_allowed(MFI_STATE_FLUSH_CACHE)
        && !fusion_post_is_allowed(0)
        && !doorbell_transition_is_allowed()
        && MFI_RESET_FLAGS == 0x7
        && MFI_DOORBELL == 0
        && MFI_INBOUND_QUEUE_PORT == 0x40
        && MFI_INBOUND_LOW_QUEUE_PORT == 0xC0
        && MFA_REQUEST_FLAGS == 0x02
        && LD_LIST_FW_BYTES == 4104
        && FUSION_POLL_SECS <= 8
        && FUSION_DOORBELL_WAIT_SECS <= 4
        && pack_mfa_descriptor(0x1000) == Some(0x1002)
        && pack_mfa_descriptor(0x1001).is_none()
        && pack_mfa_descriptor(0x1_0000_0000).is_none()
        && FUSION_DMA_PAGES == 8
        && fusion_dma_base_ok(0x1000000)
        && fusion_dma_base_ok(0xFFFF_8000)
        && !fusion_dma_base_ok(0)
        && !fusion_dma_base_ok(0x1_0000_0000)
        && !fusion_dma_base_ok(0xFFFF_F000)
        && !fusion_dma_base_ok(0x1000_1001)
        && pci_cmd_for_fusion_post(0) == PCI_CMD_MEMORY | PCI_CMD_BUS_MASTER
        && pci_cmd_newly_bus_master(0, pci_cmd_for_fusion_post(0))
        && !pci_cmd_newly_bus_master(0, pci_cmd_for_fwstate_load(0))
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
/// `0xB0`–`0xBC`. `allow` on the fusion line is [`fusion_post_is_allowed`]:
/// the READY nibble only. Iron `22ce3728` printed `allow=0` because the
/// previous image used [`fw_state_allows_mailbox`], which treats max-command
/// bits 0 and 7 as a reset request. All-zero scratch pads print `mapped=0`
/// and stop. The H840 BAR is never mapped. A READY nibble with RDPQ set
/// arms one IOC init. The post runs after the frame pool exists, from eight
/// pages below 4 GiB, to [`MFI_INBOUND_LOW_QUEUE_PORT`]. The doorbell is
/// not stored. The durable-LUN pick is not changed.
#[cfg(not(all(target_os = "uefi", feature = "uefi-bin")))]
pub fn perc_fwstate_probe() {}

#[cfg(not(all(target_os = "uefi", feature = "uefi-bin")))]
pub fn perc_post_armed() -> bool {
    false
}

#[cfg(not(all(target_os = "uefi", feature = "uefi-bin")))]
pub fn perc_post_disarm() {}

#[cfg(not(all(target_os = "uefi", feature = "uefi-bin")))]
pub fn perc_fusion_post_low(_base: u64) {}

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
    // loads. These loads do not store the doorbell. No H840.
    // KANI-TARGET: host tests cover the allow gate and the queue decoder.
    let load = |off: u32| unsafe { core::ptr::read_volatile((bar + u64::from(off)) as *const u32) };
    let s0 = load(MFI_SCRATCH_PAD_0);
    let s1 = load(MFI_SCRATCH_PAD_1);
    let s2 = load(MFI_SCRATCH_PAD_2);
    let s3 = load(MFI_SCRATCH_PAD_3);
    debug_assert!(!doorbell_transition_is_allowed());
    debug_assert!(!adapter_reset_is_allowed());
    let fallow = fusion_post_is_allowed(s0);
    let unmapped = fusion_regs_look_unmapped(s0, s1, s2, s3);
    let rdpq = !unmapped && fusion_rdpq(s1);

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
    serial::write_str(" nibble=");
    write_hex_digit((s0 >> 28) as u8);
    serial::write_str(" maxcmds=");
    write_dec(u32::from(fusion_max_cmds(s0)));
    if unmapped {
        serial::write_str(" queues=na");
    } else {
        serial::write_str(" queues=");
        write_dec(fusion_reply_queues_ventura(s1));
    }
    serial::write_str(" rdpq=");
    serial::write_byte(if rdpq { b'1' } else { b'0' });
    serial::write_str(" mapped=");
    serial::write_byte(if unmapped { b'0' } else { b'1' });
    serial::write_str(" post=0");
    serial::write_line(" (not PERC-LUN-OK)");

    if unmapped || !fallow || !rdpq {
        serial::write_str("boot: perc ioc skip");
        if unmapped {
            serial::write_str(" mapped=0");
        } else if !fallow {
            serial::write_str(" allow=0");
        } else {
            serial::write_str(" rdpq=0");
        }
        serial::write_line(" (not PERC-LUN-OK)");
        return;
    }
    // Iron `83ae471e` skipped here: the BSS static was not below 4 GiB.
    // The frame pool is carved a few lines later (`phys=0x1000000` on that
    // boot). Arm the post; `leave_firmware` supplies the pages.
    // Single-threaded: this is before the scheduler. No spinlock.
    unsafe {
        core::ptr::addr_of_mut!(PERC_POST).write(PercPost {
            bar,
            bus,
            dev,
            func,
            armed: true,
        });
    }
    serial::write_line("boot: perc ioc dma=pool (not PERC-LUN-OK)");
}

/// Mini selected for one IOC init. Filled by [`perc_fwstate_probe`], consumed
/// by [`perc_fusion_post_low`]. Single-threaded post-EBS path.
#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
#[derive(Clone, Copy)]
struct PercPost {
    bar: u64,
    bus: u8,
    dev: u8,
    func: u8,
    armed: bool,
}

#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
static mut PERC_POST: PercPost = PercPost {
    bar: 0,
    bus: 0,
    dev: 0,
    func: 0,
    armed: false,
};

/// True when the scratch pads asked for a post and the pool has not run yet.
#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
pub fn perc_post_armed() -> bool {
    unsafe { core::ptr::addr_of!(PERC_POST).read().armed }
}

/// Drop a pending post without touching the BAR.
#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
pub fn perc_post_disarm() {
    let mut slot = unsafe { core::ptr::addr_of!(PERC_POST).read() };
    slot.armed = false;
    unsafe {
        core::ptr::addr_of_mut!(PERC_POST).write(slot);
    }
}

/// Post using `base`, eight pages from the frame pool.
#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
pub fn perc_fusion_post_low(base: u64) {
    let slot = unsafe { core::ptr::addr_of!(PERC_POST).read() };
    unsafe {
        core::ptr::addr_of_mut!(PERC_POST).write(PercPost {
            armed: false,
            ..slot
        });
    }
    if !slot.armed {
        return;
    }
    perc_fusion_post(slot.bar, slot.bus, slot.dev, slot.func, base);
}

/// Byte offsets inside the eight frame-pool pages. Each region starts on
/// its own page so the MFA low byte stays clear. The LD list is the full
/// 256-entry firmware struct (4104 bytes), not the 64-entry parser cap.
#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
const DMA_OFF_IOC: usize = 4096;
#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
const DMA_OFF_IO: usize = 8192;
#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
const DMA_OFF_RDPQ: usize = 12288;
#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
const DMA_OFF_REPLY: usize = 16384;
#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
const DMA_OFF_DCMD: usize = 20480;
#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
const DMA_OFF_LD: usize = 24576;
#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
const DMA_BYTES: usize = 32768;

/// One IOC init, then one LD list if that status is 0.
///
/// `base` is the physical address of eight frame-pool pages. Prints the post
/// line before the 64-bit store. A doorbell bit 0 that stays set skips the
/// post. The doorbell register is never written. Bus master is set on this
/// Mini only, and only after the skip checks pass.
#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
fn perc_fusion_post(bar: u64, bus: u8, dev: u8, func: u8, base: u64) {
    use crate::boot::serial;
    use crate::mgmt::e1000_mmio::{pci_read32, pci_write32};

    debug_assert_eq!(DMA_OFF_IOC, 4096);
    debug_assert_eq!(DMA_BYTES, (FUSION_DMA_PAGES as usize) * 4096);
    if !fusion_dma_base_ok(base) {
        serial::write_str("boot: perc ioc skip above4g phys=0x");
        write_hex64(base);
        serial::write_line(" (not PERC-LUN-OK)");
        return;
    }
    serial::write_str("boot: perc ioc dma phys=0x");
    write_hex64(base);
    serial::write_line(" (not PERC-LUN-OK)");
    let dma = base as *mut u8;
    let init_phys = base;
    let ioc_phys = base + DMA_OFF_IOC as u64;
    let io_phys = base + DMA_OFF_IO as u64;
    let rdpq_phys = base + DMA_OFF_RDPQ as u64;
    let reply_phys = base + DMA_OFF_REPLY as u64;
    let dcmd_phys = base + DMA_OFF_DCMD as u64;
    let ld_phys = base + DMA_OFF_LD as u64;

    // SAFETY: firmware-assigned Mini BAR. One load of the doorbell. No store.
    // KANI-TARGET: host tests keep doorbell_transition_is_allowed false.
    let db = unsafe { core::ptr::read_volatile((bar + u64::from(MFI_DOORBELL)) as *const u32) };
    serial::write_str("boot: perc ioc db=0x");
    write_hex32(db);
    serial::write_line(" (not PERC-LUN-OK)");
    if db & 1 != 0 {
        let start = crate::arch::cpu::rdtsc();
        let budget = fusion_poll_cycles(
            crate::boot::raynu_f_flag::tsc_hz(),
            FUSION_DOORBELL_WAIT_SECS,
        );
        let mut still = db;
        while still & 1 != 0 && crate::arch::cpu::rdtsc().wrapping_sub(start) < budget {
            core::hint::spin_loop();
            // SAFETY: same doorbell load. Still no store.
            still =
                unsafe { core::ptr::read_volatile((bar + u64::from(MFI_DOORBELL)) as *const u32) };
        }
        if still & 1 != 0 {
            serial::write_line("boot: perc ioc skip dbbusy=1 (not PERC-LUN-OK)");
            return;
        }
    }

    let Some(desc) = pack_mfa_descriptor(init_phys) else {
        serial::write_line("boot: perc ioc skip frame (not PERC-LUN-OK)");
        return;
    };
    let Some(msg) = pack_ioc_init_request(io_phys, rdpq_phys) else {
        serial::write_line("boot: perc ioc skip frame (not PERC-LUN-OK)");
        return;
    };
    let Some(entry) = pack_rdpq_entry(reply_phys) else {
        serial::write_line("boot: perc ioc skip frame (not PERC-LUN-OK)");
        return;
    };
    let Some(frame) = pack_mfi_init_frame(ioc_phys) else {
        serial::write_line("boot: perc ioc skip frame (not PERC-LUN-OK)");
        return;
    };

    let cmd_dw = pci_read32(bus, dev, func, PCI_CFG_COMMAND);
    let next = pci_cmd_for_fusion_post(cmd_dw as u16);
    debug_assert!(
        pci_cmd_newly_bus_master(cmd_dw as u16, next) || cmd_dw as u16 & PCI_CMD_BUS_MASTER != 0
    );
    // High half stays 0 so RW1C status bits are not cleared.
    pci_write32(bus, dev, func, PCI_CFG_COMMAND, u32::from(next));
    core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);

    // SAFETY: eight frame-pool pages, identity-mapped, below 4 GiB. The bump
    // keeps them (they are not returned to a guest). Zero, then fill the
    // reply queue with 0xFF the way Linux does. No H840 pointer is formed.
    // KANI-TARGET: host tests cover the packed bytes, not this copy.
    unsafe {
        core::ptr::write_bytes(dma, 0, DMA_BYTES);
        let reply = dma.add(DMA_OFF_REPLY);
        core::ptr::write_bytes(reply, 0xFF, usize::from(FUSION_REPLY_Q_DEPTH) * 8);
        core::ptr::copy_nonoverlapping(entry.as_ptr(), dma.add(DMA_OFF_RDPQ), entry.len());
        core::ptr::copy_nonoverlapping(msg.as_ptr(), dma.add(DMA_OFF_IOC), msg.len());
        core::ptr::copy_nonoverlapping(frame.as_ptr(), dma, frame.len());
    }
    core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);

    serial::write_str("boot: perc ioc post bdf=");
    write_bdf(bus, dev, func);
    serial::write_str(" port=c0 frame=0x");
    write_hex64(init_phys);
    serial::write_str(" desc=0x");
    write_hex64(desc);
    serial::write_str(" bm=1");
    serial::write_line(" (not PERC-LUN-OK)");

    // SAFETY: Mini BAR + 0xC0. One 64-bit store. Not the legacy port 0x40.
    // Not the doorbell. Ventura requires the 64-bit write.
    // KANI-TARGET: host tests reject an unaligned descriptor.
    unsafe {
        core::ptr::write_volatile(
            (bar + u64::from(MFI_INBOUND_LOW_QUEUE_PORT)) as *mut u64,
            desc,
        );
    }
    let (st, timed_out) = poll_mfi_status(dma);
    serial::write_str("boot: perc ioc status=0x");
    write_hex8(st);
    serial::write_str(" to=");
    serial::write_byte(if timed_out { b'1' } else { b'0' });
    serial::write_line(" (not PERC-LUN-OK)");
    if timed_out || st != MFI_STAT_OK {
        return;
    }

    let Some(dcmd) = pack_ld_get_list_polled(1, ld_phys, LD_LIST_FW_BYTES) else {
        serial::write_line("boot: perc ld skip frame (not PERC-LUN-OK)");
        return;
    };
    let Some(ld_desc) = pack_mfa_descriptor(dcmd_phys) else {
        serial::write_line("boot: perc ld skip frame (not PERC-LUN-OK)");
        return;
    };
    // SAFETY: same eight pages. The DCMD frame is a different page from the init frame.
    unsafe {
        let dcmd_ptr = dma.add(DMA_OFF_DCMD);
        core::ptr::write_bytes(dcmd_ptr, 0, 4096);
        core::ptr::copy_nonoverlapping(dcmd.as_ptr(), dcmd_ptr, dcmd.len());
        let ld_ptr = dma.add(DMA_OFF_LD);
        core::ptr::write_bytes(ld_ptr, 0, LD_LIST_FW_BYTES as usize);
    }
    core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);

    serial::write_str("boot: perc ld post bdf=");
    write_bdf(bus, dev, func);
    serial::write_str(" port=c0 frame=0x");
    write_hex64(dcmd_phys);
    serial::write_str(" desc=0x");
    write_hex64(ld_desc);
    serial::write_str(" bytes=");
    write_dec(LD_LIST_FW_BYTES);
    serial::write_line(" (not PERC-LUN-OK)");

    // SAFETY: second MFA store to the same low queue port. Still not a doorbell.
    unsafe {
        core::ptr::write_volatile(
            (bar + u64::from(MFI_INBOUND_LOW_QUEUE_PORT)) as *mut u64,
            ld_desc,
        );
    }
    let dcmd_ptr = unsafe { dma.add(DMA_OFF_DCMD) };
    let (lst, lto) = poll_mfi_status(dcmd_ptr);
    let mut spare: Option<SparePick> = None;
    serial::write_str("boot: perc ld status=0x");
    write_hex8(lst);
    serial::write_str(" to=");
    serial::write_byte(if lto { b'1' } else { b'0' });
    if !lto && lst == MFI_STAT_OK {
        core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
        // SAFETY: firmware wrote at most LD_LIST_FW_BYTES into this page.
        let buf = unsafe {
            core::slice::from_raw_parts(dma.add(DMA_OFF_LD), LD_LIST_FW_BYTES as usize)
        };
        match parse_ld_list(buf, 512) {
            Ok(list) => {
                serial::write_str(" n=");
                write_dec(list.count as u32);
                let show = list.count.min(4);
                for i in 0..show {
                    let e = list.entries[i];
                    serial::write_str(" id=");
                    write_dec(u32::from(e.target_id));
                    serial::write_str(" class=");
                    serial::write_str(match classify_ld_bytes(e.size_bytes) {
                        LdClass::Ubuntu => "ubuntu",
                        LdClass::Spare => "spare",
                        LdClass::Other => "other",
                    });
                    serial::write_str(" bytes=");
                    write_dec64(e.size_bytes);
                }
                match pick_spare(&list.entries[..list.count]) {
                    Ok(p) => {
                        serial::write_str(" pick=");
                        write_dec(u32::from(p.target_id));
                        spare = Some(p);
                    }
                    Err(PickError::UbuntuOnly) => serial::write_str(" pick=ubuntu"),
                    Err(PickError::NoSpare) => serial::write_str(" pick=none"),
                    Err(PickError::ManySpares) => serial::write_str(" pick=many"),
                }
            }
            Err(_) => serial::write_str(" parse=0"),
        }
    }
    serial::write_line(" (not PERC-LUN-OK)");

    let Some(picked) = spare else {
        serial::write_line("boot: perc read skip pick (not PERC-LUN-OK)");
        return;
    };
    let Some(target) = spare_read_target(Ok(picked)) else {
        serial::write_line("boot: perc read skip pick (not PERC-LUN-OK)");
        return;
    };
    let data_phys = base + DMA_OFF_LD as u64;
    let bytes = picked.size_bytes;

    serial::write_str("boot: perc read post bdf=");
    write_bdf(bus, dev, func);
    serial::write_str(" port=c0 id=");
    write_dec(u32::from(target));
    serial::write_str(" lba=0 blocks=1 frame=0x");
    write_hex64(dcmd_phys);
    serial::write_str(" data=0x");
    write_hex64(data_phys);
    serial::write_line(" (not PERC-LUN-OK)");

    let Some((rst, rto)) = issue_ld_read16(bar, target, 0, dcmd_phys, data_phys) else {
        serial::write_line("boot: perc read skip frame (not PERC-LUN-OK)");
        return;
    };
    serial::write_str("boot: perc read status=0x");
    write_hex8(rst);
    serial::write_str(" to=");
    serial::write_byte(if rto { b'1' } else { b'0' });
    if !rto && rst == MFI_STAT_OK {
        core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
        serial::write_str(" ok sig=");
        // SAFETY: firmware wrote at most 512 bytes at data_phys. Print 16.
        let sig = unsafe { core::slice::from_raw_parts(data_phys as *const u8, 16) };
        for b in sig {
            write_hex8(*b);
        }
    }
    serial::write_line(" (not PERC-LUN-OK)");
    if rto || rst != MFI_STAT_OK {
        serial::write_line("boot: perc read2 skip status (not PERC-LUN-OK)");
        return;
    }

    let Some(last) = spare_last_lba(bytes) else {
        serial::write_line("boot: perc read2 skip lba (not PERC-LUN-OK)");
        return;
    };
    serial::write_str("boot: perc read2 post bdf=");
    write_bdf(bus, dev, func);
    serial::write_str(" port=c0 id=");
    write_dec(u32::from(target));
    serial::write_str(" lba=");
    write_dec64(last);
    serial::write_str(" blocks=1 frame=0x");
    write_hex64(dcmd_phys);
    serial::write_str(" data=0x");
    write_hex64(data_phys);
    serial::write_line(" (not PERC-LUN-OK)");

    let Some((rst2, rto2)) = issue_ld_read16(bar, target, last, dcmd_phys, data_phys) else {
        serial::write_line("boot: perc read2 skip frame (not PERC-LUN-OK)");
        return;
    };
    serial::write_str("boot: perc read2 status=0x");
    write_hex8(rst2);
    serial::write_str(" to=");
    serial::write_byte(if rto2 { b'1' } else { b'0' });
    if !rto2 && rst2 == MFI_STAT_OK {
        serial::write_str(" ok");
    }
    serial::write_line(" (not PERC-LUN-OK)");
    if rto2 || rst2 != MFI_STAT_OK {
        serial::write_line("boot: perc virtio skip read2 (not PERC-LUN-OK)");
        return;
    }

    serial::write_str("boot: perc virtio ro id=");
    write_dec(u32::from(target));
    serial::write_str(" bytes=");
    write_dec64(bytes);
    serial::write_line(" (not PERC-LUN-OK)");
    perc_spare_arm(bar, target, bytes, data_phys, dcmd_phys);
}

/// One polled READ(16). Returns `(cmd_status, timed_out)`.
///
/// Reuses the DCMD page for the frame and the LD page for 512 bytes plus
/// 32 bytes of sense. Reply queue and RDPQ stay. No doorbell. No WRITE.
#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
fn issue_ld_read16(
    bar: u64,
    target: u8,
    lba: u64,
    frame_phys: u64,
    data_phys: u64,
) -> Option<(u8, bool)> {
    let rd = pack_ld_read16_polled(target, lba, data_phys)?;
    let desc = pack_mfa_descriptor(frame_phys)?;
    // SAFETY: both addresses are pages inside the eight-page frame-pool
    // allocation, identity-mapped after EBS. One outstanding MFA. The CDB
    // is READ(16). No doorbell store. No H840 pointer.
    // KANI-TARGET: host tests cover the packed READ, not this copy.
    unsafe {
        let frame = frame_phys as *mut u8;
        core::ptr::write_bytes(frame, 0, 4096);
        core::ptr::copy_nonoverlapping(rd.as_ptr(), frame, rd.len());
        core::ptr::write_bytes(data_phys as *mut u8, 0, 512 + 32);
    }
    core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
    // SAFETY: one 64-bit store to the low inbound queue port. Not doorbell 0x00.
    unsafe {
        core::ptr::write_volatile(
            (bar + u64::from(MFI_INBOUND_LOW_QUEUE_PORT)) as *mut u64,
            desc,
        );
    }
    Some(poll_mfi_status(frame_phys as *mut u8))
}

/// Remember the Mini BAR and the spare DMA pages for later guest reads.
/// Bytes are stored last so an Acquire load of [`perc_spare_bytes`] sees them.
#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
fn perc_spare_arm(bar: u64, target: u8, bytes: u64, data_phys: u64, frame_phys: u64) {
    PERC_SPARE_BAR.store(bar, core::sync::atomic::Ordering::Release);
    PERC_SPARE_TARGET.store(target, core::sync::atomic::Ordering::Release);
    PERC_SPARE_DATA.store(data_phys, core::sync::atomic::Ordering::Release);
    PERC_SPARE_FRAME.store(frame_phys, core::sync::atomic::Ordering::Release);
    PERC_SPARE_BYTES.store(bytes, core::sync::atomic::Ordering::Release);
}

/// Guest IN path. One MFI READ(16) per 512-byte sector. Never a WRITE.
/// The iron marker prints once, after the first full success.
///
/// Linux earlycon share drops [`crate::boot::serial::write_line`]. Iron
/// `985495be` lived both boot READs and Linux `vdc`, and COM2 never showed
/// this marker. These lines use the nowait UART path.
#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
fn perc_spare_read_uefi(lba: u64, buf: &mut [u8]) -> bool {
    let bytes = PERC_SPARE_BYTES.load(core::sync::atomic::Ordering::Acquire);
    let bar = PERC_SPARE_BAR.load(core::sync::atomic::Ordering::Acquire);
    let data = PERC_SPARE_DATA.load(core::sync::atomic::Ordering::Acquire);
    let frame = PERC_SPARE_FRAME.load(core::sync::atomic::Ordering::Acquire);
    let target = PERC_SPARE_TARGET.load(core::sync::atomic::Ordering::Acquire);
    let ok_range = bytes >= 512
        && bar != 0
        && data != 0
        && frame != 0
        && !buf.is_empty()
        && buf.len() % 512 == 0
        && lba
            .checked_mul(512)
            .and_then(|off| off.checked_add(buf.len() as u64))
            .is_some_and(|end| end <= bytes);
    if !ok_range {
        perc_guest_rd_fail(lba);
        return false;
    }
    let mut off = 0usize;
    while off < buf.len() {
        let sec = lba + (off as u64 / 512);
        let Some((st, timed_out)) = issue_ld_read16(bar, target, sec, frame, data) else {
            perc_guest_rd_fail(sec);
            return false;
        };
        if timed_out || st != MFI_STAT_OK {
            perc_guest_rd_fail(sec);
            return false;
        }
        core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
        // SAFETY: firmware wrote 512 bytes at `data`. `buf` is the guest
        // sector window for this IN. No write CDB was posted.
        unsafe {
            core::ptr::copy_nonoverlapping(data as *const u8, buf[off..].as_mut_ptr(), 512);
        }
        off += 512;
    }
    if !PERC_SPARE_GUEST_OK.swap(true, core::sync::atomic::Ordering::AcqRel) {
        use crate::boot::serial;
        serial::write_str_nowait("boot: perc virtio rd lba=");
        write_dec64_nowait(lba);
        serial::write_line_nowait(" ok");
        serial::write_line_nowait(M8_PERC_LUN_OK_MARKER);
    }
    true
}

/// Guest-read failure on the nowait UART. Earlycon hushes `write_line`.
#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
fn perc_guest_rd_fail(lba: u64) {
    use crate::boot::serial;
    serial::write_str_nowait("boot: perc virtio rd fail lba=");
    write_dec64_nowait(lba);
    serial::write_line_nowait(" (not PERC-LUN-OK)");
}

#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
fn poll_mfi_status(frame: *mut u8) -> (u8, bool) {
    let start = crate::arch::cpu::rdtsc();
    let budget = fusion_poll_cycles(crate::boot::raynu_f_flag::tsc_hz(), FUSION_POLL_SECS);
    loop {
        // SAFETY: cmd_status is byte 2 of the frame we just posted.
        let st = unsafe { core::ptr::read_volatile(frame.add(2)) };
        if st != MFI_CMD_STATUS_POLL {
            return (st, false);
        }
        if crate::arch::cpu::rdtsc().wrapping_sub(start) >= budget {
            return (st, true);
        }
        core::hint::spin_loop();
    }
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
fn write_dec64(mut n: u64) {
    use crate::boot::serial;
    let mut buf = [0u8; 20];
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

/// Decimal on the guest UART ring. Boot READs stay on [`write_dec64`].
#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
fn write_dec64_nowait(mut n: u64) {
    use crate::boot::serial;
    let mut buf = [0u8; 20];
    let mut i = buf.len();
    if n == 0 {
        serial::write_byte_nowait(b'0');
        return;
    }
    while n > 0 {
        i -= 1;
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
    }
    for b in &buf[i..] {
        serial::write_byte_nowait(*b);
    }
}

#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
fn write_hex_digit(v: u8) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    crate::boot::serial::write_byte(HEX[(v & 0xF) as usize]);
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
