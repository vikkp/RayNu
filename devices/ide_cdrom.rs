//! Guest-visible ATAPI CD-ROM for the private guest-UEFI VMCS.
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** (ADR-002 / ADR-014)
//! VERIFICATION: L1 (runtime + host tests; QEMU is the guest-visible gate)
//!
//! PCI IDE at `00:00.1` (virtio `00:00.0` fn1) **and** PIIX `00:01.1`.
//! PEI only `inw`s DID of `00:00.0` (virtio). A walk of that multifunction
//! slot finds fn1; a PIIX walk finds `00:01.1`. Same ATAPI backend.
//! After reset the ATAPI signature is LBA mid=`0x14` high=`0xEB` so firmware
//! sends PACKET (`0xA0`). Interrupt reason in sector-count is CDB `0x01`,
//! data-in `0x02`, complete `0x03`. Cylinder holds the PACKET byte count.
//! CD stays GuestVisible.
//! Media is a retained ISO prefix (mock EFI catalog in host tests; placeholder
//! on QEMU if the operator has not called [`present`] yet).
//! Not virtio-in-guest. Not a distro installer. Not Everest E5.

use crate::devices::guest_platform::pci_cfg_offset;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicU8, Ordering};

/// ECMA-119 / El Torito sector size.
pub const ISO_SECTOR: usize = 2048;
/// Cap matches the host mock EFI ISO (26 × 2048) without depending on mgmt.
pub const MOCK_EFI_ISO_BYTES: usize = 26 * ISO_SECTOR;

/// QEMU / serial marker when the guest-UEFI VMCS can see CD media.
pub const M7_E5_OVMF_CDROM_OK_MARKER: &str = "RAYNU-V-M7-E5-OVMF-CDROM-OK";

/// Cap matches the host mock EFI ISO (26 × 2048).
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
const ATA_CMD_IDENTIFY: u8 = 0xEC;
const ATA_CMD_IDENTIFY_PACKET: u8 = 0xA1;
const ATA_CMD_PACKET: u8 = 0xA0;
const ATA_DEVCTL_SRST: u8 = 0x04;
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
const XFER_CAP: usize = 4 * ISO_SECTOR;

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
    iso_id: u64,
    visible: bool,
    pci_enum: bool,
    sectors_read: u32,
    pci_addr: u32,
    pci_cmd: u16,
    bar0: u32,
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
    data: [u8; XFER_CAP],
}

