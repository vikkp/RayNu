//! E5 Stage 11 — firmware EDK2-sized stage (outside Proven Core).
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** (ADR-002 / ADR-003 / ADR-014)
//! VERIFICATION: N/A
//!
//! Host/CI: `stage_edk2_ovmf_firmware` records a 1 MiB EDK2-sized `_FVH`
//! after the size-floor. That is a size-qualified candidate, **not** a
//! shipped EDK2 `OVMF.fd`. `try_vmlaunch_ovmf_firmware` then returns
//! `LaunchNotWired`. Not guest UEFI VMLAUNCH.
//! `attach_cdrom_uefi` stays `UnsupportedOnFirmware`.
//! Do not claim Everest E5 / ISO-INSTALL-OK.

use super::api::{dispatch_rest, RestMethod, RestRequest, BRINGUP_AUTH_TOKEN};
use super::guest_fw::{
    arm_ovmf_firmware_slot, bind_ovmf_firmware_guest, box_guest_firmware, dispatch_guest_fw_rest,
    guest_fw_bytes, load_guest_firmware, load_ovmf_from_esp, ovmf_edk2_is_staged,
    ovmf_floor_is_staged, prepare_ovmf_firmware_launch, probe_ovmf_firmware, reset_guest_fw,
    stage_ovmf_firmware_floor, write_mock_ovmf_fv, write_size_floor_ovmf_fv, MOCK_OVMF_FV_BYTES,
    SIZE_FLOOR_FV_BYTES,
};
#[cfg(test)]
use super::guest_fw::{
    stage_edk2_ovmf_firmware, try_vmlaunch_ovmf_firmware, write_edk2_sized_fv, GuestFwError,
    MIN_EDK2_OVMF_BYTES,
};
use super::iso::attach_cdrom_uefi;
use super::m7_e5_cdrom_attach_gate::e4_shell_launch_no_cdrom;
use super::m7_e5_fw_floor_gate::run_m7_e5_fw_floor_gate;
use super::iso::IsoError;
use super::VmTable;

/// Host / CI marker when the E5 Stage 11 firmware-EDK2 gate passes.
pub const M7_E5_FW_EDK2_OK_MARKER: &str = "RAYNU-V-M7-E5-FW-EDK2-OK";

/// Honest residual: EDK2-sized ≠ shipped OVMF.fd ≠ VMLAUNCH.
pub const E5_FW_EDK2_RESIDUAL_NOTE: &str =
    "residual: 1MiB EDK2-sized candidate is not a shipped OVMF.fd; VMLAUNCH not wired; attach_cdrom_uefi stays UnsupportedOnFirmware; no guest UEFI VMLAUNCH";

fn stage_through_floor() -> bool {
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
    if probe_ovmf_firmware(&fv).is_err()
        || load_ovmf_from_esp(&fv).is_err()
        || arm_ovmf_firmware_slot().is_err()
        || bind_ovmf_firmware_guest().is_err()
        || prepare_ovmf_firmware_launch().is_err()
    {
        return false;
    }
    let mut floor = [0u8; SIZE_FLOOR_FV_BYTES];
    if write_size_floor_ovmf_fv(&mut floor).is_err() {
        return false;
    }
    stage_ovmf_firmware_floor(&floor).is_ok() && ovmf_floor_is_staged()
}

#[cfg(test)]
fn edk2_fixture() -> Option<Vec<u8>> {
    let mut buf = vec![0u8; MIN_EDK2_OVMF_BYTES];
    write_edk2_sized_fv(&mut buf).ok()?;
    Some(buf)
}

/// Floor then stage a 1 MiB EDK2-sized candidate. VMLAUNCH stays LaunchNotWired.
pub fn prop_fw_edk2_after_floor() -> bool {
    #[cfg(not(test))]
    {
        return false;
    }
    #[cfg(test)]
    {
        if !stage_through_floor() {
            return false;
        }
        let mut mock = [0u8; MOCK_OVMF_FV_BYTES];
        if write_mock_ovmf_fv(&mut mock).is_err() {
            return false;
        }
        if stage_edk2_ovmf_firmware(&mock) != Err(GuestFwError::TooSmall) {
            return false;
        }
        let mut floor = [0u8; SIZE_FLOOR_FV_BYTES];
        if write_size_floor_ovmf_fv(&mut floor).is_err() {
            return false;
        }
        if stage_edk2_ovmf_firmware(&floor) != Err(GuestFwError::TooSmall) {
            return false;
        }
        let Some(edk2) = edk2_fixture() else {
            return false;
        };
        match stage_edk2_ovmf_firmware(&edk2) {
            Ok(e) => {
                e.bytes_len == MIN_EDK2_OVMF_BYTES as u64
                    && ovmf_edk2_is_staged()
                    && try_vmlaunch_ovmf_firmware() == Err(GuestFwError::LaunchNotWired)
                    && MIN_EDK2_OVMF_BYTES > SIZE_FLOOR_FV_BYTES
            }
            Err(_) => false,
        }
    }
}

