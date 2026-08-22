//! E5 Stage 40 — guest-UEFI CD visible (`attach_cdrom_uefi`).
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** (ADR-002 / ADR-014)
//! VERIFICATION: N/A
//!
//! FirmwareArmed → GuestVisible. PCI IDE/ATAPI on the private VMCS.
//! Unarmed `attach_cdrom_uefi` stays `UnsupportedOnFirmware`. Not installer.

use super::api::{dispatch_rest, RestMethod, RestRequest, BRINGUP_AUTH_TOKEN};
use super::datastore::ImageTable;
use super::el_torito::{write_mock_efi_iso, MOCK_EFI_ISO_BYTES};
use super::guest_fw::reset_guest_fw;
use super::guest_image::GuestImageType;
use super::iso::{
    attach_cdrom_firmware, attach_cdrom_host, attach_cdrom_uefi, dispatch_iso_attach_rest,
    dispatch_iso_firmware_rest, dispatch_iso_uefi_rest, reset_host_cdrom, CdromAttachState,
    CdromTable, IsoError,
};
use super::m7_e5_cdrom_attach_gate::e4_shell_launch_no_cdrom;
use super::m7_e5_ovmf_past_sec_gate::run_m7_e5_ovmf_past_sec_gate;
use super::VmTable;
use crate::devices::ide_cdrom::{
    cdrom_visible_evidence, host_identify_word0, host_read10, pci_read_data, pci_write_addr,
    GUEST_CD_PCI_DEVICE, GUEST_CD_PCI_VENDOR, M7_E5_OVMF_CDROM_OK_MARKER,
};
use crate::vmx::guest_uefi::E5_OVMF_VMLAUNCH_RESIDUAL_NOTE;

/// Host / CI / QEMU marker when the guest-UEFI VMCS can see CD media.
pub const M7_E5_OVMF_CDROM_GATE_MARKER: &str = M7_E5_OVMF_CDROM_OK_MARKER;

pub fn prop_uefi_attach_after_firmware() -> bool {
    reset_host_cdrom();
    let mut iso = [0u8; MOCK_EFI_ISO_BYTES];
    if write_mock_efi_iso(&mut iso).is_err() {
        return false;
    }
    if attach_cdrom_uefi(3) != Err(IsoError::UnsupportedOnFirmware) {
        return false;
    }
    let host = match attach_cdrom_host(&iso, 3, GuestImageType::LinuxIso) {
        Ok(r) => r,
        Err(_) => return false,
    };
    if attach_cdrom_uefi(3) != Err(IsoError::UnsupportedOnFirmware) {
        return false;
    }
    let armed = match attach_cdrom_firmware(&iso, host) {
        Ok(r) => r,
        Err(_) => return false,
    };
    if armed.state != CdromAttachState::FirmwareArmed {
        return false;
    }
    let vis = match attach_cdrom_uefi(3) {
        Ok(r) => r,
        Err(_) => return false,
    };
    if vis.state != CdromAttachState::GuestVisible || vis.iso_id != 3 {
        return false;
    }
    pci_write_addr(0x8000_0000);
    let id = pci_read_data(0xCFC, 4);
    if id as u16 != GUEST_CD_PCI_VENDOR || (id >> 16) as u16 != GUEST_CD_PCI_DEVICE {
        return false;
    }
    if host_identify_word0() != Some(0x8500) {
        return false;
    }
    let pvd = match host_read10(16) {
        Some(s) => s,
        None => return false,
    };
    // Mock EFI ISO puts the boot record at LBA 17; LBA 16 is zeroed in the mock.
    // READ(10) of the catalog (LBA 20) must succeed for a GuestVisible CD.
    let cat = match host_read10(20) {
        Some(s) => s,
        None => return false,
    };
    cdrom_visible_evidence(true, true, 1) && pvd.len() == 2048 && cat[0] != 0
}

/// Bearer REST: attach → firmware → uefi. Unarmed uefi → 409. `iso=0` SHELL stays 201.
pub fn prop_rest_uefi_cdrom() -> bool {
    reset_host_cdrom();
    let tok = Some(BRINGUP_AUTH_TOKEN);
    let mut store = ImageTable::new();
    let mut cdrom = CdromTable::empty();

    let missing = dispatch_iso_uefi_rest(
        &mut store,
        &mut cdrom,
        RestRequest {
            method: RestMethod::Post,
            path: "/iso/7/uefi",
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
    let uefi = dispatch_iso_uefi_rest(
        &mut store,
        &mut cdrom,
        RestRequest {
            method: RestMethod::Post,
            path: "/iso/7/uefi",
            auth_token: tok,
        },
    );
    if uefi.status != 201 {
        return false;
    }
    if cdrom.get(7).map(|r| r.state) != Some(CdromAttachState::GuestVisible) {
        return false;
    }
    let st = dispatch_iso_uefi_rest(
        &mut store,
        &mut cdrom,
        RestRequest {
            method: RestMethod::Get,
            path: "/iso/uefi",
            auth_token: tok,
        },
    );
    if st.status != 200 {
        return false;
    }

    let denied = dispatch_iso_uefi_rest(
        &mut store,
        &mut cdrom,
        RestRequest {
            method: RestMethod::Post,
            path: "/iso/8/uefi",
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

pub fn ovmf_cdrom_surface_present() -> bool {
    reset_host_cdrom();
    let spa = include_str!("../assets/webui.html");
    let adr = include_str!("../docs/adr/ADR-014.md");
    let qemu = include_str!("../tools/qemu-boot-test.sh");
    let guest = include_str!("../vmx/guest_uefi.rs");
    let http = include_str!("http.rs");
    attach_cdrom_uefi(1) == Err(IsoError::UnsupportedOnFirmware)
        && !spa.contains("Launch OVMF")
        && !spa.contains("btn-vl")
        && adr.contains("RAYNU-V-M7-E5-OVMF-CDROM-OK")
        && qemu.contains("RAYNU-V-M7-E5-OVMF-CDROM-OK")
        && guest.contains("maybe_print_cdrom")
        && guest.contains("handle_pci")
        && http.contains("is_iso_uefi_path")
        && e4_shell_launch_no_cdrom()
}

/// Full E5 Stage 40 package. Host gate + QEMU marker — not iron, not Everest E5.
pub fn run_m7_e5_ovmf_cdrom_gate() -> bool {
    reset_guest_fw();
    reset_host_cdrom();
    let ok = ovmf_cdrom_surface_present()
        && prop_uefi_attach_after_firmware()
        && prop_rest_uefi_cdrom()
        && run_m7_e5_ovmf_past_sec_gate()
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("GuestVisible")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("UnsupportedOnFirmware")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("not ISO-INSTALL-OK")
        && M7_E5_OVMF_CDROM_GATE_MARKER == "RAYNU-V-M7-E5-OVMF-CDROM-OK";
    reset_host_cdrom();
    reset_guest_fw();
    ok
}

#[cfg(test)]
#[path = "m7_e5_ovmf_cdrom_gate_test.rs"]
mod m7_e5_ovmf_cdrom_gate_test;
