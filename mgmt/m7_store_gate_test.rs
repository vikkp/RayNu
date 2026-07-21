use super::{
    run_m7_store_gate, store_scripts_present, store_surface_present, store_uefi_stub_honest,
    M7_STORE_GATE_MARKER,
};
use crate::mgmt::datastore::prop_datastore_package;

#[test]
fn m7_2_store_gate_passes() {
    assert_eq!(M7_STORE_GATE_MARKER, "RAYNU-V-M7-STORE-OK");
    assert!(store_surface_present(), "mgmt/datastore must embed M7.2 package");
    assert!(store_uefi_stub_honest(), "UEFI persist stub must be honest");
    assert!(store_scripts_present(), "smoke + runbook must be present");
    assert!(prop_datastore_package(), "datastore package prop must hold");
    assert!(run_m7_store_gate());
    println!("RAYNU-V-M7-STORE-OK");
}
