//! E5 Stage 0 — ADR-014 boot spec on the wire (outside Proven Core).
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** (ADR-002 / ADR-014)
//! VERIFICATION: N/A
//!
//! Host/CI: REST/SPA carry `linux_iso` | `windows_iso` | `generic_uefi`;
//! El Torito catalog parse works on a mock ISO; `attach_cdrom_uefi` stays
//! `UnsupportedOnFirmware`; E4 SHELL (`iso=0`) stays valid; launch.rs does
//! not VMLAUNCH guest UEFI firmware. Do not claim Everest E5 / ISO-INSTALL-OK.

use super::api::{dispatch_rest, RestMethod, RestRequest, BRINGUP_AUTH_TOKEN};
use super::el_torito::{parse_el_torito, write_mock_efi_iso, MOCK_EFI_ISO_BYTES};
use super::guest_image::{adr014_present, GuestFirmware, GuestImageType};
use super::iso::{attach_cdrom_uefi, IsoError};
use super::{VmLifecycle, VmTable};

/// Host / CI marker when the E5 Stage 0 boot-spec gate passes.
pub const M7_E5_BOOT_SPEC_OK_MARKER: &str = "RAYNU-V-M7-E5-BOOT-SPEC-OK";

/// Honest residual: parse ≠ attach ≠ VMLAUNCH.
pub const E5_BOOT_SPEC_RESIDUAL_NOTE: &str =
    "residual: El Torito catalog parse is host-only; attach_cdrom_uefi stays UnsupportedOnFirmware; no guest UEFI VMLAUNCH";

/// Product ISO types on REST → 201 and a UEFI boot spec. Lab bzImage → 409.
pub fn prop_rest_boot_spec() -> bool {
    let tok = Some(BRINGUP_AUTH_TOKEN);

    let mut t = VmTable::new();
    let linux = dispatch_rest(
        &mut t,
        RestRequest {
            method: RestMethod::Post,
            path: "/vms/5/spec/2/2048/10240/3/linux_iso",
            auth_token: tok,
        },
    );
    if linux.status != 201 {
        return false;
    }
    let rec = match t.get(5) {
        Some(r) => *r,
        None => return false,
    };
    if rec.state != VmLifecycle::Defined
        || rec.iso_id != 3
        || rec.image_type != Some(GuestImageType::LinuxIso)
    {
        return false;
    }
    let spec = match rec.boot_spec() {
        Some(s) => s,
        None => return false,
    };
    if !spec.is_product_path() || spec.firmware != GuestFirmware::Uefi {
        return false;
    }

    let mut t = VmTable::new();
    let win = dispatch_rest(
        &mut t,
        RestRequest {
            method: RestMethod::Post,
            path: "/vms/6/spec/2/2048/10240/4/windows_iso",
            auth_token: tok,
        },
    );
    if win.status != 201 {
        return false;
    }
    if t.get(6).and_then(|r| r.image_type) != Some(GuestImageType::WindowsIso) {
        return false;
    }

    let mut t = VmTable::new();
    let gen = dispatch_rest(
        &mut t,
        RestRequest {
            method: RestMethod::Post,
            path: "/vms/8/spec/1/512/1024/9/generic_uefi",
            auth_token: tok,
        },
    );
    if gen.status != 201 {
        return false;
    }
    if t.get(8).and_then(|r| r.image_type) != Some(GuestImageType::GenericUefi) {
        return false;
    }

    let mut t = VmTable::new();
    let bz = dispatch_rest(
        &mut t,
        RestRequest {
            method: RestMethod::Post,
            path: "/vms/9/spec/1/512/1024/1/linux_bzimage",
            auth_token: tok,
        },
    );
    if bz.status != 409 || t.get(9).is_some() {
        return false;
    }

    true
}

