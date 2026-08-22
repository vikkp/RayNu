//! E5 Stage 7 — guest firmware slot arm (outside Proven Core).
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** (ADR-002 / ADR-003 / ADR-014)
//! VERIFICATION: N/A
//!
//! Host/CI: `arm_ovmf_firmware_slot` records slot 1 after ESP load.
//! That is bookkeeping, not guest UEFI VMLAUNCH.
//! `attach_cdrom_uefi` stays `UnsupportedOnFirmware`.
//! Do not claim Everest E5 / ISO-INSTALL-OK.

use super::api::{dispatch_rest, RestMethod, RestRequest, BRINGUP_AUTH_TOKEN};
use super::guest_fw::{
    arm_ovmf_firmware_slot, box_guest_firmware, dispatch_guest_fw_rest, guest_fw_bytes,
    load_guest_firmware, load_ovmf_from_esp, ovmf_slot_is_armed, probe_ovmf_firmware,
    reset_guest_fw, write_mock_ovmf_fv, GuestFwError, MOCK_OVMF_FV_BYTES, OVMF_FW_SLOT_ID,
};
use super::iso::attach_cdrom_uefi;
use super::m7_e5_cdrom_attach_gate::e4_shell_launch_no_cdrom;
use super::m7_e5_ovmf_esp_gate::run_m7_e5_ovmf_esp_gate;
use super::iso::IsoError;
use super::VmTable;

/// Host / CI marker when the E5 Stage 7 firmware-slot gate passes.
pub const M7_E5_OVMF_SLOT_OK_MARKER: &str = "RAYNU-V-M7-E5-OVMF-SLOT-OK";

/// Honest residual: slot arm ≠ VMLAUNCH.
pub const E5_OVMF_SLOT_RESIDUAL_NOTE: &str =
    "residual: FW slot arm is bookkeeping after ESP load; attach_cdrom_uefi stays UnsupportedOnFirmware; not embedded EDK2; no guest UEFI VMLAUNCH";

fn stage_through_esp() -> bool {
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
    probe_ovmf_firmware(&fv).is_ok() && load_ovmf_from_esp(&fv).is_ok()
}

/// ESP load then arm slot 1.
pub fn prop_ovmf_slot_after_esp() -> bool {
    if !stage_through_esp() {
        return false;
    }
    match arm_ovmf_firmware_slot() {
        Ok(s) => s.slot_id == OVMF_FW_SLOT_ID && ovmf_slot_is_armed(),
        Err(_) => false,
    }
}

/// Slot arm without ESP load, reject.
pub fn prop_ovmf_slot_rejects() -> bool {
    reset_guest_fw();
    if arm_ovmf_firmware_slot() != Err(GuestFwError::NotEspLoaded) {
        return false;
    }
    attach_cdrom_uefi(1) == Err(IsoError::UnsupportedOnFirmware)
}

/// Bearer REST: ESP then slot. Slot without ESP → 409. `iso=0` SHELL stays 201.
pub fn prop_rest_ovmf_slot() -> bool {
    reset_guest_fw();
    let tok = Some(BRINGUP_AUTH_TOKEN);

    let missing = dispatch_guest_fw_rest(RestRequest {
        method: RestMethod::Post,
        path: "/fw/slot",
        auth_token: tok,
    });
    if missing.status != 409 {
        return false;
    }

    for path in ["/fw/box", "/fw/load", "/fw/ovmf", "/fw/ovmf/esp"] {
        if dispatch_guest_fw_rest(RestRequest {
            method: RestMethod::Post,
            path,
            auth_token: tok,
        })
        .status
            != 201
        {
            return false;
        }
    }

    let armed = dispatch_guest_fw_rest(RestRequest {
        method: RestMethod::Post,
        path: "/fw/slot",
        auth_token: tok,
    });
    if armed.status != 201 || !ovmf_slot_is_armed() {
        return false;
    }

    let st = dispatch_guest_fw_rest(RestRequest {
        method: RestMethod::Get,
        path: "/fw/slot",
        auth_token: tok,
    });
    if st.status != 200 {
        return false;
    }

    let denied = dispatch_guest_fw_rest(RestRequest {
        method: RestMethod::Post,
        path: "/fw/slot",
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

/// SPA + ADR-014 Stage 7 phrases. Slot arm is not VMLAUNCH.
pub fn ovmf_slot_surface_present() -> bool {
    let spa = include_str!("../assets/webui.html");
    let adr = include_str!("../docs/adr/ADR-014.md");
    let src = include_str!("guest_fw.rs");
    let launch = include_str!("../vmx/launch.rs");
    spa.contains("Arm FW slot")
        && spa.contains("Load ESP OVMF")
        && spa.contains("Probe OVMF")
        && spa.contains("not OVMF")
        && spa.contains("UEFI-first")
        && spa.contains("extract-boot is lab")
        && spa.contains("not guest UEFI")
        && crate::mgmt::webui::webui_len().saturating_add(256) <= 16384
        && adr.contains("Stage 7")
        && adr.contains("arm_ovmf_firmware_slot")
        && src.contains("fn arm_ovmf_firmware_slot")
        && src.contains("OVMF_FW_SLOT_ID")
        && !launch.contains("arm_ovmf_firmware_slot")
        && !launch.contains("OvmfFirmwareSlotArmed")
        && e4_shell_launch_no_cdrom()
}

/// Full E5 Stage 7 package. Host gate only — not iron, not Everest E5.
pub fn run_m7_e5_ovmf_slot_gate() -> bool {
    let _ = (M7_E5_OVMF_SLOT_OK_MARKER, E5_OVMF_SLOT_RESIDUAL_NOTE);
    reset_guest_fw();
    let ok = prop_ovmf_slot_after_esp()
        && prop_ovmf_slot_rejects()
        && prop_rest_ovmf_slot()
        && ovmf_slot_surface_present()
        && run_m7_e5_ovmf_esp_gate()
        && E5_OVMF_SLOT_RESIDUAL_NOTE.contains("UnsupportedOnFirmware")
        && E5_OVMF_SLOT_RESIDUAL_NOTE.contains("no guest UEFI VMLAUNCH")
        && M7_E5_OVMF_SLOT_OK_MARKER == "RAYNU-V-M7-E5-OVMF-SLOT-OK";
    reset_guest_fw();
    ok
}

#[cfg(test)]
#[path = "m7_e5_ovmf_slot_gate_test.rs"]
mod m7_e5_ovmf_slot_gate_test;
