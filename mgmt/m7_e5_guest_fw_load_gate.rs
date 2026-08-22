//! E5 Stage 4 — guest firmware stub payload load (outside Proven Core).
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** (ADR-002 / ADR-003 / ADR-014)
//! VERIFICATION: N/A
//!
//! Host/CI: `load_guest_firmware` identity-lazy loads the `RAYNUFD` stub
//! after the envelope is boxed. `attach_cdrom_uefi` stays
//! `UnsupportedOnFirmware`. The stub is not OVMF. E4 SHELL VMLAUNCH is
//! unchanged. Do not claim Everest E5 / ISO-INSTALL-OK.

use super::api::{dispatch_rest, RestMethod, RestRequest, BRINGUP_AUTH_TOKEN};
use super::guest_fw::{
    box_guest_firmware, dispatch_guest_fw_rest, guest_fw_bytes, guest_fw_is_loaded, guest_fw_payload,
    load_guest_firmware, reset_guest_fw, GuestFwError, GUEST_FW_STUB_PAYLOAD_LEN,
};
use super::iso::{attach_cdrom_uefi, IsoError};
use super::m7_e5_cdrom_attach_gate::e4_shell_launch_no_cdrom;
use super::m7_e5_guest_fw_gate::run_m7_e5_guest_fw_gate;
use super::VmTable;

/// Host / CI marker when the E5 Stage 4 guest-firmware load gate passes.
pub const M7_E5_GUEST_FW_LOAD_OK_MARKER: &str = "RAYNU-V-M7-E5-GUEST-FW-LOAD-OK";

/// Honest residual: loaded stub ≠ OVMF ≠ VMLAUNCH.
pub const E5_GUEST_FW_LOAD_RESIDUAL_NOTE: &str =
    "residual: guest FW load is identity-lazy stub; attach_cdrom_uefi stays UnsupportedOnFirmware; not OVMF; no guest UEFI VMLAUNCH";

/// Box then load the embedded stub payload.
pub fn prop_guest_fw_load_after_box() -> bool {
    reset_guest_fw();
    if box_guest_firmware(guest_fw_bytes()).is_err() {
        return false;
    }
    let loaded = match load_guest_firmware(guest_fw_bytes()) {
        Ok(b) => b,
        Err(_) => return false,
    };
    loaded.payload_len == GUEST_FW_STUB_PAYLOAD_LEN
        && guest_fw_is_loaded()
        && guest_fw_payload(guest_fw_bytes()).is_ok()
}

/// Load without box, and the UEFI stub, reject.
pub fn prop_guest_fw_load_rejects() -> bool {
    reset_guest_fw();
    if load_guest_firmware(guest_fw_bytes()) != Err(GuestFwError::NotBoxed) {
        return false;
    }
    attach_cdrom_uefi(1) == Err(IsoError::UnsupportedOnFirmware)
}

/// Bearer REST: box then load. Load without box → 409. `iso=0` SHELL stays 201.
pub fn prop_rest_guest_fw_load() -> bool {
    reset_guest_fw();
    let tok = Some(BRINGUP_AUTH_TOKEN);

    let missing = dispatch_guest_fw_rest(RestRequest {
        method: RestMethod::Post,
        path: "/fw/load",
        auth_token: tok,
    });
    if missing.status != 409 {
        return false;
    }

    let boxed = dispatch_guest_fw_rest(RestRequest {
        method: RestMethod::Post,
        path: "/fw/box",
        auth_token: tok,
    });
    if boxed.status != 201 {
        return false;
    }

    let loaded = dispatch_guest_fw_rest(RestRequest {
        method: RestMethod::Post,
        path: "/fw/load",
        auth_token: tok,
    });
    if loaded.status != 201 || !guest_fw_is_loaded() {
        return false;
    }

    let st = dispatch_guest_fw_rest(RestRequest {
        method: RestMethod::Get,
        path: "/fw/load",
        auth_token: tok,
    });
    if st.status != 200 {
        return false;
    }

    let denied = dispatch_guest_fw_rest(RestRequest {
        method: RestMethod::Post,
        path: "/fw/load",
        auth_token: None,
    });
    if denied.status != 401 {
        return false;
    }

    let mut t = VmTable::new();
    let shell = dispatch_rest(
        &mut t,
        RestRequest {
            method: RestMethod::Post,
            path: "/vms/1/spec/1/512/1024/0",
            auth_token: tok,
        },
    );
    shell.status == 201 && t.get(1).map(|r| r.iso_id) == Some(0)
}

/// SPA + ADR-014 Stage 4 phrases. Stub load is not OVMF.
pub fn guest_fw_load_surface_present() -> bool {
    let spa = include_str!("../assets/webui.html");
    let adr = include_str!("../docs/adr/ADR-014.md");
    let http = include_str!("http.rs");
    let src = include_str!("guest_fw.rs");
    let launch = include_str!("../vmx/launch.rs");
    spa.contains("Load guest FW")
        && spa.contains("not OVMF")
        && spa.contains("UEFI-first")
        && spa.contains("extract-boot is lab")
        && spa.contains("not guest UEFI")
        && crate::mgmt::webui::webui_len().saturating_add(256) <= 16384
        && http.contains("is_guest_fw_path")
        && adr.contains("Stage 4")
        && adr.contains("load_guest_firmware")
        && src.contains("fn load_guest_firmware")
        && src.contains("RAYNUFD")
        && !launch.contains("load_guest_firmware")
        && !launch.contains("guest_fw_payload")
        && e4_shell_launch_no_cdrom()
}

/// Full E5 Stage 4 package. Host gate only — not iron, not Everest E5.
pub fn run_m7_e5_guest_fw_load_gate() -> bool {
    let _ = (M7_E5_GUEST_FW_LOAD_OK_MARKER, E5_GUEST_FW_LOAD_RESIDUAL_NOTE);
    reset_guest_fw();
    let ok = prop_guest_fw_load_after_box()
        && prop_guest_fw_load_rejects()
        && prop_rest_guest_fw_load()
        && guest_fw_load_surface_present()
        && run_m7_e5_guest_fw_gate()
        && E5_GUEST_FW_LOAD_RESIDUAL_NOTE.contains("UnsupportedOnFirmware")
        && E5_GUEST_FW_LOAD_RESIDUAL_NOTE.contains("not OVMF")
        && M7_E5_GUEST_FW_LOAD_OK_MARKER == "RAYNU-V-M7-E5-GUEST-FW-LOAD-OK";
    reset_guest_fw();
    ok
}

#[cfg(test)]
#[path = "m7_e5_guest_fw_load_gate_test.rs"]
mod m7_e5_guest_fw_load_gate_test;
