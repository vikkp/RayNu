//! Guest-visible ATAPI CD-ROM for the private guest-UEFI VMCS.
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** (ADR-002 / ADR-014)
//! VERIFICATION: L1 (runtime + host tests; QEMU is the guest-visible gate)
//!
//! PCI IDE at `00:00.1` (virtio `00:00.0` fn1) **and** PIIX `00:01.1`.
//! PEI only `inw`s DID of `00:00.0` (virtio). A walk of that multifunction
//! slot finds fn1; a PIIX walk finds `00:01.1`. Same ATAPI backend.
//! linux hides duplicate slot0 IDE after Linux earlycon (iron COM2 BAR
//! conflict `00:00.1`/`00:01.1`; `ata_piix` secondary `-22`). linux hides
//! PIIX IDE after Linux high-half so built-in `ata_piix` does not
//! SRST-`msleep` past `Freeing initrd` (iron COM2 silence). Bootimg earlycon
//! share is too early: GRUB still needs PIIX ATAPI on iso=0. Product ISO
//! hid PIIX IDE (`8336a06` / `ea30da1`) so ConnectAll would not Start
//! IdeBus CpuSleep (iron COM2 `d61dc7e`: scsi@3 first, then `pci_ide=1`,
//! HLT `rip=0x7f0680d0` `ataio=0`, no virtio-iso IN). Hide plus
//! skip-after-inject `vec=0x20` livelocked the timer ISR through the
//! 16_777_216 cap (`pci_ide=0` `hlt=0` stop `rip=0x7f03fbe5`). OVMF El
//! Torito needs PIIX ATAPI (Stage 45 / nested iso=0); `product_iso_hides_ide`
//! stays in the model and returns false. firmware HLT skip without inject.
//! product ISO fw_cfg bootorder El Torito ide@ first.
//! iso=0 keeps IDE. windows_iso / generic_uefi stay in
//! the model. Compatibility-mode ISA `0x1F0`/`0x170` stays decoded after
//! PCI hide; linux ATA floating bus returns `0xFF` after Linux high-half
//! so leftover `ata_piix` SRST skips without `ata_msleep`.
//! Not `ISO-INSTALL-OK`.
//! After reset the ATAPI signature is LBA mid=`0x14` high=`0xEB` so firmware
//! sends PACKET (`0xA0`). Interrupt reason in sector-count is CDB `0x01`,
//! data-in `0x02`, complete `0x03`. Cylinder holds the PACKET byte count.
//! EXECUTE DEVICE DIAGNOSTIC (`0x90`) restores `0xEB14` (OVMF detect).
//! IDENTIFY PACKET word 0 is `0x85C0` (ATAPI CD-ROM, removable, 12-byte).
//! Product ISO IDENTIFY is PIO-only (LBA + PIO3/4, no MWDMA/UDMA) so Linux
//! `ata_piix` does not start BMIDE. PACKET DRQ is up to 31 CD sectors
//! (16-bit ATAPI byte count) **per DRQ**, not per CDB. READ(10)/READ(12)
//! continues DRQs until the CDB count is complete (QEMU-style). A 4-sector
//! DRQ used to complete Linux READ(10) short; a 31-sector cap without
//! continuation still dropped sector 32 of a 64 KiB `sr`/GRUB BlockIo.
//! nIEN (device-control bit 1) suppresses IRQ 14. SET FEATURES (`0xEF`) succeeds with DRDY (QEMU-compatible). Nested
//! Intel `48c598a`: BOTH-OK `ataio=1308` `packet=0` (`insn=ef` then
//! `edc9c3` IN EAX,DX poll) because ABRT never reached PACKET `0xA0`.
//! Slave (DEV bit 4) is absent so a 4-drive probe does not see four CDs
//! (nested Intel `f93caee`: `0xA1`×4 `ataio=408` `packet=0`).
//! Command BARs are 8-byte I/O (`0xFFFFFFFF` probe → `0xFFFFFFF9`); ATA
//! decodes legacy `0x1F0`/`0x170` and BAR-relocated ports. BMIDE BAR4 is
//! 16-byte I/O RAZ/WI so a bus-master probe is not `0xFF`.
//! CD stays GuestVisible.
//! Media is a retained ISO prefix (mock EFI catalog in host tests; placeholder
//! on QEMU if the operator has not called [`present`] yet). Bytes larger than
//! [`GUEST_CD_ISO_CAP`] stay in a product ISO window (pointer + length) so
//! ATAPI READ does not truncate a distro image into the 72 KiB lab stub.
//! Not virtio-in-guest. Not a distro installer. Not Everest E5.

use crate::devices::guest_platform::pci_cfg_offset;
use core::sync::atomic::{AtomicBool, AtomicU16, AtomicU32, AtomicU64, AtomicU8, Ordering};

/// ECMA-119 / El Torito sector size.
pub const ISO_SECTOR: usize = 2048;
/// Cap: PVD + Boot Record + catalog + 8-sector FAT ESP + ISO9660 `\EFI\BOOT`.
pub const MOCK_EFI_ISO_BYTES: usize = 36 * ISO_SECTOR;
/// El Torito catalog LBA in the mock / placeholder ISO.
pub const ELTORITO_CATALOG_LBA: u32 = 20;
/// El Torito no-emulation EFI load LBA (FAT12 ESP lives here).
pub const ELTORITO_LOAD_LBA: u32 = 22;
/// Catalog `SectorCount` in CD blocks. EDK2 PartitionDxe no-emulation sets
/// the child `BlockIo` size to 2048, so the FAT BPB must also be 2048.
/// Eight blocks hold reserved+FATs+root plus EFI/BOOT/BOOTX64 clusters.
/// Iron `df7d158` 512-byte BPB on that 2048-byte child: catalog+bootimg,
/// then VD re-reads (`readlba=17`) and no StartImage.
pub const ELTORITO_SECTOR_COUNT: u16 = 8;
/// FAT12 bytes per sector — matches El Torito no-emulation child BlockIo.
pub const ELTORITO_FAT_BPS: usize = 2048;
/// `\EFI\BOOT\BOOTX64.EFI` starts at FAT cluster 4 (data sector 6).
pub const ELTORITO_BOOTX64_OFF: usize = 6 * ELTORITO_FAT_BPS;
/// ISO9660 path table (type L). Type M is unused (PVD field 0).
pub const ISO9660_PATHTABLE_LBA: u32 = 19;
/// ISO9660 root / `EFI` / `BOOT` / `BOOTX64.EFI;1` after the FAT ESP.
pub const ISO9660_ROOT_LBA: u32 = 30;
pub const ISO9660_EFI_LBA: u32 = 31;
pub const ISO9660_BOOTDIR_LBA: u32 = 32;
pub const ISO9660_BOOTX64_LBA: u32 = 33;
/// COM1 bytes the CD EFI writes when it actually runs. Not a sector count.
pub const ELTORITO_PAYLOAD_MAGIC: &[u8] = b"RN-ELT";

/// QEMU / serial marker when the guest-UEFI VMCS can see CD media.
pub const M7_E5_OVMF_CDROM_OK_MARKER: &str = "RAYNU-V-M7-E5-OVMF-CDROM-OK";

/// Cap of the in-BSS lab stub. Product ISO bytes use [`retain`]'s window.
pub const GUEST_CD_ISO_CAP: usize = MOCK_EFI_ISO_BYTES;

pub const GUEST_CD_PCI_BUS: u8 = 0;
pub const GUEST_CD_PCI_DEV: u8 = 0;
pub const GUEST_CD_PCI_FN: u8 = 1;
pub const GUEST_CD_PCI_VENDOR: u16 = 0x8086;
pub const GUEST_CD_PCI_DEVICE: u16 = 0x7010;

const ATA_STATUS_ERR: u8 = 0x01;
const ATA_STATUS_DRQ: u8 = 0x08;
const ATA_STATUS_SEEK: u8 = 0x10;
const ATA_STATUS_DRDY: u8 = 0x40;
const ATA_STATUS_BSY: u8 = 0x80;
const ATA_CMD_DEVICE_RESET: u8 = 0x08;
const ATA_CMD_DIAGNOSTIC: u8 = 0x90;
const ATA_CMD_IDENTIFY: u8 = 0xEC;
const ATA_CMD_IDENTIFY_PACKET: u8 = 0xA1;
const ATA_CMD_PACKET: u8 = 0xA0;
const ATA_CMD_SET_FEATURES: u8 = 0xEF;
const ATA_DEVCTL_SRST: u8 = 0x04;
/// Device Control bit 1: nIEN — 1 = do not assert IRQ.
const ATA_DEVCTL_NIEN: u8 = 0x02;
/// ATAPI interrupt reason (sector-count): CDB write.
const ATAPI_INT_CD: u8 = 0x01;
/// ATAPI interrupt reason: data-in to host.
const ATAPI_INT_IO: u8 = 0x02;
const ATAPI_SIG_LBA: [u8; 3] = [0x01, 0x14, 0xEB];
const SCSI_TEST_UNIT: u8 = 0x00;
const SCSI_REQUEST_SENSE: u8 = 0x03;
const SCSI_INQUIRY: u8 = 0x12;
const SCSI_MODE_SENSE6: u8 = 0x1A;
const SCSI_START_STOP: u8 = 0x1B;
const SCSI_PREVENT: u8 = 0x1E;
const SCSI_READ_CAPACITY: u8 = 0x25;
const SCSI_READ10: u8 = 0x28;
const SCSI_READ_TOC: u8 = 0x43;
const SCSI_GET_CONFIG: u8 = 0x46;
const SCSI_GET_EVENT: u8 = 0x4A;
const SCSI_MODE_SENSE10: u8 = 0x5A;
const SCSI_READ12: u8 = 0xA8;
const SCSI_SENSE_ILLEGAL: u8 = 0x05;
const SCSI_ASC_INVALID_OPCODE: u8 = 0x20;
/// ATAPI cylinder is 16-bit, so one DRQ is at most 31 × 2048 = 63488 bytes.
/// Linux `sr` READ(10) is typically 32 KiB–64 KiB (32 CD sectors = two DRQs).
const XFER_CAP: usize = 31 * ISO_SECTOR;
const XFER_SEC: usize = XFER_CAP / ISO_SECTOR;

#[derive(Clone, Copy, PartialEq, Eq)]
enum AtaXfer {
    Idle,
    Identify,
    PacketCdb,
    PacketData,
}

struct CdMedia {
    iso: [u8; GUEST_CD_ISO_CAP],
    len: usize,
    /// Product ISO bytes when `len > GUEST_CD_ISO_CAP`. Null for the lab stub.
    ext_ptr: *const u8,
    ext_len: usize,
    iso_id: u64,
    visible: bool,
    pci_enum: bool,
    sectors_read: u32,
    catalog_read: bool,
    boot_image_read: bool,
    last_read_lba: u32,
    pci_addr: u32,
    pci_cmd: u16,
    bar0: u32,
    bar1: u32,
    bar2: u32,
    bar3: u32,
    bar4: u32,
    ata_feat: u8,
    ata_count: u8,
    ata_lba: [u8; 3],
    ata_dev: u8,
    ata_err: u8,
    ata_status: u8,
    ata_devctl: u8,
    byte_limit: u16,
    sense_key: u8,
    sense_asc: u8,
    xfer: AtaXfer,
    xfer_off: usize,
    xfer_end: usize,
    cdb: [u8; 12],
    cdb_got: usize,
    /// Remaining READ(10)/READ(12) LBA after the current DRQ is filled.
    pkt_lba: u32,
    /// Remaining CD sectors for this CDB. Zero when the packet is not a READ.
    pkt_left: u32,
    data: [u8; XFER_CAP],
}

