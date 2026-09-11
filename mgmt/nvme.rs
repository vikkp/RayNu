//! Post-EBS NVMe I/O for iron DurableLun (outside Proven Core).
//!
//! Pillar: [Z] [A] [D]
//! Proven Core: **outside** (ADR-002 / ADR-018)
//! VERIFICATION: L1 host tests (command encode + mock controller).
//!
//! Virtio-blk / RayNu-F BlockIo read and write the namespace, not leftover
//! DRAM. USB mass-storage is still residual. Nested File persist is QEMU
//! RAM. Not `ISO-INSTALL-OK`. Not iron `RAYNU-V-M8-DISK-PERSIST-OK`.
//!
//! ADR-004: persist backing is virtio-blk / BlockIo only.

use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// Admin Identify.
pub const NVME_ADMIN_IDENTIFY: u8 = 0x06;
/// Create I/O submission queue.
pub const NVME_ADMIN_CREATE_SQ: u8 = 0x01;
/// Create I/O completion queue.
pub const NVME_ADMIN_CREATE_CQ: u8 = 0x05;
/// I/O Write.
pub const NVME_IO_WRITE: u8 = 0x01;
/// I/O Read.
pub const NVME_IO_READ: u8 = 0x02;

pub const NVME_REG_CAP: u32 = 0x00;
pub const NVME_REG_CC: u32 = 0x14;
pub const NVME_REG_CSTS: u32 = 0x1C;
pub const NVME_REG_AQA: u32 = 0x24;
pub const NVME_REG_ASQ: u32 = 0x28;
pub const NVME_REG_ACQ: u32 = 0x30;
pub const NVME_REG_SQ0TDBL: u32 = 0x1000;

pub const NVME_CC_EN: u32 = 1;
pub const NVME_CSTS_RDY: u32 = 1;
pub const NVME_CC_IOSQES: u32 = 6 << 16;
pub const NVME_CC_IOCQES: u32 = 4 << 20;

pub const NVME_QSIZE: u16 = 16;
pub const NVME_CMD_BYTES: usize = 64;
pub const NVME_CPL_BYTES: usize = 16;
pub const NVME_IDENTIFY_BYTES: usize = 4096;

/// Honesty: QEMU NVMe ≠ Force Off persist. USB I/O is still residual.
pub const NVME_IO_RESIDUAL_NOTE: &str =
    "residual: NVMe I/O is not iron RAYNU-V-M8-DISK-PERSIST-OK; USB I/O still residual; leftover DRAM remains the fallback when Identify fails; do not format PERC; do not print ISO-INSTALL-OK";

/// Why NVMe bring-up failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NvmeError {
    Cap,
    DisableTimeout,
    EnableTimeout,
    Admin,
    Identify,
    TooSmall,
    IoQueue,
    Xfer,
}

/// One 64-byte SQ entry (NVMe 1.4 Figure 12).
#[derive(Clone, Copy)]
pub struct NvmeCmd {
    pub bytes: [u8; NVME_CMD_BYTES],
}

impl NvmeCmd {
    pub const fn empty() -> Self {
        Self {
            bytes: [0; NVME_CMD_BYTES],
        }
    }

    pub fn identify(cid: u16, nsid: u32, prp1: u64, cns: u8) -> Self {
        let mut c = Self::empty();
        c.bytes[0] = NVME_ADMIN_IDENTIFY;
        put_u16(&mut c.bytes, 2, cid);
        put_u32(&mut c.bytes, 4, nsid);
        put_u64(&mut c.bytes, 24, prp1);
        c.bytes[40] = cns;
        c
    }

    pub fn create_cq(cid: u16, prp1: u64, qid: u16, qsize: u16) -> Self {
        let mut c = Self::empty();
        c.bytes[0] = NVME_ADMIN_CREATE_CQ;
        put_u16(&mut c.bytes, 2, cid);
        put_u64(&mut c.bytes, 24, prp1);
        put_u32(
            &mut c.bytes,
            40,
            u32::from(qid) | (u32::from(qsize.saturating_sub(1)) << 16),
        );
        // PC=1, IEN=0
        put_u32(&mut c.bytes, 44, 1);
        c
    }

