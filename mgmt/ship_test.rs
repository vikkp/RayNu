use super::{
    prop_package_script_complete, prop_release_kit_package, prop_size_gate_in_release_path,
    prop_usb_idrac_runbook, prop_version_stamp_present, M7_SHIP_OK_MARKER, SHIP_GAP_NOTE,
};

#[test]
fn release_kit_package() {
    assert_eq!(M7_SHIP_OK_MARKER, "RAYNU-V-M7-SHIP-OK");
    assert!(SHIP_GAP_NOTE.contains("CLOSED M7.0"));
    assert!(prop_version_stamp_present());
    assert!(prop_package_script_complete());
    assert!(prop_size_gate_in_release_path());
    assert!(prop_usb_idrac_runbook());
    assert!(prop_release_kit_package());
    println!("RAYNU-V-M7-SHIP-OK");
}