impl CdMedia {
    const fn empty() -> Self {
        Self {
            iso: [0u8; GUEST_CD_ISO_CAP],
            len: 0,
            ext_ptr: core::ptr::null(),
            ext_len: 0,
            iso_id: 0,
            visible: false,
            pci_enum: false,
            sectors_read: 0,
            catalog_read: false,
            boot_image_read: false,
            last_read_lba: 0,
            pci_addr: 0,
            pci_cmd: 0x0005,
            bar0: 0x1F1,
            bar1: 0x03F5,
            bar2: 0x0171,
            bar3: 0x0375,
            bar4: 1,
            ata_feat: 0,
            ata_count: 0x01,
            ata_lba: ATAPI_SIG_LBA,
            ata_dev: 0,
            ata_err: 0x01,
            ata_status: ATA_STATUS_DRDY | ATA_STATUS_SEEK,
            ata_devctl: 0,
            byte_limit: 0,
            sense_key: 0,
            sense_asc: 0,
            xfer: AtaXfer::Idle,
            xfer_off: 0,
            xfer_end: 0,
            cdb: [0; 12],
            cdb_got: 0,
            pkt_lba: 0,
            pkt_left: 0,
            data: [0; XFER_CAP],
        }
    }
}

// JUSTIFICATION: one guest-UEFI CD; mgmt attach and the private VMCS share it.
// Host tests take the spinlock. Firmware is single-threaded after EBS.
struct GuestCd(core::cell::UnsafeCell<CdMedia>);

// SAFETY: exclusive access is enforced by `GUEST_CD_LOCK`.
// KANI-TARGET: management-plane CD media; outside Proven Core.
unsafe impl Sync for GuestCd {}

static GUEST_CD: GuestCd = GuestCd(core::cell::UnsafeCell::new(CdMedia::empty()));
static GUEST_CD_LOCK: AtomicBool = AtomicBool::new(false);

fn with_cd<R>(f: impl FnOnce(&mut CdMedia) -> R) -> R {
    while GUEST_CD_LOCK.swap(true, Ordering::Acquire) {
        core::hint::spin_loop();
    }
    // SAFETY: lock held; exclusive mutable access to the guest CD.
    // KANI-TARGET: guest CD media mutex (outside Proven Core).
    let out = unsafe { f(&mut *GUEST_CD.0.get()) };
    GUEST_CD_LOCK.store(false, Ordering::Release);
    out
}

static VISIBLE: AtomicBool = AtomicBool::new(false);
static PCI_ENUM: AtomicBool = AtomicBool::new(false);
static HIDE_SLOT0: AtomicBool = AtomicBool::new(false);
static HIDE_PIIX: AtomicBool = AtomicBool::new(false);
static HIDE_PRODUCT: AtomicBool = AtomicBool::new(false);
static ATA_FLOAT: AtomicBool = AtomicBool::new(false);
static SECTORS: AtomicU32 = AtomicU32::new(0);
static ISO_ID: AtomicU64 = AtomicU64::new(0);
static ISO_LEN: AtomicU64 = AtomicU64::new(0);
static MARKER: AtomicBool = AtomicBool::new(false);
static PACKET_N: AtomicU32 = AtomicU32::new(0);
static LAST_SCSI: AtomicU8 = AtomicU8::new(0);
static ATA_CMD_N: AtomicU32 = AtomicU32::new(0);
static LAST_ATA_CMD: AtomicU8 = AtomicU8::new(0);
static ATA_IO_N: AtomicU32 = AtomicU32::new(0);
static PCI_CMD_WRITES: AtomicU32 = AtomicU32::new(0);
static LAST_PCI_CMD_WR: AtomicU16 = AtomicU16::new(0);
const PCI_CMD_WR_SEQ_CAP: usize = 8;
static PCI_CMD_WR_SEQ: [AtomicU16; PCI_CMD_WR_SEQ_CAP] = [
    AtomicU16::new(0),
    AtomicU16::new(0),
    AtomicU16::new(0),
    AtomicU16::new(0),
    AtomicU16::new(0),
    AtomicU16::new(0),
    AtomicU16::new(0),
    AtomicU16::new(0),
];
static CATALOG_READ: AtomicBool = AtomicBool::new(false);
static BOOT_IMAGE_READ: AtomicBool = AtomicBool::new(false);
static LAST_READ_LBA: AtomicU32 = AtomicU32::new(0);

/// Decode PCI config address (mechanism #1).
pub fn pci_bdf(addr: u32) -> (u8, u8, u8, u8) {
    let bus = ((addr >> 16) & 0xff) as u8;
    let dev = ((addr >> 11) & 0x1f) as u8;
    let fun = ((addr >> 8) & 7) as u8;
    let off = (addr & 0xfc) as u8;
    (bus, dev, fun, off)
}

pub fn pci_addr_selects_cd(addr: u32) -> bool {
    if (addr & 0x8000_0000) == 0 {
        return false;
    }
    let (bus, dev, fun, _) = pci_bdf(addr);
    // Objective: virtio fn1 `00:00.1`. PIIX fn1 `00:01.1` is the same CD.
    bus == 0 && fun == 1 && (dev == 0 || dev == 1)
}

/// Firmware needs slot-0 fn1. Linux PCI scan of both IDE functions BAR-conflicts
/// (iron COM2 `ata_piix` secondary `-22`). linux hides duplicate slot0 IDE.
pub fn linux_hides_duplicate_slot0_ide(linux: bool, addr: u32) -> bool {
    if !linux || (addr & 0x8000_0000) == 0 {
        return false;
    }
    let (bus, dev, fun, _) = pci_bdf(addr);
    bus == 0 && dev == 0 && fun == 1
}

/// Firmware already booted the El Torito CD. Built-in alpine-virt `ata_piix`
/// is a device_initcall after `Freeing initrd` and `ata_msleep`s on SRST
/// (iron COM2 then silent). Do **not** use earlycon share (bootimg): GRUB
/// still reads the kernel from PIIX ATAPI. linux hides PIIX IDE after
/// Linux high-half. Media is virtio-iso `00:03.0`. Not `ISO-INSTALL-OK`.
pub fn linux_hides_piix_ide(linux_high_half: bool, addr: u32) -> bool {
    if !linux_high_half || (addr & 0x8000_0000) == 0 {
        return false;
    }
    let (bus, dev, fun, _) = pci_bdf(addr);
    bus == 0 && dev == 1 && fun == 1
}

/// Iron COM2 `d61dc7e`: product `bootorder` scsi@3 first was served, then
/// PciBus ConnectAll still Started AtaAtapiPassThru (`pci select 00:01.01`,
/// `pci_ide=1`, HLT `rip=0x7f0680d0` `ataio=0`, inj climbing, no
/// `virtio-iso IN`). Hide-IDE (`8336a06` / `ea30da1`) then skip-after-inject
/// `vec=0x20` livelocked the timer ISR through the 16_777_216 cap
/// (`pci_ide=0` `ataio=0` `hlt=0`, no virtio-iso IN, stop `rip=0x7f03fbe5`).
/// OVMF El Torito needs PIIX ATAPI (Stage 45 / nested iso=0). Do not hide.
/// firmware HLT skip without inject. product ISO hides PIIX IDE.
/// product ISO fw_cfg bootorder El Torito ide@ first.
/// Not `ISO-INSTALL-OK`.
pub fn product_iso_hides_ide(_addr: u32) -> bool {
    false
}

/// Compatibility-mode ISA `0x1F0`/`0x170` stays decoded after PCI hide.
/// `ata_piix` SRST-`msleep`s while BSY/DRDY look live (iron COM2 after
/// `Freeing initrd`). Return floating-bus `0xFF` after Linux high-half so
/// libata skips the port without a timer wait. linux ATA floating bus.
/// Not `ISO-INSTALL-OK`.
pub fn linux_ata_floating_bus(linux_high_half: bool) -> bool {
    linux_high_half
}

/// PCI config address for the guest IDE function (`00:00.1`).
pub fn pci_config_addr() -> u32 {
    0x8000_0000
        | (u32::from(GUEST_CD_PCI_BUS) << 16)
        | (u32::from(GUEST_CD_PCI_DEV) << 11)
        | (u32::from(GUEST_CD_PCI_FN) << 8)
}

pub fn is_ata_primary_port(port: u16) -> bool {
    with_cd(|m| ata_reg(m, port).is_some())
}

/// ATA data register (command-block offset 0): legacy `0x1F0`/`0x170` or a
/// BAR-relocated command block. `rep insw` IDENTIFY/PACKET FIFOs land here.
pub fn is_ata_data_port(port: u16) -> bool {
    with_cd(|m| ata_reg(m, port) == Some(0))
}

fn ata_is_slave(m: &CdMedia) -> bool {
    (m.ata_dev & 0x10) != 0
}

fn ata_absent_status() -> u8 {
    0
}

fn apply_no_device(m: &mut CdMedia) {
    m.xfer = AtaXfer::Idle;
    m.ata_err = 0x04;
    m.ata_status = ATA_STATUS_ERR;
    m.ata_count = 0;
    m.ata_lba = [0, 0, 0];
}

/// Map an I/O port onto the ATA command block (0–7) or control port.
fn ata_reg(m: &CdMedia, port: u16) -> Option<u8> {
    // Compatibility mode keeps ISA ports even after PciBus relocates BARs.
    if (0x01F0..=0x01F7).contains(&port) {
        return Some((port - 0x01F0) as u8);
    }
    if (0x0170..=0x0177).contains(&port) {
        return Some((port - 0x0170) as u8);
    }
    if port == 0x03F6 || port == 0x0376 {
        return Some(8);
    }
    let cmd = (m.bar0 & !7) as u16;
    if port.wrapping_sub(cmd) < 8 {
        return Some((port - cmd) as u8);
    }
    let cmd2 = (m.bar2 & !7) as u16;
    if port.wrapping_sub(cmd2) < 8 {
        return Some((port - cmd2) as u8);
    }
    let ctl = (m.bar1 & !3) as u16;
    if port == ctl.wrapping_add(2) {
        return Some(8);
    }
    let ctl2 = (m.bar3 & !3) as u16;
    if port == ctl2.wrapping_add(2) {
        return Some(8);
    }
    None
}

fn bmide_base(m: &CdMedia) -> u16 {
    (m.bar4 & !0xF) as u16
}

/// Bus-master IDE (BAR4, 16-byte I/O). Address 0 is unprogrammed — do not
/// steal the PIC/PIT range.
pub fn is_bmide_port(port: u16) -> bool {
    with_cd(|m| {
        let base = bmide_base(m);
        base != 0 && port.wrapping_sub(base) < 16
    })
}

/// RAZ/WI BMIDE. Unhandled `IN` was `0xFF` (looks busy/error).
pub fn bmide_io(_port: u16, is_in: bool, size: u8, rax: u64) -> u64 {
    if is_in {
        let mask = io_mask(size);
        rax & !mask
    } else {
        rax
    }
}

pub fn is_pci_data_port(port: u16) -> bool {
    (0x0CFC..=0x0CFF).contains(&port)
}

/// Honest guest-visible evidence: media presented and firmware enumerated
/// the IDE function or read an ATAPI sector.
pub fn cdrom_visible_evidence(visible: bool, pci_enum: bool, sectors: u32) -> bool {
    visible && (pci_enum || sectors > 0)
}

pub fn is_visible() -> bool {
    VISIBLE.load(Ordering::Acquire)
}

pub fn pci_enumerated() -> bool {
    PCI_ENUM.load(Ordering::Acquire)
}

pub fn sectors_read() -> u32 {
    SECTORS.load(Ordering::Acquire)
}

/// Firmware READ covered the El Torito catalog LBA. Not a completed CD boot.
pub fn eltorito_catalog_read() -> bool {
    CATALOG_READ.load(Ordering::Acquire)
}

/// Firmware READ covered the El Torito load LBA (EFI image). Not StartImage.
pub fn eltorito_boot_image_read() -> bool {
    BOOT_IMAGE_READ.load(Ordering::Acquire)
}

/// Last SCSI READ(10/12) LBA. 0 until the first data READ.
pub fn last_read_lba() -> u32 {
    LAST_READ_LBA.load(Ordering::Acquire)
}

