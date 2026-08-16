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
    assert!(ISO_INSTALL_GAP_NOTE.contains("OPEN M7.7"));
    assert!(ISO_INSTALL_HOST_LIMIT_NOTE.contains("cannot close"));
}

#[test]
fn install_phase_machine_roundtrip() {
    let mut store = ImageTable::new();
    let mut install = InstallToDiskPlan::empty();
    register_iso(&mut store, 3, 100, "test.iso").unwrap();
    let c = begin_install_to_disk(&store, &mut install, 3).unwrap();
    assert_eq!(c.install_disk_bytes, DEFAULT_INSTALL_DISK_BYTES);
    assert_eq!(install.phase, InstallPhase::ContractReady);
    mark_disk_written(&mut install).unwrap();
    mark_reboot_pending(&mut install).unwrap();
    mark_booted_from_disk(&mut install).unwrap();
    assert!(install.is_install_complete());
}

#[test]
fn iso_install_package() {
    assert!(prop_iso_install_package());
}
