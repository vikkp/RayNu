use super::{
    acpi_pm_timer_reads, boot_menu_wait_skips_bds, boot_order_cd_then_disk, bootorder_nul_terminated, cmos_above_16m_chunks,
    cmos_extended_kb, cmos_mem_served, e820_byte, fwcfg_boot_wait_served, fwcfg_bootorder_served,
    fwcfg_e820_served, fwcfg_ram_served, host_bridge_enumerated, host_pci_config_addr, hpet_init_sink,
    hpet_tick_sink, hpet_tick_sink_by, io, is_acpi_pm_timer_io, is_hpet_gpa, is_kbc_port, is_pic_port,
    is_piix_pm_io, is_platform_io_port, is_platform_sink_gpa, is_xapic_2m_gpa, last_cmos_index,
    pci_addr_selects_host, pci_addr_selects_isa, pci_addr_selects_pm, pci_cfg_offset,
    pci_header_is_multifunction, pci_read_data, pci_write_addr, pci_write_data,
    platform_memory_served, pm_pci_config_addr, reset, ACPI_PM_STEP, BOOTORDER, BOOT_MENU_WAIT,
    E820_ENTRY_BYTES, E820_FILE_BYTES, E820_RAM, E820_RESERVED, FW_CFG_BOOTORDER_SEL, FW_CFG_BOOT_MENU, FW_CFG_BOOT_WAIT_SEL,
    FW_CFG_E820_SEL, FW_CFG_NAMED_FILE_COUNT, HOST_BRIDGE_DEVICE, HOST_BRIDGE_VENDOR, HPET_CAP_REV,
    HPET_CLK_PERIOD_FS, HPET_GPA, HPET_MAIN_STEP, HPET_SINK_OFF, HV_IDENTITY_PML4, HV_IDENTITY_PML4_BYTES, ISA_BRIDGE_DEVICE,
    ISA_BRIDGE_VENDOR, PCI_HEADER_MULTIFUNCTION, PLATFORM_RAM_BYTES, PM_BRIDGE_DEVICE,
    PM_BRIDGE_VENDOR,
};
use crate::memory::ept_hw::GUEST_UEFI_LOW_RAM_BYTES;

#[test]
fn ram_window_matches_guest_uefi_ept() {
    assert_eq!(PLATFORM_RAM_BYTES, GUEST_UEFI_LOW_RAM_BYTES);
    assert_eq!(cmos_above_16m_chunks(PLATFORM_RAM_BYTES), 0x0100);
    assert_eq!(cmos_extended_kb(PLATFORM_RAM_BYTES), 15 * 1024);
    assert_eq!(cmos_above_16m_chunks(16 * 1024 * 1024), 0);
}

#[test]
fn cmos_reports_honest_32m_not_0xff() {
    reset();
    let _ = io(0x70, false, 1, 0x35);
    let hi = io(0x71, true, 1, 0) as u8;
    let _ = io(0x70, false, 1, 0x34);
    let lo = io(0x71, true, 1, 0) as u8;
    assert_eq!(lo, 0x00);
    assert_eq!(hi, 0x01);
    assert_ne!(hi, 0xFF);
    assert!(cmos_mem_served());
    assert!(platform_memory_served());
    reset();
    assert!(!cmos_mem_served());
}

#[test]
fn fwcfg_signature_and_ram_size() {
    reset();
    let _ = io(0x510, false, 2, 0x00);
    let mut sig = [0u8; 4];
    for b in &mut sig {
        *b = io(0x511, true, 1, 0) as u8;
    }
    assert_eq!(&sig, b"QEMU");
    let _ = io(0x510, false, 2, 0x03);
    let mut ram = 0u64;
    for i in 0..8 {
        ram |= (io(0x511, true, 1, 0) & 0xff) << (8 * i);
    }
    assert_eq!(ram, PLATFORM_RAM_BYTES);
    assert!(fwcfg_ram_served());
    reset();
}