pub fn retained_iso_id() -> u64 {
    ISO_ID.load(Ordering::Acquire)
}

pub fn retained_len() -> usize {
    ISO_LEN.load(Ordering::Acquire) as usize
}

/// True when ATAPI media is larger than the 72 KiB lab El Torito stub.
///
/// INVARIANTS:
/// - `false` for idle CD and for [`present_placeholder`]
/// - `true` only after [`retain`] / [`present`] of `len > GUEST_CD_ISO_CAP`
/// - Does not imply a distro installer or `ISO-INSTALL-OK`
pub fn product_iso_window_armed() -> bool {
    retained_len() > GUEST_CD_ISO_CAP
}

/// Product ISO window pointer (not the 72 KiB lab stub).
///
/// INVARIANTS:
/// - `None` when idle or lab El Torito
/// - Pointer stays valid until [`reset`]
/// - Does not imply `ISO-INSTALL-OK`
pub fn product_iso_window_ptr() -> Option<(*const u8, usize)> {
    with_cd(|m| {
        if m.ext_len > GUEST_CD_ISO_CAP && !m.ext_ptr.is_null() {
            Some((m.ext_ptr, m.ext_len))
        } else {
            None
        }
    })
}

/// Lab 72 KiB RN-ELT CD (Stage 45). Product ISO continues past El Torito.
pub fn is_lab_eltorito_media() -> bool {
    !product_iso_window_armed()
}

/// Pure size check used by host tests and the Stage 46 stop policy.
pub fn is_lab_eltorito_stub_len(cd_len: usize) -> bool {
    cd_len == MOCK_EFI_ISO_BYTES || cd_len == 0
}

pub fn is_retained_for(iso_id: u64) -> bool {
    iso_id != 0 && retained_iso_id() == iso_id && retained_len() >= ISO_SECTOR
}

pub fn marker_printed() -> bool {
    MARKER.load(Ordering::Acquire)
}

pub fn reset() {
    with_cd(|m| *m = CdMedia::empty());
    VISIBLE.store(false, Ordering::Release);
    PCI_ENUM.store(false, Ordering::Release);
    HIDE_SLOT0.store(false, Ordering::Release);
    HIDE_PIIX.store(false, Ordering::Release);
    HIDE_PRODUCT.store(false, Ordering::Release);
    ATA_FLOAT.store(false, Ordering::Release);
    SECTORS.store(0, Ordering::Release);
    ISO_ID.store(0, Ordering::Release);
    ISO_LEN.store(0, Ordering::Release);
    MARKER.store(false, Ordering::Release);
    PACKET_N.store(0, Ordering::Release);
    LAST_SCSI.store(0, Ordering::Release);
    ATA_CMD_N.store(0, Ordering::Release);
    LAST_ATA_CMD.store(0, Ordering::Release);
    ATA_IO_N.store(0, Ordering::Release);
    PCI_CMD_WRITES.store(0, Ordering::Release);
    LAST_PCI_CMD_WR.store(0, Ordering::Release);
    for slot in PCI_CMD_WR_SEQ.iter() {
        slot.store(0, Ordering::Release);
    }
    CATALOG_READ.store(false, Ordering::Release);
    BOOT_IMAGE_READ.store(false, Ordering::Release);
    LAST_READ_LBA.store(0, Ordering::Release);
    crate::devices::guest_irq::reset();
}

/// PACKET commands issued since last reset (firmware ATAPI activity).
pub fn packet_commands() -> u32 {
    PACKET_N.load(Ordering::Acquire)
}

/// Last SCSI opcode from a completed 12-byte CDB.
pub fn last_scsi() -> u8 {
    LAST_SCSI.load(Ordering::Acquire)
}

/// ATA commands written to 0x1F7 since last reset.
pub fn ata_commands() -> u32 {
    ATA_CMD_N.load(Ordering::Acquire)
}

/// Last byte written to the ATA command register.
pub fn last_ata_cmd() -> u8 {
    LAST_ATA_CMD.load(Ordering::Acquire)
}

/// Device-control nIEN (1 = do not assert IRQ 14).
/// firmware take IOAPIC ATA. Not `ISO-INSTALL-OK`.
pub fn ata_nien() -> bool {
    with_cd(|m| (m.ata_devctl & ATA_DEVCTL_NIEN) != 0)
}

/// ATA PIO accesses (status polls and commands). Nested VT-x `8e55abf`
/// `ata=0x0` only counted command-register writes.
pub fn ata_io_accesses() -> u32 {
    ATA_IO_N.load(Ordering::Acquire)
}

/// Live IDE PCI command (offset 0x04). Default `0x0005` (IO+BusMaster).
/// Iron COM2 `21dc562` never printed this; ADR-015 needs `cmdwr`/`pcicmd`.
pub fn pci_cmd() -> u16 {
    with_cd(|m| m.pci_cmd)
}

/// Count of firmware writes to IDE PCI command. Iron COM2 `184ee61`:
/// `cmdwr=6` `wr=0x0` `pcicmd=0x1` (OR `0x0001` hid the disable).
/// Iron COM2 `abba969`: honor stuck `pcicmd=0` `wr=0` still `ataio=0`.
/// Store the write as-is. Do not OR `0x0001`.
pub fn pci_cmd_writes() -> u32 {
    PCI_CMD_WRITES.load(Ordering::Acquire)
}

/// Last firmware value written to IDE PCI command (stored as written).
pub fn last_pci_cmd_wr() -> u16 {
    LAST_PCI_CMD_WR.load(Ordering::Acquire)
}

/// Firmware COMMAND write `i` (0-based, first eight). Iron `abba969`
/// left only the last `wr=`; the next proof is the six-write sequence.
pub fn pci_cmd_wr_at(i: usize) -> u16 {
    PCI_CMD_WR_SEQ
        .get(i)
        .map(|slot| slot.load(Ordering::Acquire))
        .unwrap_or(0)
}

/// Retain ISO bytes without making the PCI device live.
///
/// INVARIANTS:
/// - `iso.len() <= GUEST_CD_ISO_CAP`: copy into the BSS stub (lab)
/// - `iso.len() > GUEST_CD_ISO_CAP`: prefix copy + product window on `iso`
///   (caller keeps the slice alive for the guest-UEFI CD life)
/// - Advertised ATAPI size is `iso.len()`, not truncated to 72 KiB
pub fn retain(iso: &[u8], iso_id: u64) -> bool {
    if iso_id == 0 || iso.len() < ISO_SECTOR {
        return false;
    }
    let n = iso.len();
    with_cd(|m| {
        let copy_n = core::cmp::min(n, GUEST_CD_ISO_CAP);
        m.iso[..copy_n].copy_from_slice(&iso[..copy_n]);
        if copy_n < GUEST_CD_ISO_CAP {
            m.iso[copy_n..].fill(0);
        }
        if n > GUEST_CD_ISO_CAP {
            m.ext_ptr = iso.as_ptr();
            m.ext_len = n;
        } else {
            m.ext_ptr = core::ptr::null();
            m.ext_len = 0;
        }
        m.len = n;
        m.iso_id = iso_id;
    });
    ISO_ID.store(iso_id, Ordering::Release);
    ISO_LEN.store(n as u64, Ordering::Release);
    true
}

/// Mark already-retained media live on the PCI IDE function.
pub fn make_visible() -> bool {
    let ok = with_cd(|m| {
        if m.len < ISO_SECTOR || m.iso_id == 0 {
            return false;
        }
        m.visible = true;
        m.pci_enum = false;
        m.sectors_read = 0;
        m.catalog_read = false;
        m.boot_image_read = false;
        m.last_read_lba = 0;
        apply_atapi_signature(m);
        true
    });
    if ok {
        VISIBLE.store(true, Ordering::Release);
        PCI_ENUM.store(false, Ordering::Release);
        SECTORS.store(0, Ordering::Release);
        MARKER.store(false, Ordering::Release);
        CATALOG_READ.store(false, Ordering::Release);
        BOOT_IMAGE_READ.store(false, Ordering::Release);
        LAST_READ_LBA.store(0, Ordering::Release);
    }
    ok
}

/// Present retained (or supplied) media on the guest PCI IDE function.
pub fn present(iso: &[u8], iso_id: u64) -> bool {
    if !retain(iso, iso_id) {
        return false;
    }
    make_visible()
}

/// Placeholder ISO: PVD `CD001` at LBA 16 plus a checksummed EFI El Torito
/// catalog, a 2048-byte FAT12 ESP at the load LBA, and ISO9660 `\EFI\BOOT`.
pub fn present_placeholder() -> bool {
    let ok = with_cd(|m| {
        m.iso.fill(0);
        write_placeholder_iso(&mut m.iso);
        m.ext_ptr = core::ptr::null();
        m.ext_len = 0;
        m.len = MOCK_EFI_ISO_BYTES;
        m.iso_id = 1;
        true
    });
    if !ok {
        return false;
    }
    ISO_ID.store(1, Ordering::Release);
    ISO_LEN.store(MOCK_EFI_ISO_BYTES as u64, Ordering::Release);
    make_visible()
}

fn iso_both_u16(dst: &mut [u8], v: u16) {
    if dst.len() < 4 {
        return;
    }
    dst[0..2].copy_from_slice(&v.to_le_bytes());
    dst[2..4].copy_from_slice(&v.to_be_bytes());
}

fn iso_both_u32(dst: &mut [u8], v: u32) {
    if dst.len() < 8 {
        return;
    }
    dst[0..4].copy_from_slice(&v.to_le_bytes());
    dst[4..8].copy_from_slice(&v.to_be_bytes());
}

/// ECMA-119 directory record. Name length 1 uses 0x00 (`.`) / 0x01 (`..`).
fn iso9660_put_dirent(
    dst: &mut [u8],
    off: usize,
    lba: u32,
    size: u32,
    flags: u8,
    name: &[u8],
) -> usize {
    let nlen = name.len();
    if nlen == 0 || nlen > 255 {
        return off;
    }
    let mut rec = 33 + nlen;
    if rec & 1 != 0 {
        rec += 1;
    }
    if off + rec > dst.len() {
        return off;
    }
    dst[off] = rec as u8;
    dst[off + 1] = 0;
    iso_both_u32(&mut dst[off + 2..off + 10], lba);
    iso_both_u32(&mut dst[off + 10..off + 18], size);
    dst[off + 18] = 126;
    dst[off + 19] = 8;
    dst[off + 20] = 27;
    dst[off + 25] = flags;
    iso_both_u16(&mut dst[off + 28..off + 32], 1);
    dst[off + 32] = nlen as u8;
    dst[off + 33..off + 33 + nlen].copy_from_slice(name);
    off + rec
}

fn iso9660_put_path(dst: &mut [u8], off: usize, lba: u32, parent: u16, name: &[u8]) -> usize {
    let nlen = if name.is_empty() { 1 } else { name.len() };
    let mut rec = 8 + nlen;
    if rec & 1 != 0 {
        rec += 1;
    }
    if off + rec > dst.len() {
        return off;
    }
    dst[off] = nlen as u8;
    dst[off + 1] = 0;
    dst[off + 2..off + 6].copy_from_slice(&lba.to_le_bytes());
    dst[off + 6..off + 8].copy_from_slice(&parent.to_le_bytes());
    if name.is_empty() {
        dst[off + 8] = 0;
    } else {
        dst[off + 8..off + 8 + name.len()].copy_from_slice(name);
    }
    off + rec
}

fn iso9660_name_eq(fname: &[u8], want: &[u8]) -> bool {
    if fname.is_empty() || fname == [0] || fname == [1] {
        return false;
    }
    let base = match fname.iter().position(|&c| c == b';') {
        Some(i) => &fname[..i],
        None => fname,
    };
    base.eq_ignore_ascii_case(want)
}

