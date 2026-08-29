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
//! fw_cfg `etc/e820` (EPT 32 MiB; CMOS/fw_cfg **report** 2 GiB LowMemory so PEI HOB ends at `Uc32Base`; classic VGA hole `[640KiB, 1MiB)` not RAM; reserved PCI UC `[2GiB, 4GiB)`; iron `f9a08c9` type-2 mid-gap ignored; iron `7e5d70f` live GPA0 4K still ASSERT — stop PT peek/poke), fw_cfg `etc/boot-menu-wait` 0 ms
//! (skip BdsWait), 8259 PIC RAZ/WI (lab El Torito; Stage 46 product ISO
//! uses a real PIC + IOAPIC in `guest_irq`), and a
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
//! `0xFF` / IBF stuck). Status-only `0x10` (OBF never set) left
//! Ps2Keyboard `KeyboardWaitForValue` Stall-looping: nested Intel
//! `c19b91f` BOTH-OK then n=32768 `ataio=0` `acpi=14903` `port=0x64`.
//! Self-test `0xAA`→`0x55` plus command ACK so that wait returns.
//! Nested VT-x `2674629`: 32768 cap then
//! `acpi=16612` `port=0` `ataio=0` (BdsWait; BootMenu 0x0e was 0).
//! Nested VT-x `8e55abf` stop `cf8=0x80000838`
//! is PIIX ISA `00:01.0` offset `0x38` (PciBus programming, not
//! empty-slot scan). PIRQ `0x60-0x63` reset `0x80` (disabled) matches
//! QEMU so IRQ assign is not IRQ0.

use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering};

/// Must match [`crate::memory::ept_hw::GUEST_UEFI_LOW_RAM_BYTES`] (32 MiB).
/// Kept local so this module does not import the Proven Core EPT builder.
/// This is **EPT backing**, not the CMOS/fw_cfg LowMemory PEI reads.
pub const PLATFORM_RAM_BYTES: u64 = 32 * 1024 * 1024;
/// PEI `GetSystemMemorySizeBelow4gb` / fw_cfg RAM_SIZE. Iron `f9a08c9`:
/// e820 type-2 mid-gap was ignored (`mtrrv=0` `pde8000=E7` same ASSERT).
/// Report 2 GiB so the system-memory HOB ends at [`E820_PCI_UC_BASE`].
/// Do **not** identity-map `[PLATFORM_RAM_BYTES, PLATFORM_REPORT_RAM_BYTES)`
/// (`89c3731`). Guest-UEFI lazy-maps 2 MiB WB on EPT (iron `fad19b2`
/// `gpa=0x7bddd000`).
pub const PLATFORM_REPORT_RAM_BYTES: u64 = 0x8000_0000;

/// i440FX host bridge at `00:08.0` (Intel 82441FX).
/// Nested VT-x: PEI only `inw` Device ID of `00:00.0` — never Header Type,
/// never another BDF. Slot 0 stays i440FX `0x1237` so `HostBridgeDevId`
/// takes the stock QEMU map (VGA IoMemory HOB) and CpuDxe
/// `AcpiTimerLibConstructor` matches PIIX4 (`PIIX4_PMBA_VALUE` `0xB000`)
/// instead of `ASSERT(FALSE)` on virtio `0x1042` (iron `2cbf9e8` `retcmp=`).
/// DXE latches virtio `0x1042` at `00:02.0` on the first other-BDF CF8.
/// Host stays here (Stage 41 parking).
pub const HOST_BRIDGE_VENDOR: u16 = 0x8086;
pub const HOST_BRIDGE_DEVICE: u16 = 0x1237;
pub const HOST_BRIDGE_DEV: u8 = 8;
pub const HOST_BRIDGE_FN: u8 = 0;

/// PIIX3 ISA / LPC at `00:01.0`.
pub const ISA_BRIDGE_VENDOR: u16 = 0x8086;
pub const ISA_BRIDGE_DEVICE: u16 = 0x7000;

