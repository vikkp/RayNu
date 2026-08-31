use super::{
    acpi_pm_timer_reads, boot_menu_wait_skips_bds, boot_order_cd_then_disk, boot_order_product_virtio_iso_first, bootorder_bytes, bootorder_nul_terminated, cmos_above_16m_chunks,
    cmos_extended_kb, cmos_mem_served, e820_byte, e820_splits_gcd_mid_gap, e820_splits_mtrr_uc_hole, e820_splits_vga_below_1m, fwcfg_boot_wait_served,
    fwcfg_bootorder_served,
    fwcfg_e820_served, fwcfg_file_dir_served, fwcfg_ram_served, fwcfg_acpi_served, fwcfg_named_file_count, host_bridge_enumerated, host_pci_config_addr, hpet_init_sink,
    hpet_tick_sink, hpet_tick_sink_by, hpet_ticks_from_tsc_delta, io, is_acpi_pm_timer_io, is_hpet_gpa, is_kbc_port, is_pic_port,
    is_piix_pm_io, is_acpi_pm1_io, is_platform_io_port, is_platform_sink_gpa, is_unbacked_report_ram_gpa, is_xapic_2m_gpa, is_fwcfg_data_port, last_cmos_index,
    pci_addr_selects_host, pci_addr_selects_isa, pci_addr_selects_pm, pci_cfg_offset,
    pci_header_is_multifunction, pci_read_data, pci_write_addr, pci_write_data,
    platform_memory_served, platform_reports_2g_lowmem, pm_pci_config_addr, reset, ACPI_PM_STEP, BOOTORDER, BOOTORDER_PRODUCT, BOOT_MENU_WAIT,
    E820_ENTRY_BYTES, E820_ENTRY_COUNT, E820_FILE_BYTES, E820_MID_GAP_BASE, E820_MID_GAP_BYTES,
    E820_PCI_UC_BASE, E820_PCI_UC_BYTES, E820_RAM, E820_VGA_BASE, E820_VGA_BYTES, E820_LOW_1M,
    E820_RESERVED, FW_CFG_BOOTORDER_SEL, FW_CFG_BOOT_MENU, FW_CFG_BOOT_WAIT_SEL,
    FW_CFG_E820_SEL, FW_CFG_NAMED_FILE_COUNT, HOST_BRIDGE_DEVICE, HOST_BRIDGE_VENDOR, HPET_CAP_REV,
    HPET_CLK_PERIOD_FS, HPET_GPA, HPET_INSN_STEP, HPET_MAIN_STEP, HPET_SINK_OFF, HPET_UART_IO_STEP_CAP, HV_IDENTITY_PML4, HV_IDENTITY_PML4_BYTES, ISA_BRIDGE_DEVICE, TSC_PER_HPET_TICK,
    ISA_BRIDGE_VENDOR, PCI_HEADER_MULTIFUNCTION, PLATFORM_RAM_BYTES, PLATFORM_REPORT_RAM_BYTES, PM_BRIDGE_DEVICE,
    PM_BRIDGE_VENDOR, PM1_CNT_SCI_EN, PIIX4_PMBA_ALT,
};
use crate::memory::ept_hw::GUEST_UEFI_LOW_RAM_BYTES;

#[test]
fn ram_window_matches_guest_uefi_ept() {
    assert_eq!(PLATFORM_RAM_BYTES, GUEST_UEFI_LOW_RAM_BYTES);
    assert_eq!(cmos_above_16m_chunks(PLATFORM_RAM_BYTES), 0x0100);
    assert_eq!(cmos_extended_kb(PLATFORM_RAM_BYTES), 15 * 1024);
    assert_eq!(cmos_above_16m_chunks(16 * 1024 * 1024), 0);
    assert_eq!(cmos_above_16m_chunks(PLATFORM_REPORT_RAM_BYTES), 0x7F00);
    assert_eq!(cmos_extended_kb(PLATFORM_REPORT_RAM_BYTES), 15 * 1024);
    assert!(platform_reports_2g_lowmem());
}

