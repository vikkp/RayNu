//! Guest-UEFI platform (i440FX-class) for OVMF PEI/DXE.
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** (ADR-002 / ADR-014)
//! VERIFICATION: L1 (runtime + host tests; QEMU is the DXE / CD-boot gate)
//!
//! Stage 40 stopped on EPT at `0xFCF8_F000` after unhandled CMOS `IN`
//! returned `0xFF` (PEI treated that as nearly 4 GiB of RAM). This module
//! serves honest CMOS memory size, QEMU fw_cfg RAM_SIZE, an i440FX
//! host bridge at `00:08.0`, PIIX3 ISA at `00:01.0` (multifunction),
//! PIIX4 PM at `00:01.3`, fw_cfg `bootorder` (CD then virtio disk),
//! fw_cfg `etc/e820` (32 MiB RAM), fw_cfg `etc/boot-menu-wait` 0 ms
//! (skip BdsWait), 8259 PIC RAZ/WI, and a
//! 24-bit ACPI PM timer (port 0 dword + PIIX `0x408` + programmed PMBA).
//! Nested VT-x `20763e4`: 4 MiB flash + empty VARS `_FVH` stopped the
//! `0xFFC00000` EPT, then QEMU hit the 300 s kill with no `stop n=`
//! (no `00:00.1`). Nested VT-x `105ffbe`: live HPET + preemption hit
//! the 2048 cap (`reason=0x34` `rip=0x6e812d` `pci_ide=0`) because
//! `HPET_MAIN_STEP` was ~10 ms per VMEXIT. 1 s of HPET time per
//! **preemption/HLT/HPET-EPT** exit so Delay can finish without burning
//! the cap. Nested VT-x `5d9e346`: BOTH-OK then n=8192 `ataio=0`
//! `unh=3` `port=0xcf8` after the empty-slot walk; 1 s per PCI I/O
//! made guest time jump ~2 h before AtaAtapiPassThru Start. PCI I/O
//! does not advance HPET. 8042 `0x60`/`0x64` (unhandled `IN` was
//! `0xFF` / IBF stuck). Nested VT-x `2674629`: 32768 cap then
//! `acpi=16612` `port=0` `ataio=0` (BdsWait; BootMenu 0x0e was 0).
//! Nested VT-x `8e55abf` stop `cf8=0x80000838`
//! is PIIX ISA `00:01.0` offset `0x38` (PciBus programming, not
//! empty-slot scan). PIRQ `0x60-0x63` reset `0x80` (disabled) matches
//! QEMU so IRQ assign is not IRQ0.

use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering};

/// Must match [`crate::memory::ept_hw::GUEST_UEFI_LOW_RAM_BYTES`] (32 MiB).
/// Kept local so this module does not import the Proven Core EPT builder.
pub const PLATFORM_RAM_BYTES: u64 = 32 * 1024 * 1024;

/// i440FX host bridge at `00:08.0` (Intel 82441FX).
/// Nested VT-x: PEI only `inw` Device ID of `00:00.0` — never Header Type,
/// never another BDF. Slot 0 is virtio-blk so that probe can enum it.
/// Host stays here (Stage 41 parking).
pub const HOST_BRIDGE_VENDOR: u16 = 0x8086;
pub const HOST_BRIDGE_DEVICE: u16 = 0x1237;
pub const HOST_BRIDGE_DEV: u8 = 8;
pub const HOST_BRIDGE_FN: u8 = 0;

/// PIIX3 ISA / LPC at `00:01.0`.
pub const ISA_BRIDGE_VENDOR: u16 = 0x8086;
pub const ISA_BRIDGE_DEVICE: u16 = 0x7000;

/// PIIX4 PM/ACPI at `00:01.3` (Intel 82371AB). OVMF programs PMBA here
/// after the i440FX host-bridge switch. DID remap lets that switch match
/// virtio `0x1042` at `00:00.0` without faking the Device ID read.
pub const PM_BRIDGE_VENDOR: u16 = 0x8086;
pub const PM_BRIDGE_DEVICE: u16 = 0x7113;
pub const PM_BRIDGE_DEV: u8 = 1;
pub const PM_BRIDGE_FN: u8 = 3;
/// Default PIIX4 PMBA (IO bit set). Timer is at `PMBA&~1 + 8` = `0x408`.
pub const PIIX4_PMBA_DEFAULT: u32 = 0x401;

