//! M7.5 R640 boot scaffold gate (outside Proven Core).
//!
//! Pillar: [D] [Z]
//! Proven Core: **outside** (ADR-009)
//!
//! Proves runbook + evidence package + ship-kit cross-refs exist.
//! Does **not** print `RAYNU-V-R640-BOOT-OK` — that marker is iron-only
//! (claimed in `docs/evidence/r640/` after real PowerEdge serial).

/// Iron marker — real PowerEdge R640 evidence only (never printed by host smoke).
pub const M7_R640_OK_MARKER: &str = "RAYNU-V-R640-BOOT-OK";

/// Host / CI marker when the M7.5 **scaffold** (not iron boot) passes.
pub const M7_R640_SCAFFOLD_MARKER: &str = "RAYNU-V-M7-R640-SCAFFOLD-OK";

/// Closed after real R640 COM2 evidence (2026-08-15).
pub const R640_GAP_NOTE: &str = "GAP(CLOSED M7.5): Real R640 boot";

/// Honesty: Latitude/QEMU cannot invent iron evidence.
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

/// True when M7.5 runbook + closed evidence package exist with required phrases.
pub fn r640_scripts_present() -> bool {
    let smoke = include_str!("../tools/m7-r640-smoke.sh");
    let runbook = include_str!("../docs/runbooks/r640_boot.md");
    let iron_week = include_str!("../docs/runbooks/r640_iron_week.md");
    let evidence = include_str!("../docs/evidence/r640/TEMPLATE.md");
    let status = include_str!("../docs/evidence/r640/STATUS");
    let first_light = include_str!("../docs/evidence/r640/2026-08-15-r640-first-light.md");
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
        && iron_week.contains("r640_field_guide.md")
        && include_str!("../docs/runbooks/r640_field_guide.md").contains("Get the box alive")
        && include_str!("../docs/runbooks/r640_field_guide.md").contains("Open the virtual console")
        && evidence.contains("SHA256")
        && evidence.contains("Serial excerpt")
        && evidence.contains(M7_R640_OK_MARKER)
        && status.contains("STATUS=closed")
        && status.contains(M7_R640_OK_MARKER)
        && first_light.contains("RAYNU-V-M3-SHELL-OK")
        && first_light.contains("RAYNU-V-M4-SMP-OK")
        && first_light.contains(M7_R640_OK_MARKER)
}

/// True when CLOSED GAP and host-limit honesty hold.
pub fn r640_honesty_holds() -> bool {
    R640_GAP_NOTE.contains("GAP(CLOSED M7.5)")
        && R640_GAP_NOTE.contains("Real R640 boot")
        && R640_HOST_LIMIT_NOTE.contains("cannot close")
        && M7_R640_OK_MARKER == "RAYNU-V-R640-BOOT-OK"
        && M7_R640_SCAFFOLD_MARKER == "RAYNU-V-M7-R640-SCAFFOLD-OK"
}

/// Full M7.5 scaffold package prop (evidence closed; host still no iron print).
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