fn iso9660_find(iso: &[u8], lba: u32, size: u32, want: &[u8], is_dir: bool) -> Option<(u32, u32)> {
    let start = (lba as usize).saturating_mul(ISO_SECTOR);
    if start >= iso.len() {
        return None;
    }
    let end = start
        .saturating_add(size as usize)
        .min(iso.len())
        .min(start + ISO_SECTOR);
    let dir = &iso[start..end];
    let mut off = 0usize;
    while off + 33 < dir.len() {
        let rec = dir[off] as usize;
        if rec < 34 {
            break;
        }
        if off + rec > dir.len() {
            return None;
        }
        let nlen = dir[off + 32] as usize;
        if off + 33 + nlen > off + rec {
            off += rec;
            continue;
        }
        let fname = &dir[off + 33..off + 33 + nlen];
        let flags = dir[off + 25];
        if ((flags & 2) != 0) == is_dir && iso9660_name_eq(fname, want) {
            let loc = u32::from_le_bytes([
                dir[off + 2],
                dir[off + 3],
                dir[off + 4],
                dir[off + 5],
            ]);
            let sz = u32::from_le_bytes([
                dir[off + 10],
                dir[off + 11],
                dir[off + 12],
                dir[off + 13],
            ]);
            return Some((loc, sz));
        }
        off += rec;
    }
    None
}

/// ISO9660 `\EFI\BOOT\BOOTX64.EFI;1` walk from the PVD root directory record.
///
/// INVARIANTS:
/// - PVD type 1 `CD001` with a non-zero root directory record at offset 156
/// - Directory flags bit 1 set for `EFI` and `BOOT`
/// - File extent starts with `MZ`
pub fn edk2_iso9660_bootx64_ok(iso: &[u8]) -> bool {
    if iso.len() < MOCK_EFI_ISO_BYTES {
        return false;
    }
    let pvd = 16 * ISO_SECTOR;
    if iso[pvd] != 1 || &iso[pvd + 1..pvd + 6] != b"CD001" {
        return false;
    }
    let rec = iso[pvd + 156] as usize;
    if rec < 34 || pvd + 156 + rec > iso.len() {
        return false;
    }
    let root_lba = u32::from_le_bytes([
        iso[pvd + 158],
        iso[pvd + 159],
        iso[pvd + 160],
        iso[pvd + 161],
    ]);
    let root_sz = u32::from_le_bytes([
        iso[pvd + 166],
        iso[pvd + 167],
        iso[pvd + 168],
        iso[pvd + 169],
    ]);
    if root_lba != ISO9660_ROOT_LBA || root_sz == 0 {
        return false;
    }
    let Some((efi_lba, efi_sz)) = iso9660_find(iso, root_lba, root_sz, b"EFI", true) else {
        return false;
    };
    let Some((boot_lba, boot_sz)) = iso9660_find(iso, efi_lba, efi_sz, b"BOOT", true) else {
        return false;
    };
    let Some((file_lba, file_sz)) = iso9660_find(iso, boot_lba, boot_sz, b"BOOTX64.EFI", false)
    else {
        return false;
    };
    if file_sz < 0x200 {
        return false;
    }
    let off = (file_lba as usize).saturating_mul(ISO_SECTOR);
    off + 2 <= iso.len() && &iso[off..off + 2] == b"MZ"
}

/// Write the mock EFI ISO prefix (PVD + Boot Record + catalog + FAT ESP + ISO9660).
///
/// INVARIANTS:
/// - Needs [`MOCK_EFI_ISO_BYTES`]
/// - Catalog validation entry 16-bit words sum to 0 (EDK2 PartitionDxe)
/// - Load LBA is a 2048-byte FAT12 ESP with `\EFI\BOOT\BOOTX64.EFI`
/// - PVD root directory record + path table name ISO9660 `\EFI\BOOT\BOOTX64.EFI;1`
pub fn write_placeholder_iso(iso: &mut [u8]) {
    if iso.len() < MOCK_EFI_ISO_BYTES {
        return;
    }
    iso[..MOCK_EFI_ISO_BYTES].fill(0);
    let pvd = 16 * ISO_SECTOR;
    iso[pvd] = 1;
    iso[pvd + 1..pvd + 6].copy_from_slice(b"CD001");
    iso[pvd + 6] = 1;
    iso[pvd + 8..pvd + 40].fill(b' ');
    iso[pvd + 40..pvd + 72].fill(b' ');
    iso[pvd + 40..pvd + 50].copy_from_slice(b"RAYNU-V-CD");
    let vol = (MOCK_EFI_ISO_BYTES / ISO_SECTOR) as u32;
    iso_both_u32(&mut iso[pvd + 80..pvd + 88], vol);
    iso_both_u16(&mut iso[pvd + 120..pvd + 124], 1);
    iso_both_u16(&mut iso[pvd + 124..pvd + 128], 1);
    iso_both_u16(&mut iso[pvd + 128..pvd + 132], ISO_SECTOR as u16);
    iso[pvd + 881] = 1;
    let term = 18 * ISO_SECTOR;
    iso[term] = 0xFF;
    iso[term + 1..term + 6].copy_from_slice(b"CD001");
    iso[term + 6] = 1;
    let br = 17 * ISO_SECTOR;
    iso[br] = 0;
    iso[br + 1..br + 6].copy_from_slice(b"CD001");
    iso[br + 6] = 1;
    iso[br + 7..br + 7 + 23].copy_from_slice(b"EL TORITO SPECIFICATION");
    iso[br + 71..br + 75].copy_from_slice(&ELTORITO_CATALOG_LBA.to_le_bytes());
    let cat = (ELTORITO_CATALOG_LBA as usize) * ISO_SECTOR;
    iso[cat] = 0x01;
    iso[cat + 1] = 0xEF;
    iso[cat + 4..cat + 11].copy_from_slice(b"RAYNU-V");
    iso[cat + 30] = 0x55;
    iso[cat + 31] = 0xAA;
    eltorito_set_validation_checksum(&mut iso[cat..cat + 32]);
    iso[cat + 32] = 0x88;
    iso[cat + 38..cat + 40].copy_from_slice(&ELTORITO_SECTOR_COUNT.to_le_bytes());
    iso[cat + 40..cat + 44].copy_from_slice(&ELTORITO_LOAD_LBA.to_le_bytes());
    let load = (ELTORITO_LOAD_LBA as usize) * ISO_SECTOR;
    let _ = write_eltorito_fat12(&mut iso[load..]);
    let bootx = (ISO9660_BOOTX64_LBA as usize) * ISO_SECTOR;
    let pe_len = write_eltorito_efi_pe(&mut iso[bootx..]) as u32;
    if pe_len == 0 {
        return;
    }
    let dir_sz = ISO_SECTOR as u32;
    let root = (ISO9660_ROOT_LBA as usize) * ISO_SECTOR;
    let mut o = 0usize;
    o = iso9660_put_dirent(
        &mut iso[root..root + ISO_SECTOR],
        o,
        ISO9660_ROOT_LBA,
        dir_sz,
        2,
        &[0x00],
    );
    o = iso9660_put_dirent(
        &mut iso[root..root + ISO_SECTOR],
        o,
        ISO9660_ROOT_LBA,
        dir_sz,
        2,
        &[0x01],
    );
    let _ = iso9660_put_dirent(
        &mut iso[root..root + ISO_SECTOR],
        o,
        ISO9660_EFI_LBA,
        dir_sz,
        2,
        b"EFI",
    );
    let efi = (ISO9660_EFI_LBA as usize) * ISO_SECTOR;
    o = 0;
    o = iso9660_put_dirent(
        &mut iso[efi..efi + ISO_SECTOR],
        o,
        ISO9660_EFI_LBA,
        dir_sz,
        2,
        &[0x00],
    );
    o = iso9660_put_dirent(
        &mut iso[efi..efi + ISO_SECTOR],
        o,
        ISO9660_ROOT_LBA,
        dir_sz,
        2,
        &[0x01],
    );
    let _ = iso9660_put_dirent(
        &mut iso[efi..efi + ISO_SECTOR],
        o,
        ISO9660_BOOTDIR_LBA,
        dir_sz,
        2,
        b"BOOT",
    );
    let boot = (ISO9660_BOOTDIR_LBA as usize) * ISO_SECTOR;
    o = 0;
    o = iso9660_put_dirent(
        &mut iso[boot..boot + ISO_SECTOR],
        o,
        ISO9660_BOOTDIR_LBA,
        dir_sz,
        2,
        &[0x00],
    );
    o = iso9660_put_dirent(
        &mut iso[boot..boot + ISO_SECTOR],
        o,
        ISO9660_EFI_LBA,
        dir_sz,
        2,
        &[0x01],
    );
    let _ = iso9660_put_dirent(
        &mut iso[boot..boot + ISO_SECTOR],
        o,
        ISO9660_BOOTX64_LBA,
        pe_len,
        0,
        b"BOOTX64.EFI;1",
    );
    let pt = (ISO9660_PATHTABLE_LBA as usize) * ISO_SECTOR;
    let mut p = 0usize;
    p = iso9660_put_path(
        &mut iso[pt..pt + ISO_SECTOR],
        p,
        ISO9660_ROOT_LBA,
        1,
        &[],
    );
    p = iso9660_put_path(
        &mut iso[pt..pt + ISO_SECTOR],
        p,
        ISO9660_EFI_LBA,
        1,
        b"EFI",
    );
    p = iso9660_put_path(
        &mut iso[pt..pt + ISO_SECTOR],
        p,
        ISO9660_BOOTDIR_LBA,
        2,
        b"BOOT",
    );
    iso_both_u32(&mut iso[pvd + 132..pvd + 140], p as u32);
    iso[pvd + 140..pvd + 144].copy_from_slice(&ISO9660_PATHTABLE_LBA.to_le_bytes());
    let rec = iso[root] as usize;
    if rec >= 34 && rec <= 34 {
        let mut root_rec = [0u8; 34];
        root_rec.copy_from_slice(&iso[root..root + 34]);
        iso[pvd + 156..pvd + 190].copy_from_slice(&root_rec);
    }
}

/// El Torito validation entry: 16-bit little-endian words sum to 0.
/// EDK2 `PartitionDxe/ElTorito.c` skips the catalog when this fails.
pub fn eltorito_set_validation_checksum(cat: &mut [u8]) {
    if cat.len() < 32 {
        return;
    }
    cat[28] = 0;
    cat[29] = 0;
    let mut sum: u16 = 0;
    for i in 0..16 {
        sum = sum.wrapping_add(u16::from_le_bytes([cat[i * 2], cat[i * 2 + 1]]));
    }
    let c = 0u16.wrapping_sub(sum);
    cat[28..30].copy_from_slice(&c.to_le_bytes());
}

pub fn eltorito_validation_checksum_ok(cat: &[u8]) -> bool {
    if cat.len() < 32 || cat[0] != 0x01 || cat[30] != 0x55 || cat[31] != 0xAA {
        return false;
    }
    let mut sum: u16 = 0;
    for i in 0..16 {
        sum = sum.wrapping_add(u16::from_le_bytes([cat[i * 2], cat[i * 2 + 1]]));
    }
    sum == 0
}

/// EDK2 PartitionDxe no-emulation child size in 2048-byte blocks.
/// `(SectorCount * SubBlockSize + BlockSize - 1) / BlockSize` with
/// `SubBlockSize = Media->BlockSize` (2048).
pub fn edk2_eltorito_partition_blocks(sector_count: u16) -> u32 {
    if sector_count < 2 {
        return 0;
    }
    let media = ISO_SECTOR as u32;
    let bytes = u32::from(sector_count).saturating_mul(media);
    bytes.saturating_add(media - 1) / media
}

fn fat12_get(fat: &[u8], cluster: u16) -> u16 {
    let i = (cluster as usize * 3) / 2;
    if i + 1 >= fat.len() {
        return 0;
    }
    if cluster & 1 == 0 {
        u16::from(fat[i]) | ((u16::from(fat[i + 1]) & 0x0F) << 8)
    } else {
        (u16::from(fat[i]) >> 4) | (u16::from(fat[i + 1]) << 4)
    }
}