/// PCI Header Type (config dword `0x0C` bits 23:16). Bit 7 = multifunction.
pub const PCI_HEADER_MULTIFUNCTION: u32 = 0x0080_0000;

pub const FW_CFG_SELECTOR_PORT: u16 = 0x0510;
pub const FW_CFG_DATA_PORT: u16 = 0x0511;
pub const CMOS_INDEX_PORT: u16 = 0x70;
pub const CMOS_DATA_PORT: u16 = 0x71;

const FW_CFG_SIGNATURE: u16 = 0x00;
const FW_CFG_ID: u16 = 0x01;
const FW_CFG_RAM_SIZE: u16 = 0x03;
const FW_CFG_NB_CPUS: u16 = 0x05;
const FW_CFG_MACHINE_ID: u16 = 0x06;
/// QEMU `FW_CFG_BOOT_MENU`. OVMF `GetFrontPageTimeoutFromQemu` treats 0 as
/// menu=off and returns `PcdPlatformBootTimeOut` (often 5 s) — nested
/// VT-x `2674629` BdsWait. 1 = menu=on, then `etc/boot-menu-wait`.
pub const FW_CFG_BOOT_MENU: u16 = 0x0E;
const FW_CFG_MAX_CPUS: u16 = 0x0F;
const FW_CFG_FILE_DIR: u16 = 0x19;
/// First named fw_cfg file selector (QEMU `FW_CFG_FILE_FIRST`).
pub const FW_CFG_BOOTORDER_SEL: u16 = 0x20;
/// Second named file: OVMF `PlatformScanE820` (`etc/e820`).
pub const FW_CFG_E820_SEL: u16 = 0x21;
/// Third named file: OVMF splash-time (UINT16 LE milliseconds).
pub const FW_CFG_BOOT_WAIT_SEL: u16 = 0x22;
/// Named files in the fw_cfg directory (`bootorder`, `etc/e820`, `etc/boot-menu-wait`).
pub const FW_CFG_NAMED_FILE_COUNT: u32 = 3;
/// QEMU `-boot menu=on,splash-time=0`. `(0+999)/1000` → 0 s FrontPage wait.
pub const BOOT_MENU_WAIT: [u8; 2] = [0, 0];
/// Packed QEMU e820 entry size (`address:u64`, `length:u64`, `type:u32`).
pub const E820_ENTRY_BYTES: u8 = 20;
/// QEMU `E820_RAM`.
pub const E820_RAM: u32 = 1;

/// QEMU `bootorder` (OFW paths). PIIX IDE `00:01.1` first (`ide@1,1`), then
/// virtio-fn1 `00:00.1` (`ide@0,1`), then virtio disk (`scsi@0`).
/// Nested VT-x BOTH-OK with `ataio=0`: ConnectDevicesFromQemu of `scsi@0`
/// enumerated IDE fn1 as a sibling and did not Start AtaAtapiPassThru.
/// QEMU/OVMF TranslatePciOfwNodes: `ide@1,1/drive@0/disk@0` →
/// `PciRoot(0x0)/Pci(0x1,0x1)/Ata(Primary,Master,0x0)`. Master is `DEV=0`.
pub const BOOTORDER: &[u8] =
    b"/pci@i0cf8/ide@1,1/drive@0/disk@0\n/pci@i0cf8/ide@0,1/drive@0/disk@0\n/pci@i0cf8/scsi@0/disk@0,0\n";

/// Product boot order is CD (PIIX then virtio-fn1) then virtio disk (ADR-014).
pub fn boot_order_cd_then_disk() -> bool {
    let piix = find_bytes(BOOTORDER, b"ide@1,1/drive@0");
    let fn1 = find_bytes(BOOTORDER, b"ide@0,1/drive@0");
    let slave = find_bytes(BOOTORDER, b"drive@1");
    let disk = find_bytes(BOOTORDER, b"scsi@0");
    match (piix, fn1, slave, disk) {
        (Some(p), Some(f), None, Some(d)) => p < f && f < d,
        _ => false,
    }
}

/// True when splash-time is 0 ms so OVMF skips BdsWait / FrontPage delay.
pub fn boot_menu_wait_skips_bds() -> bool {
    u16::from_le_bytes(BOOT_MENU_WAIT) == 0
}

