use super::{
    iso_path_honest, iso_scripts_present, iso_surface_present, run_m7_iso_gate, M7_ISO_GATE_MARKER,
};
use crate::mgmt::iso::prop_iso_deploy_package;

#[test]
fn m7_3_iso_gate_passes() {
    assert_eq!(M7_ISO_GATE_MARKER, "RAYNU-V-M7-ISO-OK");
    assert!(iso_surface_present(), "mgmt/iso must embed M7.3 package");
    assert!(iso_path_honest(), "extract-boot + CD-ROM stub must be honest");
    assert!(iso_scripts_present(), "smoke + runbook must be present");
    assert!(prop_iso_deploy_package(), "iso deploy package prop must hold");
    assert!(run_m7_iso_gate());
    println!("RAYNU-V-M7-ISO-OK");
}
