//! E5 Stage 13 — live ESP OVMF map (outside Proven Core).
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** mgmt bookkeeping; launch.rs entry is L0 (ADR-014)
//! VERIFICATION: N/A
//!
//! Host/CI: `map_live_esp_ovmf` records a live-sized (2 MiB+) `_FVH` after
//! ESP launch is armed. `try_vmlaunch_ovmf_firmware` then calls
//! `try_vmlaunch_guest_uefi_ovmf`, which refuses to issue VMLAUNCH
//! (`LiveMappedNotLaunched`). The 1 MiB EDK2 fixture is `TooSmall`.
//! `attach_cdrom_uefi` stays `UnsupportedOnFirmware`.
//! Do not claim Everest E5 / ISO-INSTALL-OK.

use super::api::{dispatch_rest, RestMethod, RestRequest, BRINGUP_AUTH_TOKEN};
use super::guest_fw::{
    arm_ovmf_esp_launch, arm_ovmf_firmware_slot, bind_ovmf_firmware_guest, box_guest_firmware,
    dispatch_guest_fw_rest, guest_fw_bytes, load_guest_firmware, load_ovmf_from_esp,
    map_live_esp_ovmf, ovmf_live_esp_is_mapped, prepare_ovmf_firmware_launch, probe_ovmf_firmware,
    reset_guest_fw, stage_ovmf_firmware_floor, write_mock_ovmf_fv, write_size_floor_ovmf_fv,
    GuestFwError, MOCK_OVMF_FV_BYTES, SIZE_FLOOR_FV_BYTES,
};
use super::iso::attach_cdrom_uefi;
use super::m7_e5_cdrom_attach_gate::e4_shell_launch_no_cdrom;
use super::m7_e5_esp_launch_gate::run_m7_e5_esp_launch_gate;
use super::iso::IsoError;
use super::VmTable;

#[cfg(test)]
use super::guest_fw::{
    stage_edk2_ovmf_firmware, try_vmlaunch_ovmf_firmware, write_edk2_sized_fv, write_live_esp_ovmf_fv,
    MIN_EDK2_OVMF_BYTES, MIN_LIVE_ESP_OVMF_BYTES,
};

/// Host / CI marker when the E5 Stage 13 live-map gate passes.
pub const M7_E5_ESP_MAP_OK_MARKER: &str = "RAYNU-V-M7-E5-ESP-MAP-OK";

/// Honest residual: live-sized map recorded; VMLAUNCH insn not issued.
pub const E5_ESP_MAP_RESIDUAL_NOTE: &str =
    "residual: live-sized ESP map is recorded; not a shipped OVMF.fd; VMLAUNCH insn not issued; attach_cdrom_uefi stays UnsupportedOnFirmware; no guest UEFI VMLAUNCH";

fn stage_through_esp_launch() -> bool {
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
    if stage_ovmf_firmware_floor(&floor).is_err() {
        return false;
    }
    #[cfg(test)]
    {
        let mut edk2 = vec![0u8; MIN_EDK2_OVMF_BYTES];
        if write_edk2_sized_fv(&mut edk2).is_err() {
            return false;
        }
        if stage_edk2_ovmf_firmware(&edk2).is_err() {
            return false;
        }
        return arm_ovmf_esp_launch().is_ok();
    }
    #[cfg(not(test))]
    {
        false
    }
}

/// ESP launch then live-sized map. VMLAUNCH stays LiveMappedNotLaunched.
pub fn prop_esp_map_after_launch() -> bool {
    #[cfg(not(test))]
    {
        return false;
    }
    #[cfg(test)]
    {
        if !stage_through_esp_launch() {
            return false;
        }
        let mut edk2 = vec![0u8; MIN_EDK2_OVMF_BYTES];
        if write_edk2_sized_fv(&mut edk2).is_err() {
            return false;
        }
        if map_live_esp_ovmf(&edk2) != Err(GuestFwError::TooSmall) {
            return false;
        }
        let mut live = vec![0u8; MIN_LIVE_ESP_OVMF_BYTES];
        if write_live_esp_ovmf_fv(&mut live).is_err() {
            return false;
        }
        match map_live_esp_ovmf(&live) {
            Ok(m) => {
                m.bytes_len == MIN_LIVE_ESP_OVMF_BYTES as u64
                    && ovmf_live_esp_is_mapped()
                    && try_vmlaunch_ovmf_firmware() == Err(GuestFwError::LiveMappedNotLaunched)
            }
            Err(_) => false,
        }
    }
}