#[test]
fn fwcfg_bootorder_is_cd_then_disk() {
    reset();
    assert!(boot_order_cd_then_disk());
    assert!(bootorder_nul_terminated());
    assert_eq!(*BOOTORDER.last().unwrap(), 0);
    let _ = io(0x510, false, 2, 0x19);
    let mut count = 0u32;
    for i in 0..4 {
        count |= (io(0x511, true, 1, 0) as u32) << (8 * (3 - i));
    }
    assert_eq!(count, FW_CFG_NAMED_FILE_COUNT);
    reset();
    let _ = io(0x510, false, 2, u64::from(FW_CFG_BOOTORDER_SEL));
    let mut got = [0u8; 128];
    let n = BOOTORDER.len();
    for b in got.iter_mut().take(n) {
        *b = io(0x511, true, 1, 0) as u8;
    }
    assert_eq!(&got[..n], BOOTORDER);
    assert!(BOOTORDER.windows(8).any(|w| w == b"drive@0/"));
    assert!(!BOOTORDER.windows(8).any(|w| w == b"drive@1/"));
    assert!(BOOTORDER.starts_with(b"/pci@i0cf8/ide@1,1/drive@0"));
    assert!(fwcfg_bootorder_served());
    reset();
}

#[test]
fn fwcfg_e820_is_32m_ram() {
    reset();
    assert_eq!(E820_ENTRY_BYTES, 20);
    assert_eq!(E820_FILE_BYTES, 60);
    assert_eq!(E820_RAM, 1);
    assert_eq!(E820_RESERVED, 2);
    assert_eq!(HV_IDENTITY_PML4, 0x200000);
    assert_eq!(HV_IDENTITY_PML4_BYTES, 0xB000);
    let _ = io(0x510, false, 2, u64::from(FW_CFG_E820_SEL));
    let mut buf = [0u8; 60];
    for b in &mut buf {
        *b = io(0x511, true, 1, 0) as u8;
    }
    for i in 0..60 {
        assert_eq!(buf[i], e820_byte(i as u16));
    }
    assert_eq!(&buf[0..8], &0u64.to_le_bytes());
    assert_eq!(&buf[8..16], &HV_IDENTITY_PML4.to_le_bytes());
    assert_eq!(&buf[16..20], &E820_RAM.to_le_bytes());
    assert_eq!(&buf[20..28], &HV_IDENTITY_PML4.to_le_bytes());
    assert_eq!(&buf[28..36], &HV_IDENTITY_PML4_BYTES.to_le_bytes());
    assert_eq!(&buf[36..40], &E820_RESERVED.to_le_bytes());
    let rest = PLATFORM_RAM_BYTES - HV_IDENTITY_PML4 - HV_IDENTITY_PML4_BYTES;
    assert_eq!(&buf[40..48], &(HV_IDENTITY_PML4 + HV_IDENTITY_PML4_BYTES).to_le_bytes());
    assert_eq!(&buf[48..56], &rest.to_le_bytes());
    assert_eq!(&buf[56..60], &E820_RAM.to_le_bytes());
    assert!(fwcfg_e820_served());
    assert!(platform_memory_served());
    reset();
    assert!(!fwcfg_e820_served());
}

