use super::{
    iso_install_honesty_holds, iso_install_scripts_present, iso_install_surface_present,
    prop_iso_install_scaffold_package, run_m7_iso_install_scaffold_gate, M7_ISO_INSTALL_GATE_MARKER,
};
use crate::mgmt::iso_install::prop_iso_install_package;

#[test]
fn m7_7_iso_install_scaffold_passes() {
    assert_eq!(
        M7_ISO_INSTALL_GATE_MARKER,
        "RAYNU-V-M7-ISO-INSTALL-SCAFFOLD-OK"
    );
    assert!(
        iso_install_surface_present(),
        "mgmt/iso_install must embed M7.7 package"
    );
    assert!(
        iso_install_honesty_holds(),
        "install-to-disk honesty + surfaces must hold"
    );
    assert!(
        iso_install_scripts_present(),
        "smoke + runbook + evidence template must be present"
    );
    assert!(prop_iso_install_package(), "iso install package prop must hold");
    assert!(prop_iso_install_scaffold_package());
    assert!(run_m7_iso_install_scaffold_gate());
    // Scaffold only — never print iron marker from this test.
    println!("RAYNU-V-M7-ISO-INSTALL-SCAFFOLD-OK");
}
