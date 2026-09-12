//! Iron DurableLun mapper: USB partition or NVMe, never PERC Ubuntu.
//!
//! Pillar: [Z] [A] [D]
//! Proven Core: **outside** (ADR-002 / ADR-018)
//! VERIFICATION: L0/L1 host tests (classify + pick). No PCI port I/O in
//! `cargo test`.
//!
//! Nested File persist is QEMU RAM (`M8_PERSIST_IMG`). Iron needs a LUN
//! that survives Force Off. This mapper **picks** that LUN and **refuses**
//! the R640 PERC (Ubuntu) and the 4 GB ESP Cruzer that already holds the ISO.
//! NVMe Identify + Read/Write and USB BOT/xHCI I/O both run **after**
//! ExitBootServices so firmware can disconnect its NVMe/xHCI drivers (HCRST).
//! Leftover DRAM remains the last fallback. QEMU NVMe/USB ≠ Force Off persist.
//! Not `ISO-INSTALL-OK`. Not iron `RAYNU-V-M8-DISK-PERSIST-OK`. Do not F11.
//!
//! ADR-004: persist backing is virtio-blk / BlockIo only.

use core::sync::atomic::{AtomicU32, AtomicU64, AtomicU8, Ordering};

use super::disk_persist::{
    M8_DISK_PERSIST_OK_MARKER, PERC_UBUNTU_UNTOUCHED_NOTE, UDISK_TOO_SMALL_NOTE,
};
use super::iso_install::LEFTOVER_DISK_TRY_BYTES;

/// Minimum LUN size for the Alpine GPT/ESP/ext4 install disk (1 GiB).
pub const DURABLE_LUN_MIN_BYTES: u64 = 1024 * 1024 * 1024;

/// 4 GB Cruzer / UDisk window that already holds alpine-extended on iron.
pub const ESP_CRUZER_MIN_BYTES: u64 = 2 * 1024 * 1024 * 1024;
/// Upper bound of that window (marketed 4 GB sticks).
pub const ESP_CRUZER_MAX_BYTES: u64 = 8 * 1024 * 1024 * 1024;

/// LSI / Broadcom MegaRAID (PERC H730 / H740P family).
pub const PCI_VENDOR_LSI: u16 = 0x1000;
/// Dell (PERC OEM IDs).
pub const PCI_VENDOR_DELL: u16 = 0x1028;
/// Mass-storage class.
pub const PCI_CLASS_STORAGE: u8 = 0x01;
/// RAID subclass (PERC).
pub const PCI_SUBCLASS_RAID: u8 = 0x04;
/// NVMe subclass.
pub const PCI_SUBCLASS_NVME: u8 = 0x08;
/// Serial-bus class (xHCI lives here, not as a LUN).
pub const PCI_CLASS_SERIAL: u8 = 0x0C;
/// USB subclass.
pub const PCI_SUBCLASS_USB: u8 = 0x03;
/// xHCI programming interface.
pub const PCI_PROG_XHCI: u8 = 0x30;

/// Honesty: census + NVMe/USB I/O ≠ Force Off persist. Host/CI never print the iron marker.
pub const DURABLE_LUN_IO_RESIDUAL_NOTE: &str =
    "residual: DurableLun NVMe/USB I/O is not iron RAYNU-V-M8-DISK-PERSIST-OK; leftover DRAM remains the fallback; do not format PERC; do not print ISO-INSTALL-OK; do not F11";

/// How a candidate is attached to the platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LunTransport {
    /// PCI class `01:08` (NVMHCI).
    Nvme,
    /// USB mass-storage LUN (not the xHCI controller).
    Usb,
    /// PERC / MegaRAID / RAID — never a persist target.
    Perc,
    /// Boot ESP Cruzer / UDisk that already holds the ISO.
    EspCruzer,
    /// Everything else (AHCI optical, xHCI, NICs, …).
    Other,
}

/// Why [`pick_durable_lun`] returned no LUN.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LunReject {
    /// No USB/NVMe candidate at all.
    None,
    /// Only PERC / RAID was storage-class.
    Perc,
    /// Only the ESP Cruzer (ISO stick) was USB.
    EspCruzer,
    /// USB/NVMe existed but every one was below 1 GiB.
    TooSmall,
}