/// EDK2 without floor, reject. Mock VMLAUNCH stays closed.
pub fn prop_fw_edk2_rejects() -> bool {
    #[cfg(not(test))]
    {
        return attach_cdrom_uefi(1) == Err(IsoError::UnsupportedOnFirmware);
    }
    #[cfg(test)]
    {
        reset_guest_fw();
        let Some(edk2) = edk2_fixture() else {
            return false;
        };
        if stage_edk2_ovmf_firmware(&edk2) != Err(GuestFwError::NotRealFirmware) {
            return false;
        }
        attach_cdrom_uefi(1) == Err(IsoError::UnsupportedOnFirmware)
    }
}

/// Bearer REST: floor then EDK2. EDK2 without floor → 409.
/// `POST /fw/vmlaunch` → 409 (not wired). `iso=0` SHELL stays 201.
pub fn prop_rest_fw_edk2() -> bool {
    reset_guest_fw();
    let tok = Some(BRINGUP_AUTH_TOKEN);

    let missing = dispatch_guest_fw_rest(RestRequest {
        method: RestMethod::Post,
        path: "/fw/edk2",
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
        "/fw/floor",
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
        path: "/fw/edk2",
        auth_token: tok,
    });
    if staged.status != 201 || !ovmf_edk2_is_staged() {
        return false;
    }

    let st = dispatch_guest_fw_rest(RestRequest {
        method: RestMethod::Get,
        path: "/fw/edk2",
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
        path: "/fw/edk2",
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

/// SPA + ADR-014 Stage 11 phrases. EDK2-sized is not shipped OVMF / not VMLAUNCH.
pub fn fw_edk2_surface_present() -> bool {
    let spa = include_str!("../assets/webui.html");
    let adr = include_str!("../docs/adr/ADR-014.md");
    let src = include_str!("guest_fw.rs");
    let launch = include_str!("../vmx/launch.rs");
    spa.contains("Stage EDK2")
        && spa.contains("Stage FW floor")
        && spa.contains("Prep FW launch")
        && spa.contains("Bind FW guest")
        && spa.contains("not OVMF")
        && spa.contains("UEFI-first")
        && spa.contains("extract-boot is lab")
        && spa.contains("not guest UEFI")
        && crate::mgmt::webui::webui_len().saturating_add(256) <= 16384
        && adr.contains("Stage 11")
        && adr.contains("stage_edk2_ovmf_firmware")
        && src.contains("fn stage_edk2_ovmf_firmware")
        && src.contains("MIN_EDK2_OVMF_BYTES")
        && src.contains("LaunchNotWired")
        && src.contains("fn write_edk2_sized_fv")
        && !launch.contains("stage_edk2_ovmf_firmware")
        && !launch.contains("try_vmlaunch_ovmf_firmware")
        && !launch.contains("OvmfFirmwareEdk2Staged")
        && !launch.contains("write_edk2_sized_fv")
        && e4_shell_launch_no_cdrom()
}

/// Full E5 Stage 11 package. Host gate only — not iron, not Everest E5.
pub fn run_m7_e5_fw_edk2_gate() -> bool {
    let _ = (M7_E5_FW_EDK2_OK_MARKER, E5_FW_EDK2_RESIDUAL_NOTE);
    reset_guest_fw();
    let ok = prop_fw_edk2_after_floor()
        && prop_fw_edk2_rejects()
        && prop_rest_fw_edk2()
        && fw_edk2_surface_present()
        && run_m7_e5_fw_floor_gate()
        && E5_FW_EDK2_RESIDUAL_NOTE.contains("UnsupportedOnFirmware")
        && E5_FW_EDK2_RESIDUAL_NOTE.contains("no guest UEFI VMLAUNCH")
        && E5_FW_EDK2_RESIDUAL_NOTE.contains("not a shipped")
        && M7_E5_FW_EDK2_OK_MARKER == "RAYNU-V-M7-E5-FW-EDK2-OK";
    reset_guest_fw();
    ok
}

#[cfg(test)]
#[path = "m7_e5_fw_edk2_gate_test.rs"]
mod m7_e5_fw_edk2_gate_test;