/// PIIX4 PM/ACPI at `00:01.3` (Intel 82371AB). OVMF programs PMBA here
/// after the i440FX host-bridge switch (`PIIX4_PMBA_VALUE` `0xB000`).
/// PEI and DXE `00:00.0` DID stay i440FX `0x1237` so that switch matches
/// without remapping `cmp bx, 0x1237`. DXE latches virtio `0x1042` at
/// `00:02.0` after MemMapInitialization.
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
/// QEMU `E820_RESERVED`. Host-owned 4 GiB identity PML4 (not OVMF MEMFD).
pub const E820_RESERVED: u32 = 2;
/// GPA of the hypervisor 4 GiB identity PML4 (2 MiB). Below OVMF MEMFD
/// `0x800000` so CpuDxe heap cannot clobber CR3 (iron `101b8ec` `pde=0x30646870`).
pub const HV_IDENTITY_PML4: u64 = 0x200000;
/// Nine 4 KiB pages became eleven: PML4 + PDPT + 4 PDs + high-half PDPT +
/// 2 PDs + leftover-high overflow PDPT+PD, plus 16 SPLIT4K PT pages
/// (iron `06b011a` `err=0x3` `pde=0x1c000e7`). Must match
/// `guest_pt::IDENTITY_RESERVED_BYTES`.
pub const HV_IDENTITY_PML4_BYTES: u64 = 27 * 4096;
/// OVMF PEI `Uc32Base` when LowMemory is 32 MiB: `GetPowerOfTwo32(4GiB-32MiB)`
/// pins the 32-bit PCI / MTRR UC window at 2 GiB (`mtrr0=0x80000000`).
/// Iron `f9a08c9`: e820 type-2 mid-gap ignored (same ASSERT as hold).
/// CMOS/fw_cfg LowMemory is 2 GiB so PEI's system-memory HOB ends here.
pub const E820_PCI_UC_BASE: u64 = 0x8000_0000;
pub const E820_PCI_UC_BYTES: u64 = 0x8000_0000;
/// `[PLATFORM_RAM_BYTES, E820_PCI_UC_BASE)` — PEI gap below Uc32Base.
/// Iron `f9a08c9` reserved this as type-2; Cruzer OVMF.fd ignored it.
/// CMOS/fw_cfg now **report** it as RAM ([`PLATFORM_REPORT_RAM_BYTES`]).
pub const E820_MID_GAP_BASE: u64 = PLATFORM_RAM_BYTES;
pub const E820_MID_GAP_BYTES: u64 = E820_PCI_UC_BASE - E820_MID_GAP_BASE;
/// ISA VGA/ROM. Fixed MTRR UC. Must not be e820 type-1 (iron `7e5d70f`).
pub const E820_VGA_BASE: u64 = 0xA0000;
pub const E820_VGA_BYTES: u64 = 0x60000;
pub const E820_LOW_1M: u64 = 0x100000;
/// Six e820 entries: 640 KiB RAM / VGA reserved / RAM to PML4 / reserved
/// PML4 / RAM to 2 GiB / PCI UC. Do **not** type-1 `[0, 2MiB)` (covers VGA).
pub const E820_ENTRY_COUNT: u8 = 6;
pub const E820_FILE_BYTES: u8 = E820_ENTRY_BYTES * E820_ENTRY_COUNT;

