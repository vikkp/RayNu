use super::*;
use crate::mgmt::datastore::ImageTable;
use crate::mgmt::iso::{register_iso, DEFAULT_INSTALL_DISK_BYTES};

#[test]
fn markers_stable() {
    assert_eq!(M7_ISO_INSTALL_OK_MARKER, "RAYNU-V-M7-ISO-INSTALL-OK");
    assert_eq!(
        M7_ISO_INSTALL_SCAFFOLD_MARKER,
        "RAYNU-V-M7-ISO-INSTALL-SCAFFOLD-OK"
    );
    assert_eq!(LAB_INSTALL_DISK_BYTES, 1024 * 1024);
    assert_eq!(M7_ISO_DISK_WRITTEN_MARKER, "RAYNU-V-M7-ISO-DISK-WRITTEN");
    assert_eq!(
        M7_ISO_INSTALL_LAB_OK_MARKER,
        "RAYNU-V-M7-ISO-INSTALL-LAB-OK"
    );
    assert_eq!(
        M7_ISO_REBOOT_PENDING_MARKER,
        "RAYNU-V-M7-ISO-REBOOT-PENDING"
    );
    assert_eq!(
        M7_ISO_BOOTED_FROM_DISK_MARKER,
        "RAYNU-V-M7-ISO-BOOTED-FROM-DISK"
    );
    assert!(ISO_INSTALL_GAP_NOTE.contains("CLOSED M7.7"));
    assert!(ISO_INSTALL_HOST_LIMIT_NOTE.contains("cannot close"));
}

#[test]
fn install_phase_machine_roundtrip() {
    let _g = iso_install_host_test_lock();
    clear_armed_install_contract();
    let mut store = ImageTable::new();
    let mut install = InstallToDiskPlan::empty();
    register_iso(&mut store, 3, 100, "test.iso").unwrap();
    let c = begin_install_to_disk(&store, &mut install, 3).unwrap();
    assert_eq!(c.install_disk_bytes, DEFAULT_INSTALL_DISK_BYTES);
    assert_eq!(install.phase, InstallPhase::ContractReady);
    assert_eq!(peek_armed_install_contract(), Some(c));
    assert_eq!(
        disk_bytes_for_virtio_launch(),
        DEFAULT_INSTALL_DISK_BYTES as usize
    );
    mark_disk_written(&mut install).unwrap();
    mark_reboot_pending(&mut install).unwrap();
    mark_booted_from_disk(&mut install).unwrap();
    assert!(install.is_install_complete());
    clear_armed_install_contract();
    assert_eq!(disk_bytes_for_virtio_launch(), PROBE_DISK_BYTES);
}

#[test]
fn iso_install_package() {
    let _g = iso_install_host_test_lock();
    assert!(prop_iso_install_package());
}

#[test]
fn armed_contract_sizes_launch_disk() {
    let _g = iso_install_host_test_lock();
    clear_armed_install_contract();
    assert!(!install_disk_armed_for_launch());
    arm_install_launch_contract(InstallLaunchContract {
        iso_id: 9,
        extract_bound: true,
        install_disk_bytes: DEFAULT_INSTALL_DISK_BYTES,
    });
    assert!(install_disk_armed_for_launch());
    assert_eq!(
        disk_bytes_for_virtio_launch(),
        DEFAULT_INSTALL_DISK_BYTES as usize
    );
    clear_armed_install_contract();
}

#[test]
fn iso_install_lab_package() {
    let _g = iso_install_host_test_lock();
    assert!(prop_iso_install_lab_package());
    println!("RAYNU-V-M7-ISO-INSTALL-LAB-OK");
}

#[test]
fn iso_reboot_lab_package() {
    let _g = iso_install_host_test_lock();
    assert!(prop_iso_reboot_lab_package());
    println!("RAYNU-V-M7-ISO-BOOTED-FROM-DISK");
}

#[test]
fn persist_image_is_marker_only_for_iron_size() {
    assert_eq!(
        persist_image_len_for_contract(LAB_INSTALL_DISK_BYTES),
        INSTALL_MARKER_PERSIST_BYTES
    );
    assert_eq!(
        persist_image_len_for_contract(DEFAULT_INSTALL_DISK_BYTES),
        INSTALL_MARKER_PERSIST_BYTES
    );
    let mut buf = [0u8; INSTALL_MARKER_PERSIST_BYTES];
    assert!(fill_persist_image(&mut buf));
    assert_eq!(
        u32::from_le_bytes(buf[0..4].try_into().unwrap()),
        crate::devices::virtio_blk::DISK_PATTERN
    );
    assert_eq!(
        u32::from_le_bytes(buf[512..516].try_into().unwrap()),
        crate::devices::virtio_blk::INSTALL_DISK_PATTERN
    );
    assert_eq!(parse_decimal_u64(b"67108864\n"), Some(67108864));
    assert_eq!(parse_decimal_u64(b"1048576"), Some(1048576));
}
