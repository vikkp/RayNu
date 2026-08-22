//! E5 Stage 10 — firmware size-floor (outside Proven Core).
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** (ADR-002 / ADR-003 / ADR-014)
//! VERIFICATION: N/A
//!
//! Host/CI: `stage_ovmf_firmware_floor` records a 4 KiB size-floor FV
//! after launch-prepare. That is larger than the 80-byte mock and
//! smaller than EDK2. `try_vmlaunch_ovmf_firmware` then returns
//! `NotRealFirmware`. Not guest UEFI VMLAUNCH.
//! `attach_cdrom_uefi` stays `UnsupportedOnFirmware`.
//! Do not claim Everest E5 / ISO-INSTALL-OK.

use super::api::{dispatch_rest, RestMethod, RestRequest, BRINGUP_AUTH_TOKEN};
use super::guest_fw::{
    arm_ovmf_firmware_slot, bind_ovmf_firmware_guest, box_guest_firmware, dispatch_guest_fw_rest,
    guest_fw_bytes, load_guest_firmware, load_ovmf_from_esp, ovmf_floor_is_staged,
    prepare_ovmf_firmware_launch, probe_ovmf_firmware, reset_guest_fw, stage_ovmf_firmware_floor,
    try_vmlaunch_ovmf_firmware, write_mock_ovmf_fv, write_size_floor_ovmf_fv, GuestFwError,
    MIN_EDK2_OVMF_BYTES, MIN_LAUNCH_FV_BYTES, MOCK_OVMF_FV_BYTES, SIZE_FLOOR_FV_BYTES,
};
use super::iso::attach_cdrom_uefi;
use super::m7_e5_cdrom_attach_gate::e4_shell_launch_no_cdrom;
use super::m7_e5_fw_prep_gate::run_m7_e5_fw_prep_gate;
use super::iso::IsoError;
use super::VmTable;

/// Host / CI marker when the E5 Stage 10 firmware-floor gate passes.
pub const M7_E5_FW_FLOOR_OK_MARKER: &str = "RAYNU-V-M7-E5-FW-FLOOR-OK";

/// Honest residual: size-floor ≠ EDK2 ≠ VMLAUNCH.
pub const E5_FW_FLOOR_RESIDUAL_NOTE: &str =
    "residual: FW size-floor is 4KiB bookkeeping; not EDK2; mock and floor are refused for VMLAUNCH; attach_cdrom_uefi stays UnsupportedOnFirmware; no guest UEFI VMLAUNCH";

fn stage_through_prepare() -> bool {
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
        && prepare_ovmf_firmware_launch().is_ok()
}

/// Prepare then stage the 4 KiB floor. VMLAUNCH stays NotRealFirmware.
pub fn prop_fw_floor_after_prep() -> bool {
    if !stage_through_prepare() {
        return false;
    }
    let mut mock = [0u8; MOCK_OVMF_FV_BYTES];
    if write_mock_ovmf_fv(&mut mock).is_err() {
        return false;
    }
    if stage_ovmf_firmware_floor(&mock) != Err(GuestFwError::TooSmall) {
        return false;
    }
    let mut floor = [0u8; SIZE_FLOOR_FV_BYTES];
    if write_size_floor_ovmf_fv(&mut floor).is_err() {
        return false;
    }
    match stage_ovmf_firmware_floor(&floor) {
        Ok(f) => {
            f.bytes_len == SIZE_FLOOR_FV_BYTES as u64
                && ovmf_floor_is_staged()
                && try_vmlaunch_ovmf_firmware() == Err(GuestFwError::NotRealFirmware)
                && SIZE_FLOOR_FV_BYTES > MOCK_OVMF_FV_BYTES
                && SIZE_FLOOR_FV_BYTES < MIN_EDK2_OVMF_BYTES
                && MIN_LAUNCH_FV_BYTES == SIZE_FLOOR_FV_BYTES
        }
        Err(_) => false,
    }
}

