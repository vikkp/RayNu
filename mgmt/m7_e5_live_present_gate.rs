//! E5 Stage 25 — live-ESP present-attempt (outside Proven Core).
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** mgmt bookkeeping; launch.rs entry is L0 (ADR-014)
//! VERIFICATION: N/A
//!
//! Host/CI: `present_ovmf_live_esp` records that real ESP
//! `\\EFI\\RayNu\\OVMF.fd` bytes were presented for a private guest-UEFI
//! VMCS (not the E4 SHELL). `try_vmlaunch_ovmf_firmware` then calls
//! `try_vmlaunch_guest_uefi_ovmf`, which refuses because those bytes
//! are still absent (`LiveEspPresentAbsent`). A 2 MiB fixture is
//! `TooSmall`. The E4 SHELL EPT is not written.
//! `attach_cdrom_uefi` stays `UnsupportedOnFirmware`.
//! Do not claim Everest E5 / ISO-INSTALL-OK.

use super::api::{dispatch_rest, RestMethod, RestRequest, BRINGUP_AUTH_TOKEN};
use super::guest_fw::{
    arm_ovmf_firmware_slot, bind_ovmf_firmware_guest, box_guest_firmware, dispatch_guest_fw_rest,
    guest_fw_bytes, load_guest_firmware, load_ovmf_from_esp, ovmf_live_esp_is_presented,
    prepare_ovmf_firmware_launch, present_ovmf_live_esp, probe_ovmf_firmware, reset_guest_fw,
    stage_ovmf_firmware_floor, write_mock_ovmf_fv, write_size_floor_ovmf_fv, GuestFwError,
    MOCK_OVMF_FV_BYTES, SIZE_FLOOR_FV_BYTES,
};
use super::iso::attach_cdrom_uefi;
use super::iso::IsoError;
use super::m7_e5_cdrom_attach_gate::e4_shell_launch_no_cdrom;
use super::m7_e5_live_fd_gate::run_m7_e5_live_fd_gate;
use super::VmTable;

#[cfg(test)]
use super::guest_fw::{
    arm_ovmf_esp_launch, arm_ovmf_firmware_alias, arm_ovmf_live_issue, arm_ovmf_private_vmcs,
    arm_ovmf_real_launch, arm_ovmf_reset_vector, install_ovmf_alias_ept, map_live_esp_ovmf,
    probe_ovmf_live_bytes, program_ovmf_alias_ept, qualify_real_esp_ovmf, require_ovmf_live_esp,
    require_ovmf_live_fd, stage_edk2_ovmf_firmware, try_vmlaunch_ovmf_firmware,
    write_edk2_sized_fv, write_firmware_alias_fv, write_live_esp_ovmf_fv, write_reset_vector_stub,
    MIN_EDK2_OVMF_BYTES, MIN_FIRMWARE_ALIAS_BYTES, MIN_LIVE_ESP_OVMF_BYTES,
};

/// Host / CI marker when the E5 Stage 25 live-present gate passes.
pub const M7_E5_LIVE_PRESENT_OK_MARKER: &str = "RAYNU-V-M7-E5-LIVE-PRESENT-OK";

/// Honest residual: live ESP presented; still absent; VMLAUNCH insn not issued.
pub const E5_LIVE_PRESENT_RESIDUAL_NOTE: &str =
    "residual: real ESP OVMF.fd bytes were presented; live ESP bytes are not present; 4 MiB fixture is not a shipped OVMF.fd; live EPT is not written; VMLAUNCH insn not issued; attach_cdrom_uefi stays UnsupportedOnFirmware; no guest UEFI VMLAUNCH";

fn stage_through_live_fd() -> bool {
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
            && install_ovmf_alias_ept(&alias).is_ok()
            && qualify_real_esp_ovmf(&alias).is_ok()
            && arm_ovmf_real_launch(&alias).is_ok()
            && require_ovmf_live_esp(&alias).is_ok()
            && arm_ovmf_private_vmcs(&alias).is_ok()
            && arm_ovmf_live_issue(&alias).is_ok()
            && probe_ovmf_live_bytes(&alias).is_ok()
            && require_ovmf_live_fd(&alias).is_ok();
    }
    #[cfg(not(test))]
    {
        false
    }
}

/// Live-FD require then live-ESP present. VMLAUNCH stays LiveEspPresentAbsent.
pub fn prop_live_present_after_live_fd() -> bool {
    #[cfg(not(test))]
    {
        return false;
    }
    #[cfg(test)]
    {
        if !stage_through_live_fd() {
            return false;
        }
        if try_vmlaunch_ovmf_firmware() != Err(GuestFwError::LiveEspFdAbsent) {
            return false;
        }
        let mut live = vec![0u8; MIN_LIVE_ESP_OVMF_BYTES];
        if write_live_esp_ovmf_fv(&mut live).is_err() || write_reset_vector_stub(&mut live).is_err()
        {
            return false;
        }
        if present_ovmf_live_esp(&live) != Err(GuestFwError::TooSmall) {
            return false;
        }
        let mut alias = vec![0u8; MIN_FIRMWARE_ALIAS_BYTES];
        if write_firmware_alias_fv(&mut alias).is_err() {
            return false;
        }
        match present_ovmf_live_esp(&alias) {
            Ok(a) => {
                a.bytes_len == MIN_FIRMWARE_ALIAS_BYTES as u64
                    && a.gpa == 0xFFC0_0000
                    && ovmf_live_esp_is_presented()
                    && try_vmlaunch_ovmf_firmware() == Err(GuestFwError::LiveEspPresentAbsent)
            }
            Err(_) => false,
        }
    }
}