fn fat_dir_find(dir: &[u8], name11: &[u8; 11]) -> Option<(u16, u32, u8)> {
    let mut off = 0;
    while off + 32 <= dir.len() {
        if dir[off] == 0 {
            break;
        }
        if dir[off] != 0xE5 && &dir[off..off + 11] == name11 {
            let cl = u16::from_le_bytes([dir[off + 26], dir[off + 27]]);
            let sz = u32::from_le_bytes([
                dir[off + 28],
                dir[off + 29],
                dir[off + 30],
                dir[off + 31],
            ]);
            return Some((cl, sz, dir[off + 11]));
        }
        off += 32;
    }
    None
}

/// FatDxe `FatOpenDevice` checks + `\EFI\BOOT\BOOTX64.EFI` 8.3 walk.
///
/// INVARIANTS:
/// - BPB BytesPerSector is 2048 (matches PartitionDxe no-emulation BlockIo)
/// - Media `0xF8` is allowed (`<= 0xF7` reject does not fire)
/// - Cluster count is FAT12 (`< 0xFF5`)
pub fn edk2_fat12_bootx64_ok(fat: &[u8]) -> bool {
    if fat.len() < ELTORITO_SECTOR_COUNT as usize * ELTORITO_FAT_BPS {
        return false;
    }
    let bps = u16::from_le_bytes([fat[11], fat[12]]) as usize;
    if bps != ELTORITO_FAT_BPS || bps.count_ones() != 1 {
        return false;
    }
    let spc = fat[13];
    if spc == 0 || (spc & (spc - 1)) != 0 {
        return false;
    }
    let reserved = u16::from_le_bytes([fat[14], fat[15]]);
    let num_fats = fat[16];
    let roots = u16::from_le_bytes([fat[17], fat[18]]);
    let sectors = u16::from_le_bytes([fat[19], fat[20]]) as usize;
    let media = fat[21];
    let spf = u16::from_le_bytes([fat[22], fat[23]]) as usize;
    if reserved == 0 || num_fats == 0 || sectors == 0 || roots == 0 || spf == 0 {
        return false;
    }
    if media <= 0xF7 && media != 0xF0 && media != 0x00 && media != 0x01 {
        return false;
    }
    let root_secs = ((roots as usize * 32) + (bps - 1)) / bps;
    let first_cluster = reserved as usize + num_fats as usize * spf + root_secs;
    if sectors <= first_cluster {
        return false;
    }
    let max_cluster = (sectors - first_cluster) / (spc as usize);
    if max_cluster >= 0xFF5 {
        return false;
    }
    let fat1 = reserved as usize * bps;
    let root = first_cluster * bps - root_secs * bps;
    let Some((efi_cl, _, efi_attr)) = fat_dir_find(&fat[root..root + bps], b"EFI        ") else {
        return false;
    };
    if efi_attr & 0x10 == 0 || efi_cl < 2 {
        return false;
    }
    let efi_off = first_cluster * bps + (efi_cl as usize - 2) * bps * spc as usize;
    if efi_off + bps > fat.len() {
        return false;
    }
    let Some((boot_cl, _, boot_attr)) = fat_dir_find(&fat[efi_off..efi_off + bps], b"BOOT       ")
    else {
        return false;
    };
    if boot_attr & 0x10 == 0 || boot_cl < 2 {
        return false;
    }
    let boot_off = first_cluster * bps + (boot_cl as usize - 2) * bps * spc as usize;
    if boot_off + bps > fat.len() {
        return false;
    }
    let Some((file_cl, file_sz, file_attr)) =
        fat_dir_find(&fat[boot_off..boot_off + bps], b"BOOTX64 EFI")
    else {
        return false;
    };
    if file_attr & 0x10 != 0 || file_cl < 2 || file_sz < 0x200 {
        return false;
    }
    let pe_off = first_cluster * bps + (file_cl as usize - 2) * bps * spc as usize;
    if pe_off + 2 > fat.len() || &fat[pe_off..pe_off + 2] != b"MZ" {
        return false;
    }
    let clusters = (file_sz as usize + bps - 1) / bps;
    let mut c = file_cl;
    for i in 0..clusters {
        if i + 1 < clusters {
            let next = fat12_get(&fat[fat1..fat1 + spf * bps], c);
            if next != c + 1 {
                return false;
            }
            c = next;
        } else if fat12_get(&fat[fat1..fat1 + spf * bps], c) < 0xFF8 {
            return false;
        }
    }
    true
}

/// DxeCore / BasePeCoff LoadImage header checks for the CD EFI.
pub fn edk2_pe_loadimage_ok(pe: &[u8]) -> bool {
    if pe.len() < 0x600 || &pe[0..2] != b"MZ" || &pe[0x80..0x84] != b"PE\0\0" {
        return false;
    }
    let machine = u16::from_le_bytes([pe[0x84], pe[0x85]]);
    let nsec = u16::from_le_bytes([pe[0x86], pe[0x87]]);
    let opt_sz = u16::from_le_bytes([pe[0x94], pe[0x95]]);
    let chars = u16::from_le_bytes([pe[0x96], pe[0x97]]);
    if machine != 0x8664 || nsec == 0 || opt_sz != 0xF0 {
        return false;
    }
    if (chars & 0x0001) != 0 {
        return false;
    }
    let opt = 0x98;
    if u16::from_le_bytes([pe[opt], pe[opt + 1]]) != 0x020B {
        return false;
    }
    let entry = u32::from_le_bytes([pe[opt + 0x10], pe[opt + 0x11], pe[opt + 0x12], pe[opt + 0x13]]);
    let sect_align =
        u32::from_le_bytes([pe[opt + 0x20], pe[opt + 0x21], pe[opt + 0x22], pe[opt + 0x23]]);
    let file_align =
        u32::from_le_bytes([pe[opt + 0x24], pe[opt + 0x25], pe[opt + 0x26], pe[opt + 0x27]]);
    let size_of_image =
        u32::from_le_bytes([pe[opt + 0x38], pe[opt + 0x39], pe[opt + 0x3A], pe[opt + 0x3B]]);
    let size_of_headers =
        u32::from_le_bytes([pe[opt + 0x3C], pe[opt + 0x3D], pe[opt + 0x3E], pe[opt + 0x3F]]);
    let subsystem = u16::from_le_bytes([pe[opt + 0x44], pe[opt + 0x45]]);
    if subsystem != 10 || sect_align < 0x1000 || file_align == 0 {
        return false;
    }
    if (sect_align & (sect_align - 1)) != 0 || (file_align & (file_align - 1)) != 0 {
        return false;
    }
    if size_of_image < size_of_headers || (size_of_image % sect_align) != 0 {
        return false;
    }
    if entry < size_of_headers || entry >= size_of_image {
        return false;
    }
    let sec = opt + 0xF0;
    for i in 0..nsec as usize {
        let s = sec + i * 40;
        if s + 40 > pe.len() {
            return false;
        }
        let vsz = u32::from_le_bytes([pe[s + 8], pe[s + 9], pe[s + 10], pe[s + 11]]);
        let va = u32::from_le_bytes([pe[s + 12], pe[s + 13], pe[s + 14], pe[s + 15]]);
        let raw = u32::from_le_bytes([pe[s + 16], pe[s + 17], pe[s + 18], pe[s + 19]]);
        let ptr = u32::from_le_bytes([pe[s + 20], pe[s + 21], pe[s + 22], pe[s + 23]]);
        if raw == 0 {
            continue;
        }
        if va < size_of_headers || ptr < size_of_headers {
            return false;
        }
        let last = ptr.saturating_add(raw.saturating_sub(1)) as usize;
        if last >= pe.len() {
            return false;
        }
        let _ = vsz;
    }
    true
}

fn fat12_set(fat: &mut [u8], cluster: u16, val: u16) {
    let i = (cluster as usize * 3) / 2;
    if i + 1 >= fat.len() {
        return;
    }
    let v = val & 0x0FFF;
    if cluster & 1 == 0 {
        fat[i] = v as u8;
        fat[i + 1] = (fat[i + 1] & 0xF0) | ((v >> 8) as u8);
    } else {
        fat[i] = (fat[i] & 0x0F) | ((v << 4) as u8);
        fat[i + 1] = (v >> 4) as u8;
    }
}

fn fat_dirent(dst: &mut [u8], name11: &[u8; 11], attr: u8, cluster: u16, size: u32) {
    if dst.len() < 32 {
        return;
    }
    dst[..32].fill(0);
    dst[..11].copy_from_slice(name11);
    dst[11] = attr;
    dst[26..28].copy_from_slice(&cluster.to_le_bytes());
    dst[28..32].copy_from_slice(&size.to_le_bytes());
}

/// Write a FAT12 EFI System Partition with `\EFI\BOOT\BOOTX64.EFI`.
///
/// INVARIANTS:
/// - Fits in [`ELTORITO_SECTOR_COUNT`] ISO 2048-byte sectors (16384 bytes)
/// - BPB BytesPerSector is 2048 so FatDxe matches the El Torito child BlockIo
/// - `BOOTX64.EFI` is the PE from [`write_eltorito_efi_pe`]
/// - Does not allocate
pub fn write_eltorito_fat12(dst: &mut [u8]) -> usize {
    const FAT_SECS: usize = ELTORITO_SECTOR_COUNT as usize;
    const NEED: usize = FAT_SECS * ELTORITO_FAT_BPS;
    if dst.len() < NEED {
        return 0;
    }
    dst[..NEED].fill(0);
    dst[0] = 0xEB;
    dst[1] = 0x3C;
    dst[2] = 0x90;
    dst[3..11].copy_from_slice(b"MSWIN4.1");
    dst[11..13].copy_from_slice(&(ELTORITO_FAT_BPS as u16).to_le_bytes());
    dst[13] = 1;
    dst[14..16].copy_from_slice(&1u16.to_le_bytes());
    dst[16] = 2;
    dst[17..19].copy_from_slice(&16u16.to_le_bytes());
    dst[19..21].copy_from_slice(&(FAT_SECS as u16).to_le_bytes());
    dst[21] = 0xF8;
    dst[22..24].copy_from_slice(&1u16.to_le_bytes());
    dst[24..26].copy_from_slice(&32u16.to_le_bytes());
    dst[26..28].copy_from_slice(&2u16.to_le_bytes());
    dst[36] = 0x80;
    dst[38] = 0x29;
    dst[39..43].copy_from_slice(&0x524E_5631u32.to_le_bytes());
    dst[43..54].copy_from_slice(b"RAYNU-V-EFI");
    dst[54..62].copy_from_slice(b"FAT12   ");
    dst[510] = 0x55;
    dst[511] = 0xAA;
    let fat1 = ELTORITO_FAT_BPS;
    let fat2 = fat1 + ELTORITO_FAT_BPS;
    let pe_off = ELTORITO_BOOTX64_OFF;
    let pe_len = write_eltorito_efi_pe(&mut dst[pe_off..]);
    if pe_len == 0 {
        return 0;
    }
    let file_clusters = (pe_len + ELTORITO_FAT_BPS - 1) / ELTORITO_FAT_BPS;
    // Clusters 2=EFI, 3=BOOT, 4+=file. Volume has clusters 2..5.
    if file_clusters == 0 || 3 + file_clusters > 5 {
        return 0;
    }
    fat12_set(&mut dst[fat1..fat2], 0, 0xFF8);
    fat12_set(&mut dst[fat1..fat2], 1, 0xFFF);
    fat12_set(&mut dst[fat1..fat2], 2, 0xFFF);
    fat12_set(&mut dst[fat1..fat2], 3, 0xFFF);
    let first = 4u16;
    for i in 0..file_clusters.saturating_sub(1) {
        let c = first + i as u16;
        fat12_set(&mut dst[fat1..fat2], c, c + 1);
    }
    fat12_set(
        &mut dst[fat1..fat2],
        first + (file_clusters as u16) - 1,
        0xFFF,
    );
    let mut fat_copy = [0u8; ELTORITO_FAT_BPS];
    fat_copy.copy_from_slice(&dst[fat1..fat2]);
    dst[fat2..fat2 + ELTORITO_FAT_BPS].copy_from_slice(&fat_copy);
    let root = fat2 + ELTORITO_FAT_BPS;
    fat_dirent(
        &mut dst[root..],
        b"EFI        ",
        0x10,
        2,
        0,
    );
    let efi_dir = root + ELTORITO_FAT_BPS;
    fat_dirent(&mut dst[efi_dir..], b".          ", 0x10, 2, 0);
    fat_dirent(&mut dst[efi_dir + 32..], b"..         ", 0x10, 0, 0);
    fat_dirent(&mut dst[efi_dir + 64..], b"BOOT       ", 0x10, 3, 0);
    let boot_dir = efi_dir + ELTORITO_FAT_BPS;
    fat_dirent(&mut dst[boot_dir..], b".          ", 0x10, 3, 0);
    fat_dirent(&mut dst[boot_dir + 32..], b"..         ", 0x10, 2, 0);
    fat_dirent(
        &mut dst[boot_dir + 64..],
        b"BOOTX64 EFI",
        0x20,
        4,
        pe_len as u32,
    );
    debug_assert_eq!(pe_off, ELTORITO_BOOTX64_OFF);
    NEED
}