#[test]
fn fwcfg_boot_menu_wait_is_zero_ms() {
    reset();
    assert!(boot_menu_wait_skips_bds());
    assert_eq!(BOOT_MENU_WAIT, [0, 0]);
    assert_eq!(FW_CFG_NAMED_FILE_COUNT, 3);
    assert_eq!(FW_CFG_BOOT_MENU, 0x0E);
    assert_eq!(FW_CFG_BOOT_WAIT_SEL, 0x22);
    let _ = io(0x510, false, 2, u64::from(FW_CFG_BOOT_MENU));
    let mut menu = 0u16;
    menu |= io(0x511, true, 1, 0) as u16;
    menu |= (io(0x511, true, 1, 0) as u16) << 8;
    assert_eq!(menu, 1, "menu=on so OVMF reads etc/boot-menu-wait");
    let _ = io(0x510, false, 2, u64::from(FW_CFG_BOOT_WAIT_SEL));
    let mut wait = 0u16;
    wait |= io(0x511, true, 1, 0) as u16;
    wait |= (io(0x511, true, 1, 0) as u16) << 8;
    assert_eq!(wait, 0);
    assert!(fwcfg_boot_wait_served());
    let _ = io(0x510, false, 2, 0x19);
    let mut name = [0u8; 56];
    for _ in 0..(4 + 64 * 2 + 8) {
        let _ = io(0x511, true, 1, 0);
    }
    for b in &mut name {
        *b = io(0x511, true, 1, 0) as u8;
    }
    let end = name.iter().position(|&b| b == 0).unwrap_or(name.len());
    assert_eq!(&name[..end], b"etc/boot-menu-wait");
    reset();
    assert!(!fwcfg_boot_wait_served());
}

#[test]
fn pic_raz_not_0xff_on_command_port() {
    reset();
    assert!(is_pic_port(0x20));
    assert!(is_pic_port(0xA1));
    assert!(is_platform_io_port(0x20));
    assert_eq!(io(0x20, true, 1, 0xFFFF) as u8, 0);
    assert_ne!(io(0x20, true, 1, 0xFFFF) as u8, 0xFF);
    assert_eq!(io(0x21, true, 1, 0) as u8, 0xFF);
    let _ = io(0x21, false, 1, 0xFB);
    assert_eq!(io(0x21, true, 1, 0) as u8, 0xFB);
    reset();
}

#[test]
fn cmos_index_is_latched() {
    reset();
    let _ = io(0x70, false, 1, 0x8F);
    assert_eq!(last_cmos_index(), 0x0F);
    let _ = io(0x71, true, 1, 0);
    assert_eq!(last_cmos_index(), 0x0F);
    reset();
    assert_eq!(last_cmos_index(), 0);
}

#[test]
fn i440fx_host_and_isa_enumerate() {
    reset();
    pci_write_addr(0x8000_0000);
    assert!(pci_read_data(0xCFC, 4).is_none());
    assert!(!pci_addr_selects_host(0x8000_0000));
    pci_write_addr(host_pci_config_addr());
    let id = pci_read_data(0xCFC, 4).expect("host");
    assert_eq!(id as u16, HOST_BRIDGE_VENDOR);
    assert_eq!((id >> 16) as u16, HOST_BRIDGE_DEVICE);
    assert!(pci_addr_selects_host(host_pci_config_addr()));
    assert!(host_bridge_enumerated());
    pci_write_addr(0x8000_0800);
    let isa = pci_read_data(0xCFC, 4).expect("isa");
    assert_eq!(isa as u16, ISA_BRIDGE_VENDOR);
    assert_eq!((isa >> 16) as u16, ISA_BRIDGE_DEVICE);
    assert!(pci_addr_selects_isa(0x8000_0800));
    pci_write_addr(0x8000_080C);
    let isa_ht = pci_read_data(0xCFC, 4).expect("isa header");
    assert_eq!(isa_ht, PCI_HEADER_MULTIFUNCTION);
    assert!(pci_header_is_multifunction(isa_ht));
    pci_write_addr(0x8000_080C);
    assert_eq!(pci_read_data(0xCFE, 1).expect("ht via cfe"), 0x80);
    // OVMF: CF8 register 0x0E + inb(0xCFC) — must not return Cache Line Size 0.
    pci_write_addr(0x8000_080E);
    assert_eq!(pci_read_data(0xCFC, 1).expect("ht via unaligned cf8"), 0x80);
    assert_eq!(pci_cfg_offset(0x8000_080E, 0xCFC), 0x0E);
    assert_eq!(pci_cfg_offset(0x8000_080C, 0xCFE), 0x0E);
    pci_write_addr(host_pci_config_addr() | 0x0C);
    let host_ht = pci_read_data(0xCFC, 4).expect("host header");
    assert!(!pci_header_is_multifunction(host_ht));
    pci_write_addr(0x8000_0900);
    assert!(pci_read_data(0xCFC, 4).is_none());
    reset();
}