    pub fn create_sq(cid: u16, prp1: u64, qid: u16, qsize: u16, cqid: u16) -> Self {
        let mut c = Self::empty();
        c.bytes[0] = NVME_ADMIN_CREATE_SQ;
        put_u16(&mut c.bytes, 2, cid);
        put_u64(&mut c.bytes, 24, prp1);
        put_u32(
            &mut c.bytes,
            40,
            u32::from(qid) | (u32::from(qsize.saturating_sub(1)) << 16),
        );
        // PC=1, CQID
        put_u32(&mut c.bytes, 44, u32::from(cqid) | (1 << 16));
        c
    }

    pub fn rw(cid: u16, write: bool, nsid: u32, prp1: u64, slba: u64, nlb_m1: u16) -> Self {
        let mut c = Self::empty();
        c.bytes[0] = if write { NVME_IO_WRITE } else { NVME_IO_READ };
        put_u16(&mut c.bytes, 2, cid);
        put_u32(&mut c.bytes, 4, nsid);
        put_u64(&mut c.bytes, 24, prp1);
        put_u32(&mut c.bytes, 40, slba as u32);
        put_u32(&mut c.bytes, 44, (slba >> 32) as u32);
        put_u32(&mut c.bytes, 48, u32::from(nlb_m1));
        c
    }
}

/// Decode CAP.DSTRD (doorbell stride in bytes = 4 << DSTRD).
pub fn cap_doorbell_stride(cap: u64) -> u32 {
    4u32 << ((cap >> 32) & 0xF)
}

/// Decode CAP.MQES (0-based).
pub fn cap_mqes(cap: u64) -> u16 {
    (cap as u16).saturating_add(1)
}

/// Identify NSZE (last LBA, 0-based) at offset 0.
pub fn identify_nsze(id: &[u8]) -> Option<u64> {
    if id.len() < 8 {
        return None;
    }
    Some(u64::from_le_bytes(id[0..8].try_into().ok()?))
}

/// Identify LBA data size: 2^LBADS from the active LBA format.
pub fn identify_lba_bytes(id: &[u8]) -> Option<u32> {
    if id.len() < 132 {
        return None;
    }
    let flbas = id[26] & 0xF;
    let off = 128usize + (flbas as usize) * 4;
    if off + 4 > id.len() {
        return None;
    }
    let lbaf = u32::from_le_bytes(id[off..off + 4].try_into().ok()?);
    let lbads = ((lbaf >> 16) & 0xFF) as u32;
    if lbads < 9 || lbads > 12 {
        return None;
    }
    Some(1u32 << lbads)
}

/// Namespace size in bytes from Identify Namespace.
pub fn identify_ns_bytes(id: &[u8]) -> Option<u64> {
    let nsze = identify_nsze(id)?;
    let lba = u64::from(identify_lba_bytes(id)?);
    nsze.checked_add(1)?.checked_mul(lba)
}

fn put_u16(b: &mut [u8], off: usize, v: u16) {
    b[off..off + 2].copy_from_slice(&v.to_le_bytes());
}

fn put_u32(b: &mut [u8], off: usize, v: u32) {
    b[off..off + 4].copy_from_slice(&v.to_le_bytes());
}

fn put_u64(b: &mut [u8], off: usize, v: u64) {
    b[off..off + 8].copy_from_slice(&v.to_le_bytes());
}

/// MMIO + DMA for one controller. Host tests mock this; UEFI uses BAR0.
pub trait NvmeHw {
    fn read32(&mut self, off: u32) -> u32;
    fn write32(&mut self, off: u32, val: u32);
    fn dma_read(&mut self, hpa: u64, buf: &mut [u8]);
    fn dma_write(&mut self, hpa: u64, buf: &[u8]);
}

fn read64(hw: &mut impl NvmeHw, off: u32) -> u64 {
    let lo = u64::from(hw.read32(off));
    let hi = u64::from(hw.read32(off + 4));
    lo | (hi << 32)
}

fn write64(hw: &mut impl NvmeHw, off: u32, val: u64) {
    hw.write32(off, val as u32);
    hw.write32(off + 4, (val >> 32) as u32);
}

/// Queue pages owned by the driver (identity-mapped).
pub struct NvmeQueues {
    pub asq: u64,
    pub acq: u64,
    pub iosq: u64,
    pub iocq: u64,
    pub bounce: u64,
}

