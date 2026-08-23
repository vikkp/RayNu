//! E5 Stage 41 — PEI/DXE platform or guest CD boot attempt.
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** (ADR-002 / ADR-014)
//! VERIFICATION: N/A
//!
//! CMOS / fw_cfg / i440FX so PEI can leave the Stage 40 EPT stall.
//! Marker after past-SEC and (ATAPI sector read or exec-from-RAM + platform).
//! Not installer. No new `*Absent` enum. No TLS.

use super::guest_fw::reset_guest_fw;
use super::iso::{attach_cdrom_uefi, reset_host_cdrom, IsoError};
use super::m7_e5_cdrom_attach_gate::e4_shell_launch_no_cdrom;
use super::m7_e5_ovmf_cdrom_gate::run_m7_e5_ovmf_cdrom_gate;
use crate::devices::guest_platform::{
    cmos_above_16m_chunks, host_pci_config_addr, io, is_platform_sink_gpa,
    pci_header_is_multifunction, pci_read_data, pci_write_addr, reset, HOST_BRIDGE_DEVICE,
    HOST_BRIDGE_VENDOR, PLATFORM_RAM_BYTES,
};
use crate::vmx::guest_uefi::{
    dxe_or_cd_boot_evidence, exec_from_low_ram, post_dxe_should_stop,
    E5_OVMF_VMLAUNCH_RESIDUAL_NOTE, GUEST_UEFI_POST_DXE_TAIL, GUEST_UEFI_RESUME_CAP,
    M7_E5_OVMF_DXE_OK_MARKER,
};

/// Host / CI / QEMU marker when PEI/DXE progressed or the guest attempted CD boot.
pub const M7_E5_OVMF_DXE_GATE_MARKER: &str = M7_E5_OVMF_DXE_OK_MARKER;

pub fn prop_platform_memory_honest() -> bool {
    reset();
    if cmos_above_16m_chunks(PLATFORM_RAM_BYTES) != 0x0100 {
        return false;
    }
    let _ = io(0x70, false, 1, 0x35);
    if (io(0x71, true, 1, 0) as u8) != 0x01 {
        return false;
    }
    let _ = io(0x510, false, 2, 0x00);
    let mut sig = [0u8; 4];
    for b in &mut sig {
        *b = io(0x511, true, 1, 0) as u8;
    }
    if &sig != b"QEMU" {
        return false;
    }
    pci_write_addr(host_pci_config_addr());
    let id = match pci_read_data(0xCFC, 4) {
        Some(v) => v,
        None => return false,
    };
    pci_write_addr(0x8000_080C);
    let isa_ht = match pci_read_data(0xCFC, 4) {
        Some(v) => v,
        None => return false,
    };
    pci_write_addr(0x8000_080E);
    let ht_byte = match pci_read_data(0xCFC, 1) {
        Some(v) => v,
        None => return false,
    };
    reset();
    id as u16 == HOST_BRIDGE_VENDOR
        && (id >> 16) as u16 == HOST_BRIDGE_DEVICE
        && pci_header_is_multifunction(isa_ht)
        && ht_byte == 0x80
}

pub fn ovmf_dxe_surface_present() -> bool {
    reset_host_cdrom();
    let spa = include_str!("../assets/webui.html");
    let adr = include_str!("../docs/adr/ADR-014.md");
    let qemu = include_str!("../tools/qemu-boot-test.sh");
    let guest = include_str!("../vmx/guest_uefi.rs");
    let plat = include_str!("../devices/guest_platform.rs");
    attach_cdrom_uefi(1) == Err(IsoError::UnsupportedOnFirmware)
        && !spa.contains("Launch OVMF")
        && !spa.contains("btn-vl")
        && adr.contains("RAYNU-V-M7-E5-OVMF-DXE-OK")
        && qemu.contains("RAYNU-V-M7-E5-OVMF-DXE-OK")
        && guest.contains("maybe_print_dxe")
        && guest.contains("post_dxe_should_stop")
        && guest.contains("handle_ept")
        && guest.contains("guest_platform")
        && plat.contains("pci_cfg_offset")
        && is_platform_sink_gpa(0xFCF8_F000)
        && e4_shell_launch_no_cdrom()
}

/// Full E5 Stage 41 package. Host gate + QEMU marker — not iron, not Everest E5.
pub fn run_m7_e5_ovmf_dxe_gate() -> bool {
    reset_guest_fw();
    reset_host_cdrom();
    reset();
    let ok = ovmf_dxe_surface_present()
        && prop_platform_memory_honest()
        && run_m7_e5_ovmf_cdrom_gate()
        && GUEST_UEFI_RESUME_CAP == 2048
        && GUEST_UEFI_POST_DXE_TAIL == GUEST_UEFI_RESUME_CAP
        && dxe_or_cd_boot_evidence(true, 1, false, false)
        && dxe_or_cd_boot_evidence(true, 0, true, true)
        && !dxe_or_cd_boot_evidence(true, 0, true, false)
        && !post_dxe_should_stop(false, 2000, 0, true, false)
        && !post_dxe_should_stop(true, 115, 115, false, false)
        && !post_dxe_should_stop(true, 115, 115, true, false)
        && post_dxe_should_stop(true, 115, 115, true, true)
        && post_dxe_should_stop(true, 115 + GUEST_UEFI_POST_DXE_TAIL, 115, false, false)
        && exec_from_low_ram(0x0010_0000)
        && !exec_from_low_ram(0xFFFD_3759)
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("CMOS/fw_cfg/i440fx")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("IDE at 00:01.1")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("i440FX host at 00:08.0")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("CF8|CFC byte offset")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("post-DXE spends the 2048-exit cap")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("past-PEI/DXE or CD boot attempt")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("not ISO-INSTALL-OK")
        && M7_E5_OVMF_DXE_GATE_MARKER == "RAYNU-V-M7-E5-OVMF-DXE-OK";
    reset();
    reset_host_cdrom();
    reset_guest_fw();
    ok
}

#[cfg(test)]
#[path = "m7_e5_ovmf_dxe_gate_test.rs"]
mod m7_e5_ovmf_dxe_gate_test;
