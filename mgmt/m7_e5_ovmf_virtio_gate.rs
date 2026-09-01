//! E5 Stage 42 — empty virtio-blk + boot order CD then disk.
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** (ADR-002 / ADR-014)
//! VERIFICATION: N/A
//!
//! Guest-UEFI PCI virtio 1.0 block at `00:02.0` plus fw_cfg `bootorder`
//! (CD then disk). This OVMF PEI only `inw` Device ID of `00:00.0`
//! (i440FX `0x1237`, so CpuDxe `AcpiTimerLibConstructor` matches).
//! IDE is `00:00.1` (slot-0 fn1). Marker after past-SEC and
//! virtio PCI enum. Not a completed firmware CD boot. Not installer.
//! No new `*Absent` enum. No TLS.

use super::guest_fw::reset_guest_fw;
use super::guest_image::{BootDevice, GuestBootSpec, GuestImageType};
use super::iso::{attach_cdrom_uefi, reset_host_cdrom, IsoError};
use super::m7_e5_cdrom_attach_gate::e4_shell_launch_no_cdrom;
use super::m7_e5_ovmf_dxe_gate::run_m7_e5_ovmf_dxe_gate;
use crate::devices::guest_platform::{
    boot_order_cd_then_disk, fwcfg_bootorder_served, host_pci_config_addr, io,
    pci_header_is_multifunction, pci_read_data as plat_pci_read, pci_write_addr as plat_pci_write,
    reset as reset_plat, BOOTORDER, FW_CFG_BOOTORDER_SEL, HOST_BRIDGE_DEVICE,
};
use crate::devices::guest_virtio_blk::{
    latch_dxe_virtio_did, pci_config_addr, pci_read_data, pci_write_addr, pei_host_bridge_did,
    present, reset as reset_virtio, virtio_disk_evidence, GUEST_VIRTIO_PCI_DEVICE,
    GUEST_VIRTIO_PCI_VENDOR, M7_E5_OVMF_VIRTIO_OK_MARKER,
};
use crate::devices::ide_cdrom;
use crate::vmx::guest_uefi::{
    post_dxe_should_stop, E5_OVMF_VMLAUNCH_RESIDUAL_NOTE, GUEST_UEFI_POST_DXE_TAIL,
};

/// Host / CI / QEMU marker when guest-UEFI sees virtio-blk + CD→disk order.
pub const M7_E5_OVMF_VIRTIO_GATE_MARKER: &str = M7_E5_OVMF_VIRTIO_OK_MARKER;

pub fn prop_virtio_pci_and_bootorder() -> bool {
    reset_virtio();
    reset_plat();
    ide_cdrom::reset();
    if !boot_order_cd_then_disk() {
        return false;
    }
    let spec = match GuestBootSpec::product_iso(GuestImageType::LinuxIso, 1, 64) {
        Some(s) => s,
        None => return false,
    };
    if spec.boot_order != [BootDevice::Cdrom, BootDevice::Disk] {
        return false;
    }
    if !present() {
        return false;
    }
    pci_write_addr(0x8000_0002);
    if (pci_read_data(0xCFC, 2) & 0xffff) != u32::from(HOST_BRIDGE_DEVICE) || !pei_host_bridge_did()
    {
        return false;
    }
    if !latch_dxe_virtio_did() {
        return false;
    }
    pci_write_addr(pci_config_addr());
    let id = pci_read_data(0xCFC, 4);
    if id as u16 != GUEST_VIRTIO_PCI_VENDOR || (id >> 16) as u16 != GUEST_VIRTIO_PCI_DEVICE {
        return false;
    }
    pci_write_addr(pci_config_addr() | 0x0C);
    if !pci_header_is_multifunction(pci_read_data(0xCFC, 4)) {
        return false;
    }
    if !ide_cdrom::present_placeholder() {
        return false;
    }
    ide_cdrom::pci_write_addr(ide_cdrom::pci_config_addr());
    let ide_id = ide_cdrom::pci_read_data(0xCFC, 4);
    if ide_id as u16 != 0x8086 || (ide_id >> 16) as u16 != 0x7010 {
        return false;
    }
    plat_pci_write(host_pci_config_addr());
    let host_id = match plat_pci_read(0xCFC, 4) {
        Some(v) => v,
        None => return false,
    };
    if (host_id >> 16) as u16 != HOST_BRIDGE_DEVICE {
        return false;
    }
    plat_pci_write(0x8000_080C);
    let isa_ht = match plat_pci_read(0xCFC, 4) {
        Some(v) => v,
        None => return false,
    };
    if !pci_header_is_multifunction(isa_ht) {
        return false;
    }
    let _ = io(0x510, false, 2, u64::from(FW_CFG_BOOTORDER_SEL));
    let mut first = [0u8; 8];
    for b in &mut first {
        *b = io(0x511, true, 1, 0) as u8;
    }
    let boot_served = fwcfg_bootorder_served();
    let virtio_ok = virtio_disk_evidence(true, true, boot_order_cd_then_disk());
    reset_virtio();
    reset_plat();
    ide_cdrom::reset();
    virtio_ok
        && boot_served
        && first == *b"/pci@i0c"
        && BOOTORDER.starts_with(b"/pci@i0cf8/ide@1,1/drive@0")
        && !BOOTORDER.windows(15).any(|w| w == b"ide@0,1/drive@0")
}

pub fn ovmf_virtio_surface_present() -> bool {
    reset_host_cdrom();
    let spa = include_str!("../assets/webui.html");
    let adr = include_str!("../docs/adr/ADR-014.md");
    let qemu = include_str!("../tools/qemu-boot-test.sh");
    let guest = include_str!("../vmx/guest_uefi.rs");
    let virt = include_str!("../devices/guest_virtio_blk.rs");
    attach_cdrom_uefi(1) == Err(IsoError::UnsupportedOnFirmware)
        && !spa.contains("Launch OVMF")
        && !spa.contains("btn-vl")
        && adr.contains("RAYNU-V-M7-E5-OVMF-VIRTIO-OK")
        && qemu.contains("RAYNU-V-M7-E5-OVMF-VIRTIO-OK")
        && guest.contains("maybe_print_virtio")
        && guest.contains("guest_virtio_blk")
        && virt.contains("00:00.0")
        && virt.contains("00:02.0")
        && qemu.contains("PEI DID slot stays i440FX")
        && e4_shell_launch_no_cdrom()
}

/// Full E5 Stage 42 package. Host gate + QEMU marker — not iron, not Everest E5.
pub fn run_m7_e5_ovmf_virtio_gate() -> bool {
    reset_guest_fw();
    reset_host_cdrom();
    reset_virtio();
    reset_plat();
    let ok = ovmf_virtio_surface_present()
        && prop_virtio_pci_and_bootorder()
        && run_m7_e5_ovmf_dxe_gate()
        && !post_dxe_should_stop(true, 115, 115, 0)
        && post_dxe_should_stop(true, 115, 115, 1)
        && post_dxe_should_stop(true, 115 + GUEST_UEFI_POST_DXE_TAIL, 115, 0)
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("empty virtio-blk at 00:02.0")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("fw_cfg bootorder CD then disk")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("not ISO-INSTALL-OK")
        && M7_E5_OVMF_VIRTIO_GATE_MARKER == "RAYNU-V-M7-E5-OVMF-VIRTIO-OK";
    reset_virtio();
    reset_plat();
    reset_host_cdrom();
    reset_guest_fw();
    ok
}

#[cfg(test)]
#[path = "m7_e5_ovmf_virtio_gate_test.rs"]
mod m7_e5_ovmf_virtio_gate_test;