fn find_bytes(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || hay.len() < needle.len() {
        return None;
    }
    for i in 0..=hay.len() - needle.len() {
        if &hay[i..i + needle.len()] == needle {
            return Some(i);
        }
    }
    None
}

fn file_entry_byte(i: usize, size: u32, sel: u16, name: &[u8]) -> u8 {
    match i {
        0..=3 => size.to_be_bytes()[i],
        4..=5 => sel.to_be_bytes()[i - 4],
        6 | 7 => 0,
        8..=63 => {
            let ni = i - 8;
            if ni < name.len() {
                name[ni]
            } else {
                0
            }
        }
        _ => 0,
    }
}

fn file_dir_byte(off: u16) -> u8 {
    const ENTRY: usize = 64;
    let o = off as usize;
    if o < 4 {
        return FW_CFG_NAMED_FILE_COUNT.to_be_bytes()[o];
    }
    let body = o - 4;
    let ent = body / ENTRY;
    let i = body % ENTRY;
    match ent {
        0 => file_entry_byte(
            i,
            BOOTORDER.len() as u32,
            FW_CFG_BOOTORDER_SEL,
            b"bootorder",
        ),
        1 => file_entry_byte(i, u32::from(E820_ENTRY_BYTES), FW_CFG_E820_SEL, b"etc/e820"),
        2 => file_entry_byte(
            i,
            BOOT_MENU_WAIT.len() as u32,
            FW_CFG_BOOT_WAIT_SEL,
            b"etc/boot-menu-wait",
        ),
        _ => 0,
    }
}

/// One packed QEMU e820 RAM entry covering [`PLATFORM_RAM_BYTES`].
pub fn e820_byte(off: u16) -> u8 {
    let o = off as usize;
    if o < 8 {
        0
    } else if o < 16 {
        PLATFORM_RAM_BYTES.to_le_bytes()[o - 8]
    } else if o < 20 {
        E820_RAM.to_le_bytes()[o - 16]
    } else {
        0
    }
}

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

/// i8042 keyboard controller. Nested VT-x `5d9e346` `IN 0x64`/`0x60`
/// was unhandled `0xFF` (IBF stuck). Status `0x10` = sys flag, buffers empty.
pub fn is_kbc_port(port: u16) -> bool {
    port == 0x60 || port == 0x64
}

/// 8259 PIC + ELCR. Unhandled `IN` was `0xFF` (all IRQs in service).
pub fn is_pic_port(port: u16) -> bool {
    matches!(port, 0x20 | 0x21 | 0xA0 | 0xA1 | 0x4D0 | 0x4D1)
}

/// OVMF `AcpiTimerLibConstructor` leaves `mAcpiTimerIoAddr=0` when `00:00.0`
/// DID is not i440FX/Q35. PEI then `IoRead32(0)` in `InternalAcpiDelay`.
/// PIIX4 PMBA timer is `0x408`. Q35/ICH9 default is `0xB008`.
/// Programmed PMBA+8 is also a timer (after firmware writes `00:01.3`).
pub fn is_acpi_pm_timer_io(port: u16, size: u8) -> bool {
    if acpi_pm_timer_fixed(port, size) {
        return true;
    }
    with_plat(|p| acpi_pm_timer_pmba(port, p.pmba))
}

fn acpi_pm_timer_fixed(port: u16, size: u8) -> bool {
    (port == 0 && size == 4) || (0x408..=0x40B).contains(&port) || (0xB008..=0xB00B).contains(&port)
}

fn acpi_pm_timer_pmba(port: u16, pmba: u32) -> bool {
    let base = (pmba & !1).wrapping_add(8);
    let p32 = u32::from(port);
    p32 >= base && p32 < base.wrapping_add(4)
}

fn acpi_pm_timer_matches(port: u16, size: u8, pmba: u32) -> bool {
    acpi_pm_timer_fixed(port, size) || acpi_pm_timer_pmba(port, pmba)
}

/// PIIX4 PM I/O block (64 bytes at PMBA). Timer is at +8; other regs RAZ/WI.
pub fn is_piix_pm_io(port: u16) -> bool {
    with_plat(|p| is_piix_pm_io_port(port, p.pmba))
}

fn is_piix_pm_io_port(port: u16, pmba: u32) -> bool {
    let base = pmba & !1;
    let p32 = u32::from(port);
    p32 >= base && p32 < base.wrapping_add(64)
}