#[test]
fn piix3_isa_pirq_resets_disabled_like_qemu() {
    reset();
    pci_write_addr(0x8000_0860);
    let pirq = pci_read_data(0xCFC, 4).expect("pirq");
    assert_eq!(pirq, 0x8080_8080, "PIRQA-D reset 0x80 (disabled), not IRQ0");
    pci_write_data(0xCFC, 4, 0x0E0B_0A09);
    assert_eq!(pci_read_data(0xCFC, 4).expect("pirq wr"), 0x0E0B_0A09);
    pci_write_addr(0x8000_080C);
    let isa_ht = pci_read_data(0xCFC, 4).expect("ht after pirq");
    assert_eq!(isa_ht, PCI_HEADER_MULTIFUNCTION);
    pci_write_addr(0x8000_0800);
    pci_write_data(0xCFC, 4, 0x1234_5678);
    assert_eq!(
        pci_read_data(0xCFC, 4).expect("vid"),
        u32::from(ISA_BRIDGE_VENDOR) | (u32::from(ISA_BRIDGE_DEVICE) << 16),
        "VID/DID stay 8086:7000"
    );
    reset();
}

#[test]
fn sink_gpa_covers_stage40_fault() {
    assert!(is_platform_sink_gpa(0xFCF8_F000));
    assert!(is_xapic_2m_gpa(0xFEE0_0000));
    assert!(is_xapic_2m_gpa(0xFEE0_1000));
    assert!(!is_platform_sink_gpa(0xFEE0_0000));
    assert!(is_platform_sink_gpa(0xFEC0_0000));
    assert!(is_platform_sink_gpa(0xFED0_0000));
    assert!(!is_platform_sink_gpa(0x0000_1000));
    assert!(is_platform_sink_gpa(0xC000_0000));
    assert!(is_platform_sink_gpa(0xC01D_F1B7));
    assert!(is_platform_sink_gpa(0x8000_0000));
    assert!(!is_platform_sink_gpa(0xFFC0_0000));
    assert!(!is_platform_sink_gpa(0xFFFF_FFF0));
    assert!(is_platform_io_port(0x70));
    assert!(is_platform_io_port(0x510));
    assert!(is_platform_io_port(0x40));
    assert!(!is_platform_io_port(0xCF8));
    assert!(!is_platform_io_port(0x3F8));
    assert!(is_platform_io_port(0x20));
    assert!(is_platform_io_port(0x60));
    assert!(is_platform_io_port(0x64));
    assert!(is_kbc_port(0x60));
    assert!(is_kbc_port(0x64));
    assert!(!is_kbc_port(0x61));
    assert!(is_hpet_gpa(HPET_GPA));
    assert!(is_hpet_gpa(HPET_GPA + 0xF0));
    assert!(!is_hpet_gpa(0xFEC0_0000));
    assert!(is_acpi_pm_timer_io(0, 4));
    assert!(!is_acpi_pm_timer_io(0, 1));
    assert!(is_acpi_pm_timer_io(0x408, 4));
}

#[test]
fn acpi_pm_timer_ticks_port0_and_pmba() {
    reset();
    assert_eq!(acpi_pm_timer_reads(), 0);
    let a = io(0, true, 4, 0) as u32;
    let b = io(0, true, 4, 0) as u32;
    assert_eq!(a, 0);
    assert_eq!(ACPI_PM_STEP, 0x0040_0000);
    assert!(ACPI_PM_STEP >= 3_579_545, ">= 1s of PM ticks");
    assert_eq!(b, ACPI_PM_STEP);
    assert_ne!(b, 0xFFFF_FFFF);
    assert_eq!(acpi_pm_timer_reads(), 2);
    let c = io(0x408, true, 4, 0) as u32;
    assert_eq!(c, ACPI_PM_STEP.wrapping_mul(2));
    assert_eq!(acpi_pm_timer_reads(), 3);
    reset();
    assert_eq!(acpi_pm_timer_reads(), 0);
}