/// One firmware-visible disk or PCI storage function.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LunCandidate {
    pub transport: LunTransport,
    pub vendor: u16,
    pub device: u16,
    pub bus: u8,
    pub dev: u8,
    pub func: u8,
    /// 0 = unknown (PCI census without Identify / BlockIo).
    pub size_bytes: u64,
    /// True when this is the volume that loaded `BOOTX64.EFI`.
    pub is_esp_boot: bool,
}

impl LunCandidate {
    pub const fn pci(
        transport: LunTransport,
        vendor: u16,
        device: u16,
        bus: u8,
        dev: u8,
        func: u8,
        size_bytes: u64,
    ) -> Self {
        Self {
            transport,
            vendor,
            device,
            bus,
            dev,
            func,
            size_bytes,
            is_esp_boot: false,
        }
    }
}

static PICK_BDF: AtomicU32 = AtomicU32::new(0);
static PICK_BYTES: AtomicU64 = AtomicU64::new(0);
static PICK_TRANSPORT: AtomicU8 = AtomicU8::new(0);
static LAST_REJECT: AtomicU8 = AtomicU8::new(0);
static XHCI_BDF0: AtomicU32 = AtomicU32::new(0);
static XHCI_BDF1: AtomicU32 = AtomicU32::new(0);
static XHCI_N: AtomicU8 = AtomicU8::new(0);

fn bdf_pack(bus: u8, dev: u8, func: u8) -> u32 {
    (u32::from(bus) << 16) | (u32::from(dev) << 8) | u32::from(func)
}

fn transport_code(t: LunTransport) -> u8 {
    match t {
        LunTransport::Nvme => 1,
        LunTransport::Usb => 2,
        LunTransport::Perc => 3,
        LunTransport::EspCruzer => 4,
        LunTransport::Other => 0,
    }
}

fn reject_code(r: LunReject) -> u8 {
    match r {
        LunReject::None => 1,
        LunReject::Perc => 2,
        LunReject::EspCruzer => 3,
        LunReject::TooSmall => 4,
    }
}

/// PCI class `01:04`, LSI MegaRAID, or known Dell PERC IDs.
pub fn pci_is_perc(vendor: u16, device: u16, class: u8, subclass: u8) -> bool {
    if class == PCI_CLASS_STORAGE && subclass == PCI_SUBCLASS_RAID {
        return true;
    }
    if vendor == PCI_VENDOR_LSI && class == PCI_CLASS_STORAGE {
        return true;
    }
    if vendor == PCI_VENDOR_DELL && class == PCI_CLASS_STORAGE && subclass != PCI_SUBCLASS_NVME {
        match device {
            0x0014 | 0x0016 | 0x005D | 0x1F47 | 0x1F4B | 0x1F4C | 0x1F4D => return true,
            _ => {}
        }
    }
    let _ = device;
    false
}

/// Model string from lsblk / SMBIOS (host tests + flash scripts).
pub fn model_looks_perc(model: &str) -> bool {
    ascii_contains_ignore_case(model, "perc")
        || ascii_contains_ignore_case(model, "h740")
        || ascii_contains_ignore_case(model, "h730")
        || ascii_contains_ignore_case(model, "megaraid")
}

fn ascii_contains_ignore_case(hay: &str, needle: &str) -> bool {
    if needle.is_empty() || hay.len() < needle.len() {
        return needle.is_empty();
    }
    let h = hay.as_bytes();
    let n = needle.as_bytes();
    let mut i = 0;
    while i + n.len() <= h.len() {
        let mut ok = true;
        for (j, b) in n.iter().enumerate() {
            if h[i + j].to_ascii_lowercase() != b.to_ascii_lowercase() {
                ok = false;
                break;
            }
        }
        if ok {
            return true;
        }
        i += 1;
    }
    false
}

/// PCI class/subclass → transport. Does not look at size.
pub fn classify_pci_storage(vendor: u16, device: u16, class: u8, subclass: u8) -> LunTransport {
    if pci_is_perc(vendor, device, class, subclass) {
        return LunTransport::Perc;
    }
    if class == PCI_CLASS_STORAGE && subclass == PCI_SUBCLASS_NVME {
        return LunTransport::Nvme;
    }
    LunTransport::Other
}

/// USB LUN: ESP Cruzer vs a data stick. xHCI itself is [`LunTransport::Other`].
pub fn classify_usb_lun(_size_bytes: u64, is_esp_boot: bool) -> LunTransport {
    if is_esp_boot {
        LunTransport::EspCruzer
    } else {
        LunTransport::Usb
    }
}

