//! M3.15 host verification gate (frozen Verus toolchain, ADR-008).
//!
//! Pillar: [V]
//! Proven Core: companion tooling gate (not a boot path).
//!
//! Checks that `verus-version.toml` carries an exact release tag, git commit,
//! and Linux asset sha256 (never "latest"). Runtime install is exercised by
//! `tools/verus-smoke.sh` (CI / Latitude host).

/// Host / CI marker when the M3.15 Verus pin gate passes.
pub const M3_VERUS_OK_MARKER: &str = "RAYNU-V-M3-VERUS-OK";

fn pin_toml() -> &'static str {
    include_str!("../verus-version.toml")
}

pub(crate) fn toml_string(key: &str) -> Option<&'static str> {
    // no_std-safe: no format!/alloc — parse `key = "value"` lines only.
    for line in pin_toml().lines() {
        let line = line.trim();
        let Some(after_key) = line.strip_prefix(key) else {
            continue;
        };
        let after_key = after_key.trim_start();
        let Some(after_eq) = after_key.strip_prefix('=') else {
            continue;
        };
        let after_eq = after_eq.trim_start();
        let Some(rest) = after_eq.strip_prefix('"') else {
            continue;
        };
        let v = rest.trim_end_matches('"');
        if !v.is_empty() {
            return Some(v);
        }
    }
    None
}

/// Pinned Verus version string from `verus-version.toml`.
pub fn pinned_verus_version() -> Option<&'static str> {
    match toml_string("version") {
        Some("unpinned-scaffold") | None => None,
        Some(v) => Some(v),
    }
}

/// True when the pin file is a frozen weekly release (tag + commit + sha256).
pub fn verus_pin_is_concrete() -> bool {
    let s = pin_toml();
    let Some(version) = pinned_verus_version() else {
        return false;
    };
    let Some(tag) = toml_string("tag") else {
        return false;
    };
    let Some(commit) = toml_string("commit") else {
        return false;
    };
    let Some(sha) = toml_string("sha256_linux") else {
        return false;
    };
    // Weekly releases look like 0.YYYY.MM.DD.<git>
    let version_ok = version.starts_with("0.20") && version.matches('.').count() >= 3;
    let tag_ok = tag.starts_with("release/") && !tag.contains("latest") && !tag.contains("rolling");
    let commit_ok = commit.len() == 40 && commit.chars().all(|c| c.is_ascii_hexdigit());
    let sha_ok = sha.len() == 64 && sha.chars().all(|c| c.is_ascii_hexdigit());
    version_ok
        && tag_ok
        && commit_ok
        && sha_ok
        && s.contains("toolchain = \"")
        && s.contains("never releases/latest")
        && s.contains(M3_VERUS_OK_MARKER)
}

/// True when install + smoke scripts enforce the frozen pin.
pub fn verus_scripts_present() -> bool {
    let install = include_str!("../tools/install-verus.sh");
    let smoke = include_str!("../tools/verus-smoke.sh");
    install.contains("verus-version.toml")
        && install.contains("sha256_linux")
        && install.contains("sha256sum")
        && install.contains("must not reference 'latest'")
        && install.contains("version.json")
        && smoke.contains("cargo verus verify")
        && smoke.contains(M3_VERUS_OK_MARKER)
        && smoke.contains("install-verus.sh")
        && smoke.contains("sha256_linux=")
}

/// Full M3.15 pin artifact gate (does not download Verus).
pub fn run_verus_pin_gate() -> bool {
    verus_pin_is_concrete() && verus_scripts_present()
}

#[cfg(test)]
#[path = "verus_gate_test.rs"]
mod verus_gate_test;
