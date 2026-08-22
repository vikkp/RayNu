//! Guest-UEFI platform (i440FX-class) for OVMF PEI/DXE.
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** (ADR-002 / ADR-014)
//! VERIFICATION: L1 (runtime + host tests; QEMU is the DXE / CD-boot gate)
//!
//! Stage 40 stopped on EPT at `0xFCF8_F000` after unhandled CMOS `IN`
//! returned `0xFF` (PEI treated that as nearly 4 GiB of RAM). This module
//! serves honest CMOS memory size, QEMU fw_cfg RAM_SIZE, and an i440FX
//! host bridge so PEI can leave that stall. Not virtio-in-guest. Not
//! installer. Not Everest E5.

use core::sync::atomic::{AtomicBool, Ordering};

/// Must match [`crate::memory::ept_hw::GUEST_UEFI_LOW_RAM_BYTES`] (32 MiB).
/// Kept local so this module does not import the Proven Core EPT builder.
pub const PLATFORM_RAM_BYTES: u64 = 32 * 1024 * 1024;

/// i440FX host bridge at `00:00.0` (Intel 82441FX).
pub const HOST_BRIDGE_VENDOR: u16 = 0x8086;
pub const HOST_BRIDGE_DEVICE: u16 = 0x1237;

/// PIIX3 ISA / LPC at `00:01.0`.
pub const ISA_BRIDGE_VENDOR: u16 = 0x8086;
pub const ISA_BRIDGE_DEVICE: u16 = 0x7000;

pub const FW_CFG_SELECTOR_PORT: u16 = 0x0510;
pub const FW_CFG_DATA_PORT: u16 = 0x0511;
pub const CMOS_INDEX_PORT: u16 = 0x70;
pub const CMOS_DATA_PORT: u16 = 0x71;

const FW_CFG_SIGNATURE: u16 = 0x00;
const FW_CFG_ID: u16 = 0x01;
const FW_CFG_RAM_SIZE: u16 = 0x03;
const FW_CFG_NB_CPUS: u16 = 0x05;
const FW_CFG_MACHINE_ID: u16 = 0x06;
const FW_CFG_MAX_CPUS: u16 = 0x0F;
const FW_CFG_FILE_DIR: u16 = 0x19;

/// Memory above 16 MiB in 64 KiB chunks (CMOS 0x34/0x35).
pub fn cmos_above_16m_chunks(ram_bytes: u64) -> u16 {
    const SIXTEEN: u64 = 16 * 1024 * 1024;
    const CHUNK: u64 = 64 * 1024;
    if ram_bytes <= SIXTEEN {
        0
    } else {
        ((ram_bytes - SIXTEEN) / CHUNK) as u16
    }
}

/// Extended memory 1–16 MiB, in KiB (CMOS 0x17/0x18 / 0x30/0x31).
pub fn cmos_extended_kb(ram_bytes: u64) -> u16 {
    const ONE_MIB: u64 = 1024 * 1024;
    const SIXTEEN: u64 = 16 * 1024 * 1024;
    if ram_bytes <= ONE_MIB {
        0
    } else {
        let top = core::cmp::min(ram_bytes, SIXTEEN);
        ((top - ONE_MIB) / 1024) as u16
    }
}

pub fn is_cmos_port(port: u16) -> bool {
    port == CMOS_INDEX_PORT || port == CMOS_DATA_PORT
}

pub fn is_fwcfg_port(port: u16) -> bool {
    port == FW_CFG_SELECTOR_PORT || port == FW_CFG_DATA_PORT || (0x0514..=0x051B).contains(&port)
}

pub fn is_timer_port(port: u16) -> bool {
    (0x40..=0x43).contains(&port) || port == 0x61 || port == 0x80 || port == 0x92
}

pub fn is_platform_io_port(port: u16) -> bool {
    is_cmos_port(port) || is_fwcfg_port(port) || is_timer_port(port)
}

