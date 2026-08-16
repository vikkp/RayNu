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
    assert!(ISO_INSTALL_GAP_NOTE.contains("OPEN M7.7"));
    assert!(ISO_INSTALL_HOST_LIMIT_NOTE.contains("cannot close"));
}

#[test]
fn install_phase_machine_roundtrip() {
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
    assert!(prop_iso_install_package());
}

#[test]
fn armed_contract_sizes_launch_disk() {
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
    assert!(prop_iso_install_lab_package());
    println!("RAYNU-V-M7-ISO-INSTALL-LAB-OK");
}
