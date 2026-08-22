//! E5 Stage 2 — firmware-facing CD attach (outside Proven Core).
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** (ADR-002 / ADR-014)
//! VERIFICATION: N/A
//!
//! Host/CI: `attach_cdrom_firmware` arms `FirmwareArmed` from a host attach
//! after validating the El Torito boot-image sector range.
//! `attach_cdrom_uefi` stays `UnsupportedOnFirmware`. No OVMF blob.
//! E4 SHELL VMLAUNCH is unchanged. Do not claim Everest E5 / ISO-INSTALL-OK.

use super::api::{dispatch_rest, RestMethod, RestRequest, BRINGUP_AUTH_TOKEN};
use super::datastore::ImageTable;
use super::el_torito::{write_mock_efi_iso, ISO_SECTOR, MOCK_EFI_ISO_BYTES};
use super::guest_image::GuestImageType;
use super::iso::{
    attach_cdrom_firmware, attach_cdrom_host, attach_cdrom_uefi, cdrom_read_sector,
    dispatch_iso_attach_rest, dispatch_iso_firmware_rest, firmware_boot_image, reset_host_cdrom,
    CdromAttachState, CdromTable, IsoError,
};
use super::m7_e5_boot_spec_gate::run_m7_e5_boot_spec_gate;
use super::m7_e5_cdrom_attach_gate::{e4_shell_launch_no_cdrom, run_m7_e5_cdrom_attach_gate};
use super::VmTable;

/// Host / CI marker when the E5 Stage 2 firmware-CD gate passes.
pub const M7_E5_CDROM_FIRMWARE_OK_MARKER: &str = "RAYNU-V-M7-E5-CDROM-FIRMWARE-OK";

/// Honest residual: FirmwareArmed ≠ OVMF ≠ VMLAUNCH.
pub const E5_CDROM_FIRMWARE_RESIDUAL_NOTE: &str =
    "residual: FirmwareArmed is firmware-facing sector validate; attach_cdrom_uefi stays UnsupportedOnFirmware; no OVMF; no guest UEFI VMLAUNCH";

/// Host attach → firmware arm + boot-image sector range.
pub fn prop_firmware_arm_from_host() -> bool {
    let mut iso = [0u8; MOCK_EFI_ISO_BYTES];
    if write_mock_efi_iso(&mut iso).is_err() {
        return false;
    }
    let host = match attach_cdrom_host(&iso, 3, GuestImageType::LinuxIso) {
        Ok(r) => r,
        Err(_) => return false,
    };
    let armed = match attach_cdrom_firmware(&iso, host) {
        Ok(r) => r,
        Err(_) => return false,
    };
    if armed.state != CdromAttachState::FirmwareArmed || !armed.efi {
        return false;
    }
    let boot = match firmware_boot_image(&iso, &armed) {
        Ok(b) => b,
        Err(_) => return false,
    };
    if boot.load_lba != 22 || boot.sector_count != 4 || !boot.efi {
        return false;
    }
    cdrom_read_sector(&iso, boot.load_lba).is_ok()
        && cdrom_read_sector(&iso, boot.load_lba + u32::from(boot.sector_count) - 1).is_ok()
}

/// Firmware arm without host attach, truncated boot image, and the UEFI stub.
pub fn prop_firmware_arm_rejects() -> bool {
    let mut iso = [0u8; MOCK_EFI_ISO_BYTES];
    if write_mock_efi_iso(&mut iso).is_err() {
        return false;
    }
    let mut host = match attach_cdrom_host(&iso, 4, GuestImageType::GenericUefi) {
        Ok(r) => r,
        Err(_) => return false,
    };
    host.state = CdromAttachState::Parsed;
    if attach_cdrom_firmware(&iso, host) != Err(IsoError::BadState) {
        return false;
    }

    let host = match attach_cdrom_host(&iso, 4, GuestImageType::GenericUefi) {
        Ok(r) => r,
        Err(_) => return false,
    };
    let mut short = [0u8; 23 * ISO_SECTOR];
    short.copy_from_slice(&iso[..23 * ISO_SECTOR]);
    if attach_cdrom_firmware(&short, host) != Err(IsoError::Catalog) {
        return false;
    }

    attach_cdrom_uefi(1) == Err(IsoError::UnsupportedOnFirmware)
}