/// Admin + I/O doorbell indices.
pub struct NvmeDoorbells {
    pub asq_tail: u16,
    pub acq_head: u16,
    pub acq_phase: u16,
    pub iosq_tail: u16,
    pub iocq_head: u16,
    pub iocq_phase: u16,
    pub cid: u16,
    pub stride: u32,
}

impl NvmeDoorbells {
    pub const fn new(stride: u32) -> Self {
        Self {
            asq_tail: 0,
            acq_head: 0,
            acq_phase: 1,
            iosq_tail: 0,
            iocq_head: 0,
            iocq_phase: 1,
            cid: 1,
            stride,
        }
    }
}

fn wait_csts(hw: &mut impl NvmeHw, ready: bool, spins: u32) -> bool {
    for _ in 0..spins {
        let rdy = hw.read32(NVME_REG_CSTS) & NVME_CSTS_RDY != 0;
        if rdy == ready {
            return true;
        }
    }
    false
}

fn cpl_status_ok(cpl: &[u8; NVME_CPL_BYTES]) -> bool {
    let dw3 = u32::from_le_bytes(cpl[12..16].try_into().unwrap_or([0; 4]));
    (dw3 >> 17) & 0x7FF == 0
}

fn cpl_phase(cpl: &[u8; NVME_CPL_BYTES]) -> u16 {
    let dw3 = u32::from_le_bytes(cpl[12..16].try_into().unwrap_or([0; 4]));
    ((dw3 >> 16) & 1) as u16
}

fn submit_admin(
    hw: &mut impl NvmeHw,
    q: &NvmeQueues,
    db: &mut NvmeDoorbells,
    cmd: NvmeCmd,
) -> Result<(), NvmeError> {
    let idx = u64::from(db.asq_tail);
    let hpa = q.asq.saturating_add(idx * NVME_CMD_BYTES as u64);
    hw.dma_write(hpa, &cmd.bytes);
    db.asq_tail = (db.asq_tail + 1) % NVME_QSIZE;
    hw.write32(NVME_REG_SQ0TDBL, u32::from(db.asq_tail));
    let mut cpl = [0u8; NVME_CPL_BYTES];
    let chpa = q
        .acq
        .saturating_add(u64::from(db.acq_head) * NVME_CPL_BYTES as u64);
    let mut spins = 0u32;
    loop {
        hw.dma_read(chpa, &mut cpl);
        if cpl_phase(&cpl) == db.acq_phase {
            break;
        }
        spins = spins.saturating_add(1);
        if spins > 100_000 {
            return Err(NvmeError::Admin);
        }
    }
    if !cpl_status_ok(&cpl) {
        return Err(NvmeError::Admin);
    }
    db.acq_head = (db.acq_head + 1) % NVME_QSIZE;
    if db.acq_head == 0 {
        db.acq_phase ^= 1;
    }
    hw.write32(NVME_REG_SQ0TDBL + db.stride, u32::from(db.acq_head));
    Ok(())
}

fn submit_io(
    hw: &mut impl NvmeHw,
    q: &NvmeQueues,
    db: &mut NvmeDoorbells,
    cmd: NvmeCmd,
) -> Result<(), NvmeError> {
    let idx = u64::from(db.iosq_tail);
    let hpa = q.iosq.saturating_add(idx * NVME_CMD_BYTES as u64);
    hw.dma_write(hpa, &cmd.bytes);
    db.iosq_tail = (db.iosq_tail + 1) % NVME_QSIZE;
    hw.write32(NVME_REG_SQ0TDBL + 2 * db.stride, u32::from(db.iosq_tail));
    let mut cpl = [0u8; NVME_CPL_BYTES];
    let chpa = q
        .iocq
        .saturating_add(u64::from(db.iocq_head) * NVME_CPL_BYTES as u64);
    let mut spins = 0u32;
    loop {
        hw.dma_read(chpa, &mut cpl);
        if cpl_phase(&cpl) == db.iocq_phase {
            break;
        }
        spins = spins.saturating_add(1);
        if spins > 100_000 {
            return Err(NvmeError::Xfer);
        }
    }
    if !cpl_status_ok(&cpl) {
        return Err(NvmeError::Xfer);
    }
    db.iocq_head = (db.iocq_head + 1) % NVME_QSIZE;
    if db.iocq_head == 0 {
        db.iocq_phase ^= 1;
    }
    hw.write32(NVME_REG_SQ0TDBL + 3 * db.stride, u32::from(db.iocq_head));
    Ok(())
}