impl CdMedia {
    const fn empty() -> Self {
        Self {
            iso: [0u8; GUEST_CD_ISO_CAP],
            len: 0,
            iso_id: 0,
            visible: false,
            pci_enum: false,
            sectors_read: 0,
            pci_addr: 0,
            pci_cmd: 0x0001,
            bar0: 0x1F1,
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
static SECTORS: AtomicU32 = AtomicU32::new(0);
static ISO_ID: AtomicU64 = AtomicU64::new(0);
static ISO_LEN: AtomicU32 = AtomicU32::new(0);
static MARKER: AtomicBool = AtomicBool::new(false);
static PACKET_N: AtomicU32 = AtomicU32::new(0);
static LAST_SCSI: AtomicU8 = AtomicU8::new(0);

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

/// PCI config address for the guest IDE function (`00:00.1`).
pub fn pci_config_addr() -> u32 {
    0x8000_0000
        | (u32::from(GUEST_CD_PCI_BUS) << 16)
        | (u32::from(GUEST_CD_PCI_DEV) << 11)
        | (u32::from(GUEST_CD_PCI_FN) << 8)
}

pub fn is_ata_primary_port(port: u16) -> bool {
    (0x01F0..=0x01F7).contains(&port) || port == 0x03F6
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

pub fn retained_iso_id() -> u64 {
    ISO_ID.load(Ordering::Acquire)
}

pub fn retained_len() -> usize {
    ISO_LEN.load(Ordering::Acquire) as usize
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
    SECTORS.store(0, Ordering::Release);
    ISO_ID.store(0, Ordering::Release);
    ISO_LEN.store(0, Ordering::Release);
    MARKER.store(false, Ordering::Release);
    PACKET_N.store(0, Ordering::Release);
    LAST_SCSI.store(0, Ordering::Release);
}

/// PACKET commands issued since last reset (firmware ATAPI activity).
pub fn packet_commands() -> u32 {
    PACKET_N.load(Ordering::Acquire)
}

/// Last SCSI opcode from a completed 12-byte CDB.
pub fn last_scsi() -> u8 {
    LAST_SCSI.load(Ordering::Acquire)
}

/// Retain ISO bytes without making the PCI device live.
pub fn retain(iso: &[u8], iso_id: u64) -> bool {
    if iso_id == 0 || iso.len() < ISO_SECTOR {
        return false;
    }
    let n = core::cmp::min(iso.len(), GUEST_CD_ISO_CAP);
    with_cd(|m| {
        m.iso[..n].copy_from_slice(&iso[..n]);
        if n < GUEST_CD_ISO_CAP {
            m.iso[n..].fill(0);
        }
        m.len = n;
        m.iso_id = iso_id;
    });
    ISO_ID.store(iso_id, Ordering::Release);
    ISO_LEN.store(n as u32, Ordering::Release);
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
        apply_atapi_signature(m);
        true
    });
    if ok {
        VISIBLE.store(true, Ordering::Release);
        PCI_ENUM.store(false, Ordering::Release);
        SECTORS.store(0, Ordering::Release);
        MARKER.store(false, Ordering::Release);
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

/// Placeholder ISO: PVD `CD001` at LBA 16 plus a minimal EFI El Torito catalog.
pub fn present_placeholder() -> bool {
    let mut iso = [0u8; GUEST_CD_ISO_CAP];
    write_placeholder_iso(&mut iso);
    present(&iso, 1)
}

fn write_placeholder_iso(iso: &mut [u8]) {
    let pvd = 16 * ISO_SECTOR;
    iso[pvd] = 1;
    iso[pvd + 1..pvd + 6].copy_from_slice(b"CD001");
    iso[pvd + 40..pvd + 50].copy_from_slice(b"RAYNU-V-CD");
    let br = 17 * ISO_SECTOR;
    iso[br] = 0;
    iso[br + 1..br + 6].copy_from_slice(b"CD001");
    iso[br + 6] = 1;
    iso[br + 7..br + 7 + 23].copy_from_slice(b"EL TORITO SPECIFICATION");
    iso[br + 71..br + 75].copy_from_slice(&20u32.to_le_bytes());
    let cat = 20 * ISO_SECTOR;
    iso[cat] = 0x01;
    iso[cat + 1] = 0xEF;
    iso[cat + 30] = 0x55;
    iso[cat + 31] = 0xAA;
    iso[cat + 32] = 0x91;
    iso[cat + 33] = 0xEF;
    iso[cat + 64] = 0x88;
    iso[cat + 70..cat + 72].copy_from_slice(&4u16.to_le_bytes());
    iso[cat + 72..cat + 76].copy_from_slice(&22u32.to_le_bytes());
    let load = 22 * ISO_SECTOR;
    iso[load] = b'M';
    iso[load + 1] = b'Z';
}

/// QEMU launch path: present a CD if the operator has not attached one.
pub fn present_placeholder_if_idle() -> bool {
    if is_visible() && retained_len() >= ISO_SECTOR {
        return true;
    }
    present_placeholder()
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
        0x14 => 0x03F5,
        0x18 => 0x0171,
        0x1C => 0x0375,
        0x2C => 0x0000_0000,
        0x3C => 0x0000_010E, // pin 1, IRQ 14
        _ => 0,
    }
}

pub fn pci_read_data(port: u16, size: u8) -> u32 {
    with_cd(|m| {
        if !m.visible {
            return 0xFFFF_FFFF;
        }
        let addr = m.pci_addr;
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

pub fn pci_write_data(port: u16, size: u8, val: u32) {
    with_cd(|m| {
        if !m.visible || !pci_addr_selects_cd(m.pci_addr) {
            return;
        }
        let off = pci_cfg_offset(m.pci_addr, port);
        if off == 0x04 {
            m.pci_cmd = (val as u16) | 0x0001;
        } else if off == 0x10 {
            let mask = if size >= 4 { 0xFFFF_FFFC } else { 0xFFFF };
            m.bar0 = (val & mask) | 1;
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
}

fn packet_ok(m: &mut CdMedia) {
    m.xfer = AtaXfer::Idle;
    m.ata_err = 0;
    m.ata_count = ATAPI_INT_IO | ATAPI_INT_CD;
    m.ata_status = ATA_STATUS_DRDY | ATA_STATUS_SEEK;
}

fn packet_error(m: &mut CdMedia, sense: u8, asc: u8) {
    m.sense_key = sense;
    m.sense_asc = asc;
    m.xfer = AtaXfer::Idle;
    m.ata_err = sense << 4;
    m.ata_count = ATAPI_INT_IO | ATAPI_INT_CD;
    m.ata_status = ATA_STATUS_DRDY | ATA_STATUS_ERR;
}

fn begin_packet_data(m: &mut CdMedia, n: usize) {
    let n = n.min(XFER_CAP);
    let mut limit = m.byte_limit as usize;
    if limit == 0 || limit == 0xffff {
        limit = n;
    }
    let size = n.min(limit).max(2);
    m.xfer = AtaXfer::PacketData;
    m.xfer_off = 0;
    m.xfer_end = n.min(XFER_CAP);
    m.ata_count = ATAPI_INT_IO;
    m.ata_lba[1] = size as u8;
    m.ata_lba[2] = (size >> 8) as u8;
    m.ata_err = 0;
    m.ata_status = ATA_STATUS_DRDY | ATA_STATUS_SEEK | ATA_STATUS_DRQ;
}

fn start_identify(m: &mut CdMedia) {
    m.data.fill(0);
    // Word 0: ATAPI CD-ROM, packet size 12.
    m.data[0] = 0x00;
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
    m.xfer = AtaXfer::Identify;
    m.xfer_off = 0;
    m.xfer_end = 512;
    m.ata_err = 0;
    m.ata_status = ATA_STATUS_DRDY | ATA_STATUS_SEEK | ATA_STATUS_DRQ;
}

fn load_sectors(m: &mut CdMedia, lba: u32, count: u32) -> bool {
    if count == 0 {
        packet_ok(m);
        return true;
    }
    let nsec = (count as usize).min(XFER_CAP / ISO_SECTOR).max(1);
    let start = (lba as usize).saturating_mul(ISO_SECTOR);
    let bytes = nsec.saturating_mul(ISO_SECTOR);
    let end = start.saturating_add(bytes);
    if end > m.len {
        packet_error(m, SCSI_SENSE_ILLEGAL, 0x21);
        return false;
    }
    m.data[..bytes].copy_from_slice(&m.iso[start..end]);
    m.sectors_read = m.sectors_read.saturating_add(nsec as u32);
    SECTORS.store(m.sectors_read, Ordering::Release);
    begin_packet_data(m, bytes);
    true
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

/// ATA primary PIO. Returns the value to merge into RAX on IN.
pub fn ata_io(port: u16, is_in: bool, size: u8, rax: u64) -> u64 {
    with_cd(|m| {
        if !m.visible {
            return if is_in { rax | 0xff } else { rax };
        }
        if is_in {
            let val = match port {
                0x01F0 => read_data(m, size),
                0x01F1 => u64::from(m.ata_err),
                0x01F2 => u64::from(m.ata_count),
                0x01F3 => u64::from(m.ata_lba[0]),
                0x01F4 => u64::from(m.ata_lba[1]),
                0x01F5 => u64::from(m.ata_lba[2]),
                0x01F6 => u64::from(m.ata_dev),
                0x01F7 | 0x03F6 => u64::from(m.ata_status),
                _ => 0,
            };
            let mask = io_mask(size);
            (rax & !mask) | (val & mask)
        } else {
            let v = rax as u8;
            match port {
                0x01F0 => write_data(m, size, rax),
                0x01F1 => m.ata_feat = v,
                0x01F2 => m.ata_count = v,
                0x01F3 => m.ata_lba[0] = v,
                0x01F4 => m.ata_lba[1] = v,
                0x01F5 => m.ata_lba[2] = v,
                0x01F6 => m.ata_dev = v,
                0x01F7 => match v {
                    ATA_CMD_IDENTIFY_PACKET => start_identify(m),
                    ATA_CMD_PACKET => {
                        PACKET_N.fetch_add(1, Ordering::AcqRel);
                        m.xfer = AtaXfer::PacketCdb;
                        m.cdb_got = 0;
                        m.cdb.fill(0);
                        m.byte_limit = u16::from(m.ata_lba[1]) | (u16::from(m.ata_lba[2]) << 8);
                        m.ata_err = 0;
                        m.ata_count = ATAPI_INT_CD;
                        m.ata_status = ATA_STATUS_DRDY | ATA_STATUS_SEEK | ATA_STATUS_DRQ;
                    }
                    ATA_CMD_DEVICE_RESET => apply_atapi_signature(m),
                    ATA_CMD_IDENTIFY => {
                        // ATAPI devices abort ATA IDENTIFY (not PACKET).
                        m.ata_err = 0x04;
                        m.ata_status = ATA_STATUS_DRDY | ATA_STATUS_ERR;
                        m.ata_lba = ATAPI_SIG_LBA;
                        m.ata_count = 0x01;
                        m.xfer = AtaXfer::Idle;
                    }
                    _ => {
                        m.ata_err = 0x04;
                        m.ata_status = ATA_STATUS_DRDY | ATA_STATUS_ERR;
                    }
                },
                0x03F6 => {
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
            packet_ok(m);
        } else {
            m.xfer = AtaXfer::Idle;
            m.ata_status = ATA_STATUS_DRDY | ATA_STATUS_SEEK;
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
