//! Product-ISO fw_cfg ACPI (OVMF `InstallQemuFwCfgTables`).
//!
//! Pillar: [Z]
//! Proven Core: **outside** (ADR-002 / ADR-014)
//! VERIFICATION: L1 (host tests)
//!
//! Iron `bc6fb70`: `efi:` had no `ACPI=` then `APIC: ACPI MADT or MP tables
//! are not detected`. Without MADT Linux `disable_ioapic_support()` and
//! never programs GSI 17/18. product ISO fw_cfg ACPI MADT (iso=0 named files stay 3).
//! DSDT PCI0 _PRT (slot 2 INTA GSI 17, slot 3 INTA GSI 18) so Linux ACPI
//! IRQ routing finds virtio after tables install. DSDT PCI0 _CRS (bus 0,
//! CF8/IO, VGA, PCI MMIO `0xC0000000..0xFEBFFFFF`) so ACPI-on BAR assign
//! misses the 2 GiB UC scratch at `0x80000000`. Not `ISO-INSTALL-OK`.

/// QEMU `FW_CFG_FILE_FIRST` + 3. `etc/table-loader`.
pub const FW_CFG_ACPI_LOADER_SEL: u16 = 0x23;
/// `etc/acpi/tables`.
pub const FW_CFG_ACPI_TABLES_SEL: u16 = 0x24;
/// `etc/acpi/rsdp`.
pub const FW_CFG_ACPI_RSDP_SEL: u16 = 0x25;

/// Extra named files when the product ISO window is armed.
pub const FW_CFG_NAMED_FILE_COUNT_ACPI: u32 = 3;

pub const ACPI_TABLES_LEN: u16 = DSDT_OFF + DSDT_LEN;
pub const ACPI_RSDP_LEN: u16 = 20;
pub const ACPI_LOADER_ENTRIES: usize = 11;
pub const ACPI_LOADER_LEN: u16 = (ACPI_LOADER_ENTRIES * 128) as u16;

const RSDT_OFF: u16 = 0;
const RSDT_LEN: u16 = 44;
const FACP_OFF: u16 = 0x40;
const FACP_LEN: u16 = 116;
const MADT_OFF: u16 = 0xC0;
const MADT_LEN: u16 = 74;
const DSDT_OFF: u16 = 0x110;
/// iasl AML: PCI0 PNP0A03 + _PRT + _CRS. DSDT PCI0 _PRT. DSDT PCI0 _CRS.
/// Not `ISO-INSTALL-OK`.
const DSDT_LEN: u16 = 225;

const CMD_ALLOC: u32 = 1;
const CMD_ADD_PTR: u32 = 2;
const CMD_CKSUM: u32 = 3;
/// QEMU HIGH (top of conventional / 2GiB CMOS lie). Product ISO tables
/// are FSEG so IoReadFifo8 dest stays in the 32MiB identity slab.
/// FSEG dest holds ACPI tables (conventional 640KiB).
#[allow(dead_code)]
const ZONE_HIGH: u8 = 1;
const ZONE_FSEG: u8 = 2;

/// `etc/acpi/tables` byte. Offsets are RSDT-relative until the linker adds
/// the allocated FSEG base (ADD_POINTER). ACPI tables ZONE_FSEG.
pub fn acpi_tables_byte(off: u16) -> u8 {
    if off >= ACPI_TABLES_LEN {
        return 0;
    }
    if off >= DSDT_OFF {
        return dsdt_byte(off - DSDT_OFF);
    }
    if off >= MADT_OFF {
        return madt_byte(off - MADT_OFF);
    }
    if off >= FACP_OFF {
        return facp_byte(off - FACP_OFF);
    }
    rsdt_byte(off)
}

pub fn acpi_rsdp_byte(off: u16) -> u8 {
    match off {
        0..=7 => b"RSD PTR "[off as usize],
        8 => 0, // checksum filled by table-loader
        9..=14 => b"RAYNUV"[off as usize - 9],
        15 => 0, // rev 0 → 20-byte RSDP
        16..=19 => 0, // RSDT phys; ADD_POINTER adds tables base
        _ => 0,
    }
}

pub fn acpi_loader_byte(off: u16) -> u8 {
    let o = off as usize;
    if o >= ACPI_LOADER_LEN as usize {
        return 0;
    }
    loader_entry_byte(o / 128, o % 128)
}

fn hdr_byte(off: u16, sig: &[u8; 4], len: u16, rev: u8) -> Option<u8> {
    match off {
        0..=3 => Some(sig[off as usize]),
        4..=7 => Some(u32::from(len).to_le_bytes()[off as usize - 4]),
        8 => Some(rev),
        9 => Some(0),
        10..=15 => Some(b"RAYNUV"[off as usize - 10]),
        16..=23 => Some(b"RAYNUV  "[off as usize - 16]),
        24..=27 => Some(1u32.to_le_bytes()[off as usize - 24]),
        28..=31 => Some(*b"RNUV".get(off as usize - 28).unwrap_or(&0)),
        32..=35 => Some(1u32.to_le_bytes()[off as usize - 32]),
        _ => None,
    }
}

