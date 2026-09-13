use super::{
    disk_persist_surface_present, run_m8_disk_persist_host_gate, M8_DISK_PERSIST_GATE_MARKER,
    M8_DISK_PERSIST_RESIDUAL_NOTE,
};
use crate::mgmt::disk_persist::{
    host_never_prints_iso_install_ok, M8_DISK_PERSIST_HOST_OK_MARKER, M8_DISK_PERSIST_OK_MARKER,
};

#[test]
fn m8_disk_persist_host_gate_passes() {
    assert_eq!(
        M8_DISK_PERSIST_GATE_MARKER,
        "RAYNU-V-M8-DISK-PERSIST-HOST-OK"
    );
    assert_eq!(M8_DISK_PERSIST_OK_MARKER, "RAYNU-V-M8-DISK-PERSIST-OK");
    assert_eq!(
        crate::mgmt::disk_persist::M8_DISK_PERSIST_NESTED_OK_MARKER,
        "RAYNU-V-M8-DISK-PERSIST-NESTED-OK"
    );
    assert_ne!(
        crate::mgmt::disk_persist::M8_DISK_PERSIST_NESTED_OK_MARKER,
        M8_DISK_PERSIST_OK_MARKER
    );
    assert_ne!(M8_DISK_PERSIST_HOST_OK_MARKER, "RAYNU-V-M7-ISO-INSTALL-OK");
    assert!(M8_DISK_PERSIST_RESIDUAL_NOTE.contains("ISO-INSTALL-OK"));
    assert!(host_never_prints_iso_install_ok());
    assert!(disk_persist_surface_present());
    assert!(
        run_m8_disk_persist_host_gate(),
        "M8.0 host persist package must hold (not ISO-INSTALL-OK)"
    );
    println!("{M8_DISK_PERSIST_GATE_MARKER}");
}