/// High MMIO / leftover “RAM” PEI walks when CMOS is wrong, plus APIC/HPET.
pub fn is_platform_sink_gpa(gpa: u64) -> bool {
    const GIB: u64 = 1 << 30;
    const MMIO_LO: u64 = 0xF000_0000;
    const FW_FLOOR: u64 = 0xFFC0_0000;
    const IOAPIC: u64 = 0xFEC0_0000;
    const APIC_TOP: u64 = 0xFEF0_0000;
    (gpa >= PLATFORM_RAM_BYTES && gpa < GIB)
        || (gpa >= MMIO_LO && gpa < FW_FLOOR)
        || (gpa >= IOAPIC && gpa < APIC_TOP)
}

pub fn pci_bdf(addr: u32) -> (u8, u8, u8, u8) {
    let bus = ((addr >> 16) & 0xff) as u8;
    let dev = ((addr >> 11) & 0x1f) as u8;
    let fun = ((addr >> 8) & 7) as u8;
    let off = (addr & 0xfc) as u8;
    (bus, dev, fun, off)
}

pub fn pci_addr_selects_host(addr: u32) -> bool {
    if (addr & 0x8000_0000) == 0 {
        return false;
    }
    let (bus, dev, fun, _) = pci_bdf(addr);
    bus == 0 && dev == 0 && fun == 0
}

pub fn pci_addr_selects_isa(addr: u32) -> bool {
    if (addr & 0x8000_0000) == 0 {
        return false;
    }
    let (bus, dev, fun, _) = pci_bdf(addr);
    bus == 0 && dev == 1 && fun == 0
}

struct Platform {
    pci_addr: u32,
    cmos_idx: u8,
    cmos: [u8; 128],
    fw_sel: u16,
    fw_off: u16,
    fw_buf: [u8; 16],
    fw_len: u8,
    pit: u16,
    port61: u8,
}

impl Platform {
    const fn empty() -> Self {
        Self {
            pci_addr: 0,
            cmos_idx: 0,
            cmos: [0u8; 128],
            fw_sel: 0,
            fw_off: 0,
            fw_buf: [0u8; 16],
            fw_len: 0,
            pit: 0xFFFF,
            port61: 0x10,
        }
    }
}

// JUSTIFICATION: one guest-UEFI platform; firmware is single-threaded after EBS.
// Host tests take the spinlock.
struct GuestPlat(core::cell::UnsafeCell<Platform>);

// SAFETY: exclusive access is enforced by `PLAT_LOCK`.
// KANI-TARGET: guest-UEFI platform mutex (outside Proven Core).
unsafe impl Sync for GuestPlat {}

static PLAT: GuestPlat = GuestPlat(core::cell::UnsafeCell::new(Platform::empty()));
static PLAT_LOCK: AtomicBool = AtomicBool::new(false);
static CMOS_MEM: AtomicBool = AtomicBool::new(false);
static FWCFG_RAM: AtomicBool = AtomicBool::new(false);
static HOST_ENUM: AtomicBool = AtomicBool::new(false);

fn with_plat<R>(f: impl FnOnce(&mut Platform) -> R) -> R {
    while PLAT_LOCK.swap(true, Ordering::Acquire) {
        core::hint::spin_loop();
    }
    // SAFETY: lock held; exclusive mutable access.
    // KANI-TARGET: guest-UEFI platform mutex (outside Proven Core).
    let out = unsafe {
        let p = &mut *PLAT.0.get();
        if p.cmos[0x0D] == 0 && p.cmos[0x15] == 0 && p.cmos[0x35] == 0 {
            fill_cmos(&mut p.cmos);
        }
        f(p)
    };
    PLAT_LOCK.store(false, Ordering::Release);
    out
}

