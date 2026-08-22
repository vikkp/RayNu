//! E5 Stage 18 — real-ESP VMLAUNCH-ready contract (outside Proven Core).
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** mgmt bookkeeping; launch.rs entry is L0 (ADR-014)
//! VERIFICATION: N/A
//!
//! Host/CI: `qualify_real_esp_ovmf` records that VMLAUNCH is ready for
//! real ESP `\\EFI\\RayNu\\OVMF.fd` only. `try_vmlaunch_ovmf_firmware`
//! then calls `try_vmlaunch_guest_uefi_ovmf`, which refuses to issue
//! VMLAUNCH (`RealEspNotLaunched`). A 2 MiB fixture is `TooSmall`.
//! The E4 SHELL EPT is not written. `attach_cdrom_uefi` stays
//! `UnsupportedOnFirmware`.
//! Do not claim Everest E5 / ISO-INSTALL-OK.

use super::api::{dispatch_rest, RestMethod, RestRequest, BRINGUP_AUTH_TOKEN};
use super::guest_fw::{
    arm_ovmf_firmware_slot, bind_ovmf_firmware_guest, box_guest_firmware, dispatch_guest_fw_rest,
    guest_fw_bytes, load_guest_firmware, load_ovmf_from_esp, ovmf_real_esp_is_qualified,
    prepare_ovmf_firmware_launch, probe_ovmf_firmware, qualify_real_esp_ovmf, reset_guest_fw,
    stage_ovmf_firmware_floor, write_mock_ovmf_fv, write_size_floor_ovmf_fv, GuestFwError,
    MOCK_OVMF_FV_BYTES, SIZE_FLOOR_FV_BYTES,
};
use super::iso::attach_cdrom_uefi;
use super::iso::IsoError;
use super::m7_e5_cdrom_attach_gate::e4_shell_launch_no_cdrom;
use super::m7_e5_ept_install_gate::run_m7_e5_ept_install_gate;
use super::VmTable;

#[cfg(test)]
use super::guest_fw::{
    arm_ovmf_esp_launch, arm_ovmf_firmware_alias, arm_ovmf_reset_vector, install_ovmf_alias_ept,
    map_live_esp_ovmf, program_ovmf_alias_ept, stage_edk2_ovmf_firmware,
    try_vmlaunch_ovmf_firmware, write_edk2_sized_fv, write_firmware_alias_fv,
    write_live_esp_ovmf_fv, write_reset_vector_stub, MIN_EDK2_OVMF_BYTES, MIN_FIRMWARE_ALIAS_BYTES,
    MIN_LIVE_ESP_OVMF_BYTES,
};

/// Host / CI marker when the E5 Stage 18 real-ESP qualify gate passes.
pub const M7_E5_REAL_ESP_OK_MARKER: &str = "RAYNU-V-M7-E5-REAL-ESP-OK";

/// Honest residual: real-ESP contract recorded; VMLAUNCH insn not issued.
pub const E5_REAL_ESP_RESIDUAL_NOTE: &str =
    "residual: real-ESP VMLAUNCH-ready contract is recorded; 4 MiB fixture is not a shipped OVMF.fd; live EPT is not written; VMLAUNCH insn not issued; attach_cdrom_uefi stays UnsupportedOnFirmware; no guest UEFI VMLAUNCH";

fn stage_through_ept_install() -> bool {
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
        if arm_ovmf_esp_launch().is_err() {
            return false;
        }
        let mut live = vec![0u8; MIN_LIVE_ESP_OVMF_BYTES];
        if write_live_esp_ovmf_fv(&mut live).is_err() || write_reset_vector_stub(&mut live).is_err()
        {
            return false;
        }
        if map_live_esp_ovmf(&live).is_err() || arm_ovmf_reset_vector(&live).is_err() {
            return false;
        }
        let mut alias = vec![0u8; MIN_FIRMWARE_ALIAS_BYTES];
        if write_firmware_alias_fv(&mut alias).is_err() {
            return false;
        }
        return arm_ovmf_firmware_alias(&alias).is_ok()
            && program_ovmf_alias_ept(&alias).is_ok()
            && install_ovmf_alias_ept(&alias).is_ok();
    }
    #[cfg(not(test))]
    {
        false
    }
}

/// Private install then real-ESP qualify. VMLAUNCH stays RealEspNotLaunched.
pub fn prop_real_esp_after_install() -> bool {
    #[cfg(not(test))]
    {
        return false;
    }
    #[cfg(test)]
    {
        if !stage_through_ept_install() {
            return false;
        }
        if try_vmlaunch_ovmf_firmware() != Err(GuestFwError::AliasEptInstalledNotLaunched) {
            return false;
        }
        let mut live = vec![0u8; MIN_LIVE_ESP_OVMF_BYTES];
        if write_live_esp_ovmf_fv(&mut live).is_err() || write_reset_vector_stub(&mut live).is_err()
        {
            return false;
        }
        if qualify_real_esp_ovmf(&live) != Err(GuestFwError::TooSmall) {
            return false;
        }
        let mut alias = vec![0u8; MIN_FIRMWARE_ALIAS_BYTES];
        if write_firmware_alias_fv(&mut alias).is_err() {
            return false;
        }
        match qualify_real_esp_ovmf(&alias) {
            Ok(a) => {
                a.bytes_len == MIN_FIRMWARE_ALIAS_BYTES as u64
                    && a.gpa == 0xFFC0_0000
                    && ovmf_real_esp_is_qualified()
                    && try_vmlaunch_ovmf_firmware() == Err(GuestFwError::RealEspNotLaunched)
            }
            Err(_) => false,
        }
    }
}