/// Marketed 4 GB Cruzer / UDisk that already holds alpine-extended.
pub fn usb_is_esp_cruzer_window(size_bytes: u64) -> bool {
    size_bytes >= ESP_CRUZER_MIN_BYTES && size_bytes <= ESP_CRUZER_MAX_BYTES
}

fn rank(t: LunTransport) -> u8 {
    match t {
        LunTransport::Nvme => 2,
        LunTransport::Usb => 1,
        _ => 0,
    }
}

fn better(a: LunCandidate, b: Option<LunCandidate>) -> bool {
    let Some(b) = b else {
        return rank(a.transport) != 0;
    };
    let ra = rank(a.transport);
    let rb = rank(b.transport);
    if ra != rb {
        return ra > rb;
    }
    if a.size_bytes != b.size_bytes {
        return a.size_bytes > b.size_bytes;
    }
    bdf_pack(a.bus, a.dev, a.func) < bdf_pack(b.bus, b.dev, b.func)
}

/// Pick NVMe then USB ≥ 1 GiB. Never PERC. Never the ESP Cruzer.
///
/// Unknown-size NVMe (PCI census) is eligible so iron can name the BDF
/// before Identify exists. USB with `size_bytes == 0` is not.
pub fn pick_durable_lun(cands: &[LunCandidate]) -> Result<LunCandidate, LunReject> {
    let mut best: Option<LunCandidate> = None;
    let mut saw_perc = false;
    let mut saw_cruzer = false;
    let mut saw_small = false;
    for &c in cands {
        let transport = if c.is_esp_boot {
            LunTransport::EspCruzer
        } else {
            c.transport
        };
        match transport {
            LunTransport::Perc => {
                saw_perc = true;
                continue;
            }
            LunTransport::EspCruzer => {
                saw_cruzer = true;
                continue;
            }
            LunTransport::Other => continue,
            LunTransport::Nvme | LunTransport::Usb => {
                if transport == LunTransport::Usb && c.size_bytes == 0 {
                    continue;
                }
                if c.size_bytes != 0 && c.size_bytes < DURABLE_LUN_MIN_BYTES {
                    saw_small = true;
                    continue;
                }
                let cand = LunCandidate { transport, ..c };
                if better(cand, best) {
                    best = Some(cand);
                }
            }
        }
    }
    if let Some(b) = best {
        return Ok(b);
    }
    if saw_perc {
        return Err(LunReject::Perc);
    }
    if saw_cruzer {
        return Err(LunReject::EspCruzer);
    }
    if saw_small {
        return Err(LunReject::TooSmall);
    }
    Err(LunReject::None)
}

/// Virtio may use the LUN when NVMe or USB BOT I/O exists.
pub fn durable_lun_can_virtio_attach(pick: &LunCandidate, post_ebs_io: bool) -> bool {
    post_ebs_io
        && matches!(pick.transport, LunTransport::Nvme | LunTransport::Usb)
        && !pick.is_esp_boot
        && (pick.size_bytes == 0 || pick.size_bytes >= DURABLE_LUN_MIN_BYTES)
        && !(pick.transport == LunTransport::Usb && usb_is_esp_cruzer_window(pick.size_bytes))
}

/// NVMe Identify + I/O queues, or USB BOT after EBS.
pub fn durable_lun_post_ebs_io_ready() -> bool {
    crate::mgmt::nvme::nvme_io_ready() || crate::mgmt::usb_bot::usb_bot_io_ready()
}

/// True when virtio/BlockIo should read/write the LUN, not leftover DRAM.
pub fn durable_lun_serving() -> bool {
    durable_lun_post_ebs_io_ready()
}

/// Read or write the NVMe namespace, USB BOT LUN, or host-test Vec. Splits at 4 KiB.
pub fn durable_lun_rw(off: u64, buf: &mut [u8], write: bool) -> bool {
    if buf.is_empty() {
        return true;
    }
    if write {
        lun_cache_clear();
    }
    if !durable_lun_post_ebs_io_ready() {
        return false;
    }
    let nvme = crate::mgmt::nvme::nvme_io_ready();
    let lba = if nvme {
        crate::mgmt::nvme::nvme_lba_bytes()
    } else {
        crate::mgmt::usb_bot::usb_bot_lba_bytes()
    };
    let lba = if lba == 0 { 512 } else { lba };
    let mut done = 0usize;
    while done < buf.len() {
        let take = (buf.len() - done).min(4096);
        let cur = off.saturating_add(done as u64);
        if cur % u64::from(lba) != 0 || (take as u64) % u64::from(lba) != 0 {
            return false;
        }
        let slice = &mut buf[done..done + take];
        let ok = if cfg!(test) {
            if nvme {
                crate::mgmt::nvme::host_nvme_rw(cur, slice, write)
            } else {
                crate::mgmt::usb_bot::host_usb_rw(cur, slice, write)
            }
        } else if nvme {
            crate::mgmt::nvme::nvme_live_rw(cur, slice, write)
        } else {
            crate::mgmt::xhci::xhci_live_rw(cur, slice, write)
        };
        if !ok {
            return false;
        }
        done = done.saturating_add(take);
    }
    true
}