/// Write a PE32+ EFI application that OUTs [`ELTORITO_PAYLOAD_MAGIC`] to COM1.
///
/// INVARIANTS:
/// - Relocatable: no `RELOCS_STRIPPED`; empty `.reloc` so DxeCore LoadImage
///   can `AllocateAnyPages` in the 32MiB guest (ImageBase 0)
/// - Characteristics `0x2022` match EDK2 GenFw EFI applications
///   (`EXECUTABLE | LARGE_ADDRESS_AWARE | DLL`)
/// - SectionAlignment 0x1000 so DxeCore ProtectUefiImage can set X on
///   `.text` (nested `8881cdd`: catalog+bootimg, `elt=0`; 0x200 `.text`+`.reloc`
///   share one NX LoaderCode page when NX policy skips sub-page protect)
/// - `.text` VirtualSize is the file-aligned size so LoadImage copies the
///   UART setup, not only the first 0x20 bytes
/// - Entry clears COM1 LCR.DLAB then OUTs magic (THR is 0x3F8 only when DLAB=0)
/// - Entry point ignores ImageHandle/SystemTable and returns EFI_SUCCESS
/// - Does not allocate
pub fn write_eltorito_efi_pe(dst: &mut [u8]) -> usize {
    const HDR: usize = 0x200;
    const FILE_ALIGN: usize = 0x200;
    const SECT_ALIGN: usize = 0x1000;
    const CODE: usize = FILE_ALIGN;
    const TEXT_RVA: usize = SECT_ALIGN;
    const RELOC_RVA: usize = SECT_ALIGN * 2;
    const RELOC_FILE: usize = HDR + FILE_ALIGN;
    const RELOC_DIR: u32 = 8;
    const SIZE_OF_IMAGE: usize = SECT_ALIGN * 3;
    const NEED: usize = RELOC_FILE + FILE_ALIGN;
    if dst.len() < NEED {
        return 0;
    }
    dst[..NEED].fill(0);
    dst[0] = b'M';
    dst[1] = b'Z';
    dst[0x3C..0x40].copy_from_slice(&0x80u32.to_le_bytes());
    dst[0x40..0x40 + ELTORITO_PAYLOAD_MAGIC.len()].copy_from_slice(ELTORITO_PAYLOAD_MAGIC);
    dst[0x80..0x84].copy_from_slice(b"PE\0\0");
    dst[0x84..0x86].copy_from_slice(&0x8664u16.to_le_bytes());
    dst[0x86..0x88].copy_from_slice(&2u16.to_le_bytes());
    dst[0x94..0x96].copy_from_slice(&0x00F0u16.to_le_bytes());
    // IMAGE_FILE_EXECUTABLE_IMAGE | LARGE_ADDRESS_AWARE | DLL (GenFw EFI app)
    dst[0x96..0x98].copy_from_slice(&0x2022u16.to_le_bytes());
    let opt = 0x98;
    dst[opt..opt + 2].copy_from_slice(&0x020Bu16.to_le_bytes());
    dst[opt + 2] = 14;
    dst[opt + 4..opt + 8].copy_from_slice(&(CODE as u32).to_le_bytes());
    dst[opt + 8..opt + 12].copy_from_slice(&(FILE_ALIGN as u32).to_le_bytes());
    dst[opt + 0x10..opt + 0x14].copy_from_slice(&(TEXT_RVA as u32).to_le_bytes());
    dst[opt + 0x14..opt + 0x18].copy_from_slice(&(TEXT_RVA as u32).to_le_bytes());
    dst[opt + 0x20..opt + 0x24].copy_from_slice(&(SECT_ALIGN as u32).to_le_bytes());
    dst[opt + 0x24..opt + 0x28].copy_from_slice(&(FILE_ALIGN as u32).to_le_bytes());
    dst[opt + 0x38..opt + 0x3C].copy_from_slice(&(SIZE_OF_IMAGE as u32).to_le_bytes());
    dst[opt + 0x3C..opt + 0x40].copy_from_slice(&(HDR as u32).to_le_bytes());
    dst[opt + 0x44..opt + 0x46].copy_from_slice(&10u16.to_le_bytes());
    // HIGH_ENTROPY_VA | DYNAMIC_BASE | NX_COMPAT (ProtectUefiImage)
    dst[opt + 0x46..opt + 0x48].copy_from_slice(&0x0160u16.to_le_bytes());
    dst[opt + 0x6C..opt + 0x70].copy_from_slice(&16u32.to_le_bytes());
    let dd5 = opt + 0x70 + 5 * 8;
    dst[dd5..dd5 + 4].copy_from_slice(&(RELOC_RVA as u32).to_le_bytes());
    dst[dd5 + 4..dd5 + 8].copy_from_slice(&RELOC_DIR.to_le_bytes());
    let sec = opt + 0xF0;
    dst[sec..sec + 5].copy_from_slice(b".text");
    dst[sec + 8..sec + 12].copy_from_slice(&(CODE as u32).to_le_bytes());
    dst[sec + 12..sec + 16].copy_from_slice(&(TEXT_RVA as u32).to_le_bytes());
    dst[sec + 16..sec + 20].copy_from_slice(&(FILE_ALIGN as u32).to_le_bytes());
    dst[sec + 20..sec + 24].copy_from_slice(&(HDR as u32).to_le_bytes());
    dst[sec + 36..sec + 40].copy_from_slice(&0x6000_0020u32.to_le_bytes());
    let rel = sec + 40;
    dst[rel..rel + 6].copy_from_slice(b".reloc");
    dst[rel + 8..rel + 12].copy_from_slice(&RELOC_DIR.to_le_bytes());
    dst[rel + 12..rel + 16].copy_from_slice(&(RELOC_RVA as u32).to_le_bytes());
    dst[rel + 16..rel + 20].copy_from_slice(&(FILE_ALIGN as u32).to_le_bytes());
    dst[rel + 20..rel + 24].copy_from_slice(&(RELOC_FILE as u32).to_le_bytes());
    dst[rel + 36..rel + 40].copy_from_slice(&0x4200_0040u32.to_le_bytes());
    dst[RELOC_FILE + 4..RELOC_FILE + 8].copy_from_slice(&RELOC_DIR.to_le_bytes());
    // mov edx, 0x3FB ; mov al, 3 ; out dx, al  (LCR: 8N1, DLAB=0)
    // mov edx, 0x3F8 ; OUT each magic byte ; xor eax,eax ; ret
    let mut i = HDR;
    dst[i] = 0xBA;
    dst[i + 1] = 0xFB;
    dst[i + 2] = 0x03;
    dst[i + 3] = 0x00;
    dst[i + 4] = 0x00;
    dst[i + 5] = 0xB0;
    dst[i + 6] = 0x03;
    dst[i + 7] = 0xEE;
    i += 8;
    dst[i] = 0xBA;
    dst[i + 1] = 0xF8;
    dst[i + 2] = 0x03;
    dst[i + 3] = 0x00;
    dst[i + 4] = 0x00;
    i += 5;
    for &b in ELTORITO_PAYLOAD_MAGIC {
        dst[i] = 0xB0;
        dst[i + 1] = b;
        dst[i + 2] = 0xEE;
        i += 3;
    }
    dst[i] = 0x31;
    dst[i + 1] = 0xC0;
    dst[i + 2] = 0xC3;
    debug_assert!(i + 3 - HDR <= CODE);
    NEED
}

/// QEMU launch path: present a CD if the operator has not attached one.
pub fn present_placeholder_if_idle() -> bool {
    if is_visible() && retained_len() >= ISO_SECTOR {
        return true;
    }
    present_placeholder()
}

fn write_hex16_nowait(v: u16) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    crate::boot::serial::write_str_nowait("0x");
    for shift in [12, 8, 4, 0] {
        let nibble = ((v >> shift) & 0xf) as usize;
        crate::boot::serial::write_byte_nowait(HEX[nibble]);
    }
}

pub fn pci_write_addr(addr: u32) {
    with_cd(|m| m.pci_addr = addr);
}

pub fn pci_read_addr() -> u32 {
    with_cd(|m| m.pci_addr)
}

fn config_dword(m: &CdMedia, off: u8) -> u32 {
    match off {
        0x00 => u32::from(GUEST_CD_PCI_VENDOR) | (u32::from(GUEST_CD_PCI_DEVICE) << 16),
        0x04 => u32::from(m.pci_cmd) | 0x0200_0000,
        0x08 => 0x01018000, // class IDE, prog-if 0x80
        // Multifunction bit lives on ISA `00:01.0`. This is PIIX IDE fn1.
        0x0C => 0x0000_0000,
        0x10 => m.bar0,
        0x14 => m.bar1,
        0x18 => m.bar2,
        0x1C => m.bar3,
        0x20 => m.bar4,
        0x2C => 0x0000_0000,
        0x3C => 0x0000_010E, // pin 1, IRQ 14
        _ => 0,
    }
}

fn note_hide_slot0() {
    if !HIDE_SLOT0.swap(true, Ordering::AcqRel) {
        crate::boot::serial::write_line_nowait(
            "boot: guest-UEFI linux hides duplicate slot0 IDE (not ISO-INSTALL-OK)",
        );
    }
}

fn note_hide_piix() {
    if !HIDE_PIIX.swap(true, Ordering::AcqRel) {
        crate::boot::serial::write_line_nowait(
            "boot: guest-UEFI linux hides PIIX IDE (not ISO-INSTALL-OK)",
        );
    }
}

fn note_hide_product() {
    if !HIDE_PRODUCT.swap(true, Ordering::AcqRel) {
        crate::boot::serial::write_line_nowait(
            "boot: product ISO hides PIIX IDE (not ISO-INSTALL-OK)",
        );
    }
}

pub fn pci_read_data(port: u16, size: u8) -> u32 {
    with_cd(|m| {
        if !m.visible {
            return 0xFFFF_FFFF;
        }
        let addr = m.pci_addr;
        if product_iso_hides_ide(addr) {
            note_hide_product();
            return 0xFFFF_FFFF;
        }
        let linux = crate::boot::serial::linux_earlycon_share();
        if linux_hides_duplicate_slot0_ide(linux, addr) {
            note_hide_slot0();
            return 0xFFFF_FFFF;
        }
        if linux_hides_piix_ide(crate::boot::serial::linux_high_half(), addr) {
            note_hide_piix();
            return 0xFFFF_FFFF;
        }
        if !pci_addr_selects_cd(addr) {
            return 0xFFFF_FFFF;
        }
        let off = pci_cfg_offset(addr, port);
        let aligned = off & 0xFC;
        if aligned == 0 {
            m.pci_enum = true;
            PCI_ENUM.store(true, Ordering::Release);
        }
        let dword = config_dword(m, aligned);
        let shift = (off & 3) * 8;
        let shifted = dword >> shift;
        match size {
            1 => shifted & 0xff,
            2 => shifted & 0xffff,
            _ => shifted,
        }
    })
}