fn rsdt_byte(off: u16) -> u8 {
    if let Some(b) = hdr_byte(off, b"RSDT", RSDT_LEN, 1) {
        return b;
    }
    match off {
        36..=39 => u32::from(FACP_OFF).to_le_bytes()[off as usize - 36],
        40..=43 => u32::from(MADT_OFF).to_le_bytes()[off as usize - 40],
        _ => 0,
    }
}

fn facp_byte(off: u16) -> u8 {
    if let Some(b) = hdr_byte(off, b"FACP", FACP_LEN, 1) {
        return b;
    }
    match off {
        40..=43 => u32::from(DSDT_OFF).to_le_bytes()[off as usize - 40],
        44 => 1, // dual 8259
        46 => 9, // SCI
        47 => 0,
        // SMI_CMD / ACPI_ENABLE / ACPI_DISABLE stay 0 (already ACPI).
        // PM1 SCI_EN at reset. Linux acpi_hw_get_mode skips the PM1 write.
        56..=59 => 0xB000u32.to_le_bytes()[off as usize - 56], // PM1a_EVT
        64..=67 => 0xB004u32.to_le_bytes()[off as usize - 64], // PM1a_CNT
        76..=79 => 0xB008u32.to_le_bytes()[off as usize - 76], // PM_TMR
        88 => 4,
        89 => 2,
        91 => 4,
        _ => 0,
    }
}

fn madt_byte(off: u16) -> u8 {
    if let Some(b) = hdr_byte(off, b"APIC", MADT_LEN, 1) {
        return b;
    }
    match off {
        36..=39 => 0xFEE0_0000u32.to_le_bytes()[off as usize - 36],
        40 => 1, // PCAT_COMPAT
        41..=43 => 0,
        // Local APIC
        44 => 0,
        45 => 8,
        46 => 0,
        47 => 0,
        48 => 1,
        49..=51 => 0,
        // I/O APIC
        52 => 1,
        53 => 12,
        54 => 0,
        55 => 0,
        56..=59 => 0xFEC0_0000u32.to_le_bytes()[off as usize - 56],
        60..=63 => 0,
        // Interrupt Source Override: ISA IRQ 0 → GSI 2 (QEMU/PCAT).
        // MADT IRQ0 ISO GSI 2. Not `ISO-INSTALL-OK`.
        64 => 2,
        65 => 10,
        66 => 0,
        67 => 0,
        68..=71 => 2u32.to_le_bytes()[off as usize - 68],
        72..=73 => 0,
        _ => 0,
    }
}

fn dsdt_byte(off: u16) -> u8 {
    DSDT_AML.get(off as usize).copied().unwrap_or(0)
}

/// DSDT PCI0 _PRT. Slot 2 INTA → GSI 17, slot 3 INTA → GSI 18.
/// DSDT PCI0 _CRS. PCI MMIO producer `0xC0000000..0xFEBFFFFF` (not
/// `0x80000000` scratch). `iasl` of `PNP0A03` PCI0 (no `_ADR`).
/// Not `ISO-INSTALL-OK`.
const DSDT_AML: [u8; DSDT_LEN as usize] = [
    0x44, 0x53, 0x44, 0x54, 0xE1, 0x00, 0x00, 0x00, 0x02, 0xB8, 0x52, 0x41, 0x59, 0x4E, 0x55, 0x56,
    0x52, 0x41, 0x59, 0x4E, 0x55, 0x56, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x49, 0x4E, 0x54, 0x4C,
    0x28, 0x06, 0x23, 0x20, 0x10, 0x4C, 0x0B, 0x5F, 0x53, 0x42, 0x5F, 0x5B, 0x82, 0x44, 0x0B, 0x50,
    0x43, 0x49, 0x30, 0x08, 0x5F, 0x48, 0x49, 0x44, 0x0C, 0x41, 0xD0, 0x0A, 0x03, 0x08, 0x5F, 0x55,
    0x49, 0x44, 0x00, 0x08, 0x5F, 0x42, 0x42, 0x4E, 0x00, 0x08, 0x5F, 0x50, 0x52, 0x54, 0x12, 0x1A,
    0x02, 0x12, 0x0B, 0x04, 0x0C, 0xFF, 0xFF, 0x02, 0x00, 0x00, 0x00, 0x0A, 0x11, 0x12, 0x0B, 0x04,
    0x0C, 0xFF, 0xFF, 0x03, 0x00, 0x00, 0x00, 0x0A, 0x12, 0x08, 0x5F, 0x43, 0x52, 0x53, 0x11, 0x42,
    0x07, 0x0A, 0x6E, 0x88, 0x0D, 0x00, 0x02, 0x0C, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x01, 0x00, 0x47, 0x01, 0xF8, 0x0C, 0xF8, 0x0C, 0x01, 0x08, 0x88, 0x0D, 0x00, 0x01, 0x0C,
    0x03, 0x00, 0x00, 0x00, 0x00, 0xF7, 0x0C, 0x00, 0x00, 0xF8, 0x0C, 0x88, 0x0D, 0x00, 0x01, 0x0C,
    0x03, 0x00, 0x00, 0x00, 0x0D, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0xF3, 0x87, 0x17, 0x00, 0x00, 0x0C,
    0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x0A, 0x00, 0xFF, 0xFF, 0x0B, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x02, 0x00, 0x87, 0x17, 0x00, 0x00, 0x0C, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0xC0, 0xFF, 0xFF, 0xBF, 0xFE, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xC0, 0x3E, 0x79,
    0x00,
];

