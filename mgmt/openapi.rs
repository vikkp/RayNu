//! Versioned management API contract (ADR-021 E12).
//!
//! Pillar: [A] [Z]
//! Proven Core: **outside** (ADR-002).
//! VERIFICATION: L1. `./tools/check-openapi.sh` is the CI route check.
//!
//! [`API_V1_PATHS`] is the stable SKU surface. Each entry is a path the
//! firmware matches. `docs/openapi/v1.yaml` is the same list at version 1.
//! The `/fw/*` stage probes are not this contract.

/// Published contract version. Bump the yaml `info.version` with this.
pub const API_VERSION: &str = "1";

/// Stable routes. A path here must appear in `mgmt/` and in the OpenAPI file.
pub const API_V1_PATHS: &[&str] = &[
    "/auth/login",
    "/auth/operator",
    "/host/overview",
    "/host/clock",
    "/host/addr",
    "/host/addr/clear",
    "/host/cert",
    "/host/cert/clear",
    "/host/poweroff",
    "/host/panic",
    "/logs/serial",
    "/logs/guest",
    "/console/keys",
    "/vms",
    "/images",
    "/perc/guests",
    "/perc/guests/1/start",
    "/perc/guests/2/start",
    "/perc/guests/stop",
    "/perc/boot",
    "/perc/boot/installed",
    "/perc/boot/reinstall",
    "/perc/library/",
    "/perc/library/clear",
    "/perc/nic/",
    "/iso/install",
    "/iso/deploy",
    "/iso/attach",
    "/iso/firmware",
    "/audit/export.json",
    "/audit/export.csv",
];

/// Host / CI marker when the v1 contract matches the routes.
pub const E12_OPENAPI_OK_MARKER: &str = "RAYNU-V-E12-OPENAPI-OK";

/// True when the yaml lists exactly [`API_V1_PATHS`] and each path is in `mgmt/`.
pub fn openapi_v1_matches_routes() -> bool {
    let yaml = include_str!("../docs/openapi/v1.yaml");
    if !yaml.contains("openapi: 3.0.3") || !yaml.contains("version: \"1\"") {
        return false;
    }
    let mut yaml_paths = [""; 64];
    let mut n = 0usize;
    for line in yaml.lines() {
        let Some(rest) = line.strip_prefix("  ") else {
            continue;
        };
        if !rest.starts_with('/') {
            continue;
        }
        let Some(path) = rest.strip_suffix(':') else {
            continue;
        };
        if n >= yaml_paths.len() {
            return false;
        }
        yaml_paths[n] = path;
        n += 1;
    }
    if n != API_V1_PATHS.len() {
        return false;
    }
    let sources = [
        include_str!("http.rs"),
        include_str!("api.rs"),
        include_str!("host_addr.rs"),
        include_str!("host_cert.rs"),
        include_str!("host_power.rs"),
        include_str!("host_overview.rs"),
        include_str!("iso.rs"),
        include_str!("iso_install.rs"),
        include_str!("datastore.rs"),
        include_str!("guest_catalog.rs"),
        include_str!("operator.rs"),
        include_str!("perc_boot_choice.rs"),
        include_str!("iso_library.rs"),
    ];
    for path in API_V1_PATHS {
        if !yaml_paths[..n].contains(path) {
            return false;
        }
        if !sources.iter().any(|src| src.contains(path)) {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v1_contract_matches_the_firmware_routes() {
        assert!(openapi_v1_matches_routes());
        println!("{E12_OPENAPI_OK_MARKER}");
    }
}
