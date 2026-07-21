use super::{
    run_m7_ship_gate, ship_build_path_present, ship_scripts_present, ship_surface_present,
    M7_SHIP_GATE_MARKER,
};
use crate::mgmt::ship::prop_release_kit_package;

#[test]
fn m7_0_ship_gate_passes() {
    assert_eq!(M7_SHIP_GATE_MARKER, "RAYNU-V-M7-SHIP-OK");
    assert!(ship_surface_present(), "mgmt/ship must embed M7.0 package");
    assert!(ship_scripts_present(), "package/smoke/runbook must be present");
    assert!(ship_build_path_present(), "build.sh + check-size.sh must exist");
    assert!(
        prop_release_kit_package(),
        "release kit package prop must hold"
    );
    assert!(run_m7_ship_gate());
    println!("RAYNU-V-M7-SHIP-OK");
}