fn loader_entry_byte(ent: usize, i: usize) -> u8 {
    match ent {
        0 => alloc_byte(i, b"etc/acpi/tables", 64, ZONE_FSEG),
        1 => alloc_byte(i, b"etc/acpi/rsdp", 16, ZONE_FSEG),
        2 => add_ptr_byte(i, b"etc/acpi/rsdp", b"etc/acpi/tables", 16, 4),
        3 => add_ptr_byte(i, b"etc/acpi/tables", b"etc/acpi/tables", 36, 4),
        4 => add_ptr_byte(i, b"etc/acpi/tables", b"etc/acpi/tables", 40, 4),
        5 => add_ptr_byte(i, b"etc/acpi/tables", b"etc/acpi/tables", u32::from(FACP_OFF) + 40, 4),
        6 => cksum_byte(i, b"etc/acpi/rsdp", 8, 0, u32::from(ACPI_RSDP_LEN)),
        7 => cksum_byte(i, b"etc/acpi/tables", 9, 0, u32::from(RSDT_LEN)),
        8 => cksum_byte(
            i,
            b"etc/acpi/tables",
            u32::from(FACP_OFF) + 9,
            u32::from(FACP_OFF),
            u32::from(FACP_LEN),
        ),
        9 => cksum_byte(
            i,
            b"etc/acpi/tables",
            u32::from(MADT_OFF) + 9,
            u32::from(MADT_OFF),
            u32::from(MADT_LEN),
        ),
        10 => cksum_byte(
            i,
            b"etc/acpi/tables",
            u32::from(DSDT_OFF) + 9,
            u32::from(DSDT_OFF),
            u32::from(DSDT_LEN),
        ),
        _ => 0,
    }
}

fn cmd_byte(i: usize, cmd: u32) -> u8 {
    if i < 4 {
        cmd.to_le_bytes()[i]
    } else {
        0
    }
}

fn name_at(i: usize, base: usize, name: &[u8]) -> u8 {
    let n = i.saturating_sub(base);
    if n < 56 {
        *name.get(n).unwrap_or(&0)
    } else {
        0
    }
}

fn alloc_byte(i: usize, name: &[u8], align: u32, zone: u8) -> u8 {
    match i {
        0..=3 => cmd_byte(i, CMD_ALLOC),
        4..=59 => name_at(i, 4, name),
        60..=63 => align.to_le_bytes()[i - 60],
        64 => zone,
        _ => 0,
    }
}

/// QEMU `BiosLinkerLoaderEntry.pointer` (`hw/acpi/bios-linker-loader.c`):
/// dest_file[56], src_file[56], dest offset u32, size u8.
/// In-file pointer bytes already hold src_offset (OVMF adds pointee base).
fn add_ptr_byte(i: usize, dest: &[u8], src: &[u8], offset: u32, size: u8) -> u8 {
    match i {
        0..=3 => cmd_byte(i, CMD_ADD_PTR),
        4..=59 => name_at(i, 4, dest),
        60..=115 => name_at(i, 60, src),
        116..=119 => offset.to_le_bytes()[i - 116],
        120 => size,
        _ => 0,
    }
}

fn cksum_byte(i: usize, name: &[u8], offset: u32, start: u32, len: u32) -> u8 {
    match i {
        0..=3 => cmd_byte(i, CMD_CKSUM),
        4..=59 => name_at(i, 4, name),
        60..=63 => offset.to_le_bytes()[i - 60],
        64..=67 => start.to_le_bytes()[i - 64],
        68..=71 => len.to_le_bytes()[i - 68],
        _ => 0,
    }
}

/// MADT local APIC address field.
pub fn acpi_madt_lapic_addr() -> u32 {
    0xFEE0_0000
}

/// MADT I/O APIC address field.
pub fn acpi_madt_ioapic_addr() -> u32 {
    0xFEC0_0000
}

#[cfg(test)]
#[path = "guest_acpi_test.rs"]
mod guest_acpi_test;
