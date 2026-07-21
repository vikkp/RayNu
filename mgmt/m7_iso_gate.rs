//! M7.3 host verification gate (ISO deploy path).
//!
//! Pillar: [Z] [A]
//! Proven Core: outside (companion to `mgmt/iso` — ADR-009).
//!
//! Checks ISO register into datastore, documented extract-boot surface,
//! virtio-blk install target, CD-ROM stub honesty, runbook, and smoke/CI.

use super::iso::{
    attach_cdrom_uefi, extract_boot_surface_present, install_disk_surface_present,
    prop_iso_deploy_package, IsoError, ISO_EXTRACT_BOOT_NOTE, ISO_GAP_NOTE, M7_ISO_OK_MARKER,
};

/// Host / CI marker when the M7.3 ISO gate passes.
pub const M7_ISO_GATE_MARKER: &str = M7_ISO_OK_MARKER;

/// True when iso module exposes package props, closed GAP, extract note, marker.
pub fn iso_surface_present() -> bool {
    let s = include_str!("iso.rs");
    s.contains("fn prop_iso_deploy_package(")
        && s.contains("fn register_iso(")
        && s.contains("fn bind_extract_boot(")
        && s.contains("fn configure_install_disk(")
        && s.contains("fn attach_cdrom_uefi(")
        && s.contains("fn dispatch_iso_rest(")
        && s.contains(M7_ISO_OK_MARKER)
        && s.contains(ISO_GAP_NOTE)
        && s.contains(ISO_EXTRACT_BOOT_NOTE)
        && ISO_GAP_NOTE.contains("CLOSED M7.3")
}

/// True when CD-ROM stub is honest and extract/install surfaces exist.
pub fn iso_path_honest() -> bool {
    attach_cdrom_uefi(1) == Err(IsoError::UnsupportedOnFirmware)
        && extract_boot_surface_present()
        && install_disk_surface_present()
        && ISO_EXTRACT_BOOT_NOTE.contains("kernel-extract")
}

/// True when runbook + smoke script exist with required phrases.
pub fn iso_scripts_present() -> bool {
    let smoke = include_str!("../tools/m7-iso-smoke.sh");
    let runbook = include_str!("../docs/runbooks/iso.md");
    smoke.contains(M7_ISO_OK_MARKER)
        && smoke.contains("m7_3_iso_gate_passes")
        && smoke.contains("prop_iso_deploy_package")
        && smoke.contains("register_bind_install_roundtrip")
        && runbook.contains("RAYNU-V-M7-ISO-OK")
        && runbook.contains("kernel-extract")
        && runbook.contains("virtio-blk")
        && runbook.contains("UnsupportedOnFirmware")
        && runbook.contains("El Torito")
}

/// Full M7.3 artifact + package gate.
pub fn run_m7_iso_gate() -> bool {
    iso_surface_present() && iso_path_honest() && iso_scripts_present() && prop_iso_deploy_package()
}

#[cfg(test)]
#[path = "m7_iso_gate_test.rs"]
mod m7_iso_gate_test;