fn fill_cmos(c: &mut [u8; 128]) {
    c.fill(0);
    c[0x00] = 0x00;
    c[0x02] = 0x00;
    c[0x04] = 0x12;
    c[0x06] = 0x06;
    c[0x07] = 0x16;
    c[0x08] = 0x08;
    c[0x09] = 0x26;
    c[0x0A] = 0x26;
    c[0x0B] = 0x02;
    c[0x0D] = 0x80;
    // Base memory 640 KiB.
    c[0x15] = 0x80;
    c[0x16] = 0x02;
    let ext = cmos_extended_kb(PLATFORM_RAM_BYTES).to_le_bytes();
    c[0x17] = ext[0];
    c[0x18] = ext[1];
    c[0x30] = ext[0];
    c[0x31] = ext[1];
    c[0x32] = 0x20;
    let above = cmos_above_16m_chunks(PLATFORM_RAM_BYTES).to_le_bytes();
    c[0x34] = above[0];
    c[0x35] = above[1];
}

fn select_fwcfg(p: &mut Platform, sel: u16) {
    p.fw_sel = sel;
    p.fw_off = 0;
    p.fw_buf.fill(0);
    match sel {
        FW_CFG_SIGNATURE => {
            p.fw_buf[..4].copy_from_slice(b"QEMU");
            p.fw_len = 4;
        }
        FW_CFG_ID => {
            p.fw_buf[..4].copy_from_slice(&1u32.to_le_bytes());
            p.fw_len = 4;
        }
        FW_CFG_RAM_SIZE => {
            p.fw_buf[..8].copy_from_slice(&PLATFORM_RAM_BYTES.to_le_bytes());
            p.fw_len = 8;
            FWCFG_RAM.store(true, Ordering::Release);
        }
        FW_CFG_NB_CPUS | FW_CFG_MAX_CPUS => {
            p.fw_buf[..2].copy_from_slice(&1u16.to_le_bytes());
            p.fw_len = 2;
        }
        FW_CFG_MACHINE_ID => {
            p.fw_buf[..4].copy_from_slice(&1u32.to_le_bytes());
            p.fw_len = 4;
        }
        FW_CFG_FILE_DIR => {
            p.fw_len = 4;
        }
        _ => {
            p.fw_len = 0;
        }
    }
}

fn note_cmos_mem(idx: u8) {
    if matches!(
        idx,
        0x17 | 0x18 | 0x30 | 0x31 | 0x34 | 0x35 | 0x5B | 0x5C | 0x5D
    ) {
        CMOS_MEM.store(true, Ordering::Release);
    }
}

pub fn reset() {
    with_plat(|p| {
        *p = Platform::empty();
        fill_cmos(&mut p.cmos);
    });
    CMOS_MEM.store(false, Ordering::Release);
    FWCFG_RAM.store(false, Ordering::Release);
    HOST_ENUM.store(false, Ordering::Release);
}

pub fn cmos_mem_served() -> bool {
    CMOS_MEM.load(Ordering::Acquire)
}

pub fn fwcfg_ram_served() -> bool {
    FWCFG_RAM.load(Ordering::Acquire)
}

pub fn host_bridge_enumerated() -> bool {
    HOST_ENUM.load(Ordering::Acquire)
}

/// Honest platform-memory evidence: CMOS size or fw_cfg RAM_SIZE was read.
pub fn platform_memory_served() -> bool {
    cmos_mem_served() || fwcfg_ram_served()
}

pub fn pci_write_addr(addr: u32) {
    with_plat(|p| p.pci_addr = addr);
}

pub fn pci_read_addr() -> u32 {
    with_plat(|p| p.pci_addr)
}

fn host_dword(off: u8) -> u32 {
    match off {
        0x00 => u32::from(HOST_BRIDGE_VENDOR) | (u32::from(HOST_BRIDGE_DEVICE) << 16),
        0x04 => 0x0000_0006,
        0x08 => 0x0600_0000,
        0x0C => 0x0000_0000,
        _ => 0,
    }
}

fn isa_dword(off: u8) -> u32 {
    match off {
        0x00 => u32::from(ISA_BRIDGE_VENDOR) | (u32::from(ISA_BRIDGE_DEVICE) << 16),
        0x04 => 0x0000_0007,
        0x08 => 0x0601_0000,
        0x0C => 0x0000_0000,
        _ => 0,
    }
}