/// Live map without ESP launch, reject. attach_cdrom_uefi stays closed.
pub fn prop_esp_map_rejects() -> bool {
    reset_guest_fw();
    if arm_ovmf_esp_launch() != Err(GuestFwError::LaunchNotWired) {
        return false;
    }
    if map_live_esp_ovmf(&[]) != Err(GuestFwError::LaunchNotWired) {
        return false;
    }
    attach_cdrom_uefi(1) == Err(IsoError::UnsupportedOnFirmware)
}

/// Bearer REST: ESP launch then live map. Without launch → 409.
/// `POST /fw/vmlaunch` → 409 (LiveMappedNotLaunched). `iso=0` SHELL stays 201.
pub fn prop_rest_esp_map() -> bool {
    reset_guest_fw();
    let tok = Some(BRINGUP_AUTH_TOKEN);

    let missing = dispatch_guest_fw_rest(RestRequest {
        method: RestMethod::Post,
        path: "/fw/esp-map",
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
        "/fw/edk2",
        "/fw/esp-launch",
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

    let mapped = dispatch_guest_fw_rest(RestRequest {
        method: RestMethod::Post,
        path: "/fw/esp-map",
        auth_token: tok,
    });
    if mapped.status != 201 || !ovmf_live_esp_is_mapped() {
        return false;
    }

    let st = dispatch_guest_fw_rest(RestRequest {
        method: RestMethod::Get,
        path: "/fw/esp-map",
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
        path: "/fw/esp-map",
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

/// SPA + ADR-014 Stage 13 phrases. Live map recorded; VMLAUNCH insn not issued.
pub fn esp_map_surface_present() -> bool {
    let spa = include_str!("../assets/webui.html");
    let adr = include_str!("../docs/adr/ADR-014.md");
    let src = include_str!("guest_fw.rs");
    let launch = include_str!("../vmx/launch.rs");
    spa.contains("Map live ESP")
        && spa.contains("Arm ESP launch")
        && spa.contains("Stage EDK2")
        && spa.contains("Stage FW floor")
        && spa.contains("not OVMF")
        && spa.contains("UEFI-first")
        && spa.contains("extract-boot is lab")
        && spa.contains("not guest UEFI")
        && crate::mgmt::webui::webui_len().saturating_add(256) <= 16384
        && adr.contains("Stage 13")
        && adr.contains("map_live_esp_ovmf")
        && adr.contains("LiveMappedNotLaunched")
        && src.contains("fn map_live_esp_ovmf")
        && src.contains("ovmf_live_esp_is_mapped")
        && src.contains("MIN_LIVE_ESP_OVMF_BYTES")
        && launch.contains("fn try_vmlaunch_guest_uefi_ovmf")
        && launch.contains("fn arm_live_esp_ovmf_mapping")
        && launch.contains("LiveMappedNotLaunched")
        && launch.contains("MIN_LIVE_ESP_OVMF_BYTES")
        && launch.contains("GUEST_UEFI_OVMF_ESP_PATH")
        && !launch.contains("map_live_esp_ovmf")
        && !launch.contains("stage_edk2_ovmf_firmware")
        && !launch.contains("OvmfEspLaunchArmed")
        && !launch.contains("arm_ovmf_esp_launch")
        && !launch.contains("try_vmlaunch_ovmf_firmware")
        && !launch.contains("OvmfFirmwareEdk2Staged")
        && !launch.contains("write_edk2_sized_fv")
        && e4_shell_launch_no_cdrom()
}

/// Full E5 Stage 13 package. Host gate only — not iron, not Everest E5.
pub fn run_m7_e5_esp_map_gate() -> bool {
    let _ = (M7_E5_ESP_MAP_OK_MARKER, E5_ESP_MAP_RESIDUAL_NOTE);
    reset_guest_fw();
    let ok = prop_esp_map_after_launch()
        && prop_esp_map_rejects()
        && prop_rest_esp_map()
        && esp_map_surface_present()
        && run_m7_e5_esp_launch_gate()
        && E5_ESP_MAP_RESIDUAL_NOTE.contains("UnsupportedOnFirmware")
        && E5_ESP_MAP_RESIDUAL_NOTE.contains("no guest UEFI VMLAUNCH")
        && E5_ESP_MAP_RESIDUAL_NOTE.contains("not a shipped OVMF.fd")
        && E5_ESP_MAP_RESIDUAL_NOTE.contains("VMLAUNCH insn not issued")
        && M7_E5_ESP_MAP_OK_MARKER == "RAYNU-V-M7-E5-ESP-MAP-OK";
    reset_guest_fw();
    ok
}

#[cfg(test)]
#[path = "m7_e5_esp_map_gate_test.rs"]
mod m7_e5_esp_map_gate_test;
