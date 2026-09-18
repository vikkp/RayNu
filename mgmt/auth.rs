//! M8.2 operator auth (outside Proven Core).
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** (ADR-002 / ADR-018). Do not touch VMX/EPT.
//! VERIFICATION: L1 host tests (operator token required; bring-up rejected).
//!
//! Firmware HTTP still uses [`crate::mgmt::api::auth_allows`]: lab
//! `raynu-v-bringup` unless ESP `EFI/RayNu/auth.token` is armed. That is
//! **not** the M8.2 product default. This module is the host-first slice:
//! [`AuthMode::HostReady`] never accepts the bring-up latch.
//!
//! Iron close marker [`M8_AUTH_OK_MARKER`] is ESP operator token required on
//! coexist after `BOOT-OK` (bring-up → 401). Host/CI print
//! [`M8_AUTH_HOST_OK_MARKER`]. Nested QEMU ≠ R640. Do not flash.

use crate::mgmt::api::{
    auth_allows, clear_operator_token, set_operator_token, AUTH_TOKEN_SOURCE_NOTE,
    BRINGUP_AUTH_TOKEN,
};

/// Iron COM2 / operator LAN close: product default is ESP `auth.token`.
/// Host/CI/nested must **never** print this.
pub const M8_AUTH_OK_MARKER: &str = "RAYNU-V-M8-AUTH-OK";

/// Host/CI: operator token is the product latch; bring-up is lab-only. Not iron.
pub const M8_AUTH_HOST_OK_MARKER: &str = "RAYNU-V-M8-AUTH-HOST-OK";

/// Honesty: host product policy ≠ iron ESP-required default.
pub const AUTH_HOST_RESIDUAL_NOTE: &str =
    "residual: HostReady is not iron RAYNU-V-M8-AUTH-OK; firmware HTTP still accepts raynu-v-bringup when no ESP auth.token; nested QEMU ≠ R640; do not print iron AUTH-OK from host/CI";

/// Firmware listen today: bring-up lab latch unless ESP token is armed.
pub const AUTH_FIRMWARE_LAB_NOTE: &str =
    "firmware REST still accepts BRINGUP_AUTH_TOKEN when no ESP auth.token (M8.2 host-ready; iron operator default not claimed)";

/// How REST decides the operator credential.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthMode {
    /// Iron / firmware: bring-up mock unless ESP `auth.token` is armed.
    LabBringUp,
    /// Host/`cfg(test)`: operator token required; bring-up never allowed.
    HostReady,
}

/// Product firmware auth today. Host tests use [`AuthMode::HostReady`].
pub const FIRMWARE_AUTH_MODE: AuthMode = AuthMode::LabBringUp;

/// True until coexist requires ESP `auth.token` as the product default.
pub fn firmware_auth_is_lab_bringup() -> bool {
    matches!(FIRMWARE_AUTH_MODE, AuthMode::LabBringUp)
        && AUTH_FIRMWARE_LAB_NOTE.contains("BRINGUP_AUTH_TOKEN")
}

/// Host/CI must never print the iron auth marker.
pub fn host_never_prints_iron_auth_ok() -> bool {
    M8_AUTH_OK_MARKER == "RAYNU-V-M8-AUTH-OK"
        && M8_AUTH_HOST_OK_MARKER == "RAYNU-V-M8-AUTH-HOST-OK"
        && M8_AUTH_HOST_OK_MARKER != M8_AUTH_OK_MARKER
        && M8_AUTH_OK_MARKER != crate::mgmt::api::M6_AUTH_OK_MARKER
        && M8_AUTH_OK_MARKER != crate::mgmt::tls::M8_TLS_OK_MARKER
}

/// Product policy: bring-up is never the HostReady latch.
pub fn auth_allows_for(mode: AuthMode, token: Option<&str>) -> bool {
    match mode {
        AuthMode::LabBringUp => auth_allows(token),
        AuthMode::HostReady => {
            let Some(t) = token else {
                return false;
            };
            if t == BRINGUP_AUTH_TOKEN {
                return false;
            }
            auth_allows(token)
        }
    }
}

/// Host package: HostReady rejects bring-up; operator token works; firmware lab.
pub fn prop_auth_host_package() -> bool {
    clear_operator_token();
    let plan = include_str!("../docs/m8_plan.md");
    let host_ready_denies_bringup = !auth_allows_for(AuthMode::HostReady, Some(BRINGUP_AUTH_TOKEN))
        && !auth_allows_for(AuthMode::HostReady, None);
    let armed = set_operator_token(b"raynu-v-op-m82").is_ok()
        && !auth_allows_for(AuthMode::HostReady, Some(BRINGUP_AUTH_TOKEN))
        && auth_allows_for(AuthMode::HostReady, Some("raynu-v-op-m82"))
        && auth_allows_for(AuthMode::LabBringUp, Some("raynu-v-op-m82"));
    clear_operator_token();
    let lab_fallback = auth_allows_for(AuthMode::LabBringUp, Some(BRINGUP_AUTH_TOKEN));
    firmware_auth_is_lab_bringup()
        && host_never_prints_iron_auth_ok()
        && host_ready_denies_bringup
        && armed
        && lab_fallback
        && AUTH_HOST_RESIDUAL_NOTE.contains("not iron")
        && AUTH_TOKEN_SOURCE_NOTE.contains("auth.token")
        && AUTH_TOKEN_SOURCE_NOTE.contains("not product default")
        && plan.contains("M8.2")
        && plan.contains("Product default is not")
}

#[cfg(test)]
#[path = "auth_test.rs"]
mod auth_test;