/// FAT keep-detect issues 32-byte dirents; each used to be a BOT command.
/// Sixteen 4 KiB lines cover GPT + ESP prefix + ext4 so a second `find_esp`
/// does not evict the first and wrap the 256-TRB xHCI event ring.
/// Not iron persist OK.
const LUN_CACHE_LINE: usize = 4096;
const LUN_CACHE_LINES: usize = 16;
static LUN_CACHE_TAG: [AtomicU64; LUN_CACHE_LINES] =
    [const { AtomicU64::new(u64::MAX) }; LUN_CACHE_LINES];
static LUN_CACHE: spinlock_cache::Bytes = spinlock_cache::Bytes::new();

mod spinlock_cache {
    use core::cell::UnsafeCell;
    pub struct Bytes(UnsafeCell<[u8; super::LUN_CACHE_LINE * super::LUN_CACHE_LINES]>);
    impl Bytes {
        pub const fn new() -> Self {
            Self(UnsafeCell::new([0; super::LUN_CACHE_LINE * super::LUN_CACHE_LINES]))
        }
        pub fn line_mut(&self, slot: usize) -> &mut [u8] {
            let off = slot * super::LUN_CACHE_LINE;
            // SAFETY: DurableLun I/O is single-threaded on BSP (LIVE_LOCK /
            // host --test-threads=1).
            unsafe {
                core::slice::from_raw_parts_mut(
                    (self.0.get() as *mut u8).add(off),
                    super::LUN_CACHE_LINE,
                )
            }
        }
        pub fn line(&self, slot: usize) -> &[u8] {
            let off = slot * super::LUN_CACHE_LINE;
            // SAFETY: same as line_mut — BSP / test-threads=1.
            unsafe {
                core::slice::from_raw_parts(
                    (self.0.get() as *const u8).add(off),
                    super::LUN_CACHE_LINE,
                )
            }
        }
    }
    // SAFETY: same as line_mut — BSP / test-threads=1.
    unsafe impl Sync for Bytes {}
}

fn lun_cache_clear() {
    for t in &LUN_CACHE_TAG {
        t.store(u64::MAX, Ordering::Release);
    }
}

fn lun_ns_bytes() -> u64 {
    if crate::mgmt::nvme::nvme_io_ready() {
        crate::mgmt::nvme::nvme_ns_bytes()
    } else {
        crate::mgmt::usb_bot::usb_bot_ns_bytes()
    }
}

fn lun_cache_slot(aligned: u64) -> usize {
    ((aligned / LUN_CACHE_LINE as u64) as usize) % LUN_CACHE_LINES
}

fn lun_cache_fill(aligned: u64, lba: u64) -> bool {
    let chunk = LUN_CACHE_LINE as u64;
    if lba == 0 || chunk % lba != 0 || aligned % lba != 0 {
        return false;
    }
    let ns = lun_ns_bytes();
    let mut n = chunk;
    if ns != 0 {
        if aligned >= ns {
            return false;
        }
        n = chunk.min(ns.saturating_sub(aligned));
        n = (n / lba) * lba;
        if n == 0 {
            return false;
        }
    }
    let slot = lun_cache_slot(aligned);
    let buf = LUN_CACHE.line_mut(slot);
    buf.fill(0);
    let mut got = 0u64;
    while got < n {
        let take = (n - got).min(lba) as usize;
        let slice = &mut buf[got as usize..got as usize + take];
        if !durable_lun_rw(aligned.saturating_add(got), slice, false) {
            LUN_CACHE_TAG[slot].store(u64::MAX, Ordering::Release);
            return false;
        }
        got = got.saturating_add(take as u64);
    }
    LUN_CACHE_TAG[slot].store(aligned, Ordering::Release);
    true
}