/// `iso=0` stays E4 SHELL (`image_type = None`). Type without ISO → 409.
pub fn prop_e4_shell_iso_zero() -> bool {
    let tok = Some(BRINGUP_AUTH_TOKEN);
    let mut t = VmTable::new();
    let created = dispatch_rest(
        &mut t,
        RestRequest {
            method: RestMethod::Post,
            path: "/vms/1/spec/1/512/1024/0",
            auth_token: tok,
        },
    );
    if created.status != 201 {
        return false;
    }
    match t.get(1) {
        Some(r) => {
            if r.iso_id != 0 || r.image_type.is_some() || r.boot_spec().is_some() {
                return false;
            }
        }
        None => return false,
    }

    let mut t = VmTable::new();
    let typed_zero = dispatch_rest(
        &mut t,
        RestRequest {
            method: RestMethod::Post,
            path: "/vms/2/spec/1/512/1024/0/linux_iso",
            auth_token: tok,
        },
    );
    typed_zero.status == 409 && t.get(2).is_none()
}

/// Omitted type with `iso != 0` defaults to `linux_iso` (not bzImage).
pub fn prop_product_iso_default() -> bool {
    let tok = Some(BRINGUP_AUTH_TOKEN);
    let mut t = VmTable::new();
    let created = dispatch_rest(
        &mut t,
        RestRequest {
            method: RestMethod::Post,
            path: "/vms/4/spec/2/2048/10240/1",
            auth_token: tok,
        },
    );
    created.status == 201
        && t.get(4).map(|r| r.image_type) == Some(Some(GuestImageType::LinuxIso))
}

/// Host-only catalog parse. Attach stays the honest stub.
pub fn prop_el_torito_parse_not_attach() -> bool {
    let mut iso = [0u8; MOCK_EFI_ISO_BYTES];
    if write_mock_efi_iso(&mut iso).is_err() {
        return false;
    }
    let img = match parse_el_torito(&iso) {
        Ok(i) => i,
        Err(_) => return false,
    };
    img.efi
        && img.catalog_lba == 20
        && img.load_lba == 22
        && attach_cdrom_uefi(1) == Err(IsoError::UnsupportedOnFirmware)
}

/// SPA + ADR-014 phrases. Product path is UEFI-first, not extract-and-jump.
pub fn boot_spec_surface_present() -> bool {
    let spa = include_str!("../assets/webui.html");
    let adr = include_str!("../docs/adr/ADR-014.md");
    let api = include_str!("api.rs");
    spa.contains("linux_iso")
        && spa.contains("windows_iso")
        && spa.contains("generic_uefi")
        && spa.contains("f-image")
        && spa.contains("UEFI-first")
        && spa.contains("extract-boot is lab")
        && api.contains("linux_iso|windows_iso|generic_uefi")
        && adr.contains("Stage 0")
        && adr.contains("boot spec on the wire")
        && adr014_present()
}

/// E4 SHELL VMLAUNCH path is unchanged. No guest UEFI firmware launch.
pub fn e4_shell_launch_unchanged() -> bool {
    let launch = include_str!("../vmx/launch.rs");
    launch.contains("fn try_spa_vmlaunch(")
        && launch.contains("M7_E4_SPA_LAUNCH_OK_MARKER")
        && !launch.contains("attach_cdrom_uefi")
        && !launch.contains("parse_el_torito")
        && !launch.contains("linux_iso")
        && !launch.contains("GuestImageType")
}

/// Full E5 Stage 0 package. Host gate only — not iron, not Everest E5.
pub fn run_m7_e5_boot_spec_gate() -> bool {
    let _ = (M7_E5_BOOT_SPEC_OK_MARKER, E5_BOOT_SPEC_RESIDUAL_NOTE);
    prop_rest_boot_spec()
        && prop_e4_shell_iso_zero()
        && prop_product_iso_default()
        && prop_el_torito_parse_not_attach()
        && boot_spec_surface_present()
        && e4_shell_launch_unchanged()
        && E5_BOOT_SPEC_RESIDUAL_NOTE.contains("UnsupportedOnFirmware")
        && M7_E5_BOOT_SPEC_OK_MARKER == "RAYNU-V-M7-E5-BOOT-SPEC-OK"
}

#[cfg(test)]
#[path = "m7_e5_boot_spec_gate_test.rs"]
mod m7_e5_boot_spec_gate_test;
