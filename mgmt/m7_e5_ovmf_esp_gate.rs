//! E5 Stage 6 — ESP OVMF load (outside Proven Core).
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** (ADR-002 / ADR-003 / ADR-014)
//! VERIFICATION: N/A
//!
//! Host/CI: `load_ovmf_from_esp` validates split-mode `_FVH` bytes after
//! the Stage 5 probe. REST uses a tiny mock FV as the ESP fixture, not a
//! 4 MiB EDK2 image. Real bytes stay on ESP `\\EFI\\RayNu\\OVMF.fd`.
//! `attach_cdrom_uefi` stays `UnsupportedOnFirmware`. Not VMLAUNCH.
//! Do not claim Everest E5 / ISO-INSTALL-OK.

use super::api::{dispatch_rest, RestMethod, RestRequest, BRINGUP_AUTH_TOKEN};
use super::guest_fw::{
    box_guest_firmware, dispatch_guest_fw_rest, guest_fw_bytes, load_guest_firmware,
    load_ovmf_from_esp, ovmf_esp_is_loaded, probe_ovmf_firmware, reset_guest_fw,
    write_mock_ovmf_fv, GuestFwError, MOCK_OVMF_FV_BYTES,
};
use super::iso::attach_cdrom_uefi;
use super::m7_e5_cdrom_attach_gate::e4_shell_launch_no_cdrom;
use super::m7_e5_ovmf_probe_gate::run_m7_e5_ovmf_probe_gate;
use super::iso::IsoError;
use super::VmTable;

/// Host / CI marker when the E5 Stage 6 ESP-OVMF gate passes.
pub const M7_E5_OVMF_ESP_OK_MARKER: &str = "RAYNU-V-M7-E5-OVMF-ESP-OK";

/// Honest residual: ESP load ≠ embedded EDK2 ≠ VMLAUNCH.
pub const E5_OVMF_ESP_RESIDUAL_NOTE: &str =
    "residual: ESP OVMF load is host mock fixture + split-mode path; attach_cdrom_uefi stays UnsupportedOnFirmware; not embedded EDK2; no guest UEFI VMLAUNCH";

/// Box + load + probe + ESP-load the host mock fixture.
pub fn prop_ovmf_esp_after_probe() -> bool {
    reset_guest_fw();
    if box_guest_firmware(guest_fw_bytes()).is_err() {
        return false;
    }
    if load_guest_firmware(guest_fw_bytes()).is_err() {
        return false;
    }
    let mut fv = [0u8; MOCK_OVMF_FV_BYTES];
    if write_mock_ovmf_fv(&mut fv).is_err() {
        return false;
    }
    if probe_ovmf_firmware(&fv).is_err() {
        return false;
    }
    match load_ovmf_from_esp(&fv) {
        Ok(p) => p.fv_len == MOCK_OVMF_FV_BYTES as u64 && ovmf_esp_is_loaded(),
        Err(_) => false,
    }
}

/// ESP load without probe, and empty bytes, reject.
pub fn prop_ovmf_esp_rejects() -> bool {
    reset_guest_fw();
    let mut fv = [0u8; MOCK_OVMF_FV_BYTES];
    if write_mock_ovmf_fv(&mut fv).is_err() {
        return false;
    }
    if load_ovmf_from_esp(&fv) != Err(GuestFwError::NotProbed) {
        return false;
    }
    if box_guest_firmware(guest_fw_bytes()).is_err()
        || load_guest_firmware(guest_fw_bytes()).is_err()
        || probe_ovmf_firmware(&fv).is_err()
    {
        return false;
    }
    if load_ovmf_from_esp(&[]) != Err(GuestFwError::MissingEsp) {
        return false;
    }
    attach_cdrom_uefi(1) == Err(IsoError::UnsupportedOnFirmware)
}