/// Peek any byte range (GPT/FAT). Rounds to LBA and copies the overlap.
pub fn durable_lun_read_any(off: u64, buf: &mut [u8]) -> bool {
    if buf.is_empty() {
        return true;
    }
    if !durable_lun_post_ebs_io_ready() {
        return false;
    }
    let lba = if crate::mgmt::nvme::nvme_io_ready() {
        crate::mgmt::nvme::nvme_lba_bytes()
    } else {
        crate::mgmt::usb_bot::usb_bot_lba_bytes()
    };
    let lba = if lba == 0 { 512u64 } else { u64::from(lba) };
    if lba == 0 || lba > 4096 {
        return false;
    }
    let chunk = LUN_CACHE_LINE as u64;
    if chunk % lba != 0 {
        return false;
    }
    let mut copied = 0usize;
    while copied < buf.len() {
        let cur = off.saturating_add(copied as u64);
        let aligned = (cur / chunk) * chunk;
        let slot = lun_cache_slot(aligned);
        if LUN_CACHE_TAG[slot].load(Ordering::Acquire) != aligned
            && !lun_cache_fill(aligned, lba)
        {
            return false;
        }
        let skip = (cur - aligned) as usize;
        if skip >= LUN_CACHE_LINE {
            return false;
        }
        let n = (LUN_CACHE_LINE - skip).min(buf.len() - copied);
        buf[copied..copied + n].copy_from_slice(&LUN_CACHE.line(slot)[skip..skip + n]);
        copied = copied.saturating_add(n);
    }
    true
}

/// Bring up NVMe I/O on the census pick. USB waits until after EBS.
pub fn init_durable_lun_io() {
    crate::mgmt::nvme::clear_nvme_ready();
    let Some((bus, dev, func, _, transport)) = durable_lun_pick() else {
        return;
    };
    if transport != LunTransport::Nvme {
        return;
    }
    match crate::mgmt::nvme::nvme_init_pci(bus, dev, func, DURABLE_LUN_MIN_BYTES) {
        Ok(bytes) => {
            crate::mgmt::nvme::reserve_durable_lun_install_disk(bytes);
            #[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
            {
                use crate::boot::serial;
                serial::write_str("boot: Stage 46 durable LUN nvme I/O ready bytes=");
                write_dec(bytes);
                serial::write_line(" (not ISO-INSTALL-OK)");
                serial_lun_peek("nvme");
            }
            let _ = (bus, dev, func);
        }
        Err(_) => {
            #[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
            {
                use crate::boot::serial;
                serial::write_str("boot: Stage 46 durable LUN nvme I/O fail err=");
                write_dec(u64::from(crate::mgmt::nvme::nvme_last_err()));
                serial::write_str(" bar=0x");
                write_hex64(crate::mgmt::nvme::nvme_last_bar());
                serial::write_str(" cap=0x");
                write_hex64(crate::mgmt::nvme::nvme_last_cap());
                serial::write_str(" asq=0x");
                write_hex64(crate::mgmt::nvme::nvme_last_asq());
                serial::write_str(" cpl=0x");
                write_hex64(crate::mgmt::nvme::nvme_last_cpl());
                serial::write_line(" (leftover DRAM; not ISO-INSTALL-OK)");
            }
        }
    }
}

/// Post-EBS USB BOT on a named xHCI. Skipped when NVMe I/O already serves.
pub fn init_durable_lun_usb_io() {
    if crate::mgmt::nvme::nvme_io_ready() {
        return;
    }
    crate::mgmt::usb_bot::clear_usb_bot_ready();
    let n = XHCI_N.load(Ordering::Acquire);
    if n == 0 {
        return;
    }
    let bdfs = [
        XHCI_BDF0.load(Ordering::Acquire),
        XHCI_BDF1.load(Ordering::Acquire),
    ];
    for packed in bdfs.iter().take(n as usize) {
        if *packed == 0 {
            continue;
        }
        let bus = (*packed >> 16) as u8;
        let dev = (*packed >> 8) as u8;
        let func = *packed as u8;
        match crate::mgmt::xhci::xhci_init_pci(bus, dev, func, DURABLE_LUN_MIN_BYTES) {
            Ok(bytes) => {
                if usb_is_esp_cruzer_window(bytes) {
                    crate::mgmt::usb_bot::clear_usb_bot_ready();
                    continue;
                }
                let pick = LunCandidate::pci(LunTransport::Usb, 0, 0, bus, dev, func, bytes);
                store_durable_lun_pick(Some(pick), LunReject::None);
                crate::mgmt::usb_bot::reserve_durable_lun_usb(bytes);
                #[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
                {
                    use crate::boot::serial;
                    serial::write_str("boot: Stage 46 durable LUN usb I/O ready bytes=");
                    write_dec(bytes);
                    serial::write_line(" (not ISO-INSTALL-OK)");
                    serial_lun_peek("usb");
                }
                return;
            }
            Err(_) => {
                #[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
                {
                    use crate::boot::serial;
                    serial::write_str("boot: Stage 46 durable LUN usb I/O fail err=");
                    write_dec(u64::from(crate::mgmt::usb_bot::usb_bot_last_err()));
                    serial::write_str(" bar=0x");
                    write_hex64(crate::mgmt::usb_bot::usb_bot_last_bar());
                    serial::write_str(" portsc=0x");
                    write_hex64(crate::mgmt::usb_bot::usb_bot_last_portsc());
                    serial::write_str(" cmpl=0x");
                    write_hex64(crate::mgmt::usb_bot::usb_bot_last_cmpl());
                    serial::write_line(" (leftover DRAM; not ISO-INSTALL-OK)");
                }
            }
        }
    }
}