/// Reset, enable admin queues, Identify NS 1, create I/O qid 1.
pub fn nvme_bring_up(
    hw: &mut impl NvmeHw,
    q: &NvmeQueues,
    min_bytes: u64,
) -> Result<(NvmeDoorbells, u64, u32), NvmeError> {
    let cap = read64(hw, NVME_REG_CAP);
    if cap_mqes(cap) < NVME_QSIZE {
        return Err(NvmeError::Cap);
    }
    let stride = cap_doorbell_stride(cap);
    hw.write32(NVME_REG_CC, 0);
    if !wait_csts(hw, false, 100_000) {
        return Err(NvmeError::DisableTimeout);
    }
    let aqa = u32::from(NVME_QSIZE - 1) | (u32::from(NVME_QSIZE - 1) << 16);
    hw.write32(NVME_REG_AQA, aqa);
    write64(hw, NVME_REG_ASQ, q.asq);
    write64(hw, NVME_REG_ACQ, q.acq);
    hw.write32(NVME_REG_CC, NVME_CC_EN | NVME_CC_IOSQES | NVME_CC_IOCQES);
    if !wait_csts(hw, true, 100_000) {
        return Err(NvmeError::EnableTimeout);
    }
    let mut db = NvmeDoorbells::new(stride);
    let mut ident = [0u8; NVME_IDENTIFY_BYTES];
    hw.dma_write(q.bounce, &ident);
    db.cid = 1;
    let cid = db.cid;
    submit_admin(hw, q, &mut db, NvmeCmd::identify(cid, 1, q.bounce, 0))?;
    hw.dma_read(q.bounce, &mut ident);
    let ns_bytes = identify_ns_bytes(&ident).ok_or(NvmeError::Identify)?;
    if ns_bytes < min_bytes {
        return Err(NvmeError::TooSmall);
    }
    let lba = identify_lba_bytes(&ident).ok_or(NvmeError::Identify)?;
    db.cid = 2;
    let cid = db.cid;
    submit_admin(
        hw,
        q,
        &mut db,
        NvmeCmd::create_cq(cid, q.iocq, 1, NVME_QSIZE),
    )?;
    db.cid = 3;
    let cid = db.cid;
    submit_admin(
        hw,
        q,
        &mut db,
        NvmeCmd::create_sq(cid, q.iosq, 1, NVME_QSIZE, 1),
    )?;
    Ok((db, ns_bytes, lba))
}

/// Read or write `buf` at byte `off` on NSID 1. `buf.len()` must be a
/// multiple of LBA size and not exceed the bounce page (4 KiB).
pub fn nvme_rw(
    hw: &mut impl NvmeHw,
    q: &NvmeQueues,
    db: &mut NvmeDoorbells,
    lba_bytes: u32,
    off: u64,
    buf: &mut [u8],
    write: bool,
) -> Result<(), NvmeError> {
    let lba = u64::from(lba_bytes);
    if lba == 0 || buf.is_empty() || off % lba != 0 || (buf.len() as u64) % lba != 0 {
        return Err(NvmeError::Xfer);
    }
    if buf.len() > 4096 {
        return Err(NvmeError::Xfer);
    }
    let slba = off / lba;
    let nlb = (buf.len() as u64) / lba;
    if nlb == 0 || nlb > 0x10000 {
        return Err(NvmeError::Xfer);
    }
    if write {
        hw.dma_write(q.bounce, buf);
    }
    db.cid = db.cid.wrapping_add(1);
    if db.cid == 0 {
        db.cid = 1;
    }
    let cid = db.cid;
    submit_io(
        hw,
        q,
        db,
        NvmeCmd::rw(cid, write, 1, q.bounce, slba, (nlb - 1) as u16),
    )?;
    if !write {
        hw.dma_read(q.bounce, buf);
    }
    Ok(())
}

static IO_READY: AtomicBool = AtomicBool::new(false);
static NS_BYTES: AtomicU64 = AtomicU64::new(0);
static LBA_BYTES: AtomicU64 = AtomicU64::new(0);
static LUN_RESERVED: AtomicBool = AtomicBool::new(false);

/// True after Identify + I/O queues succeed (or a host-test Vec is attached).
pub fn nvme_io_ready() -> bool {
    IO_READY.load(Ordering::Acquire)
}

