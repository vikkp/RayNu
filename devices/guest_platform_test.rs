use super::{
    cmos_above_16m_chunks, cmos_extended_kb, cmos_mem_served, fwcfg_ram_served,
    host_bridge_enumerated, host_pci_config_addr, io, is_platform_io_port, is_platform_sink_gpa,
    pci_addr_selects_host, pci_addr_selects_isa, pci_cfg_offset, pci_header_is_multifunction,
    pci_read_data, pci_write_addr, platform_memory_served, reset, HOST_BRIDGE_DEVICE,
    HOST_BRIDGE_VENDOR, ISA_BRIDGE_DEVICE, ISA_BRIDGE_VENDOR, PCI_HEADER_MULTIFUNCTION,
    PLATFORM_RAM_BYTES,
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
fn i440fx_host_and_isa_enumerate() {
    reset();
    pci_write_addr(0x8000_0000);
    assert!(pci_read_data(0xCFC, 4).is_none());
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
fn sink_gpa_covers_stage40_fault() {
    assert!(is_platform_sink_gpa(0xFCF8_F000));
    assert!(is_platform_sink_gpa(0xFEE0_0000));
    assert!(is_platform_sink_gpa(0xFEC0_0000));
    assert!(!is_platform_sink_gpa(0x0000_1000));
    assert!(!is_platform_sink_gpa(0xFFC0_0000));
    assert!(!is_platform_sink_gpa(0xFFFF_FFF0));
    assert!(is_platform_io_port(0x70));
    assert!(is_platform_io_port(0x510));
    assert!(is_platform_io_port(0x40));
    assert!(!is_platform_io_port(0xCF8));
    assert!(!is_platform_io_port(0x3F8));
}
