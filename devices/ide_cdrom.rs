//! Guest-visible ATAPI CD-ROM for the private guest-UEFI VMCS.
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** (ADR-002 / ADR-014)
//! VERIFICATION: L1 (runtime + host tests; QEMU is the guest-visible gate)
//!
//! PCI IDE at `00:00.0` plus primary ATA PIO (`0x1F0`/`0x3F6`).
//! OVMF PEI only probes `00:00.0` Device ID (not a full bus walk). Stage 40
//! had `pci_ide=1` here; Stage 41 host bridge stole that slot.
//! Media is a retained ISO prefix (mock EFI catalog in host tests; placeholder
//! on QEMU if the operator has not called [`present`] yet).
//! Not virtio-in-guest. Not a distro installer. Not Everest E5.

use crate::devices::guest_platform::pci_cfg_offset;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

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
pub const GUEST_CD_PCI_FN: u8 = 0;
pub const GUEST_CD_PCI_VENDOR: u16 = 0x8086;
pub const GUEST_CD_PCI_DEVICE: u16 = 0x7010;

const ATA_STATUS_DRDY: u8 = 0x40;
const ATA_STATUS_DRQ: u8 = 0x08;
const ATA_CMD_IDENTIFY_PACKET: u8 = 0xA1;
const ATA_CMD_PACKET: u8 = 0xA0;
const SCSI_READ10: u8 = 0x28;
const SCSI_INQUIRY: u8 = 0x12;
const SCSI_TEST_UNIT: u8 = 0x00;
const SCSI_READ_CAPACITY: u8 = 0x25;

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
    ata_status: u8,
    xfer: AtaXfer,
    xfer_off: usize,
    xfer_end: usize,
    cdb: [u8; 12],
    cdb_got: usize,
    data: [u8; 2048],
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
            ata_count: 0,
            ata_lba: [0; 3],
            ata_dev: 0,
            ata_status: ATA_STATUS_DRDY,
            xfer: AtaXfer::Idle,
            xfer_off: 0,
            xfer_end: 0,
            cdb: [0; 12],
            cdb_got: 0,
            data: [0; 2048],
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
    bus == GUEST_CD_PCI_BUS && dev == GUEST_CD_PCI_DEV && fun == GUEST_CD_PCI_FN
}

/// PCI config address for the guest IDE function (`00:00.0`).
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
        m.ata_status = ATA_STATUS_DRDY;
        m.xfer = AtaXfer::Idle;
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

/// Placeholder ISO: `CD001` at LBA 16 so ATAPI READ is distinguishable.
pub fn present_placeholder() -> bool {
    let mut iso = [0u8; GUEST_CD_ISO_CAP];
    let pvd = 16 * ISO_SECTOR;
    iso[pvd] = 1;
    iso[pvd + 1..pvd + 6].copy_from_slice(b"CD001");
    iso[pvd + 40..pvd + 50].copy_from_slice(b"RAYNU-V-CD");
    present(&iso, 1)
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
    m.ata_status = ATA_STATUS_DRDY | ATA_STATUS_DRQ;
}

fn load_sector(m: &mut CdMedia, lba: u32) -> bool {
    let start = (lba as usize).saturating_mul(ISO_SECTOR);
    let end = start.saturating_add(ISO_SECTOR);
    if end > m.len {
        return false;
    }
    m.data[..ISO_SECTOR].copy_from_slice(&m.iso[start..end]);
    m.xfer = AtaXfer::PacketData;
    m.xfer_off = 0;
    m.xfer_end = ISO_SECTOR;
    m.ata_status = ATA_STATUS_DRDY | ATA_STATUS_DRQ;
    m.sectors_read = m.sectors_read.saturating_add(1);
    SECTORS.store(m.sectors_read, Ordering::Release);
    true
}

fn finish_packet(m: &mut CdMedia) {
    match m.cdb[0] {
        SCSI_TEST_UNIT => {
            m.xfer = AtaXfer::Idle;
            m.ata_status = ATA_STATUS_DRDY;
        }
        SCSI_INQUIRY => {
            m.data.fill(0);
            m.data[0] = 0x05; // CD/DVD
            m.data[1] = 0x80; // RMB
            m.data[2] = 0x05;
            m.data[3] = 0x02;
            m.data[4] = 31;
            m.data[8..16].copy_from_slice(b"RAYNU-V ");
            m.data[16..32].copy_from_slice(b"GUEST CD        ");
            m.xfer = AtaXfer::PacketData;
            m.xfer_off = 0;
            m.xfer_end = 36;
            m.ata_status = ATA_STATUS_DRDY | ATA_STATUS_DRQ;
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
            m.xfer = AtaXfer::PacketData;
            m.xfer_off = 0;
            m.xfer_end = 8;
            m.ata_status = ATA_STATUS_DRDY | ATA_STATUS_DRQ;
        }
        SCSI_READ10 => {
            let lba = u32::from_be_bytes([m.cdb[2], m.cdb[3], m.cdb[4], m.cdb[5]]);
            if !load_sector(m, lba) {
                m.xfer = AtaXfer::Idle;
                m.ata_status = ATA_STATUS_DRDY | 0x01;
            }
        }
        _ => {
            m.xfer = AtaXfer::Idle;
            m.ata_status = ATA_STATUS_DRDY;
        }
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
                0x01F1 => 0,
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
                        m.xfer = AtaXfer::PacketCdb;
                        m.cdb_got = 0;
                        m.cdb.fill(0);
                        m.ata_status = ATA_STATUS_DRDY | ATA_STATUS_DRQ;
                    }
                    _ => m.ata_status = ATA_STATUS_DRDY,
                },
                0x03F6 => {}
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
        m.xfer = AtaXfer::Idle;
        m.ata_status = ATA_STATUS_DRDY;
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
