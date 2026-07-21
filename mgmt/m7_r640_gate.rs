//! M7.5 R640 boot scaffold gate (outside Proven Core).
//!
//! Pillar: [D] [Z]
//! Proven Core: **outside** (ADR-009)
//!
//! Proves runbook + evidence template + ship-kit cross-refs exist.
//! Does **not** claim `RAYNU-V-R640-BOOT-OK` — that marker is iron-only.

/// Iron marker — real PowerEdge R640 evidence only (never printed by host smoke).
pub const M7_R640_OK_MARKER: &str = "RAYNU-V-R640-BOOT-OK";

/// Host / CI marker when the M7.5 **scaffold** (not iron boot) passes.
pub const M7_R640_SCAFFOLD_MARKER: &str = "RAYNU-V-M7-R640-SCAFFOLD-OK";

/// Open GAP until real iron evidence lands.
pub const R640_GAP_NOTE: &str = "GAP: Real R640 boot (M7.5 — iron only)";

/// Honesty: Latitude/QEMU cannot close E2.
pub const R640_HOST_LIMIT_NOTE: &str =
    "Latitude/QEMU host smoke cannot close RAYNU-V-R640-BOOT-OK; real PowerEdge R640 required";

/// True when ship kit still names the R640 EFI binary.
pub fn ship_kit_names_r640_efi() -> bool {
    let pkg = include_str!("../tools/package-release.sh");
    let usb = include_str!("../docs/runbooks/usb_idrac.md");
    pkg.contains("r640-hypervisor.efi")
        && usb.contains("r640-hypervisor.efi")
        && usb.contains("RAYNU-V-R640-BOOT-OK")
}

/// True when M7.5 runbook + evidence template exist with required phrases.
pub fn r640_scripts_present() -> bool {
    let smoke = include_str!("../tools/m7-r640-smoke.sh");
    let runbook = include_str!("../docs/runbooks/r640_boot.md");
    let iron_week = include_str!("../docs/runbooks/r640_iron_week.md");
    let evidence = include_str!("../docs/evidence/r640/TEMPLATE.md");
    let status = include_str!("../docs/evidence/r640/STATUS");
    smoke.contains(M7_R640_SCAFFOLD_MARKER)
        && smoke.contains("m7_5_r640_scaffold_passes")
        && smoke.contains(M7_R640_OK_MARKER)
        && smoke.contains("never print iron marker")
        && runbook.contains(M7_R640_OK_MARKER)
        && runbook.contains("RAYNU-V-M0-BOOT-OK")
        && runbook.contains("iDRAC")
        && runbook.contains("USB")
        && runbook.contains("Latitude / QEMU")
        && runbook.contains("docs/evidence/r640")
        && runbook.contains("r640_iron_week.md")
        && iron_week.contains("Rack basics")
        && iron_week.contains("evidence template")
        && iron_week.contains(M7_R640_OK_MARKER)
        && evidence.contains("SHA256")
        && evidence.contains("Serial excerpt")
        && evidence.contains(M7_R640_OK_MARKER)
        && status.contains("STATUS=open")
}

/// True when honesty constants and open GAP hold.
pub fn r640_honesty_holds() -> bool {
    R640_GAP_NOTE.contains("iron only")
        && !R640_GAP_NOTE.contains("CLOSED")
        && R640_HOST_LIMIT_NOTE.contains("cannot close")
        && M7_R640_OK_MARKER == "RAYNU-V-R640-BOOT-OK"
        && M7_R640_SCAFFOLD_MARKER == "RAYNU-V-M7-R640-SCAFFOLD-OK"
}

/// Full M7.5 scaffold package prop (not iron close).
pub fn prop_r640_scaffold_package() -> bool {
    let _ = (R640_GAP_NOTE, R640_HOST_LIMIT_NOTE);
    ship_kit_names_r640_efi() && r640_scripts_present() && r640_honesty_holds()
}

/// Full M7.5 scaffold gate.
pub fn run_m7_r640_scaffold_gate() -> bool {
    prop_r640_scaffold_package()
}

#[cfg(test)]
#[path = "m7_r640_gate_test.rs"]
mod m7_r640_gate_test;