/// Bearer REST: attach then firmware arm. Arm without attach → 409. `iso=0` SHELL stays 201.
pub fn prop_rest_firmware_arm() -> bool {
    reset_host_cdrom();
    let tok = Some(BRINGUP_AUTH_TOKEN);
    let mut store = ImageTable::new();
    let mut cdrom = CdromTable::empty();

    let missing = dispatch_iso_firmware_rest(
        &mut store,
        &mut cdrom,
        RestRequest {
            method: RestMethod::Post,
            path: "/iso/7/firmware",
            auth_token: tok,
        },
    );
    if missing.status != 409 {
        return false;
    }

    let attached = dispatch_iso_attach_rest(
        &mut store,
        &mut cdrom,
        RestRequest {
            method: RestMethod::Post,
            path: "/iso/7/attach/linux_iso",
            auth_token: tok,
        },
    );
    if attached.status != 201 {
        return false;
    }

    let armed = dispatch_iso_firmware_rest(
        &mut store,
        &mut cdrom,
        RestRequest {
            method: RestMethod::Post,
            path: "/iso/7/firmware",
            auth_token: tok,
        },
    );
    if armed.status != 201 {
        return false;
    }
    if cdrom.get(7).map(|r| r.state) != Some(CdromAttachState::FirmwareArmed) {
        return false;
    }

    let st = dispatch_iso_firmware_rest(
        &mut store,
        &mut cdrom,
        RestRequest {
            method: RestMethod::Get,
            path: "/iso/firmware",
            auth_token: tok,
        },
    );
    if st.status != 200 {
        return false;
    }

    let denied = dispatch_iso_firmware_rest(
        &mut store,
        &mut cdrom,
        RestRequest {
            method: RestMethod::Post,
            path: "/iso/8/firmware",
            auth_token: None,
        },
    );
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

/// SPA + ADR-014 Stage 2 phrases. No OVMF guest blob.
pub fn firmware_cd_surface_present() -> bool {
    let spa = include_str!("../assets/webui.html");
    let adr = include_str!("../docs/adr/ADR-014.md");
    let http = include_str!("http.rs");
    let launch = include_str!("../vmx/launch.rs");
    spa.contains("Arm firmware")
        && spa.contains("Firmware arm")
        && spa.contains("not guest UEFI")
        && spa.contains("UEFI-first")
        && spa.contains("extract-boot is lab")
        && crate::mgmt::webui::webui_len().saturating_add(256) <= 16384
        && http.contains("is_iso_firmware_path")
        && adr.contains("Stage 2")
        && adr.contains("FirmwareArmed")
        && adr.contains("attach_cdrom_firmware")
        && !launch.contains("attach_cdrom_firmware")
        && !launch.contains("FirmwareArmed")
        && !launch.contains("firmware_boot_image")
        && e4_shell_launch_no_cdrom()
}

/// Full E5 Stage 2 package. Host gate only — not iron, not Everest E5.
pub fn run_m7_e5_cdrom_firmware_gate() -> bool {
    let _ = (M7_E5_CDROM_FIRMWARE_OK_MARKER, E5_CDROM_FIRMWARE_RESIDUAL_NOTE);
    reset_host_cdrom();
    let ok = prop_firmware_arm_from_host()
        && prop_firmware_arm_rejects()
        && prop_rest_firmware_arm()
        && firmware_cd_surface_present()
        && run_m7_e5_cdrom_attach_gate()
        && run_m7_e5_boot_spec_gate()
        && E5_CDROM_FIRMWARE_RESIDUAL_NOTE.contains("UnsupportedOnFirmware")
        && E5_CDROM_FIRMWARE_RESIDUAL_NOTE.contains("no OVMF")
        && M7_E5_CDROM_FIRMWARE_OK_MARKER == "RAYNU-V-M7-E5-CDROM-FIRMWARE-OK";
    reset_host_cdrom();
    ok
}

#[cfg(test)]
#[path = "m7_e5_cdrom_firmware_gate_test.rs"]
mod m7_e5_cdrom_firmware_gate_test;