/// QEMU `bootorder` (OFW paths). PIIX IDE `00:01.1` first (`ide@1,1`), then
/// virtio-fn1 `00:00.1` (`ide@0,1`), then virtio disk at `00:02.0` (`scsi@2`).
/// Nested VT-x BOTH-OK with `ataio=0`: ConnectDevicesFromQemu of `scsi@0`
/// enumerated IDE fn1 as a sibling and did not Start AtaAtapiPassThru.
/// QEMU/OVMF TranslatePciOfwNodes: `ide@1,1/drive@0/disk@0` →
/// `PciRoot(0x0)/Pci(0x1,0x1)/Ata(Primary,Master,0x0)`. Master is `DEV=0`.
/// Trailing NUL: OVMF `ConnectDevicesFromQemu` rejects the file unless the
/// last byte is `'\0'` (`RETURN_INVALID_PARAMETER` otherwise).
pub const BOOTORDER: &[u8] =
    b"/pci@i0cf8/ide@1,1/drive@0/disk@0\n/pci@i0cf8/ide@0,1/drive@0/disk@0\n/pci@i0cf8/scsi@2/disk@0,0\n\0";

/// Product boot order is CD (PIIX then virtio-fn1) then virtio disk (ADR-014).
pub fn boot_order_cd_then_disk() -> bool {
    let piix = find_bytes(BOOTORDER, b"ide@1,1/drive@0");
    let fn1 = find_bytes(BOOTORDER, b"ide@0,1/drive@0");
    let slave = find_bytes(BOOTORDER, b"drive@1");
    let disk = find_bytes(BOOTORDER, b"scsi@2");
    match (piix, fn1, slave, disk) {
        (Some(p), Some(f), None, Some(d)) => p < f && f < d,
        _ => false,
    }
}

/// OVMF `ConnectDevicesFromQemu` / `StoreQemuBootOrder` require a C string.
pub fn bootorder_nul_terminated() -> bool {
    matches!(BOOTORDER.last(), Some(&0))
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
        1 => file_entry_byte(i, u32::from(E820_FILE_BYTES), FW_CFG_E820_SEL, b"etc/e820"),
        2 => file_entry_byte(
            i,
            BOOT_MENU_WAIT.len() as u32,
            FW_CFG_BOOT_WAIT_SEL,
            b"etc/boot-menu-wait",
        ),
        _ => 0,
    }
}

/// Six packed QEMU e820 entries: 640 KiB RAM, VGA reserved, RAM to HV PML4,
/// reserved identity tables, RAM up to [`PLATFORM_REPORT_RAM_BYTES`]
/// (2 GiB LowMemory lie), reserved PCI UC `[2GiB, 4GiB)`.
/// Iron `7e5d70f`: type-1 `[0, 2MiB)` covered VGA UC; CpuDxe RefreshGcd
/// `SetMemorySpaceAttributes(WB)` on that GCD range returns UNSUPPORTED.
/// Do **not** lower CMOS (32 MiB LowMemory already ASSERTed). Do **not**
/// retry P3 mid-gap type-2. EPT stays 32 MiB.
pub fn e820_byte(off: u16) -> u8 {
    let o = off as usize;
    let ent = o / 20;
    let i = o % 20;
    let ram_after_hv = HV_IDENTITY_PML4 + HV_IDENTITY_PML4_BYTES;
    let (addr, len, typ) = match ent {
        0 => (0u64, E820_VGA_BASE, E820_RAM),
        1 => (E820_VGA_BASE, E820_VGA_BYTES, E820_RESERVED),
        2 => (
            E820_LOW_1M,
            HV_IDENTITY_PML4.saturating_sub(E820_LOW_1M),
            E820_RAM,
        ),
        3 => (HV_IDENTITY_PML4, HV_IDENTITY_PML4_BYTES, E820_RESERVED),
        4 => (
            ram_after_hv,
            PLATFORM_REPORT_RAM_BYTES.saturating_sub(ram_after_hv),
            E820_RAM,
        ),
        5 => (E820_PCI_UC_BASE, E820_PCI_UC_BYTES, E820_RESERVED),
        _ => return 0,
    };
    if i < 8 {
        addr.to_le_bytes()[i]
    } else if i < 16 {
        len.to_le_bytes()[i - 8]
    } else if i < 20 {
        typ.to_le_bytes()[i - 16]
    } else {
        0
    }
}