/// Real-ESP qualify without install, reject. attach_cdrom_uefi stays closed.
pub fn prop_real_esp_rejects() -> bool {
    reset_guest_fw();
    if qualify_real_esp_ovmf(&[]) != Err(GuestFwError::LaunchNotWired) {
        return false;
    }
    attach_cdrom_uefi(1) == Err(IsoError::UnsupportedOnFirmware)
}

/// Bearer REST: install then qualify. Without install → 409.
/// `POST /fw/vmlaunch` → 409 (RealEspNotLaunched). `iso=0` SHELL stays 201.
pub fn prop_rest_real_esp() -> bool {
    reset_guest_fw();
    let tok = Some(BRINGUP_AUTH_TOKEN);

    let missing = dispatch_guest_fw_rest(RestRequest {
        method: RestMethod::Post,
        path: "/fw/real-esp",
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
        "/fw/esp-map",
        "/fw/reset-vec",
        "/fw/alias",
        "/fw/alias-ept",
        "/fw/ept-install",
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

    let qualified = dispatch_guest_fw_rest(RestRequest {
        method: RestMethod::Post,
        path: "/fw/real-esp",
        auth_token: tok,
    });
    if qualified.status != 201 || !ovmf_real_esp_is_qualified() {
        return false;
    }

    let st = dispatch_guest_fw_rest(RestRequest {
        method: RestMethod::Get,
        path: "/fw/real-esp",
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
        path: "/fw/real-esp",
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

/// SPA + ADR-014 Stage 18 phrases. Real-ESP contract recorded; VMLAUNCH insn not issued.
pub fn real_esp_surface_present() -> bool {
    let spa = include_str!("../assets/webui.html");
    let adr = include_str!("../docs/adr/ADR-014.md");
    let src = include_str!("guest_fw.rs");
    let launch = include_str!("../vmx/launch.rs");
    spa.contains("Qualify real ESP")
        && spa.contains("Install FW EPT")
        && spa.contains("Program alias EPT")
        && spa.contains("Arm FW alias")
        && spa.contains("Arm reset vec")
        && spa.contains("Map live ESP")
        && spa.contains("Arm ESP launch")
        && spa.contains("Stage EDK2")
        && spa.contains("Stage FW floor")
        && spa.contains("not OVMF")
        && spa.contains("UEFI-first")
        && spa.contains("extract-boot is lab")
        && spa.contains("not guest UEFI")
        && spa.contains("Host attach")
        && spa.contains("Firmware arm")
        && spa.contains("Arm firmware")
        && crate::mgmt::webui::webui_len().saturating_add(256) <= 16384
        && adr.contains("Stage 18")
        && adr.contains("qualify_real_esp_ovmf")
        && adr.contains("RealEspNotLaunched")
        && src.contains("fn qualify_real_esp_ovmf")
        && src.contains("ovmf_real_esp_is_qualified")
        && launch.contains("fn try_vmlaunch_guest_uefi_ovmf")
        && launch.contains("fn qualify_guest_uefi_real_esp")
        && launch.contains("GUEST_UEFI_OVMF_ESP_PATH")
        && launch.contains("RealEspNotLaunched")
        && launch.contains("AliasEptInstalledNotLaunched")
        && !launch.contains("qualify_real_esp_ovmf")
        && !launch.contains("install_ovmf_alias_ept")
        && !launch.contains("program_ovmf_alias_ept")
        && !launch.contains("arm_ovmf_firmware_alias")
        && !launch.contains("arm_ovmf_reset_vector")
        && !launch.contains("map_live_esp_ovmf")
        && !launch.contains("stage_edk2_ovmf_firmware")
        && !launch.contains("OvmfEspLaunchArmed")
        && !launch.contains("arm_ovmf_esp_launch")
        && !launch.contains("try_vmlaunch_ovmf_firmware")
        && !launch.contains("OvmfFirmwareEdk2Staged")
        && !launch.contains("write_edk2_sized_fv")
        && e4_shell_launch_no_cdrom()
}

/// Full E5 Stage 18 package. Host gate only — not iron, not Everest E5.
pub fn run_m7_e5_real_esp_gate() -> bool {
    let _ = (M7_E5_REAL_ESP_OK_MARKER, E5_REAL_ESP_RESIDUAL_NOTE);
    reset_guest_fw();
    let ok = prop_real_esp_after_install()
        && prop_real_esp_rejects()
        && prop_rest_real_esp()
        && real_esp_surface_present()
        && run_m7_e5_ept_install_gate()
        && E5_REAL_ESP_RESIDUAL_NOTE.contains("UnsupportedOnFirmware")
        && E5_REAL_ESP_RESIDUAL_NOTE.contains("no guest UEFI VMLAUNCH")
        && E5_REAL_ESP_RESIDUAL_NOTE.contains("not a shipped OVMF.fd")
        && E5_REAL_ESP_RESIDUAL_NOTE.contains("VMLAUNCH insn not issued")
        && E5_REAL_ESP_RESIDUAL_NOTE.contains("live EPT is not written")
        && M7_E5_REAL_ESP_OK_MARKER == "RAYNU-V-M7-E5-REAL-ESP-OK";
    reset_guest_fw();
    ok
}

#[cfg(test)]
#[path = "m7_e5_real_esp_gate_test.rs"]
mod m7_e5_real_esp_gate_test;
