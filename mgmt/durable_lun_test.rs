//! Host tests for the iron DurableLun mapper.
//!
//! Never prints `RAYNU-V-M7-ISO-INSTALL-OK` or `RAYNU-V-M8-DISK-PERSIST-OK`.

use super::*;
use crate::mgmt::disk_persist::{
    kind_survives_hv_reboot, select_persist_kind, PersistKind, M8_DISK_PERSIST_OK_MARKER,
};

fn nvme(bus: u8, size: u64) -> LunCandidate {
    LunCandidate::pci(LunTransport::Nvme, 0x144D, 0xA80A, bus, 0, 0, size)
}

fn perc_h740() -> LunCandidate {
    LunCandidate::pci(
        LunTransport::Perc,
        PCI_VENDOR_LSI,
        0x0016,
        1,
        0,
        0,
        200 * 1024 * 1024 * 1024,
    )
}

fn cruzer_esp() -> LunCandidate {
    LunCandidate {
        transport: LunTransport::Usb,
        vendor: 0,
        device: 0,
        bus: 0,
        dev: 0,
        func: 0,
        size_bytes: 4 * 1024 * 1024 * 1024,
        is_esp_boot: true,
    }
}

fn usb_data(size: u64) -> LunCandidate {
    LunCandidate {
        transport: LunTransport::Usb,
        vendor: 0,
        device: 0,
        bus: 2,
        dev: 0,
        func: 0,
        size_bytes: size,
        is_esp_boot: false,
    }
}

#[test]
fn perc_h740p_and_raid_class_are_never_picked() {
    assert!(pci_is_perc(
        PCI_VENDOR_LSI,
        0x0016,
        PCI_CLASS_STORAGE,
        PCI_SUBCLASS_RAID
    ));
    assert!(pci_is_perc(
        PCI_VENDOR_LSI,
        0x005D,
        PCI_CLASS_STORAGE,
        PCI_SUBCLASS_NVME
    ));
    assert_eq!(
        classify_pci_storage(PCI_VENDOR_LSI, 0x0016, PCI_CLASS_STORAGE, PCI_SUBCLASS_RAID),
        LunTransport::Perc
    );
    assert_eq!(
        classify_pci_storage(0x8086, 0x0953, PCI_CLASS_STORAGE, PCI_SUBCLASS_NVME),
        LunTransport::Nvme
    );
    assert!(model_looks_perc("PERC H740P Mini"));
    assert!(model_looks_perc("MegaRAID SAS 3108"));
    assert!(!model_looks_perc("Samsung NVMe"));
    assert_eq!(pick_durable_lun(&[perc_h740()]), Err(LunReject::Perc));
    let mixed = [perc_h740(), nvme(2, 16 * 1024 * 1024 * 1024)];
    let p = pick_durable_lun(&mixed).expect("nvme over PERC");
    assert_eq!(p.transport, LunTransport::Nvme);
    assert_eq!(p.bus, 2);
}

#[test]
fn esp_cruzer_rejected_data_usb_and_nvme_win() {
    assert_eq!(
        classify_usb_lun(4 * 1024 * 1024 * 1024, true),
        LunTransport::EspCruzer
    );
    assert!(usb_is_esp_cruzer_window(4 * 1024 * 1024 * 1024));
    assert!(!usb_is_esp_cruzer_window(DURABLE_LUN_MIN_BYTES));
    assert_eq!(pick_durable_lun(&[cruzer_esp()]), Err(LunReject::EspCruzer));
    let usb = usb_data(16 * 1024 * 1024 * 1024);
    assert_eq!(
        pick_durable_lun(&[cruzer_esp(), usb]).unwrap().transport,
        LunTransport::Usb
    );
    let both = [usb, nvme(3, 32 * 1024 * 1024 * 1024)];
    assert_eq!(
        pick_durable_lun(&both).unwrap().transport,
        LunTransport::Nvme
    );
}

#[test]
fn too_small_usb_is_not_a_lun() {
    let tiny = usb_data(256 * 1024 * 1024);
    assert_eq!(pick_durable_lun(&[tiny]), Err(LunReject::TooSmall));
    assert_eq!(pick_durable_lun(&[]), Err(LunReject::None));
}

#[test]
fn unknown_size_nvme_is_named_before_identify() {
    let p = pick_durable_lun(&[nvme(1, 0)]).expect("pci nvme");
    assert_eq!(p.size_bytes, 0);
    assert!(!durable_lun_can_virtio_attach(&p, false));
    assert!(durable_lun_can_virtio_attach(&p, true));
    assert!(!durable_lun_post_ebs_io_ready());
}

#[test]
fn durable_kind_survives_hv_reboot_but_census_is_not_iron_ok() {
    durable_lun_clear();
    assert!(!durable_lun_present());
    let p = nvme(1, 16 * 1024 * 1024 * 1024);
    store_durable_lun_pick(Some(p), LunReject::None);
    assert!(durable_lun_present());
    assert_eq!(
        select_persist_kind(false, durable_lun_present()),
        PersistKind::DurableLun
    );
    assert!(kind_survives_hv_reboot(PersistKind::DurableLun));
    assert_eq!(
        select_persist_kind(true, durable_lun_present()),
        PersistKind::File
    );
    assert_eq!(M8_DISK_PERSIST_OK_MARKER, "RAYNU-V-M8-DISK-PERSIST-OK");
    assert!(durable_lun_policy_holds());
    assert!(DURABLE_LUN_IO_RESIDUAL_NOTE.contains("leftover DRAM"));
    assert!(!DURABLE_LUN_IO_RESIDUAL_NOTE.contains("RAYNU-V-M7-ISO-INSTALL-OK"));
    durable_lun_clear();
}

#[test]
fn usb_io_stays_residual_nvme_host_vec_serves_virtio() {
    use crate::devices::guest_virtio_blk::{
        attach_lun, blk_sector_rw, reset, VIRTIO_BLK_S_OK, VIRTIO_BLK_T_IN, VIRTIO_BLK_T_OUT,
    };
    durable_lun_clear();
    reset();
    assert!(!durable_lun_can_virtio_attach(
        &usb_data(16 * 1024 * 1024 * 1024),
        true
    ));
    let ns = Box::leak(vec![0u8; 2 * 1024 * 1024].into_boxed_slice());
    let ns_len = ns.len();
    crate::mgmt::nvme::host_nvme_attach(ns, 512);
    assert!(durable_lun_serving());
    assert!(attach_lun(ns_len, false));
    let mut buf = [0u8; 512];
    buf[..8].copy_from_slice(b"EFI PART");
    assert_eq!(
        blk_sector_rw(&mut [], VIRTIO_BLK_T_OUT, 1, &mut buf),
        VIRTIO_BLK_S_OK
    );
    let mut back = [0u8; 512];
    assert_eq!(
        blk_sector_rw(&mut [], VIRTIO_BLK_T_IN, 1, &mut back),
        VIRTIO_BLK_S_OK
    );
    assert_eq!(&back[..8], b"EFI PART");
    let mut peek = [0u8; 8];
    assert!(durable_lun_read_any(512, &mut peek));
    assert_eq!(&peek, b"EFI PART");
    assert!(!crate::mgmt::disk_persist::persist_lun_looks_installed());
    reset();
    durable_lun_clear();
}