pub fn nvme_ns_bytes() -> u64 {
    NS_BYTES.load(Ordering::Acquire)
}

pub fn nvme_lba_bytes() -> u32 {
    LBA_BYTES.load(Ordering::Acquire) as u32
}

pub fn store_nvme_ready(bytes: u64, lba: u32) {
    NS_BYTES.store(bytes, Ordering::Release);
    LBA_BYTES.store(u64::from(lba), Ordering::Release);
    IO_READY.store(true, Ordering::Release);
}

pub fn clear_nvme_ready() {
    IO_READY.store(false, Ordering::Release);
    NS_BYTES.store(0, Ordering::Release);
    LBA_BYTES.store(0, Ordering::Release);
    LUN_RESERVED.store(false, Ordering::Release);
}

/// One-shot: virtio should attach this LUN, not leftover DRAM.
pub fn reserve_durable_lun_install_disk(bytes: u64) {
    NS_BYTES.store(bytes, Ordering::Release);
    LUN_RESERVED.store(true, Ordering::Release);
}

pub fn durable_lun_install_reserved() -> bool {
    LUN_RESERVED.load(Ordering::Acquire)
        && IO_READY.load(Ordering::Acquire)
        && NS_BYTES.load(Ordering::Acquire) != 0
}

pub fn take_durable_lun_install_disk() -> Option<u64> {
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

/// Host tests attach a Vec as the namespace (no MMIO).
#[cfg(test)]
pub fn host_nvme_attach(ns: &'static mut [u8], lba: u32) {
    HOST_NS.store(ns.as_mut_ptr() as u64, Ordering::Release);
    HOST_NS_LEN.store(ns.len() as u64, Ordering::Release);
    store_nvme_ready(ns.len() as u64, lba);
}

#[cfg(test)]
static HOST_NS: AtomicU64 = AtomicU64::new(0);
#[cfg(test)]
static HOST_NS_LEN: AtomicU64 = AtomicU64::new(0);

#[cfg(test)]
pub fn host_nvme_rw(off: u64, buf: &mut [u8], write: bool) -> bool {
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
    // SAFETY: host_nvme_attach promised a live test allocation.
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
pub fn host_nvme_rw(_off: u64, _buf: &mut [u8], _write: bool) -> bool {
    false
}

#[repr(C, align(4096))]
struct NvmePage([u8; 4096]);

#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
struct LiveNvme {
    mmio: u64,
    q: NvmeQueues,
    db: NvmeDoorbells,
    lba: u32,
}

#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
static mut LIVE: Option<LiveNvme> = None;
#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
static LIVE_LOCK: AtomicBool = AtomicBool::new(false);
#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
static mut ASQ: NvmePage = NvmePage([0; 4096]);
#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
static mut ACQ: NvmePage = NvmePage([0; 4096]);
#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
static mut IOSQ: NvmePage = NvmePage([0; 4096]);
#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
static mut IOCQ: NvmePage = NvmePage([0; 4096]);
#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
static mut BOUNCE: NvmePage = NvmePage([0; 4096]);

#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
struct MmioNvme {
    base: u64,
}

#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
impl NvmeHw for MmioNvme {
    fn read32(&mut self, off: u32) -> u32 {
        // SAFETY: firmware-assigned NVMe BAR; identity map after EBS.
        // KANI-TARGET: host tests use MockNvme, not this MMIO.
        unsafe { core::ptr::read_volatile((self.base + u64::from(off)) as *const u32) }
    }

    fn write32(&mut self, off: u32, val: u32) {
        // SAFETY: firmware-assigned NVMe BAR.
        // KANI-TARGET: host tests use MockNvme, not this MMIO.
        unsafe {
            core::ptr::write_volatile((self.base + u64::from(off)) as *mut u32, val);
        }
    }

    fn dma_read(&mut self, hpa: u64, buf: &mut [u8]) {
        if buf.is_empty() {
            return;
        }
        // SAFETY: queue/bounce pages in this EFI image.
        unsafe {
            core::ptr::copy_nonoverlapping(hpa as *const u8, buf.as_mut_ptr(), buf.len());
        }
    }

    fn dma_write(&mut self, hpa: u64, buf: &[u8]) {
        if buf.is_empty() {
            return;
        }
        // SAFETY: queue/bounce pages in this EFI image.
        unsafe {
            core::ptr::copy_nonoverlapping(buf.as_ptr(), hpa as *mut u8, buf.len());
        }
    }
}

#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
fn nvme_bar(bus: u8, dev: u8, func: u8) -> u64 {
    use crate::mgmt::e1000_mmio::{pci_read32, pci_write32};
    let cmd = pci_read32(bus, dev, func, 0x04) as u16;
    let next = cmd | (1 << 1) | (1 << 2);
    let rest = pci_read32(bus, dev, func, 0x04) & 0xFFFF_0000;
    pci_write32(bus, dev, func, 0x04, rest | u32::from(next));
    let bar0 = pci_read32(bus, dev, func, 0x10);
    let lo = u64::from(bar0 & !0xF);
    if bar0 & 0x4 != 0 {
        let bar1 = pci_read32(bus, dev, func, 0x14);
        lo | (u64::from(bar1) << 32)
    } else {
        lo
    }
}

/// Bring up the census NVMe BDF. Identity-mapped BAR + .bss queues.
#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
pub fn nvme_init_pci(bus: u8, dev: u8, func: u8, min_bytes: u64) -> Result<u64, NvmeError> {
    if LIVE_LOCK.swap(true, Ordering::Acquire) {
        return Err(NvmeError::Admin);
    }
    let bar = nvme_bar(bus, dev, func);
    if bar == 0 {
        LIVE_LOCK.store(false, Ordering::Release);
        return Err(NvmeError::Cap);
    }
    // SAFETY: BSP-only; queues are .bss in this image.
    let q = unsafe {
        ASQ.0.fill(0);
        ACQ.0.fill(0);
        IOSQ.0.fill(0);
        IOCQ.0.fill(0);
        BOUNCE.0.fill(0);
        NvmeQueues {
            asq: core::ptr::addr_of_mut!(ASQ) as u64,
            acq: core::ptr::addr_of_mut!(ACQ) as u64,
            iosq: core::ptr::addr_of_mut!(IOSQ) as u64,
            iocq: core::ptr::addr_of_mut!(IOCQ) as u64,
            bounce: core::ptr::addr_of_mut!(BOUNCE) as u64,
        }
    };
    let mut hw = MmioNvme { base: bar };
    let result = nvme_bring_up(&mut hw, &q, min_bytes);
    match result {
        Ok((db, bytes, lba)) => {
            // SAFETY: lock held; single LIVE writer.
            unsafe {
                LIVE = Some(LiveNvme {
                    mmio: bar,
                    q,
                    db,
                    lba,
                });
            }
            store_nvme_ready(bytes, lba);
            LIVE_LOCK.store(false, Ordering::Release);
            Ok(bytes)
        }
        Err(e) => {
            LIVE_LOCK.store(false, Ordering::Release);
            Err(e)
        }
    }
}

#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
pub fn nvme_live_rw(off: u64, buf: &mut [u8], write: bool) -> bool {
    if LIVE_LOCK.swap(true, Ordering::Acquire) {
        return false;
    }
    // SAFETY: lock held; LIVE set by nvme_init_pci.
    let ok = unsafe {
        match LIVE.as_mut() {
            Some(live) => {
                let mut hw = MmioNvme { base: live.mmio };
                let mut slice = [0u8; 4096];
                if buf.len() > slice.len() {
                    false
                } else {
                    if write {
                        slice[..buf.len()].copy_from_slice(buf);
                    }
                    let r = nvme_rw(
                        &mut hw,
                        &live.q,
                        &mut live.db,
                        live.lba,
                        off,
                        &mut slice[..buf.len()],
                        write,
                    );
                    if r.is_ok() && !write {
                        buf.copy_from_slice(&slice[..buf.len()]);
                    }
                    r.is_ok()
                }
            }
            None => false,
        }
    };
    LIVE_LOCK.store(false, Ordering::Release);
    ok
}

#[cfg(not(all(target_os = "uefi", feature = "uefi-bin")))]
pub fn nvme_init_pci(_bus: u8, _dev: u8, _func: u8, _min_bytes: u64) -> Result<u64, NvmeError> {
    Err(NvmeError::Cap)
}

#[cfg(not(all(target_os = "uefi", feature = "uefi-bin")))]
pub fn nvme_live_rw(_off: u64, _buf: &mut [u8], _write: bool) -> bool {
    false
}

#[cfg(test)]
#[path = "nvme_test.rs"]
mod nvme_test;
