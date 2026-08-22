//! E5 Stage 9 — firmware launch-prepare (outside Proven Core).
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** (ADR-002 / ADR-003 / ADR-014)
//! VERIFICATION: N/A
//!
//! Host/CI: `prepare_ovmf_firmware_launch` records launch-prepare after
//! bind. `try_vmlaunch_ovmf_firmware` refuses the 80-byte mock FV.
//! That is not guest UEFI VMLAUNCH. `attach_cdrom_uefi` stays
//! `UnsupportedOnFirmware`. Do not claim Everest E5 / ISO-INSTALL-OK.

use super::api::{dispatch_rest, RestMethod, RestRequest, BRINGUP_AUTH_TOKEN};
use super::guest_fw::{
    arm_ovmf_firmware_slot, bind_ovmf_firmware_guest, box_guest_firmware, dispatch_guest_fw_rest,
    guest_fw_bytes, load_guest_firmware, load_ovmf_from_esp, ovmf_launch_is_prepared,
    prepare_ovmf_firmware_launch, probe_ovmf_firmware, reset_guest_fw, try_vmlaunch_ovmf_firmware,
    write_mock_ovmf_fv, GuestFwError, MOCK_OVMF_FV_BYTES, OVMF_FW_GUEST_ID, OVMF_FW_SLOT_ID,
};
use super::iso::attach_cdrom_uefi;
use super::m7_e5_cdrom_attach_gate::e4_shell_launch_no_cdrom;
use super::m7_e5_fw_bind_gate::run_m7_e5_fw_bind_gate;
use super::iso::IsoError;
use super::VmTable;

/// Host / CI marker when the E5 Stage 9 firmware-prep gate passes.
pub const M7_E5_FW_PREP_OK_MARKER: &str = "RAYNU-V-M7-E5-FW-PREP-OK";

/// Honest residual: launch-prepare ≠ VMLAUNCH; mock is refused.
pub const E5_FW_PREP_RESIDUAL_NOTE: &str =
    "residual: FW launch-prepare is bookkeeping; mock FV is refused for VMLAUNCH; attach_cdrom_uefi stays UnsupportedOnFirmware; not embedded EDK2; no guest UEFI VMLAUNCH";

fn stage_through_bind() -> bool {
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
    probe_ovmf_firmware(&fv).is_ok()
        && load_ovmf_from_esp(&fv).is_ok()
        && arm_ovmf_firmware_slot().is_ok()
        && bind_ovmf_firmware_guest().is_ok()
}

/// Bind then prepare launch. Mock VMLAUNCH is refused.
pub fn prop_fw_prep_after_bind() -> bool {
    if !stage_through_bind() {
        return false;
    }
    match prepare_ovmf_firmware_launch() {
        Ok(p) => {
            p.guest_id == OVMF_FW_GUEST_ID
                && p.slot_id == OVMF_FW_SLOT_ID
                && ovmf_launch_is_prepared()
                && try_vmlaunch_ovmf_firmware() == Err(GuestFwError::MockFirmwareRefused)
        }
        Err(_) => false,
    }
}

/// Prepare without bind, reject. Mock refuse stays closed.
pub fn prop_fw_prep_rejects() -> bool {
    reset_guest_fw();
    if prepare_ovmf_firmware_launch() != Err(GuestFwError::NotGuestBound) {
        return false;
    }
    if try_vmlaunch_ovmf_firmware() != Err(GuestFwError::NotGuestBound) {
        return false;
    }
    attach_cdrom_uefi(1) == Err(IsoError::UnsupportedOnFirmware)
}

/// Bearer REST: bind then prepare. Prepare without bind → 409.
/// `POST /fw/vmlaunch` → 409 (mock refused). `iso=0` SHELL stays 201.
pub fn prop_rest_fw_prep() -> bool {
    reset_guest_fw();
    let tok = Some(BRINGUP_AUTH_TOKEN);

    let missing = dispatch_guest_fw_rest(RestRequest {
        method: RestMethod::Post,
        path: "/fw/prepare",
        auth_token: tok,
    });
    if missing.status != 409 {
        return false;
    }

    for path in [
        "/fw/box",
        "/fw/load",
        "/fw/ovmf",
        "/fw/ovmf/esp",
        "/fw/slot",
        "/fw/bind",
    ] {
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

    let prepped = dispatch_guest_fw_rest(RestRequest {
        method: RestMethod::Post,
        path: "/fw/prepare",
        auth_token: tok,
    });
    if prepped.status != 201 || !ovmf_launch_is_prepared() {
        return false;
    }

    let st = dispatch_guest_fw_rest(RestRequest {
        method: RestMethod::Get,
        path: "/fw/prepare",
        auth_token: tok,
    });
    if st.status != 200 {
        return false;
    }

    let refused = dispatch_guest_fw_rest(RestRequest {
        method: RestMethod::Post,
        path: "/fw/vmlaunch",
        auth_token: tok,
    });
    if refused.status != 409 {
        return false;
    }

    let denied = dispatch_guest_fw_rest(RestRequest {
        method: RestMethod::Post,
        path: "/fw/prepare",
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

/// SPA + ADR-014 Stage 9 phrases. Prep is not VMLAUNCH.
pub fn fw_prep_surface_present() -> bool {
    let spa = include_str!("../assets/webui.html");
    let adr = include_str!("../docs/adr/ADR-014.md");
    let src = include_str!("guest_fw.rs");
    let launch = include_str!("../vmx/launch.rs");
    spa.contains("Prep FW launch")
        && spa.contains("Bind FW guest")
        && spa.contains("Arm FW slot")
        && spa.contains("not OVMF")
        && spa.contains("UEFI-first")
        && spa.contains("extract-boot is lab")
        && spa.contains("not guest UEFI")
        && crate::mgmt::webui::webui_len().saturating_add(256) <= 16384
        && adr.contains("Stage 9")
        && adr.contains("prepare_ovmf_firmware_launch")
        && src.contains("fn prepare_ovmf_firmware_launch")
        && src.contains("fn try_vmlaunch_ovmf_firmware")
        && src.contains("MockFirmwareRefused")
        && !launch.contains("prepare_ovmf_firmware_launch")
        && !launch.contains("try_vmlaunch_ovmf_firmware")
        && !launch.contains("OvmfFirmwareLaunchPrepared")
        && e4_shell_launch_no_cdrom()
}

/// Full E5 Stage 9 package. Host gate only — not iron, not Everest E5.
pub fn run_m7_e5_fw_prep_gate() -> bool {
    let _ = (M7_E5_FW_PREP_OK_MARKER, E5_FW_PREP_RESIDUAL_NOTE);
    reset_guest_fw();
    let ok = prop_fw_prep_after_bind()
        && prop_fw_prep_rejects()
        && prop_rest_fw_prep()
        && fw_prep_surface_present()
        && run_m7_e5_fw_bind_gate()
        && E5_FW_PREP_RESIDUAL_NOTE.contains("UnsupportedOnFirmware")
        && E5_FW_PREP_RESIDUAL_NOTE.contains("no guest UEFI VMLAUNCH")
        && E5_FW_PREP_RESIDUAL_NOTE.contains("refused")
        && M7_E5_FW_PREP_OK_MARKER == "RAYNU-V-M7-E5-FW-PREP-OK";
    reset_guest_fw();
    ok
}

#[cfg(test)]
#[path = "m7_e5_fw_prep_gate_test.rs"]
mod m7_e5_fw_prep_gate_test;