/// True when NVMe or USB I/O reserved the install disk (skip leftover DRAM).
pub fn durable_lun_install_reserved() -> bool {
    crate::mgmt::nvme::durable_lun_install_reserved()
        || crate::mgmt::usb_bot::durable_lun_usb_reserved()
}

/// Take the reserved LUN size for virtio attach (NVMe first).
pub fn take_durable_lun_install_disk() -> Option<u64> {
    crate::mgmt::nvme::take_durable_lun_install_disk()
        .or_else(crate::mgmt::usb_bot::take_durable_lun_usb)
}

/// GPT peek + keep-detect after I/O ready (and TCG skip).
#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
pub fn durable_lun_serial_peek(tag: &str) {
    serial_lun_peek(tag);
}

#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
fn serial_lun_peek(tag: &str) {
    use crate::boot::serial;
    let mut sig = [0u8; 8];
    let peek = durable_lun_read_any(512, &mut sig);
    let (gpt, boot, ext4) = crate::mgmt::disk_persist::persist_lun_keep_parts();
    let inst = (gpt && boot && ext4)
        || crate::mgmt::disk_persist::persist_lun_sticky_keep();
    serial::write_str("boot: Stage 46 durable LUN peek ");
    serial::write_str(tag);
    serial::write_str(" efi=");
    if peek {
        for &b in &sig {
            if b.is_ascii_graphic() || b == b' ' {
                serial::write_byte(b);
            } else {
                serial::write_byte(b'?');
            }
        }
    } else {
        serial::write_str("read-fail err=");
        write_dec(u64::from(crate::mgmt::nvme::nvme_last_err()));
        serial::write_str(" cpl=0x");
        write_hex64(crate::mgmt::nvme::nvme_last_cpl());
    }
    serial::write_str(" gpt=");
    write_dec(u64::from(gpt));
    serial::write_str(" bootx64=");
    write_dec(u64::from(boot));
    serial::write_str(" ext4=");
    write_dec(u64::from(ext4));
    serial::write_str(" installed=");
    write_dec(u64::from(inst));
    serial::write_line(" (not ISO-INSTALL-OK)");
}

#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
fn write_dec(v: u64) {
    use crate::boot::serial;
    let mut n = v;
    let mut buf = [0u8; 20];
    let mut i = 20;
    if n == 0 {
        serial::write_byte(b'0');
        return;
    }
    while n > 0 && i > 0 {
        i -= 1;
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
    }
    for &b in &buf[i..] {
        serial::write_byte(b);
    }
}

/// True when census picked USB/NVMe. Does **not** mean virtio is on the LUN.
pub fn durable_lun_present() -> bool {
    match PICK_TRANSPORT.load(Ordering::Acquire) {
        1 | 2 => true,
        _ => false,
    }
}

/// Record a pick (probe / host tests). `None` clears.
pub fn store_durable_lun_pick(pick: Option<LunCandidate>, reject: LunReject) {
    LAST_REJECT.store(reject_code(reject), Ordering::Release);
    if let Some(p) = pick {
        PICK_BDF.store(bdf_pack(p.bus, p.dev, p.func), Ordering::Release);
        PICK_BYTES.store(p.size_bytes, Ordering::Release);
        PICK_TRANSPORT.store(transport_code(p.transport), Ordering::Release);
    } else {
        PICK_BDF.store(0, Ordering::Release);
        PICK_BYTES.store(0, Ordering::Release);
        PICK_TRANSPORT.store(0, Ordering::Release);
    }
}