fn e820_entry_at(ent: usize) -> (u64, u64, u32) {
    let base = (ent * 20) as u16;
    let mut addr = [0u8; 8];
    let mut len = [0u8; 8];
    let mut typ = [0u8; 4];
    for i in 0..8 {
        addr[i] = e820_byte(base + i as u16);
        len[i] = e820_byte(base + 8 + i as u16);
    }
    for i in 0..4 {
        typ[i] = e820_byte(base + 16 + i as u16);
    }
    (
        u64::from_le_bytes(addr),
        u64::from_le_bytes(len),
        u32::from_le_bytes(typ),
    )
}

/// True when `etc/e820` has a type-2 PCI UC hole at [`E820_PCI_UC_BASE`].
pub fn e820_splits_mtrr_uc_hole() -> bool {
    let (addr, len, typ) = e820_entry_at(5);
    E820_FILE_BYTES == E820_ENTRY_BYTES * E820_ENTRY_COUNT
        && addr == E820_PCI_UC_BASE
        && len == E820_PCI_UC_BYTES
        && typ == E820_RESERVED
}

/// Iron `f9a08c9`: type-2 mid-gap did not split GCD. Kept false.
pub fn e820_splits_gcd_mid_gap() -> bool {
    false
}

/// True when e820 does not claim VGA `[0xA0000, 1MiB)` as type-1 RAM.
///
/// Iron `7e5d70f`: live GPA0 4K matched MTRR (`pde0=0x20b027`) then the
/// same CpuDxe ASSERT. Stop PT peek/poke. First RAM entry was `[0, 2MiB)`.
pub fn e820_splits_vga_below_1m() -> bool {
    let (a0, l0, t0) = e820_entry_at(0);
    let (a1, l1, t1) = e820_entry_at(1);
    a0 == 0
        && l0 == E820_VGA_BASE
        && t0 == E820_RAM
        && a1 == E820_VGA_BASE
        && l1 == E820_VGA_BYTES
        && t1 == E820_RESERVED
        && a0.saturating_add(l0) == E820_VGA_BASE
}