#[test]
fn cmos_reports_honest_32m_not_0xff() {
    reset();
    let _ = io(0x70, false, 1, 0x35);
    let hi = io(0x71, true, 1, 0) as u8;
    let _ = io(0x70, false, 1, 0x34);
    let lo = io(0x71, true, 1, 0) as u8;
    assert_eq!(lo, 0x00);
    assert_eq!(hi, 0x7F);
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
    assert_eq!(ram, PLATFORM_REPORT_RAM_BYTES);
    assert!(fwcfg_ram_served());
    reset();
}

#[test]
fn fwcfg_bootorder_is_cd_then_disk() {
    crate::devices::ide_cdrom::reset();
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
fn product_iso_fwcfg_bootorder_virtio_iso_first() {
    crate::devices::ide_cdrom::reset();
    reset();
    assert!(boot_order_cd_then_disk());
    assert!(boot_order_product_virtio_iso_first());
    assert!(bootorder_nul_terminated());
    assert_eq!(bootorder_bytes(), BOOTORDER);
    assert!(BOOTORDER.starts_with(b"/pci@i0cf8/ide@1,1/drive@0"));
    assert!(BOOTORDER_PRODUCT.starts_with(b"/pci@i0cf8/scsi@3/disk@0,0"));
    assert!(!BOOTORDER_PRODUCT.windows(4).any(|w| w == b"ide@"));
    let extra = crate::devices::ide_cdrom::MOCK_EFI_ISO_BYTES + crate::devices::ide_cdrom::ISO_SECTOR;
    let mut iso = vec![0u8; extra];
    crate::devices::ide_cdrom::write_placeholder_iso(
        &mut iso[..crate::devices::ide_cdrom::MOCK_EFI_ISO_BYTES],
    );
    assert!(crate::devices::ide_cdrom::present(&iso, 9));
    assert!(crate::devices::ide_cdrom::product_iso_window_armed());
    assert_eq!(bootorder_bytes(), BOOTORDER_PRODUCT);
    let _ = io(0x510, false, 2, u64::from(FW_CFG_BOOTORDER_SEL));
    let n = BOOTORDER_PRODUCT.len();
    let mut got = vec![0u8; n];
    for b in got.iter_mut() {
        *b = io(0x511, true, 1, 0) as u8;
    }
    assert_eq!(&got[..], BOOTORDER_PRODUCT);
    assert!(fwcfg_bootorder_served());
    crate::devices::ide_cdrom::reset();
    reset();
    assert_eq!(bootorder_bytes(), BOOTORDER);
}

#[test]
fn fwcfg_e820_is_32m_ram() {
    reset();
    assert_eq!(E820_ENTRY_BYTES, 20);
    assert_eq!(E820_ENTRY_COUNT, 6);
    assert_eq!(E820_FILE_BYTES, 120);
    assert_eq!(E820_RAM, 1);
    assert_eq!(E820_RESERVED, 2);
    assert_eq!(HV_IDENTITY_PML4, 0x400000);
    assert_eq!(HV_IDENTITY_PML4_BYTES, 0x1B000);
    assert_eq!(E820_VGA_BASE, 0xA0000);
    assert_eq!(E820_VGA_BYTES, 0x60000);
    assert_eq!(E820_LOW_1M, 0x100000);
    assert_eq!(E820_MID_GAP_BASE, 32 * 1024 * 1024);
    assert_eq!(E820_MID_GAP_BYTES, 0x8000_0000 - 32 * 1024 * 1024);
    assert_eq!(E820_PCI_UC_BASE, 0x8000_0000);
    assert_eq!(E820_PCI_UC_BYTES, 0x8000_0000);
    assert!(!e820_splits_gcd_mid_gap());
    assert!(e820_splits_vga_below_1m());
    assert!(e820_splits_mtrr_uc_hole());
    assert!(platform_reports_2g_lowmem());
    assert!(!fwcfg_file_dir_served());
    let _ = io(0x510, false, 2, 0x19);
    assert!(fwcfg_file_dir_served(), "PEI QemuFwCfgFindFile reads file dir");
    let _ = io(0x510, false, 2, u64::from(FW_CFG_E820_SEL));
    let mut buf = [0u8; 120];
    for b in &mut buf {
        *b = io(0x511, true, 1, 0) as u8;
    }
    for i in 0..120 {
        assert_eq!(buf[i], e820_byte(i as u16));
    }
    assert_eq!(&buf[0..8], &0u64.to_le_bytes());
    assert_eq!(&buf[8..16], &E820_VGA_BASE.to_le_bytes());
    assert_eq!(&buf[16..20], &E820_RAM.to_le_bytes());
    assert_eq!(&buf[20..28], &E820_VGA_BASE.to_le_bytes());
    assert_eq!(&buf[28..36], &E820_VGA_BYTES.to_le_bytes());
    assert_eq!(&buf[36..40], &E820_RESERVED.to_le_bytes());
    assert_eq!(&buf[40..48], &E820_LOW_1M.to_le_bytes());
    assert_eq!(
        &buf[48..56],
        &(HV_IDENTITY_PML4 - E820_LOW_1M).to_le_bytes()
    );
    assert_eq!(&buf[56..60], &E820_RAM.to_le_bytes());
    assert_eq!(&buf[60..68], &HV_IDENTITY_PML4.to_le_bytes());
    assert_eq!(&buf[68..76], &HV_IDENTITY_PML4_BYTES.to_le_bytes());
    assert_eq!(&buf[76..80], &E820_RESERVED.to_le_bytes());
    let rest = PLATFORM_REPORT_RAM_BYTES - HV_IDENTITY_PML4 - HV_IDENTITY_PML4_BYTES;
    assert_eq!(
        &buf[80..88],
        &(HV_IDENTITY_PML4 + HV_IDENTITY_PML4_BYTES).to_le_bytes()
    );
    assert_eq!(&buf[88..96], &rest.to_le_bytes());
    assert_eq!(&buf[96..100], &E820_RAM.to_le_bytes());
    assert_eq!(&buf[100..108], &E820_PCI_UC_BASE.to_le_bytes());
    assert_eq!(&buf[108..116], &E820_PCI_UC_BYTES.to_le_bytes());
    assert_eq!(&buf[116..120], &E820_RESERVED.to_le_bytes());
    assert!(fwcfg_e820_served());
    assert!(platform_memory_served());
    reset();
    assert!(!fwcfg_e820_served());
    assert!(!fwcfg_file_dir_served());
}

#[test]
fn fwcfg_boot_menu_wait_is_zero_ms() {
    reset();
    assert!(boot_menu_wait_skips_bds());
    assert_eq!(BOOT_MENU_WAIT, [0, 0]);
    assert_eq!(FW_CFG_NAMED_FILE_COUNT, 3);
    assert_eq!(fwcfg_named_file_count(), 3, "iso=0 named files stay 3");
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
fn product_iso_fwcfg_acpi_dir_has_six_files() {
    crate::devices::ide_cdrom::reset();
    reset();
    assert_eq!(fwcfg_named_file_count(), 3, "iso=0 named files stay 3");
    assert!(!fwcfg_acpi_served());
    let extra = crate::devices::ide_cdrom::MOCK_EFI_ISO_BYTES + crate::devices::ide_cdrom::ISO_SECTOR;
    let mut iso = vec![0u8; extra];
    crate::devices::ide_cdrom::write_placeholder_iso(
        &mut iso[..crate::devices::ide_cdrom::MOCK_EFI_ISO_BYTES],
    );
    assert!(crate::devices::ide_cdrom::present(&iso, 9));
    assert!(crate::devices::ide_cdrom::product_iso_window_armed());
    assert_eq!(fwcfg_named_file_count(), 6);
    let _ = io(0x510, false, 2, 0x19);
    let mut count = 0u32;
    for i in 0..4 {
        count |= (io(0x511, true, 1, 0) as u32) << (8 * (3 - i));
    }
    assert_eq!(count, 6);
    let mut name = [0u8; 56];
    for _ in 0..(64 * 3 + 8) {
        let _ = io(0x511, true, 1, 0);
    }
    for b in &mut name {
        *b = io(0x511, true, 1, 0) as u8;
    }
    let end = name.iter().position(|&b| b == 0).unwrap_or(name.len());
    assert_eq!(&name[..end], b"etc/table-loader");
    let _ = io(
        0x510,
        false,
        2,
        u64::from(crate::devices::guest_acpi::FW_CFG_ACPI_TABLES_SEL),
    );
    let mut sig = [0u8; 4];
    for b in &mut sig {
        *b = io(0x511, true, 1, 0) as u8;
    }
    assert_eq!(&sig, b"RSDT");
    assert!(fwcfg_acpi_served());
    crate::devices::ide_cdrom::reset();
    reset();
    assert_eq!(fwcfg_named_file_count(), 3);
    assert!(!fwcfg_acpi_served());
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
fn piix3_isa_bar_raz_like_qemu() {
    reset();
    pci_write_addr(0x8000_0810);
    pci_write_data(0xCFC, 4, 0xFFFF_FFFF);
    assert_eq!(
        pci_read_data(0xCFC, 4).expect("bar0"),
        0,
        "PIIX3 ISA BAR RAZ"
    );
    pci_write_addr(0x8000_0814);
    pci_write_data(0xCFC, 4, 0xC058_0000);
    assert_eq!(pci_read_data(0xCFC, 4).expect("bar1"), 0);
    pci_write_addr(0x8000_0830);
    pci_write_data(0xCFC, 4, 0xFFFF_FFFF);
    assert_eq!(pci_read_data(0xCFC, 4).expect("rom"), 0);
    pci_write_addr(0x8000_0860);
    pci_write_data(0xCFC, 4, 0x0E0B_0A09);
    assert_eq!(
        pci_read_data(0xCFC, 4).expect("pirq still wr"),
        0x0E0B_0A09
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
    assert!(!is_platform_sink_gpa(PLATFORM_RAM_BYTES));
    assert!(is_unbacked_report_ram_gpa(PLATFORM_RAM_BYTES));
    assert!(is_unbacked_report_ram_gpa(0x7BDD_D000));
    assert!(!is_unbacked_report_ram_gpa(0x8000_0000));
    assert!(is_platform_sink_gpa(0xC000_0000));
    assert!(is_platform_sink_gpa(0xC01D_F1B7));
    assert!(is_platform_sink_gpa(0x8000_0000));
    assert!(!is_platform_sink_gpa(0xFFC0_0000));
    assert!(!is_platform_sink_gpa(0xFFFF_FFF0));
    assert!(is_platform_sink_gpa(0xFE00_0000), "lab stub: virtio BAR stays sink");
    assert!(is_platform_io_port(0x70));
    assert!(is_platform_io_port(0x510));
    assert!(is_fwcfg_data_port(0x511));
    assert!(!is_fwcfg_data_port(0x510));
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
    assert!(is_acpi_pm_timer_io(0xAF00, 4));
    assert!(is_acpi_pm_timer_io(0xAF08, 4));
    assert!(is_acpi_pm_timer_io(0xB000, 4));
    assert!(!is_acpi_pm_timer_io(0xB000, 2));
    assert!(is_platform_io_port(0xAF00));
    assert!(is_acpi_pm1_io(0xAF00));
}

#[test]
fn port61_tmr2_out_toggles_so_ovmf_delay_exits() {
    reset();
    // Iron after BdsDxe Start CD: `in al,0x61; test al,0x20; jz` at 0x7e149fb9.
    let a = io(0x61, true, 1, 0) as u8;
    assert_ne!(a & 0x20, 0, "first IN must set TMR2_OUT so jz does not spin");
    let b = io(0x61, true, 1, 0) as u8;
    assert_ne!(a & 0x20, b & 0x20, "TMR2_OUT must toggle for wait-until-change");
    assert_ne!(a & 0x10, b & 0x10, "refresh bit 4 still toggles");
}

#[test]
fn pit_channel0_16bit_latch_roundtrip() {
    reset();
    let a = io(0x40, true, 1, 0) as u8;
    let b = io(0x40, true, 1, 0) as u8;
    assert_ne!(a, b, "OVMF delay loops need a moving 0x40 count");
    reset();
    // Mode 2, lo/hi, channel 0 (Linux i8253).
    let _ = io(0x43, false, 1, 0x34);
    let _ = io(0x40, false, 1, 0x00);
    let _ = io(0x40, false, 1, 0x10);
    let _ = io(0x43, false, 1, 0x00);
    let lo = io(0x40, true, 1, 0) as u8;
    let hi = io(0x40, true, 1, 0) as u8;
    assert_eq!(u16::from(lo) | (u16::from(hi) << 8), 0x1000);
    crate::devices::guest_platform::pit_tick();
    let _ = io(0x43, false, 1, 0x00);
    let lo2 = io(0x40, true, 1, 0) as u8;
    let hi2 = io(0x40, true, 1, 0) as u8;
    let next = u16::from(lo2) | (u16::from(hi2) << 8);
    assert!(next < 0x1000, "pit_tick must lower the latched count");
    reset();
    let _ = io(0x43, false, 1, 0x34);
    let _ = io(0x40, false, 1, 0x00);
    let _ = io(0x40, false, 1, 0x10);
    let ulo = io(0x40, true, 1, 0) as u8;
    let uhi = io(0x40, true, 1, 0) as u8;
    assert_eq!(u16::from(ulo) | (u16::from(uhi) << 8), 0x1000);
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
fn af00_pm_timer_ticks_dword_in() {
    reset();
    assert_eq!(PIIX4_PMBA_ALT, 0xAF00);
    assert!(is_platform_io_port(0xAF00));
    assert!(is_acpi_pm1_io(0xAF00));
    assert!(is_acpi_pm1_io(0xAF04));
    assert!(is_acpi_pm_timer_io(0xAF00, 4));
    assert!(!is_acpi_pm_timer_io(0xAF00, 1));
    assert!(is_acpi_pm_timer_io(0xAF08, 4));
    let a = io(0xAF00, true, 4, 0) as u32;
    let b = io(0xAF00, true, 4, 0) as u32;
    assert_eq!(a, 0);
    assert_eq!(b, ACPI_PM_STEP);
    assert_ne!(b, 0xFFFF_FFFF);
    assert_eq!(acpi_pm_timer_reads(), 2);
    let t = io(0xAF08, true, 4, 0) as u32;
    assert_eq!(t, ACPI_PM_STEP.wrapping_mul(2));
    let _ = io(0xAF04, false, 2, u64::from(PM1_CNT_SCI_EN));
    assert_eq!(io(0xAF04, true, 2, 0) as u16, PM1_CNT_SCI_EN);
    reset();
}

#[test]
fn b000_pm_timer_ticks_dword_in() {
    reset();
    assert!(is_platform_io_port(0xB000));
    assert!(is_acpi_pm1_io(0xB000));
    assert!(is_acpi_pm1_io(0xB004));
    assert!(is_acpi_pm_timer_io(0xB000, 4));
    assert!(!is_acpi_pm_timer_io(0xB000, 2));
    assert!(!is_acpi_pm_timer_io(0xB000, 1));
    assert!(is_acpi_pm_timer_io(0xB008, 4));
    assert_eq!(io(0xB000, true, 2, 0xFFFF) as u16, 0);
    let a = io(0xB000, true, 4, 0) as u32;
    let b = io(0xB000, true, 4, 0) as u32;
    assert_eq!(a, 0);
    assert_eq!(b, ACPI_PM_STEP);
    assert_ne!(b, 0xFFFF_FFFF);
    assert_eq!(acpi_pm_timer_reads(), 2);
    let t = io(0xB008, true, 4, 0) as u32;
    assert_eq!(t, ACPI_PM_STEP.wrapping_mul(2));
    let _ = io(0xB004, false, 2, u64::from(PM1_CNT_SCI_EN));
    assert_eq!(io(0xB004, true, 2, 0) as u16, PM1_CNT_SCI_EN);
    reset();
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
    assert_eq!(io(0xB000, true, 2, 0xFFFF) as u16, 0);
    let d0 = io(0xB000, true, 4, 0) as u32;
    let d1 = io(0xB000, true, 4, 0) as u32;
    assert_eq!(d1.wrapping_sub(d0), ACPI_PM_STEP);
    let t = io(0xB008, true, 4, 0) as u32;
    assert_ne!(t, 0xFFFF_FFFF);
    reset();
}

#[test]
fn piix4_pm1_sci_en_sticky_on_fadt() {
    reset();
    assert!(is_acpi_pm1_io(0xB000));
    assert!(is_acpi_pm1_io(0xB004));
    assert!(!is_acpi_pm1_io(0xB008));
    assert!(is_platform_io_port(0xB004));
    assert_eq!(io(0xB004, true, 2, 0xFFFF) as u16, PM1_CNT_SCI_EN);
    let _ = io(0xB004, false, 2, 0);
    assert_eq!(io(0xB004, true, 2, 0) as u16, 0);
    let _ = io(0xB004, false, 2, u64::from(PM1_CNT_SCI_EN));
    assert_eq!(io(0xB004, true, 2, 0) as u16, PM1_CNT_SCI_EN);
    let _ = io(0xB004, false, 2, u64::from(PM1_CNT_SCI_EN) | (1 << 13));
    assert_eq!(io(0xB004, true, 2, 0) as u16, PM1_CNT_SCI_EN);
    assert!(is_acpi_pm_timer_io(0xB000, 4));
    assert_eq!(io(0xB000, true, 2, 0xFFFF) as u16, 0);
    assert_eq!(io(0xB000, true, 4, 0xFFFF_FFFF) as u32, 0);
    pci_write_addr(pm_pci_config_addr() | 0x40);
    pci_write_data(0xCFC, 4, 0xB001);
    assert!(is_acpi_pm1_io(0xB004));
    assert_eq!(io(0xB004, true, 2, 0) as u16, PM1_CNT_SCI_EN);
    reset();
    assert_eq!(io(0xB004, true, 2, 0) as u16, PM1_CNT_SCI_EN);
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
    assert_eq!(HPET_INSN_STEP, 100_000);
    assert!(HPET_INSN_STEP < HPET_MAIN_STEP);
    assert_eq!(HPET_UART_IO_STEP_CAP, 400);
    assert!(HPET_UART_IO_STEP_CAP < HPET_INSN_STEP);
    assert_eq!(TSC_PER_HPET_TICK, 21);
    assert_eq!(hpet_ticks_from_tsc_delta(0), 0);
    assert_eq!(hpet_ticks_from_tsc_delta(TSC_PER_HPET_TICK), 1);
    assert_eq!(
        hpet_ticks_from_tsc_delta(TSC_PER_HPET_TICK * HPET_UART_IO_STEP_CAP + 1),
        HPET_UART_IO_STEP_CAP
    );
    assert_eq!(hpet_ticks_from_tsc_delta(u64::MAX), HPET_UART_IO_STEP_CAP);
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

#[test]
fn kbc_self_test_and_reset_set_obf() {
    reset();
    let _ = io(0x64, false, 1, 0xAA);
    let st = io(0x64, true, 1, 0) as u8;
    assert_eq!(st & 0x01, 0x01, "OBF after self-test");
    assert_ne!(st, 0xFF);
    assert_eq!(io(0x60, true, 1, 0) as u8, 0x55);
    assert_eq!(io(0x64, true, 1, 0) as u8 & 0x01, 0);
    let _ = io(0x60, false, 1, 0xFF);
    assert_eq!(io(0x60, true, 1, 0) as u8, 0xFA);
    assert_eq!(io(0x60, true, 1, 0) as u8, 0xAA);
}