/// Last pick, if any.
pub fn durable_lun_pick() -> Option<(u8, u8, u8, u64, LunTransport)> {
    let t = PICK_TRANSPORT.load(Ordering::Acquire);
    let transport = match t {
        1 => LunTransport::Nvme,
        2 => LunTransport::Usb,
        _ => return None,
    };
    let bdf = PICK_BDF.load(Ordering::Acquire);
    Some((
        (bdf >> 16) as u8,
        (bdf >> 8) as u8,
        bdf as u8,
        PICK_BYTES.load(Ordering::Acquire),
        transport,
    ))
}

/// Host tests.
pub fn durable_lun_clear() {
    store_durable_lun_pick(None, LunReject::None);
    XHCI_BDF0.store(0, Ordering::Release);
    XHCI_BDF1.store(0, Ordering::Release);
    XHCI_N.store(0, Ordering::Release);
    lun_cache_clear();
    crate::mgmt::disk_persist::persist_lun_clear_sticky();
    crate::mgmt::nvme::clear_nvme_ready();
    crate::mgmt::usb_bot::clear_usb_bot_ready();
}

/// Notes the mapper must keep: no PERC, Cruzer too small for ISO+disk, 1 GiB min.
pub fn durable_lun_policy_holds() -> bool {
    DURABLE_LUN_MIN_BYTES == LEFTOVER_DISK_TRY_BYTES[0]
        && PERC_UBUNTU_UNTOUCHED_NOTE.contains("PERC")
        && UDISK_TOO_SMALL_NOTE.contains("994 MiB")
        && DURABLE_LUN_IO_RESIDUAL_NOTE.contains("ISO-INSTALL-OK")
        && M8_DISK_PERSIST_OK_MARKER == "RAYNU-V-M8-DISK-PERSIST-OK"
        && !DURABLE_LUN_IO_RESIDUAL_NOTE.contains("println!")
}

/// PRE-EBS / post-EBS PCI census. Nested QEMU usually finds nothing.
#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
pub fn probe_durable_lun() {
    use crate::boot::serial;

    let mut cands = [LunCandidate::pci(LunTransport::Other, 0, 0, 0, 0, 0, 0); 16];
    let n = pci_storage_census(&mut cands);
    for c in cands[..n].iter() {
        serial::write_str("boot: Stage 46 durable LUN pci ");
        write_bdf(c.bus, c.dev, c.func);
        serial::write_str(" ");
        write_id(c.vendor, c.device);
        match c.transport {
            LunTransport::Nvme => serial::write_line(" nvme (not ISO-INSTALL-OK)"),
            LunTransport::Perc => serial::write_line(" skip PERC (not ISO-INSTALL-OK)"),
            LunTransport::Usb => serial::write_line(" usb (not ISO-INSTALL-OK)"),
            LunTransport::EspCruzer => serial::write_line(" skip ESP Cruzer (not ISO-INSTALL-OK)"),
            LunTransport::Other => serial::write_line(" skip (not ISO-INSTALL-OK)"),
        }
    }
    match pick_durable_lun(&cands[..n]) {
        Ok(p) => {
            store_durable_lun_pick(Some(p), LunReject::None);
            serial::write_str("boot: Stage 46 durable LUN picked ");
            match p.transport {
                LunTransport::Nvme => serial::write_str("nvme "),
                LunTransport::Usb => serial::write_str("usb "),
                _ => serial::write_str("? "),
            }
            write_bdf(p.bus, p.dev, p.func);
            serial::write_line(" (not ISO-INSTALL-OK)");
            // NVMe/USB I/O wait until after ExitBootServices so firmware
            // can disconnect its own NVMe/xHCI drivers (HCRST). Census only.
            if p.transport == LunTransport::Nvme {
                serial::write_line(" (nvme I/O after EBS; not ISO-INSTALL-OK)");
            } else if XHCI_N.load(Ordering::Acquire) != 0 {
                serial::write_line(" (usb I/O after EBS; not ISO-INSTALL-OK)");
            } else {
                serial::write_line(" (no post-EBS I/O; leftover DRAM; not ISO-INSTALL-OK)");
            }
        }
        Err(r) => {
            store_durable_lun_pick(None, r);
            match r {
                LunReject::Perc => {
                    serial::write_line(
                        "boot: Stage 46 durable LUN skip PERC Ubuntu (not ISO-INSTALL-OK)",
                    );
                }
                LunReject::EspCruzer => {
                    serial::write_line(
                        "boot: Stage 46 durable LUN skip ESP Cruzer (not ISO-INSTALL-OK)",
                    );
                }
                LunReject::TooSmall => {
                    serial::write_line(
                        "boot: Stage 46 durable LUN skip too small (not ISO-INSTALL-OK)",
                    );
                }
                LunReject::None => {
                    if XHCI_N.load(Ordering::Acquire) != 0 {
                        serial::write_line(
                            "boot: Stage 46 durable LUN xhci named (usb I/O after EBS; not ISO-INSTALL-OK)",
                        );
                    } else {
                        serial::write_line(
                            "boot: Stage 46 durable LUN none (leftover DRAM; not ISO-INSTALL-OK)",
                        );
                    }
                }
            }
        }
    }
}