pub fn pci_write_data(port: u16, _size: u8, val: u32) {
    with_cd(|m| {
        if product_iso_hides_ide(m.pci_addr)
            || linux_hides_duplicate_slot0_ide(
                crate::boot::serial::linux_earlycon_share(),
                m.pci_addr,
            )
            || linux_hides_piix_ide(crate::boot::serial::linux_high_half(), m.pci_addr)
        {
            return;
        }
        if !m.visible || !pci_addr_selects_cd(m.pci_addr) {
            return;
        }
        let off = pci_cfg_offset(m.pci_addr, port);
        let aligned = off & 0xFC;
        if off == 0x04 {
            // Iron COM2 abba969: honor stuck. Last wr=0x0 stored 0x0.
            // Print each write so COM2 shows whether EnableAttributes
            // ever set IO (ADR-015). Do not OR 0x0001.
            let wr = val as u16;
            LAST_PCI_CMD_WR.store(wr, Ordering::Release);
            m.pci_cmd = wr;
            let n = PCI_CMD_WRITES.fetch_add(1, Ordering::AcqRel);
            if (n as usize) < PCI_CMD_WR_SEQ_CAP {
                PCI_CMD_WR_SEQ[n as usize].store(wr, Ordering::Release);
            }
            crate::boot::serial::write_str_nowait(
                "boot: Stage 46 IDE pci cmdwr honor wr=",
            );
            write_hex16_nowait(wr);
            crate::boot::serial::write_str_nowait(" n=");
            crate::boot::serial::write_byte_nowait(b'0' + (n.min(9) as u8));
            crate::boot::serial::write_line_nowait(" (not ISO-INSTALL-OK)");
        } else if aligned == 0x10 {
            // 8-byte I/O BAR (legacy 0x1F0). Probe 0xFFFFFFFF → 0xFFFFFFF9.
            m.bar0 = (val & 0xFFFF_FFF8) | 1;
        } else if aligned == 0x14 {
            m.bar1 = (val & 0xFFFF_FFFC) | 1;
        } else if aligned == 0x18 {
            m.bar2 = (val & 0xFFFF_FFF8) | 1;
        } else if aligned == 0x1C {
            m.bar3 = (val & 0xFFFF_FFFC) | 1;
        } else if aligned == 0x20 {
            // 16-byte I/O BMIDE. Probe 0xFFFFFFFF → 0xFFFFFFF1.
            m.bar4 = (val & 0xFFFF_FFF0) | 1;
        }
    });
}

fn apply_atapi_signature(m: &mut CdMedia) {
    m.ata_err = 0x01;
    m.ata_count = 0x01;
    m.ata_lba = ATAPI_SIG_LBA;
    m.ata_dev = m.ata_dev & 0x10;
    m.ata_status = ATA_STATUS_DRDY | ATA_STATUS_SEEK;
    m.xfer = AtaXfer::Idle;
    m.xfer_off = 0;
    m.xfer_end = 0;
    m.cdb_got = 0;
    m.pkt_lba = 0;
    m.pkt_left = 0;
}

fn raise_ata_irq(m: &CdMedia) {
    if product_iso_window_armed() && (m.ata_devctl & ATA_DEVCTL_NIEN) == 0 {
        crate::devices::guest_irq::raise_ata();
    }
}

fn lower_ata_irq() {
    if product_iso_window_armed() {
        crate::devices::guest_irq::lower_ata();
    }
}

fn packet_ok(m: &mut CdMedia) {
    m.xfer = AtaXfer::Idle;
    m.pkt_lba = 0;
    m.pkt_left = 0;
    m.ata_err = 0;
    m.ata_count = ATAPI_INT_IO | ATAPI_INT_CD;
    m.ata_status = ATA_STATUS_DRDY | ATA_STATUS_SEEK;
    raise_ata_irq(m);
}

fn packet_error(m: &mut CdMedia, sense: u8, asc: u8) {
    m.sense_key = sense;
    m.sense_asc = asc;
    m.xfer = AtaXfer::Idle;
    m.pkt_lba = 0;
    m.pkt_left = 0;
    m.ata_err = sense << 4;
    m.ata_count = ATAPI_INT_IO | ATAPI_INT_CD;
    m.ata_status = ATA_STATUS_DRDY | ATA_STATUS_ERR;
    raise_ata_irq(m);
}

fn begin_packet_data(m: &mut CdMedia, n: usize) {
    let n = n.min(XFER_CAP);
    let mut limit = m.byte_limit as usize;
    if limit == 0 || limit == 0xffff {
        limit = n;
    }
    let size = n.min(limit).max(2).min(XFER_CAP).min(0xFFFE);
    m.xfer = AtaXfer::PacketData;
    m.xfer_off = 0;
    // Advertise and complete the same byte count (ATAPI cylinder). A larger
    // `xfer_end` than `size` left DRQ after the guest finished the PIO.
    m.xfer_end = size.min(XFER_CAP);
    m.ata_count = ATAPI_INT_IO;
    m.ata_lba[1] = size as u8;
    m.ata_lba[2] = (size >> 8) as u8;
    m.ata_err = 0;
    m.ata_status = ATA_STATUS_DRDY | ATA_STATUS_SEEK | ATA_STATUS_DRQ;
    raise_ata_irq(m);
}

fn start_identify(m: &mut CdMedia) {
    m.data.fill(0);
    // Word 0: ATAPI CD-ROM, removable, 12-byte packet (QEMU 0x85C0).
    // Nested Intel f93caee: 0x8500 (no RMB) then 0xA1×4 packet=0.
    m.data[0] = 0xC0;
    m.data[1] = 0x85;
    let model = b"RAYNU-V CD                          ";
    for (i, &b) in model.iter().enumerate() {
        let word = 27 + i / 2;
        if word >= 47 {
            break;
        }
        if i % 2 == 0 {
            m.data[word * 2 + 1] = b;
        } else {
            m.data[word * 2] = b;
        }
    }
    // Word 49: LBA (bit 9). No DMA (bit 8) — BMIDE stays RAZ/WI.
    m.data[49 * 2] = 0x00;
    m.data[49 * 2 + 1] = 0x02;
    // Word 53: words 64–70 valid. Word 64: PIO3 + PIO4.
    m.data[53 * 2] = 0x02;
    m.data[64 * 2] = 0x03;
    m.xfer = AtaXfer::Identify;
    m.xfer_off = 0;
    m.xfer_end = 512;
    m.ata_err = 0;
    m.ata_status = ATA_STATUS_DRDY | ATA_STATUS_SEEK | ATA_STATUS_DRQ;
    raise_ata_irq(m);
}

fn load_sectors(m: &mut CdMedia, lba: u32, count: u32) -> bool {
    if count == 0 {
        packet_ok(m);
        return true;
    }
    let start = (lba as usize).saturating_mul(ISO_SECTOR);
    let total = (count as usize).saturating_mul(ISO_SECTOR);
    if start.saturating_add(total) > m.len {
        packet_error(m, SCSI_SENSE_ILLEGAL, 0x21);
        return false;
    }
    m.pkt_lba = lba;
    m.pkt_left = count;
    fill_read_drq(m);
    true
}

/// Fill the next ATAPI data-in DRQ from `pkt_lba` / `pkt_left`.
///
/// INVARIANTS:
/// - One DRQ is at most [`XFER_SEC`] CD sectors (16-bit cylinder)
/// - `pkt_left` after return is the CDB remainder, not silently truncated
/// - Guest cylinder `byte_limit` (0 / 0xFFFF = full DRQ) caps this DRQ only
fn fill_read_drq(m: &mut CdMedia) {
    if m.pkt_left == 0 {
        packet_ok(m);
        return;
    }
    let mut max_bytes = m.byte_limit as usize;
    if max_bytes == 0 || max_bytes == 0xffff {
        max_bytes = XFER_CAP;
    }
    let max_sec = (max_bytes / ISO_SECTOR).clamp(1, XFER_SEC);
    let nsec = (m.pkt_left as usize).min(max_sec);
    let lba = m.pkt_lba;
    let start = (lba as usize).saturating_mul(ISO_SECTOR);
    let bytes = nsec.saturating_mul(ISO_SECTOR);
    let end = start.saturating_add(bytes);
    if end > m.len {
        packet_error(m, SCSI_SENSE_ILLEGAL, 0x21);
        return;
    }
    let ext_ptr = m.ext_ptr;
    let ext_len = m.ext_len;
    if ext_len > GUEST_CD_ISO_CAP && !ext_ptr.is_null() {
        copy_product_iso_range(ext_ptr, start, bytes, &mut m.data[..bytes]);
    } else {
        m.data[..bytes].copy_from_slice(&m.iso[start..start + bytes]);
    }
    m.sectors_read = m.sectors_read.saturating_add(nsec as u32);
    SECTORS.store(m.sectors_read, Ordering::Release);
    m.last_read_lba = lba;
    LAST_READ_LBA.store(lba, Ordering::Release);
    let end_lba = lba.saturating_add(nsec as u32);
    if lba <= ELTORITO_CATALOG_LBA && ELTORITO_CATALOG_LBA < end_lba {
        m.catalog_read = true;
        CATALOG_READ.store(true, Ordering::Release);
    }
    if lba <= ELTORITO_LOAD_LBA && ELTORITO_LOAD_LBA < end_lba {
        m.boot_image_read = true;
        BOOT_IMAGE_READ.store(true, Ordering::Release);
    }
    m.pkt_lba = end_lba;
    m.pkt_left = m.pkt_left.saturating_sub(nsec as u32);
    // Advertise this DRQ's sector count. Temporarily clear cylinder so
    // `begin_packet_data` does not trim below a full-sector multiple.
    let prev_limit = m.byte_limit;
    m.byte_limit = 0;
    begin_packet_data(m, bytes);
    m.byte_limit = prev_limit;
}

/// Copy `bytes` from the product ISO window into `dst`.
fn copy_product_iso_range(ext_ptr: *const u8, start: usize, bytes: usize, dst: &mut [u8]) {
    if bytes == 0 {
        return;
    }
    // SAFETY: product ISO window is the retained operator/host slice;
    // host tests keep that Vec until [`reset`]. Guest-UEFI CD life is
    // single-threaded after EBS.
    // KANI-TARGET: ATAPI READ from product ISO window (outside Proven Core).
    unsafe {
        let src = core::slice::from_raw_parts(ext_ptr.add(start), bytes);
        dst[..bytes].copy_from_slice(src);
    }
}

fn alloc_len(cdb: &[u8; 12], off: usize, wide: bool) -> usize {
    if wide {
        u32::from_be_bytes([cdb[off], cdb[off + 1], cdb[off + 2], cdb[off + 3]]) as usize
    } else {
        u16::from_be_bytes([cdb[off], cdb[off + 1]]) as usize
    }
}

