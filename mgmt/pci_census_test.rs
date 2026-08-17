use super::{
    acpi_sig_is_dmar, census_nic_has_lab_driver, find_msix_entries, iron_marker_allowed,
    is_network_class, is_rsdp_signature, msix_table_entries, pci_id_is_iron_census, pick_lab_or_none,
    PciNicRecord, PCI_CLASS_NETWORK,
};
use crate::mgmt::e1000_mmio::{E1000_DEVICE, E1000_VENDOR};

#[test]
fn network_class_and_dmar_sig() {
    assert!(is_network_class(PCI_CLASS_NETWORK));
    assert!(!is_network_class(0x01));
    assert!(acpi_sig_is_dmar(b"DMAR"));
    assert!(!acpi_sig_is_dmar(b"APIC"));
    assert!(is_rsdp_signature(b"RSD PTR "));
}

#[test]
fn msix_table_size_from_ctrl() {
    assert_eq!(msix_table_entries(0), 1);
    assert_eq!(msix_table_entries(7), 8);
    assert_eq!(msix_table_entries(0x7FF), 0x800);
}

#[test]
fn find_msix_in_mocked_cap_list() {
    // cap at 0x40: id=0x11, next=0, msg_ctrl=0x0007 → 8 entries
    let entries = find_msix_entries(1 << 4, 0x40, |off| {
        if off == 0x40 {
            0x0007_00_11
        } else {
            0
        }
    });
    assert_eq!(entries, Some(8));
}

#[test]
fn pick_lab_only_qemu_e1000() {
    let bcm = PciNicRecord {
        bus: 0,
        dev: 1,
        func: 0,
        vendor: 0x14e4,
        device: 0x165f,
        class: PCI_CLASS_NETWORK,
        bar0: 0,
        msix_entries: None,
    };
    let e1000 = PciNicRecord {
        bus: 0,
        dev: 3,
        func: 0,
        vendor: E1000_VENDOR,
        device: E1000_DEVICE,
        class: PCI_CLASS_NETWORK,
        bar0: 0xFEBD_0000,
        msix_entries: Some(5),
    };
    assert!(pick_lab_or_none(&[bcm]).is_none());
    let p = pick_lab_or_none(&[bcm, e1000]).unwrap();
    assert_eq!(p.device, E1000_DEVICE);
    assert!(census_nic_has_lab_driver(E1000_VENDOR, E1000_DEVICE));
    assert!(!census_nic_has_lab_driver(0x14e4, 0x165f));
    assert!(pci_id_is_iron_census(0x14e4, 0x165f));
    assert!(!pci_id_is_iron_census(0x14e4, 0x1657));
    assert!(!iron_marker_allowed(E1000_VENDOR, E1000_DEVICE));
    assert!(iron_marker_allowed(0x14e4, 0x165f));
    assert!(!iron_marker_allowed(0x14e4, 0x1657));
}