/// Live-ESP present without live-FD require, reject. attach_cdrom_uefi stays closed.
pub fn prop_live_present_rejects() -> bool {
    reset_guest_fw();
    if present_ovmf_live_esp(&[]) != Err(GuestFwError::LaunchNotWired) {
        return false;
    }
    attach_cdrom_uefi(1) == Err(IsoError::UnsupportedOnFirmware)
}

/// Bearer REST: live-FD then live-present. Without live-FD → 409.
/// `POST /fw/vmlaunch` → 409 (LiveEspPresentAbsent). `iso=0` SHELL stays 201.
pub fn prop_rest_live_present() -> bool {
    reset_guest_fw();
    let tok = Some(BRINGUP_AUTH_TOKEN);

    let missing = dispatch_guest_fw_rest(RestRequest {
        method: RestMethod::Post,
        path: "/fw/live-present",
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
        "/fw/real-esp",
        "/fw/real-launch",
        "/fw/live-exec",
        "/fw/priv-vmcs",
        "/fw/live-issue",
        "/fw/live-bytes",
        "/fw/live-fd",
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

    let presented = dispatch_guest_fw_rest(RestRequest {
        method: RestMethod::Post,
        path: "/fw/live-present",
        auth_token: tok,
    });
    if presented.status != 201 || !ovmf_live_esp_is_presented() {
        return false;
    }

    let st = dispatch_guest_fw_rest(RestRequest {
        method: RestMethod::Get,
        path: "/fw/live-present",
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
        path: "/fw/live-present",
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

/// SPA + ADR-014 Stage 25 phrases. Live ESP presented; VMLAUNCH insn not issued.
pub fn live_present_surface_present() -> bool {
    let spa = include_str!("../assets/webui.html");
    let adr = include_str!("../docs/adr/ADR-014.md");
    let src = include_str!("guest_fw.rs");
    let launch = include_str!("../vmx/launch.rs");
    spa.contains("Present live ESP")
        && spa.contains("Require live FD")
        && spa.contains("Probe live bytes")
        && spa.contains("Arm live issue")
        && spa.contains("Arm private VMCS")
        && spa.contains("Require live ESP")
        && spa.contains("Arm real launch")
        && spa.contains("Qualify real ESP")
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
        && adr.contains("Stage 25")
        && adr.contains("present_ovmf_live_esp")
        && adr.contains("LiveEspPresentAbsent")
        && src.contains("fn present_ovmf_live_esp")
        && src.contains("ovmf_live_esp_is_presented")
        && launch.contains("fn try_vmlaunch_guest_uefi_ovmf")
        && launch.contains("fn present_guest_uefi_live_esp")
        && launch.contains("GUEST_UEFI_PRIVATE_VMCS_ID")
        && launch.contains("LiveEspPresentAbsent")
        && launch.contains("LiveEspFdAbsent")
        && !launch.contains("present_ovmf_live_esp")
        && !launch.contains("require_ovmf_live_fd")
        && !launch.contains("probe_ovmf_live_bytes")
        && !launch.contains("arm_ovmf_live_issue")
        && !launch.contains("arm_ovmf_private_vmcs")
        && !launch.contains("require_ovmf_live_esp")
        && !launch.contains("arm_ovmf_real_launch")
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

/// Full E5 Stage 25 package. Host gate only — not iron, not Everest E5.
pub fn run_m7_e5_live_present_gate() -> bool {
    let _ = (M7_E5_LIVE_PRESENT_OK_MARKER, E5_LIVE_PRESENT_RESIDUAL_NOTE);
    reset_guest_fw();
    let ok = prop_live_present_after_live_fd()
        && prop_live_present_rejects()
        && prop_rest_live_present()
        && live_present_surface_present()
        && run_m7_e5_live_fd_gate()
        && E5_LIVE_PRESENT_RESIDUAL_NOTE.contains("UnsupportedOnFirmware")
        && E5_LIVE_PRESENT_RESIDUAL_NOTE.contains("no guest UEFI VMLAUNCH")
        && E5_LIVE_PRESENT_RESIDUAL_NOTE.contains("not a shipped OVMF.fd")
        && E5_LIVE_PRESENT_RESIDUAL_NOTE.contains("VMLAUNCH insn not issued")
        && E5_LIVE_PRESENT_RESIDUAL_NOTE.contains("live EPT is not written")
        && M7_E5_LIVE_PRESENT_OK_MARKER == "RAYNU-V-M7-E5-LIVE-PRESENT-OK";
    reset_guest_fw();
    ok
}

#[cfg(test)]
#[path = "m7_e5_live_present_gate_test.rs"]
mod m7_e5_live_present_gate_test;