#[test]
fn piix4_pm_enumerates_and_pmba_write_ticks() {
    reset();
    assert_eq!(pm_pci_config_addr(), 0x8000_0B00);
    pci_write_addr(pm_pci_config_addr());
    let id = pci_read_data(0xCFC, 4).expect("pm");
    assert_eq!(id as u16, PM_BRIDGE_VENDOR);
    assert_eq!((id >> 16) as u16, PM_BRIDGE_DEVICE);
    assert!(pci_addr_selects_pm(pm_pci_config_addr()));
    pci_write_addr(pm_pci_config_addr() | 0x40);
    pci_write_data(0xCFC, 4, 0x501);
    assert!(is_acpi_pm_timer_io(0x508, 4));
    let v = io(0x508, true, 4, 0) as u32;
    assert_eq!(v, 0);
    assert_ne!(v, 0xFFFF_FFFF);
    let v2 = io(0x508, true, 4, 0) as u32;
    assert_eq!(v2, ACPI_PM_STEP);
    assert!(is_piix_pm_io(0x500));
    let sts = io(0x500, true, 4, 0xFFFF_FFFF) as u32;
    assert_eq!(sts, 0);
    pci_write_addr(pm_pci_config_addr() | 0x40);
    pci_write_data(0xCFC, 4, 0xB001);
    assert!(is_piix_pm_io(0xB000));
    assert_eq!(io(0xB000, true, 4, 0xFFFF_FFFF) as u32, 0);
    let t = io(0xB008, true, 4, 0) as u32;
    assert_ne!(t, 0xFFFF_FFFF);
    reset();
}

#[test]
fn hpet_lives_in_2mib_sink_and_ticks() {
    assert_eq!(HPET_GPA, 0xFED0_0000);
    assert_eq!(HPET_SINK_OFF, 0x10_0000);
    let mut too_small = vec![0u8; 4096];
    assert!(!hpet_init_sink(&mut too_small));
    assert_eq!(hpet_tick_sink(&mut too_small), 0);
    let mut sink = vec![0u8; 2 * 1024 * 1024];
    assert!(hpet_init_sink(&mut sink));
    let cap = u32::from_le_bytes(sink[HPET_SINK_OFF..HPET_SINK_OFF + 4].try_into().unwrap());
    let period = u32::from_le_bytes(
        sink[HPET_SINK_OFF + 4..HPET_SINK_OFF + 8]
            .try_into()
            .unwrap(),
    );
    let en = u32::from_le_bytes(
        sink[HPET_SINK_OFF + 0x10..HPET_SINK_OFF + 0x14]
            .try_into()
            .unwrap(),
    );
    assert_eq!(cap, HPET_CAP_REV);
    assert_eq!(period, HPET_CLK_PERIOD_FS);
    assert_eq!(en, 1);
    assert_eq!(HPET_MAIN_STEP, 100_000_000);
    assert_eq!(hpet_tick_sink(&mut sink), HPET_MAIN_STEP);
    assert_eq!(hpet_tick_sink(&mut sink), HPET_MAIN_STEP * 2);
    assert_eq!(hpet_tick_sink_by(&mut sink, 0), HPET_MAIN_STEP * 2);
}

#[test]
fn kbc_status_is_not_0xff() {
    reset();
    let st = io(0x64, true, 1, 0) as u8;
    let data = io(0x60, true, 1, 0) as u8;
    assert_eq!(st, 0x10);
    assert_eq!(data, 0);
    let _ = io(0x64, false, 1, 0xAE);
    assert_eq!(io(0x64, true, 1, 0) as u8, 0x10);
}