/// Bearer REST: probe then ESP load. ESP without probe → 409. `iso=0` SHELL stays 201.
pub fn prop_rest_ovmf_esp() -> bool {
    reset_guest_fw();
    let tok = Some(BRINGUP_AUTH_TOKEN);

    let missing = dispatch_guest_fw_rest(RestRequest {
        method: RestMethod::Post,
        path: "/fw/ovmf/esp",
        auth_token: tok,
    });
    if missing.status != 409 {
        return false;
    }

    if dispatch_guest_fw_rest(RestRequest {
        method: RestMethod::Post,
        path: "/fw/box",
        auth_token: tok,
    })
    .status
        != 201
    {
        return false;
    }
    if dispatch_guest_fw_rest(RestRequest {
        method: RestMethod::Post,
        path: "/fw/load",
        auth_token: tok,
    })
    .status
        != 201
    {
        return false;
    }
    if dispatch_guest_fw_rest(RestRequest {
        method: RestMethod::Post,
        path: "/fw/ovmf",
        auth_token: tok,
    })
    .status
        != 201
    {
        return false;
    }

    let loaded = dispatch_guest_fw_rest(RestRequest {
        method: RestMethod::Post,
        path: "/fw/ovmf/esp",
        auth_token: tok,
    });
    if loaded.status != 201 || !ovmf_esp_is_loaded() {
        return false;
    }

    let st = dispatch_guest_fw_rest(RestRequest {
        method: RestMethod::Get,
        path: "/fw/ovmf/esp",
        auth_token: tok,
    });
    if st.status != 200 {
        return false;
    }

    let denied = dispatch_guest_fw_rest(RestRequest {
        method: RestMethod::Post,
        path: "/fw/ovmf/esp",
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

/// SPA + ADR-014 Stage 6 phrases. ESP load is not embedded EDK2.
pub fn ovmf_esp_surface_present() -> bool {
    let spa = include_str!("../assets/webui.html");
    let adr = include_str!("../docs/adr/ADR-014.md");
    let src = include_str!("guest_fw.rs");
    let launch = include_str!("../vmx/launch.rs");
    spa.contains("Load ESP OVMF")
        && spa.contains("Probe OVMF")
        && spa.contains("not OVMF")
        && spa.contains("UEFI-first")
        && spa.contains("extract-boot is lab")
        && spa.contains("not guest UEFI")
        && crate::mgmt::webui::webui_len().saturating_add(256) <= 16384
        && adr.contains("Stage 6")
        && adr.contains("load_ovmf_from_esp")
        && src.contains("fn load_ovmf_from_esp")
        && src.contains("OVMF_ESP_PATH")
        && src.contains("OVMF.fd")
        && adr.contains("OVMF.fd")
        && !launch.contains("load_ovmf_from_esp")
        && !launch.contains("OvmfFirmwareEspLoaded")
        && e4_shell_launch_no_cdrom()
}

/// Full E5 Stage 6 package. Host gate only — not iron, not Everest E5.
pub fn run_m7_e5_ovmf_esp_gate() -> bool {
    let _ = (M7_E5_OVMF_ESP_OK_MARKER, E5_OVMF_ESP_RESIDUAL_NOTE);
    reset_guest_fw();
    let ok = prop_ovmf_esp_after_probe()
        && prop_ovmf_esp_rejects()
        && prop_rest_ovmf_esp()
        && ovmf_esp_surface_present()
        && run_m7_e5_ovmf_probe_gate()
        && E5_OVMF_ESP_RESIDUAL_NOTE.contains("UnsupportedOnFirmware")
        && E5_OVMF_ESP_RESIDUAL_NOTE.contains("not embedded EDK2")
        && M7_E5_OVMF_ESP_OK_MARKER == "RAYNU-V-M7-E5-OVMF-ESP-OK";
    reset_guest_fw();
    ok
}

#[cfg(test)]
#[path = "m7_e5_ovmf_esp_gate_test.rs"]
mod m7_e5_ovmf_esp_gate_test;
