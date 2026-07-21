//! M7.0 host verification gate (EFI release kit).
//!
//! Pillar: [Z] [D]
//! Proven Core: outside (packaging companion — ADR-003 / ADR-009).
//!
//! Checks version stamp, package script, size gate wiring, USB/iDRAC runbook,
//! smoke script, and release-kit props.

use super::ship::{prop_release_kit_package, M7_SHIP_OK_MARKER, SHIP_GAP_NOTE};

/// Host / CI marker when the M7.0 ship gate passes.
pub const M7_SHIP_GATE_MARKER: &str = M7_SHIP_OK_MARKER;

/// True when ship module exposes package props, closed GAP, marker.
pub fn ship_surface_present() -> bool {
    let s = include_str!("ship.rs");
    s.contains("fn prop_version_stamp_present(")
        && s.contains("fn prop_package_script_complete(")
        && s.contains("fn prop_size_gate_in_release_path(")
        && s.contains("fn prop_usb_idrac_runbook(")
        && s.contains("fn prop_release_kit_package(")
        && s.contains(M7_SHIP_OK_MARKER)
        && s.contains(SHIP_GAP_NOTE)
        && SHIP_GAP_NOTE.contains("CLOSED M7.0")
}

/// True when packaging + smoke scripts and runbook exist with required phrases.
pub fn ship_scripts_present() -> bool {
    let pkg = include_str!("../tools/package-release.sh");
    let smoke = include_str!("../tools/m7-ship-smoke.sh");
    let runbook = include_str!("../docs/runbooks/usb_idrac.md");
    pkg.contains("sha256sum")
        && pkg.contains("check-size.sh")
        && smoke.contains(M7_SHIP_OK_MARKER)
        && smoke.contains("m7_0_ship_gate_passes")
        && smoke.contains("prop_release_kit_package")
        && smoke.contains("package-release.sh")
        && runbook.contains("RAYNU-V-M7-SHIP-OK")
        && runbook.contains("iDRAC")
}

/// True when build.sh remains the single-binary entry and size check exists.
pub fn ship_build_path_present() -> bool {
    let build = include_str!("../tools/build.sh");
    let size = include_str!("../tools/check-size.sh");
    build.contains("r640-hypervisor.efi")
        && build.contains("uefi-bin")
        && size.contains("ADR-003")
}

/// Full M7.0 artifact + package gate.
pub fn run_m7_ship_gate() -> bool {
    ship_surface_present()
        && ship_scripts_present()
        && ship_build_path_present()
        && prop_release_kit_package()
}

#[cfg(test)]
#[path = "m7_ship_gate_test.rs"]
mod m7_ship_gate_test;