#[cfg(not(all(target_os = "uefi", feature = "uefi-bin")))]
pub fn probe_durable_lun() {}

#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
fn write_bdf(bus: u8, dev: u8, func: u8) {
    use crate::boot::serial;
    serial::write_str("0x");
    write_hex8(bus);
    serial::write_str(":");
    write_hex8(dev);
    serial::write_str(".");
    write_hex8(func);
}

#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
fn write_id(vendor: u16, device: u16) {
    use crate::boot::serial;
    serial::write_str("id=");
    write_hex16(vendor);
    serial::write_str(":");
    write_hex16(device);
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
fn write_hex64(v: u64) {
    write_hex8((v >> 56) as u8);
    write_hex8((v >> 48) as u8);
    write_hex8((v >> 40) as u8);
    write_hex8((v >> 32) as u8);
    write_hex8((v >> 24) as u8);
    write_hex8((v >> 16) as u8);
    write_hex8((v >> 8) as u8);
    write_hex8(v as u8);
}

#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
fn store_xhci_bdf(bus: u8, dev: u8, func: u8) {
    let packed = bdf_pack(bus, dev, func);
    let n = XHCI_N.load(Ordering::Acquire);
    if n == 0 {
        XHCI_BDF0.store(packed, Ordering::Release);
        XHCI_N.store(1, Ordering::Release);
    } else if n == 1 && XHCI_BDF0.load(Ordering::Acquire) != packed {
        XHCI_BDF1.store(packed, Ordering::Release);
        XHCI_N.store(2, Ordering::Release);
    }
}
#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
fn pci_storage_census(out: &mut [LunCandidate]) -> usize {
    let mut n = 0usize;
    // Same window as HOST-NIC census: R640 / QEMU; do not scan 256.
    for bus in 0u8..=15 {
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
                let cc = pci_read32(bus, dev, func, 0x08);
                let class = (cc >> 24) as u8;
                let subclass = (cc >> 16) as u8;
                let prog = (cc >> 8) as u8;
                if class == PCI_CLASS_SERIAL
                    && subclass == PCI_SUBCLASS_USB
                    && prog == PCI_PROG_XHCI
                {
                    store_xhci_bdf(bus, dev, func);
                    {
                        use crate::boot::serial;
                        serial::write_str("boot: Stage 46 durable LUN xhci ");
                        write_bdf(bus, dev, func);
                        serial::write_str(" ");
                        write_id(vendor, device);
                        serial::write_line(" (usb I/O after EBS; not ISO-INSTALL-OK)");
                    }
                }
                if class != PCI_CLASS_STORAGE {
                    if func == 0 {
                        let ht = (pci_read32(bus, dev, func, 0x0C) >> 16) as u8;
                        if ht & 0x80 == 0 {
                            break;
                        }
                    }
                    continue;
                }
                if n < out.len() {
                    out[n] = LunCandidate::pci(
                        classify_pci_storage(vendor, device, class, subclass),
                        vendor,
                        device,
                        bus,
                        dev,
                        func,
                        0,
                    );
                    n += 1;
                }
                if func == 0 {
                    let ht = (pci_read32(bus, dev, func, 0x0C) >> 16) as u8;
                    if ht & 0x80 == 0 {
                        break;
                    }
                }
            }
        }
    }
    n
}

#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
fn pci_read32(bus: u8, dev: u8, func: u8, offset: u8) -> u32 {
    crate::mgmt::e1000_mmio::pci_read32(bus, dev, func, offset)
}

#[cfg(test)]
#[path = "durable_lun_test.rs"]
mod durable_lun_test;