fn finish_packet(m: &mut CdMedia) {
    let op = m.cdb[0];
    LAST_SCSI.store(op, Ordering::Release);
    match op {
        SCSI_TEST_UNIT | SCSI_START_STOP | SCSI_PREVENT => packet_ok(m),
        SCSI_REQUEST_SENSE => {
            let n = m.cdb[4] as usize;
            m.data.fill(0);
            m.data[0] = 0x70;
            m.data[2] = m.sense_key;
            m.data[7] = 10;
            m.data[12] = m.sense_asc;
            begin_packet_data(m, n.min(18).max(8));
        }
        SCSI_INQUIRY => {
            let n = m.cdb[4] as usize;
            m.data.fill(0);
            m.data[0] = 0x05; // CD/DVD
            m.data[1] = 0x80; // RMB
            m.data[2] = 0x05;
            m.data[3] = 0x02;
            m.data[4] = 31;
            m.data[8..16].copy_from_slice(b"RAYNU-V ");
            m.data[16..32].copy_from_slice(b"GUEST CD        ");
            begin_packet_data(m, n.min(36).max(5));
        }
        SCSI_READ_CAPACITY => {
            let last = if m.len >= ISO_SECTOR {
                ((m.len / ISO_SECTOR) - 1) as u32
            } else {
                0
            };
            m.data.fill(0);
            m.data[0..4].copy_from_slice(&last.to_be_bytes());
            m.data[4..8].copy_from_slice(&(ISO_SECTOR as u32).to_be_bytes());
            begin_packet_data(m, 8);
        }
        SCSI_READ10 => {
            let lba = u32::from_be_bytes([m.cdb[2], m.cdb[3], m.cdb[4], m.cdb[5]]);
            let count = u16::from_be_bytes([m.cdb[7], m.cdb[8]]) as u32;
            let _ = load_sectors(m, lba, count);
        }
        SCSI_READ12 => {
            let lba = u32::from_be_bytes([m.cdb[2], m.cdb[3], m.cdb[4], m.cdb[5]]);
            let count = u32::from_be_bytes([m.cdb[6], m.cdb[7], m.cdb[8], m.cdb[9]]);
            let _ = load_sectors(m, lba, count);
        }
        SCSI_READ_TOC => {
            let last = if m.len >= ISO_SECTOR {
                (m.len / ISO_SECTOR) as u32
            } else {
                1
            };
            m.data.fill(0);
            // TOC: first/last track 1, track 1 at LBA 0, lead-out at last.
            m.data[0] = 0;
            m.data[1] = 18;
            m.data[2] = 1;
            m.data[3] = 1;
            m.data[4] = 0x01;
            m.data[5] = 0x14;
            m.data[6] = 1;
            m.data[8..12].copy_from_slice(&0u32.to_be_bytes());
            m.data[12] = 0x01;
            m.data[13] = 0x14;
            m.data[14] = 0xAA;
            m.data[16..20].copy_from_slice(&last.to_be_bytes());
            let n = alloc_len(&m.cdb, 7, false);
            begin_packet_data(m, n.min(20).max(4));
        }
        SCSI_GET_CONFIG => {
            m.data.fill(0);
            m.data[0..4].copy_from_slice(&8u32.to_be_bytes());
            m.data[6] = 0x00;
            m.data[7] = 0x08; // CD-ROM profile
            let n = alloc_len(&m.cdb, 7, false);
            begin_packet_data(m, n.min(16).max(8));
        }
        SCSI_GET_EVENT => {
            m.data.fill(0);
            m.data[0] = 0;
            m.data[1] = 6;
            m.data[2] = 0x04; // media class
            m.data[4] = 0x04; // media present
            let n = alloc_len(&m.cdb, 7, false);
            begin_packet_data(m, n.min(8).max(4));
        }
        SCSI_MODE_SENSE6 => {
            let n = m.cdb[4] as usize;
            m.data.fill(0);
            m.data[0] = 11;
            m.data[3] = 8;
            m.data[9] = 0x08;
            m.data[10] = 0x00;
            begin_packet_data(m, n.min(12).max(4));
        }
        SCSI_MODE_SENSE10 => {
            let n = alloc_len(&m.cdb, 7, false);
            m.data.fill(0);
            m.data[1] = 18;
            m.data[7] = 8;
            m.data[13] = 0x08;
            m.data[14] = 0x00;
            begin_packet_data(m, n.min(20).max(8));
        }
        _ => packet_error(m, SCSI_SENSE_ILLEGAL, SCSI_ASC_INVALID_OPCODE),
    }
}

fn note_ata_float() {
    if !ATA_FLOAT.swap(true, Ordering::AcqRel) {
        crate::boot::serial::write_line_nowait(
            "boot: guest-UEFI linux ATA floating bus (not ISO-INSTALL-OK)",
        );
    }
}

/// ATA primary PIO. Returns the value to merge into RAX on IN.
pub fn ata_io(port: u16, is_in: bool, size: u8, rax: u64) -> u64 {
    with_cd(|m| {
        if linux_ata_floating_bus(crate::boot::serial::linux_high_half()) {
            note_ata_float();
            if is_in {
                let mask = io_mask(size);
                return (rax & !mask) | (0xffu64 & mask);
            }
            return rax;
        }
        if !m.visible {
            return if is_in { rax | 0xff } else { rax };
        }
        let Some(reg) = ata_reg(m, port) else {
            return if is_in { rax | 0xff } else { rax };
        };
        ATA_IO_N.fetch_add(1, Ordering::AcqRel);
        if is_in {
            // Nested Intel f93caee: IDENTIFY PACKET 0xA1 x4 (2 channels x 2
            // devices). Slave is absent: status 0x00, no DRQ identify.
            let val = if ata_is_slave(m) {
                match reg {
                    6 => u64::from(m.ata_dev),
                    7 | 8 => u64::from(ata_absent_status()),
                    _ => 0,
                }
            } else {
                match reg {
                    0 => read_data(m, size),
                    1 => u64::from(m.ata_err),
                    2 => u64::from(m.ata_count),
                    3 => u64::from(m.ata_lba[0]),
                    4 => u64::from(m.ata_lba[1]),
                    5 => u64::from(m.ata_lba[2]),
                    6 => u64::from(m.ata_dev),
                    _ => u64::from(m.ata_status),
                }
            };
            if !ata_is_slave(m) && reg == 7 {
                lower_ata_irq();
            }
            let mask = io_mask(size);
            (rax & !mask) | (val & mask)
        } else {
            let v = rax as u8;
            match reg {
                0 => {
                    if !ata_is_slave(m) {
                        write_data(m, size, rax);
                    }
                }
                1 => {
                    if !ata_is_slave(m) {
                        m.ata_feat = v;
                    }
                }
                2 => {
                    if !ata_is_slave(m) {
                        m.ata_count = v;
                    }
                }
                3 => {
                    if !ata_is_slave(m) {
                        m.ata_lba[0] = v;
                    }
                }
                4 => {
                    if !ata_is_slave(m) {
                        m.ata_lba[1] = v;
                    }
                }
                5 => {
                    if !ata_is_slave(m) {
                        m.ata_lba[2] = v;
                    }
                }
                6 => m.ata_dev = v,
                7 => {
                    LAST_ATA_CMD.store(v, Ordering::Release);
                    ATA_CMD_N.fetch_add(1, Ordering::AcqRel);
                    if ata_is_slave(m) {
                        apply_no_device(m);
                        return rax;
                    }
                    match v {
                        ATA_CMD_IDENTIFY_PACKET => start_identify(m),
                        ATA_CMD_PACKET => {
                            PACKET_N.fetch_add(1, Ordering::AcqRel);
                            m.xfer = AtaXfer::PacketCdb;
                            m.cdb_got = 0;
                            m.cdb.fill(0);
                            m.pkt_lba = 0;
                            m.pkt_left = 0;
                            m.byte_limit = u16::from(m.ata_lba[1]) | (u16::from(m.ata_lba[2]) << 8);
                            m.ata_err = 0;
                            m.ata_count = ATAPI_INT_CD;
                            m.ata_status = ATA_STATUS_DRDY | ATA_STATUS_SEEK | ATA_STATUS_DRQ;
                            raise_ata_irq(m);
                        }
                        ATA_CMD_DEVICE_RESET | ATA_CMD_DIAGNOSTIC => apply_atapi_signature(m),
                        ATA_CMD_IDENTIFY => {
                            m.ata_err = 0x04;
                            m.ata_status = ATA_STATUS_DRDY | ATA_STATUS_ERR;
                            m.ata_lba = ATAPI_SIG_LBA;
                            m.ata_count = 0x01;
                            m.xfer = AtaXfer::Idle;
                        }
                        // Nested Intel 48c598a: OUT 0xEF then IN EAX,DX poll.
                        // Default arm ABRT'd (ERR=0x04); firmware never PACKET.
                        ATA_CMD_SET_FEATURES => {
                            m.ata_err = 0;
                            m.ata_status = ATA_STATUS_DRDY | ATA_STATUS_SEEK;
                            m.xfer = AtaXfer::Idle;
                        }
                        _ => {
                            m.ata_err = 0x04;
                            m.ata_status = ATA_STATUS_DRDY | ATA_STATUS_ERR;
                        }
                    }
                }
                8 => {
                    let prev = m.ata_devctl;
                    m.ata_devctl = v;
                    if (v & ATA_DEVCTL_SRST) != 0 {
                        m.ata_status = ATA_STATUS_BSY;
                        m.xfer = AtaXfer::Idle;
                    } else if (prev & ATA_DEVCTL_SRST) != 0 {
                        apply_atapi_signature(m);
                    }
                }
                _ => {}
            }
            rax
        }
    })
}

fn io_mask(size: u8) -> u64 {
    match size {
        1 => 0xff,
        2 => 0xffff,
        _ => 0xffff_ffff,
    }
}

fn read_data(m: &mut CdMedia, size: u8) -> u64 {
    let n = size as usize;
    if m.xfer != AtaXfer::Identify && m.xfer != AtaXfer::PacketData {
        return 0;
    }
    let mut v = 0u64;
    for i in 0..n {
        if m.xfer_off < m.xfer_end {
            v |= u64::from(m.data[m.xfer_off]) << (8 * i);
            m.xfer_off += 1;
        }
    }
    if m.xfer_off >= m.xfer_end {
        if m.xfer == AtaXfer::PacketData {
            if m.pkt_left > 0 {
                fill_read_drq(m);
            } else {
                packet_ok(m);
            }
        } else {
            m.xfer = AtaXfer::Idle;
            m.ata_status = ATA_STATUS_DRDY | ATA_STATUS_SEEK;
            raise_ata_irq(m);
        }
    }
    v
}

fn write_data(m: &mut CdMedia, size: u8, rax: u64) {
    if m.xfer != AtaXfer::PacketCdb {
        return;
    }
    let n = size as usize;
    for i in 0..n {
        if m.cdb_got < 12 {
            m.cdb[m.cdb_got] = (rax >> (8 * i)) as u8;
            m.cdb_got += 1;
        }
    }
    if m.cdb_got >= 12 {
        finish_packet(m);
    }
}

/// Host-test helper: IDENTIFY PACKET then first word.
pub fn host_identify_word0() -> Option<u16> {
    if !is_visible() {
        return None;
    }
    let _ = ata_io(0x01F7, false, 1, u64::from(ATA_CMD_IDENTIFY_PACKET));
    let lo = ata_io(0x01F0, true, 2, 0) as u16;
    Some(lo)
}

/// Host-test helper: PACKET READ(10) of `lba`.
pub fn host_read10(lba: u32) -> Option<[u8; ISO_SECTOR]> {
    if !is_visible() {
        return None;
    }
    let _ = ata_io(0x01F7, false, 1, u64::from(ATA_CMD_PACKET));
    let cdb = [
        SCSI_READ10,
        0,
        (lba >> 24) as u8,
        (lba >> 16) as u8,
        (lba >> 8) as u8,
        lba as u8,
        0,
        0,
        1,
        0,
        0,
        0,
    ];
    for chunk in cdb.chunks(2) {
        let w = u64::from(chunk[0]) | (u64::from(chunk.get(1).copied().unwrap_or(0)) << 8);
        let _ = ata_io(0x01F0, false, 2, w);
    }
    let mut out = [0u8; ISO_SECTOR];
    for i in 0..ISO_SECTOR / 2 {
        let w = ata_io(0x01F0, true, 2, 0);
        out[i * 2] = w as u8;
        out[i * 2 + 1] = (w >> 8) as u8;
    }
    Some(out)
}

pub fn take_marker() -> bool {
    if !cdrom_visible_evidence(is_visible(), pci_enumerated(), sectors_read()) {
        return false;
    }
    !MARKER.swap(true, Ordering::AcqRel)
}

#[cfg(test)]
#[path = "ide_cdrom_test.rs"]
mod ide_cdrom_test;