/// Floor without prepare, reject. Mock VMLAUNCH stays closed.
pub fn prop_fw_floor_rejects() -> bool {
    reset_guest_fw();
    let mut floor = [0u8; SIZE_FLOOR_FV_BYTES];
    if write_size_floor_ovmf_fv(&mut floor).is_err() {
        return false;
    }
    if stage_ovmf_firmware_floor(&floor) != Err(GuestFwError::NotGuestBound) {
        return false;
    }
    attach_cdrom_uefi(1) == Err(IsoError::UnsupportedOnFirmware)
}

/// Bearer REST: prepare then floor. Floor without prepare → 409.
/// `POST /fw/vmlaunch` → 409 (not EDK2). `iso=0` SHELL stays 201.
pub fn prop_rest_fw_floor() -> bool {
    reset_guest_fw();
    let tok = Some(BRINGUP_AUTH_TOKEN);

    let missing = dispatch_guest_fw_rest(RestRequest {
        method: RestMethod::Post,
        path: "/fw/floor",
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
        "/fw/prepare",
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

    let staged = dispatch_guest_fw_rest(RestRequest {
        method: RestMethod::Post,
        path: "/fw/floor",
        auth_token: tok,
    });
    if staged.status != 201 || !ovmf_floor_is_staged() {
        return false;
    }

    let st = dispatch_guest_fw_rest(RestRequest {
        method: RestMethod::Get,
        path: "/fw/floor",
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
        path: "/fw/floor",
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

/// SPA + ADR-014 Stage 10 phrases. Floor is not EDK2 / not VMLAUNCH.
pub fn fw_floor_surface_present() -> bool {
    let spa = include_str!("../assets/webui.html");
    let adr = include_str!("../docs/adr/ADR-014.md");
    let src = include_str!("guest_fw.rs");
    let launch = include_str!("../vmx/launch.rs");
    spa.contains("Stage FW floor")
        && spa.contains("Prep FW launch")
        && spa.contains("Bind FW guest")
        && spa.contains("not OVMF")
        && spa.contains("UEFI-first")
        && spa.contains("extract-boot is lab")
        && spa.contains("not guest UEFI")
        && crate::mgmt::webui::webui_len().saturating_add(256) <= 16384
        && adr.contains("Stage 10")
        && adr.contains("stage_ovmf_firmware_floor")
        && src.contains("fn stage_ovmf_firmware_floor")
        && src.contains("SIZE_FLOOR_FV_BYTES")
        && src.contains("MIN_EDK2_OVMF_BYTES")
        && src.contains("NotRealFirmware")
        && !launch.contains("stage_ovmf_firmware_floor")
        && !launch.contains("try_vmlaunch_ovmf_firmware")
        && !launch.contains("OvmfFirmwareFloorStaged")
        && e4_shell_launch_no_cdrom()
}

/// Full E5 Stage 10 package. Host gate only — not iron, not Everest E5.
pub fn run_m7_e5_fw_floor_gate() -> bool {
    let _ = (M7_E5_FW_FLOOR_OK_MARKER, E5_FW_FLOOR_RESIDUAL_NOTE);
    reset_guest_fw();
    let ok = prop_fw_floor_after_prep()
        && prop_fw_floor_rejects()
        && prop_rest_fw_floor()
        && fw_floor_surface_present()
        && run_m7_e5_fw_prep_gate()
        && E5_FW_FLOOR_RESIDUAL_NOTE.contains("UnsupportedOnFirmware")
        && E5_FW_FLOOR_RESIDUAL_NOTE.contains("no guest UEFI VMLAUNCH")
        && E5_FW_FLOOR_RESIDUAL_NOTE.contains("not EDK2")
        && M7_E5_FW_FLOOR_OK_MARKER == "RAYNU-V-M7-E5-FW-FLOOR-OK";
    reset_guest_fw();
    ok
}

#[cfg(test)]
#[path = "m7_e5_fw_floor_gate_test.rs"]
mod m7_e5_fw_floor_gate_test;