fn shift_dword(dword: u32, off: u8, size: u8) -> u32 {
    let shift = (off & 3) * 8;
    let shifted = dword >> shift;
    match size {
        1 => shifted & 0xff,
        2 => shifted & 0xffff,
        _ => shifted,
    }
}

/// `Some` when this BDF is the host or ISA bridge.
pub fn pci_read_data(port: u16, size: u8) -> Option<u32> {
    with_plat(|p| {
        let addr = p.pci_addr;
        let off = (addr as u8 & 0xFC).wrapping_add((port.wrapping_sub(0xCFC)) as u8);
        let aligned = off & 0xFC;
        if pci_addr_selects_host(addr) {
            if aligned == 0 {
                HOST_ENUM.store(true, Ordering::Release);
            }
            Some(shift_dword(host_dword(aligned), off, size))
        } else if pci_addr_selects_isa(addr) {
            Some(shift_dword(isa_dword(aligned), off, size))
        } else {
            None
        }
    })
}

pub fn pci_write_data(port: u16, _size: u8, _val: u32) {
    let _ = port;
}

fn io_mask(size: u8) -> u64 {
    match size {
        1 => 0xff,
        2 => 0xffff,
        _ => 0xffff_ffff,
    }
}

/// Platform PIO. Returns the value to merge into RAX.
pub fn io(port: u16, is_in: bool, size: u8, rax: u64) -> u64 {
    let mask = io_mask(size);
    with_plat(|p| {
        if is_cmos_port(port) {
            if port == CMOS_INDEX_PORT {
                if is_in {
                    return (rax & !mask) | (u64::from(p.cmos_idx) & mask);
                }
                p.cmos_idx = (rax as u8) & 0x7F;
                return rax;
            }
            let idx = p.cmos_idx;
            if is_in {
                note_cmos_mem(idx);
                let v = p.cmos[idx as usize] as u64;
                return (rax & !mask) | (v & mask);
            }
            p.cmos[idx as usize] = rax as u8;
            return rax;
        }
        if is_fwcfg_port(port) {
            if port == FW_CFG_SELECTOR_PORT {
                if is_in {
                    return (rax & !mask) | (u64::from(p.fw_sel) & mask);
                }
                select_fwcfg(p, rax as u16);
                return rax;
            }
            if port == FW_CFG_DATA_PORT {
                if is_in {
                    let mut v = 0u64;
                    for i in 0..size {
                        let b = if (p.fw_off as usize) < (p.fw_len as usize) {
                            let b = p.fw_buf[p.fw_off as usize];
                            p.fw_off = p.fw_off.saturating_add(1);
                            b
                        } else {
                            0
                        };
                        v |= u64::from(b) << (8 * i);
                    }
                    return (rax & !mask) | (v & mask);
                }
                return rax;
            }
            // DMA ports: traditional-only (ID bit1 clear). RAZ/WI.
            if is_in {
                return rax & !mask;
            }
            return rax;
        }
        if is_in {
            let val = match port {
                0x61 => {
                    p.port61 ^= 0x10;
                    u64::from(p.port61)
                }
                0x80 => 0,
                0x92 => 0x02,
                0x40 => {
                    let v = p.pit;
                    p.pit = p.pit.wrapping_sub(0x40);
                    u64::from(v as u8)
                }
                0x41..=0x43 => 0,
                _ => mask,
            };
            (rax & !mask) | (val & mask)
        } else {
            if port == 0x61 {
                p.port61 = (rax as u8 & !0x10) | (p.port61 & 0x10);
            } else if port == 0x40 {
                p.pit = (rax as u16) | 0x00FF;
            } else if port == 0x43 {
                p.pit = 0xFFFF;
            }
            rax
        }
    })
}

#[cfg(test)]
#[path = "guest_platform_test.rs"]
mod guest_platform_test;