fn acpi_pm_timer_shift(port: u16, pmba: u32) -> u32 {
    if port == 0 {
        return 0;
    }
    let base = (pmba & !1).wrapping_add(8);
    let p32 = u32::from(port);
    if p32 >= base && p32 < base.wrapping_add(4) {
        return (p32 - base) * 8;
    }
    if (0x408..=0x40B).contains(&port) {
        return u32::from(port - 0x408) * 8;
    }
    if (0xB008..=0xB00B).contains(&port) {
        return u32::from(port - 0xB008) * 8;
    }
    0
}

pub fn is_platform_io_port(port: u16) -> bool {
    is_cmos_port(port)
        || is_fwcfg_port(port)
        || is_timer_port(port)
        || is_pic_port(port)
        || is_kbc_port(port)
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

/// QEMU/ICH HPET MMIO. Lives in the 2 MiB sink page at [`HPET_SINK_PAGE`].
pub const HPET_GPA: u64 = 0xFED0_0000;
pub const HPET_SINK_PAGE: u64 = 0xFEC0_0000;
pub const HPET_SINK_OFF: usize = (HPET_GPA - HPET_SINK_PAGE) as usize;
/// Rev 1, 3 timers, 64-bit, Intel vendor — same shape as QEMU `hpet.c`.
pub const HPET_CAP_REV: u32 = 0x8086_A201;
/// 10 ns period in femtoseconds.
pub const HPET_CLK_PERIOD_FS: u32 = 10_000_000;
/// ~1 s of HPET time (10 ns ticks) on preemption / HLT / HPET-EPT.
/// Nested VT-x `105ffbe` burned n=1024–2048 at `rip=0x6e812d` with
/// 1e6 ticks (~10 ms) per exit — Delay never finished. Do **not**
/// apply this on PCI config I/O (`5d9e346` n=8192 `ataio=0`).
pub const HPET_MAIN_STEP: u64 = 100_000_000;

/// HPET GPA in the sink page (for EPT-exit classification).
pub fn is_hpet_gpa(gpa: u64) -> bool {
    (gpa & !0xFFFu64) == HPET_GPA
}

/// Stamp a live HPET into the 2 MiB platform sink (offset [`HPET_SINK_OFF`]).
///
/// INVARIANTS:
/// - Writes only when `sink.len()` covers HPET config + main counter
/// - Does not fake PCI enum
pub fn hpet_init_sink(sink: &mut [u8]) -> bool {
    if sink.len() < HPET_SINK_OFF + 0xF8 {
        return false;
    }
    let h = HPET_SINK_OFF;
    sink[h..h + 4].copy_from_slice(&HPET_CAP_REV.to_le_bytes());
    sink[h + 4..h + 8].copy_from_slice(&HPET_CLK_PERIOD_FS.to_le_bytes());
    sink[h + 0x10..h + 0x14].copy_from_slice(&1u32.to_le_bytes());
    true
}

/// Advance HPET main counter by [`HPET_MAIN_STEP`] (preemption/HLT path).
pub fn hpet_tick_sink(sink: &mut [u8]) -> u64 {
    hpet_tick_sink_by(sink, HPET_MAIN_STEP)
}

/// Advance HPET main counter by `step` (0 = PCI I/O exits keep guest time).
pub fn hpet_tick_sink_by(sink: &mut [u8], step: u64) -> u64 {
    if sink.len() < HPET_SINK_OFF + 0xF8 {
        return 0;
    }
    let off = HPET_SINK_OFF + 0xF0;
    let mut cur = [0u8; 8];
    cur.copy_from_slice(&sink[off..off + 8]);
    let v = u64::from_le_bytes(cur).wrapping_add(step);
    sink[off..off + 8].copy_from_slice(&v.to_le_bytes());
    v
}

pub fn pci_bdf(addr: u32) -> (u8, u8, u8, u8) {
    let bus = ((addr >> 16) & 0xff) as u8;
    let dev = ((addr >> 11) & 0x1f) as u8;
    let fun = ((addr >> 8) & 7) as u8;
    let off = (addr & 0xfc) as u8;
    (bus, dev, fun, off)
}

/// Byte offset in the 256-byte PCI config space.
///
/// QEMU `pci_host_data_read` uses `config_reg | (data_port & 3)`.
/// OVMF often writes CF8 with register `0x0E` (Header Type) and `inb(0xCFC)`.
/// Masking CF8 to `0xFC` first returns Cache Line Size (`0`) instead of `0x80`,
/// so firmware treats PIIX3 as single-function and never scans `00:01.1`.
pub fn pci_cfg_offset(addr: u32, port: u16) -> u8 {
    let data = u32::from(port.wrapping_sub(0xCFC) & 3);
    ((addr | data) & 0xff) as u8
}

pub fn host_pci_config_addr() -> u32 {
    0x8000_0000 | (u32::from(HOST_BRIDGE_DEV) << 11) | (u32::from(HOST_BRIDGE_FN) << 8)
}

pub fn pci_addr_selects_host(addr: u32) -> bool {
    if (addr & 0x8000_0000) == 0 {
        return false;
    }
    let (bus, dev, fun, _) = pci_bdf(addr);
    bus == 0 && dev == HOST_BRIDGE_DEV && fun == HOST_BRIDGE_FN
}

pub fn pci_addr_selects_isa(addr: u32) -> bool {
    if (addr & 0x8000_0000) == 0 {
        return false;
    }
    let (bus, dev, fun, _) = pci_bdf(addr);
    bus == 0 && dev == 1 && fun == 0
}

pub fn pci_addr_selects_pm(addr: u32) -> bool {
    if (addr & 0x8000_0000) == 0 {
        return false;
    }
    let (bus, dev, fun, _) = pci_bdf(addr);
    bus == 0 && dev == PM_BRIDGE_DEV && fun == PM_BRIDGE_FN
}

pub fn pm_pci_config_addr() -> u32 {
    0x8000_0000 | (u32::from(PM_BRIDGE_DEV) << 11) | (u32::from(PM_BRIDGE_FN) << 8)
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
    port92: u8,
    pic_imr: [u8; 2],
    pm_timer: u32,
    pmba: u32,
    pm_cmd: u16,
    pm_iose: u8,
    /// PIIX3 ISA config (QEMU `piix3_reset`). PIRQ at `0x60–0x63`.
    isa_cfg: [u8; 256],
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
            port92: 0x02,
            pic_imr: [0xFF, 0xFF],
            pm_timer: 0,
            pmba: PIIX4_PMBA_DEFAULT,
            pm_cmd: 0x0001,
            pm_iose: 0,
            isa_cfg: [0u8; 256],
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
static FWCFG_BOOT: AtomicBool = AtomicBool::new(false);
static FWCFG_E820: AtomicBool = AtomicBool::new(false);
static FWCFG_WAIT: AtomicBool = AtomicBool::new(false);
static HOST_ENUM: AtomicBool = AtomicBool::new(false);
static ACPI_PM: AtomicU32 = AtomicU32::new(0);
static LAST_CMOS: AtomicU8 = AtomicU8::new(0);

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
        if p.isa_cfg[0] == 0 {
            fill_isa_cfg(&mut p.isa_cfg);
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

/// QEMU `piix3_reset` (82371SB). PIRQA–D `0x80` = route disabled.
fn fill_isa_cfg(c: &mut [u8; 256]) {
    c.fill(0);
    let vid = ISA_BRIDGE_VENDOR.to_le_bytes();
    let did = ISA_BRIDGE_DEVICE.to_le_bytes();
    c[0] = vid[0];
    c[1] = vid[1];
    c[2] = did[0];
    c[3] = did[1];
    c[4] = 0x07; // I/O + memory + bus master
    c[7] = 0x02; // medium DEVSEL
    c[10] = 0x01; // PCI-to-ISA bridge
    c[11] = 0x06;
    c[0x0E] = 0x80; // Header Type multifunction
    c[0x4C] = 0x4D;
    c[0x4E] = 0x03;
    c[0x60] = 0x80;
    c[0x61] = 0x80;
    c[0x62] = 0x80;
    c[0x63] = 0x80;
    c[0x69] = 0x02;
    c[0x70] = 0x80;
    c[0x76] = 0x0C;
    c[0x77] = 0x0C;
    c[0x78] = 0x02;
    c[0xA0] = 0x08;
    c[0xA8] = 0x0F;
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
            p.fw_len = (4 + 64 * FW_CFG_NAMED_FILE_COUNT as usize) as u8;
        }
        FW_CFG_BOOT_MENU => {
            p.fw_buf[..2].copy_from_slice(&1u16.to_le_bytes());
            p.fw_len = 2;
        }
        FW_CFG_BOOTORDER_SEL => {
            p.fw_len = BOOTORDER.len() as u8;
            FWCFG_BOOT.store(true, Ordering::Release);
        }
        FW_CFG_E820_SEL => {
            p.fw_len = E820_ENTRY_BYTES;
            FWCFG_E820.store(true, Ordering::Release);
        }
        FW_CFG_BOOT_WAIT_SEL => {
            p.fw_buf[..2].copy_from_slice(&BOOT_MENU_WAIT);
            p.fw_len = 2;
            FWCFG_WAIT.store(true, Ordering::Release);
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
        fill_isa_cfg(&mut p.isa_cfg);
    });
    CMOS_MEM.store(false, Ordering::Release);
    FWCFG_RAM.store(false, Ordering::Release);
    FWCFG_BOOT.store(false, Ordering::Release);
    FWCFG_E820.store(false, Ordering::Release);
    FWCFG_WAIT.store(false, Ordering::Release);
    HOST_ENUM.store(false, Ordering::Release);
    ACPI_PM.store(0, Ordering::Release);
    LAST_CMOS.store(0, Ordering::Release);
}

/// 24-bit ACPI PM timer reads (OVMF `InternalAcpiDelay`). Not PIT.
pub fn acpi_pm_timer_reads() -> u32 {
    ACPI_PM.load(Ordering::Acquire)
}

/// ~1.17 s of ACPI PM time (3.579545 MHz, 24-bit). Nested VT-x `2674629`:
/// BOTH-OK then n=32768 `ataio=0` `acpi=16612` `port=0` `hpet=10`
/// (`in eax,dx` at `rip=0x6fb153`). 64 Ki step is ~18 ms so a 1 s
/// `MicroSecondDelay` / BdsWait takes ~55 INs; ~300 waits burned the
/// cap before AtaAtapiPassThru. One step ≥ 1 s of PM ticks.
pub const ACPI_PM_STEP: u32 = 0x0040_0000;

fn tick_pm_timer(p: &mut Platform) -> u32 {
    ACPI_PM.fetch_add(1, Ordering::AcqRel);
    let v = p.pm_timer & 0x00FF_FFFF;
    p.pm_timer = p.pm_timer.wrapping_add(ACPI_PM_STEP);
    v
}

pub fn cmos_mem_served() -> bool {
    CMOS_MEM.load(Ordering::Acquire)
}

pub fn fwcfg_ram_served() -> bool {
    FWCFG_RAM.load(Ordering::Acquire)
}

pub fn fwcfg_bootorder_served() -> bool {
    FWCFG_BOOT.load(Ordering::Acquire)
}

pub fn fwcfg_e820_served() -> bool {
    FWCFG_E820.load(Ordering::Acquire)
}

pub fn fwcfg_boot_wait_served() -> bool {
    FWCFG_WAIT.load(Ordering::Acquire)
}

/// Last CMOS index (NMI bit stripped). Nested VT-x `5b2739a` died on `port=0x71`.
pub fn last_cmos_index() -> u8 {
    LAST_CMOS.load(Ordering::Acquire)
}

pub fn host_bridge_enumerated() -> bool {
    HOST_ENUM.load(Ordering::Acquire)
}

/// Honest platform-memory evidence: CMOS size, fw_cfg RAM_SIZE, or etc/e820.
pub fn platform_memory_served() -> bool {
    cmos_mem_served() || fwcfg_ram_served() || fwcfg_e820_served()
}

/// Header Type byte (bits 23:16 of config dword `0x0C`) has the multifunction bit.
pub fn pci_header_is_multifunction(dword_0c: u32) -> bool {
    ((dword_0c >> 16) & 0xff) == 0x80
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

fn isa_dword(p: &Platform, off: u8) -> u32 {
    let o = off as usize;
    u32::from_le_bytes([
        p.isa_cfg[o],
        p.isa_cfg[o + 1],
        p.isa_cfg[o + 2],
        p.isa_cfg[o + 3],
    ])
}

fn isa_write_cfg(cfg: &mut [u8; 256], off: u8, size: u8, val: u32) {
    let n = match size {
        1 => 1usize,
        2 => 2,
        _ => 4,
    };
    for i in 0..n {
        let o = off as usize + i;
        if o >= 256 {
            break;
        }
        // Keep VID/DID, class, Header Type (multifunction bit).
        if o < 4 || (8..12).contains(&o) || o == 0x0E {
            continue;
        }
        cfg[o] = (val >> (8 * i)) as u8;
    }
}

fn pm_dword(p: &Platform, off: u8) -> u32 {
    match off {
        0x00 => u32::from(PM_BRIDGE_VENDOR) | (u32::from(PM_BRIDGE_DEVICE) << 16),
        0x04 => u32::from(p.pm_cmd) | 0x0280_0000, // cap list unused; medium DEVSEL
        0x08 => 0x0680_0000,                       // bridge / other
        0x0C => 0x0000_0000,
        0x40 => p.pmba,
        0x80 => u32::from(p.pm_iose),
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

/// `Some` when this BDF is the host, ISA, or PIIX4 PM bridge.
pub fn pci_read_data(port: u16, size: u8) -> Option<u32> {
    with_plat(|p| {
        let addr = p.pci_addr;
        let off = pci_cfg_offset(addr, port);
        let aligned = off & 0xFC;
        if pci_addr_selects_host(addr) {
            if aligned == 0 {
                HOST_ENUM.store(true, Ordering::Release);
            }
            Some(shift_dword(host_dword(aligned), off, size))
        } else if pci_addr_selects_isa(addr) {
            Some(shift_dword(isa_dword(p, aligned), off, size))
        } else if pci_addr_selects_pm(addr) {
            Some(shift_dword(pm_dword(p, aligned), off, size))
        } else {
            None
        }
    })
}

pub fn pci_write_data(port: u16, size: u8, val: u32) {
    with_plat(|p| {
        if pci_addr_selects_isa(p.pci_addr) {
            let off = pci_cfg_offset(p.pci_addr, port);
            isa_write_cfg(&mut p.isa_cfg, off, size, val);
            return;
        }
        if !pci_addr_selects_pm(p.pci_addr) {
            return;
        }
        let off = pci_cfg_offset(p.pci_addr, port);
        if off == 0x04 {
            p.pm_cmd = val as u16;
        } else if (0x40..0x44).contains(&off) {
            let shift = (off & 3) * 8;
            let mut v = p.pmba;
            let mask = match size {
                1 => 0xffu32,
                2 => 0xffff,
                _ => 0xffff_ffff,
            };
            v = (v & !(mask << shift)) | ((val & mask) << shift);
            p.pmba = v | 1;
        } else if off == 0x80 {
            p.pm_iose = val as u8;
        }
    });
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
                LAST_CMOS.store(p.cmos_idx, Ordering::Release);
                return rax;
            }
            let idx = p.cmos_idx;
            LAST_CMOS.store(idx, Ordering::Release);
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
                            let b = match p.fw_sel {
                                FW_CFG_FILE_DIR => file_dir_byte(p.fw_off),
                                FW_CFG_BOOTORDER_SEL => {
                                    BOOTORDER.get(p.fw_off as usize).copied().unwrap_or(0)
                                }
                                FW_CFG_E820_SEL => e820_byte(p.fw_off),
                                _ => p.fw_buf[p.fw_off as usize],
                            };
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
        if acpi_pm_timer_matches(port, size, p.pmba) {
            if is_in {
                let v = tick_pm_timer(p);
                let shift = acpi_pm_timer_shift(port, p.pmba);
                return (rax & !mask) | (u64::from(v >> shift) & mask);
            }
            return rax;
        }
        if is_piix_pm_io_port(port, p.pmba) {
            if is_in {
                return rax & !mask;
            }
            return rax;
        }
        if is_pic_port(port) {
            if is_in {
                let v = match port {
                    0x21 => p.pic_imr[0],
                    0xA1 => p.pic_imr[1],
                    _ => 0,
                };
                return (rax & !mask) | (u64::from(v) & mask);
            }
            if port == 0x21 {
                p.pic_imr[0] = rax as u8;
            } else if port == 0xA1 {
                p.pic_imr[1] = rax as u8;
            }
            return rax;
        }
        if is_kbc_port(port) {
            if is_in {
                // 0x64 bit2 = system flag; OBF/IBF clear so firmware does not
                // wait forever (unhandled IN was 0xFF).
                let v = if port == 0x64 { 0x10u64 } else { 0 };
                return (rax & !mask) | (v & mask);
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
                0x92 => u64::from(p.port92),
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
            } else if port == 0x92 {
                p.port92 = (rax as u8) | 0x02;
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
