//! M7.7 ISO install-to-disk scaffold / package gate (outside Proven Core).
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** (ADR-009)
//!
//! Proves runbook + evidence template + install package exist.
//! Does **not** print `RAYNU-V-M7-ISO-INSTALL-OK` from host/CI — that marker
//! is reserved for QEMU/iron install-to-disk + reboot-to-disk proof.

use super::iso_install::{
    install_launch_surfaces_present, prop_iso_install_lab_package, prop_iso_install_package,
    ISO_INSTALL_GAP_NOTE, ISO_INSTALL_HOST_LIMIT_NOTE, ISO_INSTALL_MVP_NOTE,
    M7_ISO_INSTALL_OK_MARKER, M7_ISO_INSTALL_SCAFFOLD_MARKER,
};

/// Host / CI marker when the M7.7 scaffold package passes.
pub const M7_ISO_INSTALL_GATE_MARKER: &str = M7_ISO_INSTALL_SCAFFOLD_MARKER;

/// True when iso_install module exposes package + markers + open GAP.
pub fn iso_install_surface_present() -> bool {
    let s = include_str!("iso_install.rs");
    s.contains("fn prop_iso_install_package(")
        && s.contains("fn begin_install_to_disk(")
        && s.contains("fn launch_contract(")
        && s.contains("fn arm_install_launch_contract(")
        && s.contains("fn arm_lab_install_contract(")
        && s.contains("fn disk_bytes_for_virtio_launch(")
        && s.contains("fn note_reboot_pending_lab(")
        && s.contains("fn probe_iso_install_lab_flag(")
        && s.contains("fn mark_disk_written(")
        && s.contains("fn mark_reboot_pending(")
        && s.contains("fn mark_booted_from_disk(")
        && s.contains("fn dispatch_iso_install_rest(")
        && s.contains(M7_ISO_INSTALL_OK_MARKER)
        && s.contains(M7_ISO_INSTALL_SCAFFOLD_MARKER)
        && s.contains(ISO_INSTALL_GAP_NOTE)
        && s.contains(ISO_INSTALL_MVP_NOTE)
        && ISO_INSTALL_GAP_NOTE.contains("OPEN M7.7")
        && include_str!("../src/main.rs").contains("disk_bytes_for_virtio_launch")
        && include_str!("../src/main.rs").contains("E5 install-sized virtio-blk")
        && include_str!("../src/main.rs").contains("probe_iso_install_lab_flag")
        && prop_iso_install_lab_package()
}

/// True when honesty + launch surfaces hold.
pub fn iso_install_honesty_holds() -> bool {
    ISO_INSTALL_HOST_LIMIT_NOTE.contains("cannot close")
        && ISO_INSTALL_MVP_NOTE.contains("reboot-to-disk")
        && M7_ISO_INSTALL_OK_MARKER == "RAYNU-V-M7-ISO-INSTALL-OK"
        && M7_ISO_INSTALL_SCAFFOLD_MARKER == "RAYNU-V-M7-ISO-INSTALL-SCAFFOLD-OK"
        && install_launch_surfaces_present()
}

/// True when runbook + smoke + evidence template exist with required phrases.
pub fn iso_install_scripts_present() -> bool {
    let smoke = include_str!("../tools/m7-iso-install-smoke.sh");
    let runbook = include_str!("../docs/runbooks/iso_install.md");
    let evidence = include_str!("../docs/evidence/r640/TEMPLATE-iso-install.md");
    let status = include_str!("../docs/evidence/r640/STATUS-iso-install");
    smoke.contains(M7_ISO_INSTALL_SCAFFOLD_MARKER)
        && smoke.contains("m7_7_iso_install_scaffold_passes")
        && smoke.contains(M7_ISO_INSTALL_OK_MARKER)
        && smoke.contains("never print iron marker")
        && runbook.contains(M7_ISO_INSTALL_OK_MARKER)
        && runbook.contains(M7_ISO_INSTALL_SCAFFOLD_MARKER)
        && runbook.contains("reboot-to-disk")
        && runbook.contains("extract-boot")
        && runbook.contains("El Torito")
        && runbook.contains("docs/evidence/r640")
        && runbook.contains("Latitude / QEMU")
        && evidence.contains("SHA256")
        && evidence.contains("Serial excerpt")
        && evidence.contains(M7_ISO_INSTALL_OK_MARKER)
        && evidence.contains("reboot-to-disk")
        && status.contains("STATUS=open")
        && status.contains(M7_ISO_INSTALL_OK_MARKER)
        && status.contains("scaffold")
}

/// Full M7.7 scaffold package prop.
pub fn prop_iso_install_scaffold_package() -> bool {
    let _ = (ISO_INSTALL_GAP_NOTE, ISO_INSTALL_HOST_LIMIT_NOTE);
    iso_install_surface_present()
        && iso_install_honesty_holds()
        && iso_install_scripts_present()
        && prop_iso_install_package()
}

/// Full M7.7 scaffold gate.
pub fn run_m7_iso_install_scaffold_gate() -> bool {
    prop_iso_install_scaffold_package()
}

#[cfg(test)]
#[path = "m7_iso_install_gate_test.rs"]
mod m7_iso_install_gate_test;
