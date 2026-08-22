//! E5 Stage 1 — host El Torito CD-ROM attach (outside Proven Core).
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** (ADR-002 / ADR-014)
//! VERIFICATION: N/A
//!
//! Host/CI: `attach_cdrom_host` parses a mock EFI catalog and records
//! `AttachedHost`. Lab bzImage and non-EFI catalogs are rejected.
//! `attach_cdrom_uefi` stays `UnsupportedOnFirmware`. E4 SHELL VMLAUNCH
//! is unchanged. Do not claim Everest E5 / ISO-INSTALL-OK / guest UEFI.

use super::api::{dispatch_rest, RestMethod, RestRequest, BRINGUP_AUTH_TOKEN};
use super::datastore::ImageTable;
use super::el_torito::{parse_el_torito, write_mock_efi_iso, MOCK_EFI_ISO_BYTES};
use super::guest_image::GuestImageType;
use super::iso::{
    attach_cdrom_host, attach_cdrom_uefi, dispatch_iso_attach_rest, reset_host_cdrom, CdromAttachState,
    CdromTable, IsoError,
};
use super::m7_e5_boot_spec_gate::{
    boot_spec_surface_present, e4_shell_launch_unchanged, run_m7_e5_boot_spec_gate,
};
use super::VmTable;

/// Host / CI marker when the E5 Stage 1 CD-ROM attach gate passes.
pub const M7_E5_CDROM_ATTACH_OK_MARKER: &str = "RAYNU-V-M7-E5-CDROM-ATTACH-OK";

/// Honest residual: host attach ≠ firmware CD ≠ VMLAUNCH.
pub const E5_CDROM_ATTACH_RESIDUAL_NOTE: &str =
    "residual: host attach_cdrom_host is El Torito parse + record; attach_cdrom_uefi stays UnsupportedOnFirmware; no guest UEFI VMLAUNCH";

/// Mock EFI catalog → AttachedHost. Product type required.
pub fn prop_host_attach_mock_efi() -> bool {
    let mut iso = [0u8; MOCK_EFI_ISO_BYTES];
    if write_mock_efi_iso(&mut iso).is_err() {
        return false;
    }
    let rec = match attach_cdrom_host(&iso, 3, GuestImageType::LinuxIso) {
        Ok(r) => r,
        Err(_) => return false,
    };
    rec.iso_id == 3
        && rec.efi
        && rec.catalog_lba == 20
        && rec.load_lba == 22
        && rec.sector_count == 4
        && rec.image_type == GuestImageType::LinuxIso
        && rec.state == CdromAttachState::AttachedHost
}

/// Non-EFI catalog and lab bzImage are rejected. Firmware stub unchanged.
pub fn prop_host_attach_rejects() -> bool {
    let mut iso = [0u8; MOCK_EFI_ISO_BYTES];
    if write_mock_efi_iso(&mut iso).is_err() {
        return false;
    }
    // Clear EFI platform on validation + section header.
    iso[20 * super::el_torito::ISO_SECTOR + 1] = 0x00;
    iso[20 * super::el_torito::ISO_SECTOR + 33] = 0x00;
    let parsed = match parse_el_torito(&iso) {
        Ok(i) => i,
        Err(_) => return false,
    };
    if parsed.efi {
        return false;
    }
    if attach_cdrom_host(&iso, 3, GuestImageType::LinuxIso) != Err(IsoError::NotEfi) {
        return false;
    }
    if write_mock_efi_iso(&mut iso).is_err() {
        return false;
    }
    if attach_cdrom_host(&iso, 3, GuestImageType::LinuxBzImage) != Err(IsoError::BadState) {
        return false;
    }
    if attach_cdrom_host(&iso, 0, GuestImageType::LinuxIso) != Err(IsoError::InvalidId) {
        return false;
    }
    attach_cdrom_uefi(1) == Err(IsoError::UnsupportedOnFirmware)
}

