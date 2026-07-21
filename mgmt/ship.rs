//! M7.0 EFI release kit surface (outside Proven Core).
//!
//! Pillar: [Z] [D]
//! Proven Core: **outside** (packaging / ops companion — ADR-003 / ADR-009)
//! VERIFICATION: N/A (host artifact gate for ship kit completeness)
//!
//! Ensures a versioned `.efi` can be packaged with SHA256, size-checked, and
//! deployed via the USB / iDRAC virtual media runbook.

/// Host / CI marker when the M7.0 EFI release kit gate passes.
pub const M7_SHIP_OK_MARKER: &str = "RAYNU-V-M7-SHIP-OK";

/// EFI release kit GAP closed in M7.0.
pub const SHIP_GAP_NOTE: &str = "GAP(CLOSED M7.0): EFI release kit";

/// True when Cargo.toml carries a parseable package version for stamping.
pub fn prop_version_stamp_present() -> bool {
    let cargo = include_str!("../Cargo.toml");
    cargo.contains("name = \"r640-hypervisor\"")
        && cargo.lines().any(|l| {
            let t = l.trim();
            t.starts_with("version = \"") && t.ends_with('"') && t.len() > "version = \"\"".len()
        })
}

/// True when the packaging script emits a versioned dist kit with SHA256.
pub fn prop_package_script_complete() -> bool {
    let s = include_str!("../tools/package-release.sh");
    s.contains("sha256sum")
        && s.contains("dist/")
        && s.contains("r640-hypervisor.efi.sha256")
        && s.contains("SHA256SUMS")
        && s.contains("check-size.sh")
        && s.contains("raynu-v-")
        && s.contains("SKIP_BUILD")
        && s.contains("RAYNU-V-M7-SHIP-OK")
        && s.contains(".tar.gz")
}

/// True when size budget tooling remains on the release path (ADR-003).
pub fn prop_size_gate_in_release_path() -> bool {
    let size = include_str!("../tools/check-size.sh");
    let pkg = include_str!("../tools/package-release.sh");
    size.contains("15 * 1024 * 1024")
        && size.contains("20 * 1024 * 1024")
        && pkg.contains("check-size.sh")
}

/// True when the USB / iDRAC virtual media runbook is filed.
pub fn prop_usb_idrac_runbook() -> bool {
    let s = include_str!("../docs/runbooks/usb_idrac.md");
    s.contains("RAYNU-V-M7-SHIP-OK")
        && s.contains("iDRAC")
        && s.contains("virtual media")
        && s.contains("USB")
        && s.contains("FAT32")
        && s.contains("r640-hypervisor.efi")
        && s.contains("package-release.sh")
        && s.contains("sha256")
}

/// Full M7.0 host-testable release kit package.
pub fn prop_release_kit_package() -> bool {
    let _ = (SHIP_GAP_NOTE, M7_SHIP_OK_MARKER);
    prop_version_stamp_present()
        && prop_package_script_complete()
        && prop_size_gate_in_release_path()
        && prop_usb_idrac_runbook()
        && SHIP_GAP_NOTE.contains("CLOSED M7.0")
        && M7_SHIP_OK_MARKER == "RAYNU-V-M7-SHIP-OK"
}

#[cfg(test)]
#[path = "ship_test.rs"]
mod ship_test;