/// CMOS 0x34/0x35 + fw_cfg RAM_SIZE + e820 RAM end at 2 GiB (`Uc32Base`).
pub fn platform_reports_2g_lowmem() -> bool {
    let (addr, len, typ) = e820_entry_at(4);
    let ram_end = addr.saturating_add(len);
    PLATFORM_REPORT_RAM_BYTES == E820_PCI_UC_BASE
        && cmos_above_16m_chunks(PLATFORM_REPORT_RAM_BYTES) == 0x7F00
        && ram_end == PLATFORM_REPORT_RAM_BYTES
        && typ == E820_RAM
        && e820_splits_mtrr_uc_hole()
        && e820_splits_vga_below_1m()
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
/// was unhandled `0xFF` (IBF stuck). Status `0x10` = unlocked, buffers
/// empty. Nested Intel `c19b91f` then `KeyboardWaitForValue` Stall on
/// missing OBF after `0xAA`/`0xFF` — ACK those so BDS can Start ATA.
pub fn is_kbc_port(port: u16) -> bool {
    port == 0x60 || port == 0x64
}

/// 8042 status: output-buffer full (data waiting at `0x60`).
const KBC_STAT_OBF: u8 = 0x01;
/// 8042 status: POST complete (set after controller self-test).
const KBC_STAT_SYS: u8 = 0x04;
/// 8042 status: keyboard unlocked (not IBF; unhandled `IN` was `0xFF`).
const KBC_STAT_UNLOCK: u8 = 0x10;
const KBC_QCAP: usize = 4;
const KBC_EXPECT_NONE: u8 = 0;
const KBC_EXPECT_CCB: u8 = 1;
const KBC_EXPECT_OUT: u8 = 2;
const KBC_EXPECT_LED: u8 = 3;
const KBC_EXPECT_EXTRA: u8 = 4;
/// Command-byte default: translate + enable + system flag (QEMU 8042).
const KBC_CCB_DEFAULT: u8 = 0x47;

fn kbc_status(p: &Platform) -> u8 {
    let mut s = KBC_STAT_UNLOCK | p.kbc_sys;
    if p.kbc_n > 0 {
        s |= KBC_STAT_OBF;
    }
    s
}

fn kbc_push(p: &mut Platform, b: u8) {
    let n = p.kbc_n as usize;
    if n < KBC_QCAP {
        p.kbc_q[n] = b;
        p.kbc_n = p.kbc_n.saturating_add(1);
    }
}

fn kbc_pop(p: &mut Platform) -> u8 {
    if p.kbc_n == 0 {
        return 0;
    }
    let b = p.kbc_q[0];
    let n = p.kbc_n as usize;
    for i in 1..n {
        p.kbc_q[i - 1] = p.kbc_q[i];
    }
    p.kbc_n -= 1;
    b
}

fn kbc_write_cmd(p: &mut Platform, cmd: u8) {
    p.kbc_expect = KBC_EXPECT_NONE;
    match cmd {
        0xAA => {
            p.kbc_sys = KBC_STAT_SYS;
            kbc_push(p, 0x55);
        }
        0xAB => kbc_push(p, 0x00),
        0x20 => kbc_push(p, p.kbc_ccb),
        0x60 => p.kbc_expect = KBC_EXPECT_CCB,
        0xD1 => p.kbc_expect = KBC_EXPECT_OUT,
        0xD4 => p.kbc_expect = KBC_EXPECT_EXTRA,
        _ => {}
    }
}

fn kbc_write_data(p: &mut Platform, data: u8) {
    match p.kbc_expect {
        KBC_EXPECT_CCB => {
            p.kbc_ccb = data;
            p.kbc_expect = KBC_EXPECT_NONE;
        }
        KBC_EXPECT_OUT => {
            p.kbc_expect = KBC_EXPECT_NONE;
        }
        KBC_EXPECT_LED | KBC_EXPECT_EXTRA => {
            p.kbc_expect = KBC_EXPECT_NONE;
            kbc_push(p, 0xFA);
        }
        _ => match data {
            0xFF => {
                kbc_push(p, 0xFA);
                kbc_push(p, 0xAA);
            }
            0xF2 => {
                kbc_push(p, 0xFA);
                kbc_push(p, 0xAB);
                kbc_push(p, 0x83);
            }
            0xEA | 0xEE => kbc_push(p, 0xEE),
            0xED => {
                kbc_push(p, 0xFA);
                p.kbc_expect = KBC_EXPECT_LED;
            }
            0xF0 | 0xF3 => {
                kbc_push(p, 0xFA);
                p.kbc_expect = KBC_EXPECT_EXTRA;
            }
            _ => kbc_push(p, 0xFA),
        },
    }
}

/// 8259 PIC + ELCR. Unhandled `IN` was `0xFF` (all IRQs in service).
pub fn is_pic_port(port: u16) -> bool {
    matches!(port, 0x20 | 0x21 | 0xA0 | 0xA1 | 0x4D0 | 0x4D1)
}

/// OVMF `AcpiTimerLibConstructor` `ASSERT(FALSE)` when `00:00.0` DID is
/// not i440FX `0x1237` / Q35 `0x29C0` / CloudHV `0x0D57` (iron `2cbf9e8`
/// `retcmp=`). Slot 0 stays i440FX so the constructor stores
/// `PIIX4_PMBA_VALUE` `0xB000`. PEI `IoRead32(0)` in `InternalAcpiDelay`
/// is the leftover port-0 path. PIIX4 PMBA timer is `0x408`. QEMU
/// PIIX4 default is `0xB008`. Programmed PMBA+8 is also a timer.
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

/// Local APIC 2 MiB window (`0xFEE00000`). Not a zero sink — version 0
/// is a typical OVMF `GetApicVersion() != 0` DebugAssert (iron `ad78f12`).
pub fn is_xapic_2m_gpa(gpa: u64) -> bool {
    (gpa & !0x1F_FFFF) == 0xFEE0_0000
}

/// Unbacked GPA in the PCI / MMIO hole, below the 4 MiB flash floor.
///
/// Iron `cc7d78a`: 4G identity CR3 made the PCI hole present; EPT sink
/// stopped at 1 GiB so `gpa=0xC01DF1B7` (`reason=0x30`) halted. Include
/// `[2GiB, 0xFFC00000)` (PCI hole at `0xC0000000`, IOAPIC/HPET). Reported
/// LowMemory `[32MiB, 2GiB)` is **not** a sink (lazy 2 MiB WB on EPT;
/// iron `fad19b2` `gpa=0x7bddd000`). The xAPIC 2 MiB window stays excluded so guest-UEFI can map a
/// live 4 KiB page.
pub fn is_platform_sink_gpa(gpa: u64) -> bool {
    const FW_FLOOR: u64 = 0xFFC0_0000;
    if is_xapic_2m_gpa(gpa) {
        return false;
    }
    if crate::devices::guest_virtio_blk::is_virtio_bar_2m_gpa(gpa) {
        return false;
    }
    if crate::devices::guest_irq::is_hpet_split_2m_gpa(gpa) {
        return false;
    }
    gpa >= PLATFORM_REPORT_RAM_BYTES && gpa < FW_FLOOR
}

/// GPA in the 2 GiB LowMemory lie that launch does not identity-map (32 MiB slab).
/// Guest-UEFI maps 2 MiB WB on EPT (iron `fad19b2` `gpa=0x7bddd000`).
pub fn is_unbacked_report_ram_gpa(gpa: u64) -> bool {
    gpa >= PLATFORM_RAM_BYTES && gpa < PLATFORM_REPORT_RAM_BYTES
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
/// ~1 ms of HPET time (10 ns ticks) on CPUID / RDMSR / WRMSR / non-HPET EPT.
/// Iron COM2 after leftover+#PF: CPUID exits restart the preemption timer
/// so [`HPET_MAIN_STEP`] never fires and the counter looks frozen
/// (`hpet=22896` through n=256 `leaf=0x4000bd00`). The same restart happens
/// on leftover-DRAM / MMIO EPT storms after `identify_cpu`. Do **not** use
/// [`HPET_MAIN_STEP`] here — 1 s per exit would jump hours. PCI config and
/// ATA I/O stay 0 (`5d9e346`). UART COM I/O uses [`hpet_ticks_from_tsc_delta`]
/// (not this 1 ms quantum). Not `ISO-INSTALL-OK`.
pub const HPET_INSN_STEP: u64 = 100_000;
/// Max HPET ticks injected on one UART COM I/O exit. 10 ns × 400 = 4 µs.
/// Iron `6e5c84a`: earlycon `in al,dx` storm froze `hpet=40191`. A fixed
/// 1 ms (`HPET_INSN_STEP`) per character I/O would jump jiffies. Cap so one
/// exit cannot inject more than a few microseconds. Not PCI/ATA.
pub const HPET_UART_IO_STEP_CAP: u64 = 400;
/// Host TSC ticks per 10 ns HPET tick. Xeon Silver 4110 TSC ≈ 2.1 GHz → 21.
/// Overestimate undercounts guest time (safer than 1 ms/byte).
pub const TSC_PER_HPET_TICK: u64 = 21;

/// Convert host TSC delta to HPET main-counter ticks, capped.
///
/// INVARIANTS:
/// - Returns 0 when `tsc_delta` is below one HPET tick
/// - Never returns more than [`HPET_UART_IO_STEP_CAP`]
pub fn hpet_ticks_from_tsc_delta(tsc_delta: u64) -> u64 {
    if TSC_PER_HPET_TICK == 0 {
        return 0;
    }
    core::cmp::min(tsc_delta / TSC_PER_HPET_TICK, HPET_UART_IO_STEP_CAP)
}

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
    pit_reload: u16,
    /// Next 0x40 data write is the high byte (16-bit lo/hi).
    pit_hi: bool,
    /// Next unlatched 0x40 read is the high byte (Linux lo/hi access).
    pit_rd_hi: bool,
    /// i8253 access: 1=lo, 2=hi, 3=lo/hi. 0 = not programmed (OVMF IN 0x40).
    pit_access: u8,
    pit_latch: u16,
    /// Remaining latched bytes to return (2 = lo next, 1 = hi next).
    pit_latch_n: u8,
    port61: u8,
    port92: u8,
    pic_imr: [u8; 2],
    pm_timer: u32,
    pmba: u32,
    pm_cmd: u16,
    pm_iose: u8,
    /// PIIX3 ISA config (QEMU `piix3_reset`). PIRQ at `0x60–0x63`.
    isa_cfg: [u8; 256],
    /// 8042 output queue (`0x60` reads). Not keystrokes.
    kbc_q: [u8; KBC_QCAP],
    kbc_n: u8,
    kbc_expect: u8,
    kbc_sys: u8,
    kbc_ccb: u8,
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
            pit_reload: 0xFFFF,
            pit_hi: false,
            pit_rd_hi: false,
            pit_access: 0,
            pit_latch: 0,
            pit_latch_n: 0,
            port61: 0x10,
            port92: 0x02,
            pic_imr: [0xFF, 0xFF],
            pm_timer: 0,
            pmba: PIIX4_PMBA_DEFAULT,
            pm_cmd: 0x0001,
            pm_iose: 0,
            isa_cfg: [0u8; 256],
            kbc_q: [0u8; KBC_QCAP],
            kbc_n: 0,
            kbc_expect: KBC_EXPECT_NONE,
            kbc_sys: 0,
            kbc_ccb: KBC_CCB_DEFAULT,
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
static FWCFG_DIR: AtomicBool = AtomicBool::new(false);
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
    let ext = cmos_extended_kb(PLATFORM_REPORT_RAM_BYTES).to_le_bytes();
    c[0x17] = ext[0];
    c[0x18] = ext[1];
    c[0x30] = ext[0];
    c[0x31] = ext[1];
    c[0x32] = 0x20;
    let above = cmos_above_16m_chunks(PLATFORM_REPORT_RAM_BYTES).to_le_bytes();
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
            p.fw_buf[..8].copy_from_slice(&PLATFORM_REPORT_RAM_BYTES.to_le_bytes());
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
            FWCFG_DIR.store(true, Ordering::Release);
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
            p.fw_len = E820_FILE_BYTES;
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
    FWCFG_DIR.store(false, Ordering::Release);
    FWCFG_WAIT.store(false, Ordering::Release);
    HOST_ENUM.store(false, Ordering::Release);
    ACPI_PM.store(0, Ordering::Release);
    LAST_CMOS.store(0, Ordering::Release);
    crate::devices::guest_irq::reset();
    crate::devices::guest_uart::reset();
}

/// Step the i8253 channel-0 counter. Linux `nolapic` `clockevent_i8253`
/// latches this 16-bit value; the old stub wrote `val | 0x00FF` and never
/// returned a high byte.
pub fn pit_tick() {
    with_plat(pit_tick_locked);
}

fn pit_tick_locked(p: &mut Platform) {
    let step = 0x40u16;
    if p.pit <= step {
        p.pit = if p.pit_reload == 0 {
            0xFFFF
        } else {
            p.pit_reload
        };
    } else {
        p.pit = p.pit.wrapping_sub(step);
    }
}

fn pit_write_cmd(p: &mut Platform, val: u8) {
    if (val >> 6) & 3 != 0 {
        return;
    }
    let access = (val >> 4) & 3;
    if access == 0 {
        p.pit_latch = p.pit;
        p.pit_latch_n = 2;
        return;
    }
    p.pit_hi = false;
    p.pit_rd_hi = false;
    p.pit_access = access;
    p.pit_latch_n = 0;
    p.pit = 0xFFFF;
}

fn pit_write_data(p: &mut Platform, val: u8) {
    if p.pit_access == 1 {
        p.pit_reload = (p.pit_reload & 0xFF00) | u16::from(val);
        p.pit = if p.pit_reload == 0 {
            0xFFFF
        } else {
            p.pit_reload
        };
        return;
    }
    if p.pit_access == 2 {
        p.pit_reload = (p.pit_reload & 0x00FF) | (u16::from(val) << 8);
        p.pit = if p.pit_reload == 0 {
            0xFFFF
        } else {
            p.pit_reload
        };
        return;
    }
    if !p.pit_hi {
        p.pit_reload = (p.pit_reload & 0xFF00) | u16::from(val);
        p.pit_hi = true;
    } else {
        p.pit_reload = (p.pit_reload & 0x00FF) | (u16::from(val) << 8);
        p.pit = if p.pit_reload == 0 {
            0xFFFF
        } else {
            p.pit_reload
        };
        p.pit_hi = false;
    }
}

fn pit_read_data(p: &mut Platform) -> u8 {
    if p.pit_latch_n == 2 {
        p.pit_latch_n = 1;
        return p.pit_latch as u8;
    }
    if p.pit_latch_n == 1 {
        p.pit_latch_n = 0;
        return (p.pit_latch >> 8) as u8;
    }
    if p.pit_access == 3 {
        if !p.pit_rd_hi {
            p.pit_rd_hi = true;
            return p.pit as u8;
        }
        p.pit_rd_hi = false;
        let v = (p.pit >> 8) as u8;
        pit_tick_locked(p);
        return v;
    }
    let v = p.pit as u8;
    pit_tick_locked(p);
    v
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

/// True when PEI selected fw_cfg file directory (`QemuFwCfgFindFile`).
pub fn fwcfg_file_dir_served() -> bool {
    FWCFG_DIR.load(Ordering::Acquire)
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
    if is_pic_port(port) && crate::devices::ide_cdrom::product_iso_window_armed() {
        return crate::devices::guest_irq::pic_io(port, is_in, size, rax);
    }
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
                let v = if port == 0x64 {
                    u64::from(kbc_status(p))
                } else {
                    u64::from(kbc_pop(p))
                };
                return (rax & !mask) | (v & mask);
            }
            let b = rax as u8;
            if port == 0x64 {
                kbc_write_cmd(p, b);
            } else {
                kbc_write_data(p, b);
            }
            return rax;
        }
        if is_in {
            let val = match port {
                0x61 => {
                    // Bit 4 = DRAM refresh (Linux io_delay). Bit 5 = TMR2_OUT
                    // (OVMF MicroSecondDelay `in al,0x61; test al,0x20; jz`).
                    // Iron COM2 after BdsDxe Start CD: rip=0x7e149fb9 spin.
                    p.port61 ^= 0x30;
                    u64::from(p.port61)
                }
                0x80 => 0,
                0x92 => u64::from(p.port92),
                0x40 => u64::from(pit_read_data(p)),
                0x41..=0x43 => 0,
                _ => mask,
            };
            (rax & !mask) | (val & mask)
        } else {
            if port == 0x61 {
                p.port61 = (rax as u8 & !0x30) | (p.port61 & 0x30);
            } else if port == 0x92 {
                p.port92 = (rax as u8) | 0x02;
            } else if port == 0x40 {
                pit_write_data(p, rax as u8);
            } else if port == 0x43 {
                pit_write_cmd(p, rax as u8);
            }
            rax
        }
    })
}

#[cfg(test)]
#[path = "guest_platform_test.rs"]
mod guest_platform_test;
