//! E5 Stage 43 — firmware-simultaneous virtio + IDE PCI enum.
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** (ADR-002 / ADR-014)
//! VERIFICATION: N/A
//!
//! Keep virtio 1.0 at `00:00.0` (multifunction) and IDE at `00:00.1`.
//! PIIX `00:01.1` is the same CD. ACPI PM timer so PEI Delay can end
//! when `00:00.0` DID is virtio `0x1042`. Stop the private
//! VMCS only after firmware enumerates **both** (or the post-DXE tail).
//! ISA `00:01.0` is multifunction so a bus walk finds IDE. Marker after
//! past-SEC and both PCI enums. Not ATAPI sectors. Not installer.
//! No new `*Absent` enum. No TLS.

use super::guest_fw::reset_guest_fw;
use super::iso::{attach_cdrom_uefi, reset_host_cdrom, IsoError};
use super::m7_e5_cdrom_attach_gate::e4_shell_launch_no_cdrom;
use super::m7_e5_ovmf_virtio_gate::run_m7_e5_ovmf_virtio_gate;
use crate::devices::guest_platform::{
    boot_order_cd_then_disk, host_pci_config_addr, pci_header_is_multifunction,
    pci_read_data as plat_pci_read, pci_write_addr as plat_pci_write, reset as reset_plat,
    HOST_BRIDGE_DEVICE,
};
use crate::devices::guest_virtio_blk::{
    pci_config_addr as virtio_cfg, pci_read_data as virtio_read, pci_write_addr as virtio_write,
    present as present_virtio, reset as reset_virtio, GUEST_VIRTIO_PCI_DEVICE,
    GUEST_VIRTIO_PCI_VENDOR,
};
use crate::devices::ide_cdrom;
use crate::vmx::guest_uefi::{
    both_pci_evidence, hlt_should_resume, post_dxe_should_stop, E5_OVMF_VMLAUNCH_RESIDUAL_NOTE,
    GUEST_UEFI_POST_DXE_TAIL, M7_E5_OVMF_BOTH_OK_MARKER,
};

/// Host / CI / QEMU marker when firmware enumerated both PCI functions.
pub const M7_E5_OVMF_BOTH_GATE_MARKER: &str = M7_E5_OVMF_BOTH_OK_MARKER;

pub fn prop_both_pci_on_one_boot() -> bool {
    reset_virtio();
    reset_plat();
    ide_cdrom::reset();
    if !boot_order_cd_then_disk() {
        return false;
    }
    if !present_virtio() || !ide_cdrom::present_placeholder() {
        return false;
    }
    virtio_write(virtio_cfg());
    let virt_id = virtio_read(0xCFC, 4);
    if virt_id as u16 != GUEST_VIRTIO_PCI_VENDOR
        || (virt_id >> 16) as u16 != GUEST_VIRTIO_PCI_DEVICE
    {
        return false;
    }
    virtio_write(virtio_cfg() | 0x0C);
    if !pci_header_is_multifunction(virtio_read(0xCFC, 4)) {
        return false;
    }
    ide_cdrom::pci_write_addr(ide_cdrom::pci_config_addr());
    let ide_id = ide_cdrom::pci_read_data(0xCFC, 4);
    if ide_id as u16 != 0x8086 || (ide_id >> 16) as u16 != 0x7010 {
        return false;
    }
    ide_cdrom::pci_write_addr(0x8000_0900);
    let piix_id = ide_cdrom::pci_read_data(0xCFC, 4);
    if piix_id as u16 != 0x8086 || (piix_id >> 16) as u16 != 0x7010 {
        return false;
    }
    plat_pci_write(host_pci_config_addr());
    let host_id = match plat_pci_read(0xCFC, 4) {
        Some(v) => v,
        None => return false,
    };
    plat_pci_write(0x8000_080C);
    let isa_ht = match plat_pci_read(0xCFC, 4) {
        Some(v) => v,
        None => return false,
    };
    if !pci_header_is_multifunction(isa_ht) {
        return false;
    }
    let virtio_ok = crate::devices::guest_virtio_blk::pci_enumerated();
    let ide_ok = ide_cdrom::pci_enumerated();
    reset_virtio();
    reset_plat();
    ide_cdrom::reset();
    both_pci_evidence(virtio_ok, ide_ok)
        && (host_id >> 16) as u16 == HOST_BRIDGE_DEVICE
        && virtio_cfg() == 0x8000_0000
        && ide_cdrom::pci_config_addr() == 0x8000_0100
}

pub fn ovmf_both_surface_present() -> bool {
    reset_host_cdrom();
    let spa = include_str!("../assets/webui.html");
    let adr = include_str!("../docs/adr/ADR-014.md");
    let qemu = include_str!("../tools/qemu-boot-test.sh");
    let guest = include_str!("../vmx/guest_uefi.rs");
    attach_cdrom_uefi(1) == Err(IsoError::UnsupportedOnFirmware)
        && !spa.contains("Launch OVMF")
        && !spa.contains("btn-vl")
        && adr.contains("RAYNU-V-M7-E5-OVMF-BOTH-OK")
        && qemu.contains("RAYNU-V-M7-E5-OVMF-BOTH-OK")
        && guest.contains("maybe_print_both")
        && guest.contains("both_pci_evidence")
        && guest.contains("IDE at 00:00.1")
        && guest.contains("HLT skip so DXE can walk PCI")
        && guest.contains("CR-access resume")
        && guest.contains("2048-exit cap")
        && guest.contains("ACPI PM timer")
        && e4_shell_launch_no_cdrom()
}

/// Full E5 Stage 43 package. Host gate + QEMU marker — not iron, not Everest E5.
pub fn run_m7_e5_ovmf_both_gate() -> bool {
    reset_guest_fw();
    reset_host_cdrom();
    reset_virtio();
    reset_plat();
    ide_cdrom::reset();
    let ok = ovmf_both_surface_present()
        && prop_both_pci_on_one_boot()
        && run_m7_e5_ovmf_virtio_gate()
        && !both_pci_evidence(true, false)
        && !both_pci_evidence(false, true)
        && both_pci_evidence(true, true)
        && !post_dxe_should_stop(false, 2000, 0, true, true)
        && !post_dxe_should_stop(true, 115, 115, true, false)
        && !post_dxe_should_stop(true, 115, 115, false, true)
        && post_dxe_should_stop(true, 115, 115, true, true)
        && post_dxe_should_stop(true, 115 + GUEST_UEFI_POST_DXE_TAIL, 115, false, false)
        && !post_dxe_should_stop(true, 115 + GUEST_UEFI_POST_DXE_TAIL - 1, 115, true, false)
        && hlt_should_resume()
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("firmware-simultaneous PCI enum")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("not virtio-alone")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("HLT skip so DXE can walk PCI")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("CR-access resume")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("2048-exit cap")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("ACPI PM timer")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("not ISO-INSTALL-OK")
        && M7_E5_OVMF_BOTH_GATE_MARKER == "RAYNU-V-M7-E5-OVMF-BOTH-OK";
    reset_virtio();
    reset_plat();
    ide_cdrom::reset();
    reset_host_cdrom();
    reset_guest_fw();
    ok
}

#[cfg(test)]
#[path = "m7_e5_ovmf_both_gate_test.rs"]
mod m7_e5_ovmf_both_gate_test;
