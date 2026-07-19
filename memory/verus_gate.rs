//! M3.15 host verification gate (pinned Verus toolchain, ADR-008).
//!
//! Pillar: [V]
//! Proven Core: companion tooling gate (not a boot path).
//!
//! Checks that `verus-version.toml` carries a concrete weekly pin and that the
//! install/smoke scripts exist. Runtime install is exercised by
//! `tools/verus-smoke.sh` (CI / Latitude host).

/// Host / CI marker when the M3.15 Verus pin gate passes.
pub const M3_VERUS_OK_MARKER: &str = "RAYNU-V-M3-VERUS-OK";

/// Pinned Verus version string from `verus-version.toml`.
pub fn pinned_verus_version() -> Option<&'static str> {
    for line in include_str!("../verus-version.toml").lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("version = \"") {
            let v = rest.trim_end_matches('"');
            if !v.is_empty() && v != "unpinned-scaffold" {
                return Some(v);
            }
        }
    }
    None
}

/// True when the pin file looks like an ADR-008 weekly release pin.
pub fn verus_pin_is_concrete() -> bool {
    let s = include_str!("../verus-version.toml");
    let Some(version) = pinned_verus_version() else {
        return false;
    };
    // Weekly releases look like 0.YYYY.MM.DD.<git>
    let version_ok = version.starts_with("0.20") && version.matches('.').count() >= 3;
    s.contains("tag = \"release/")
        && s.contains("toolchain = \"")
        && s.contains(M3_VERUS_OK_MARKER)
        && version_ok
}

/// True when install + smoke scripts are present and name the marker.
pub fn verus_scripts_present() -> bool {
    let install = include_str!("../tools/install-verus.sh");
    let smoke = include_str!("../tools/verus-smoke.sh");
    install.contains("verus-version.toml")
        && install.contains("x86-linux.zip")
        && smoke.contains("cargo verus verify")
        && smoke.contains(M3_VERUS_OK_MARKER)
        && smoke.contains("install-verus.sh")
}

/// Full M3.15 pin artifact gate (does not download Verus).
pub fn run_verus_pin_gate() -> bool {
    verus_pin_is_concrete() && verus_scripts_present()
}

#[cfg(test)]
#[path = "verus_gate_test.rs"]
mod verus_gate_test;