/// Bearer REST attach + status. `linux_bzimage` → 409. `iso=0` SHELL spec stays 201.
pub fn prop_rest_cdrom_attach() -> bool {
    reset_host_cdrom();
    let tok = Some(BRINGUP_AUTH_TOKEN);
    let mut store = ImageTable::new();
    let mut cdrom = CdromTable::empty();

    let armed = dispatch_iso_attach_rest(
        &mut store,
        &mut cdrom,
        RestRequest {
            method: RestMethod::Post,
            path: "/iso/7/attach/linux_iso",
            auth_token: tok,
        },
    );
    if armed.status != 201 {
        return false;
    }
    let rec = match cdrom.get(7) {
        Some(r) => r,
        None => return false,
    };
    if rec.state != CdromAttachState::AttachedHost || !rec.efi {
        return false;
    }

    let win = dispatch_iso_attach_rest(
        &mut store,
        &mut cdrom,
        RestRequest {
            method: RestMethod::Post,
            path: "/iso/8/attach/windows_iso",
            auth_token: tok,
        },
    );
    if win.status != 201 || cdrom.get(8).map(|r| r.image_type) != Some(GuestImageType::WindowsIso) {
        return false;
    }

    let st = dispatch_iso_attach_rest(
        &mut store,
        &mut cdrom,
        RestRequest {
            method: RestMethod::Get,
            path: "/iso/attach",
            auth_token: tok,
        },
    );
    if st.status != 200 {
        return false;
    }

    let bz = dispatch_iso_attach_rest(
        &mut store,
        &mut cdrom,
        RestRequest {
            method: RestMethod::Post,
            path: "/iso/9/attach/linux_bzimage",
            auth_token: tok,
        },
    );
    if bz.status != 409 || cdrom.get(9).is_some() {
        return false;
    }

    let denied = dispatch_iso_attach_rest(
        &mut store,
        &mut cdrom,
        RestRequest {
            method: RestMethod::Post,
            path: "/iso/10/attach",
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
    shell.status == 201 && t.get(1).map(|r| r.iso_id) == Some(0) && t.get(1).unwrap().image_type.is_none()
}

/// SPA + ADR-014 Stage 1 phrases. Product path is still UEFI-first.
pub fn cdrom_attach_surface_present() -> bool {
    let spa = include_str!("../assets/webui.html");
    let adr = include_str!("../docs/adr/ADR-014.md");
    let http = include_str!("http.rs");
    spa.contains("Attach CD")
        && spa.contains("Host attach")
        && spa.contains("not guest UEFI")
        && spa.contains("UEFI-first")
        && spa.contains("extract-boot is lab")
        && spa.contains("/iso/")
        && crate::mgmt::webui::webui_len().saturating_add(256) <= 16384
        && http.contains("is_iso_attach_path")
        && adr.contains("Stage 1")
        && adr.contains("host CD-ROM attach")
        && adr.contains("attach_cdrom_host")
}

/// E4 SHELL VMLAUNCH path is unchanged. No host/firmware attach in launch.rs.
pub fn e4_shell_launch_no_cdrom() -> bool {
    let launch = include_str!("../vmx/launch.rs");
    e4_shell_launch_unchanged()
        && !launch.contains("attach_cdrom_host")
        && !launch.contains("CdromAttach")
}

/// Full E5 Stage 1 package. Host gate only — not iron, not Everest E5.
pub fn run_m7_e5_cdrom_attach_gate() -> bool {
    let _ = (M7_E5_CDROM_ATTACH_OK_MARKER, E5_CDROM_ATTACH_RESIDUAL_NOTE);
    reset_host_cdrom();
    let ok = prop_host_attach_mock_efi()
        && prop_host_attach_rejects()
        && prop_rest_cdrom_attach()
        && cdrom_attach_surface_present()
        && e4_shell_launch_no_cdrom()
        && run_m7_e5_boot_spec_gate()
        && boot_spec_surface_present()
        && E5_CDROM_ATTACH_RESIDUAL_NOTE.contains("UnsupportedOnFirmware")
        && M7_E5_CDROM_ATTACH_OK_MARKER == "RAYNU-V-M7-E5-CDROM-ATTACH-OK";
    reset_host_cdrom();
    ok
}

#[cfg(test)]
#[path = "m7_e5_cdrom_attach_gate_test.rs"]
mod m7_e5_cdrom_attach_gate_test;
