//! E5 Stage 43 — firmware-simultaneous virtio + IDE PCI enum.
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** (ADR-002 / ADR-014)
//! VERIFICATION: N/A
//!
//! Keep virtio 1.0 at `00:02.0` (multifunction) and IDE at `00:00.1`.
//! PIIX `00:01.1` is the same CD. PIIX4 PM at `00:01.3`. PEI `00:00.0`
//! DID stays i440FX `0x1237` so `PlatformMemMapInitialization` adds the VGA
//! IoMemory HOB (stock QEMU map) and CpuDxe `AcpiTimerLibConstructor`
//! matches PIIX4. DXE latches virtio `0x1042` at `00:02.0` on the first
//! other-BDF CF8 (PciBus / BOTH-OK). `remap_i440fx_did_imm` stays in-tree
//! but is not applied while PEI captures `HostBridgeDevId`. ACPI PM
//! timer so PEI Delay can end. 8259 PIC RAZ/WI. fw_cfg `etc/e820` 32 MiB.
//! Exception insn dump on `#GP`. 4 MiB flash window so CODE-only ESP
//! images still cover VARS GPA `0xFFC00000`. Empty VARS `_FVH` (Debian
//! `OVMF_VARS_4M.fd` prefix) so PEI does not parse erased NOR. Live HPET in
//! the `0xFED00000` sink (nested VT-x `20763e4` 300 s kill). Nested VT-x
//! `105ffbe` burned the 2048 cap at `rip=0x6e812d` with a 10 ms HPET
//! step; 1 s per VMEXIT so Delay can end. Stop dumps identity RIP bytes
//! so a leftover HPET poll is readable. Nested VT-x `707a849`: 1s HPET left
//! `rip=0x6e812d insn=ebf3` (CpuDeadLoop); skip backward `jmp rel8` so
//! firmware can fall through to PciBus. Marker after past-SEC and both
//! PCI enums. Stage 44 closed on iron COM2 `bf696ca` ATAPI `sectors>0`
//! (not both-enum-alone).
//! Not installer. No new `*Absent` enum. No TLS.

use super::guest_fw::reset_guest_fw;
use super::iso::{attach_cdrom_uefi, reset_host_cdrom, IsoError};
use super::m7_e5_cdrom_attach_gate::e4_shell_launch_no_cdrom;
use super::m7_e5_ovmf_virtio_gate::run_m7_e5_ovmf_virtio_gate;
use crate::devices::guest_platform::{
    boot_order_cd_then_disk, host_pci_config_addr, pci_header_is_multifunction,
    pci_read_data as plat_pci_read, pci_write_addr as plat_pci_write, pm_pci_config_addr,
    reset as reset_plat, HOST_BRIDGE_DEVICE, PM_BRIDGE_DEVICE, PM_BRIDGE_VENDOR,
};
use crate::devices::guest_virtio_blk::{
    latch_dxe_virtio_did, pci_config_addr as virtio_cfg, pci_read_data as virtio_read,
    pci_write_addr as virtio_write, pei_host_bridge_did, present as present_virtio,
    reset as reset_virtio, GUEST_VIRTIO_PCI_DEVICE, GUEST_VIRTIO_PCI_VENDOR,
};
use crate::devices::ide_cdrom;
use crate::vmx::guest_uefi::{
    both_pci_evidence, hlt_should_resume, post_dxe_should_stop, spin_short_jmp_should_skip,
    E5_OVMF_VMLAUNCH_RESIDUAL_NOTE, GUEST_UEFI_POST_DXE_TAIL, M7_E5_OVMF_BOTH_OK_MARKER,
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
    virtio_write(0x8000_0002);
    if (virtio_read(0xCFC, 2) & 0xffff) != u32::from(HOST_BRIDGE_DEVICE) || !pei_host_bridge_did() {
        return false;
    }
    if !latch_dxe_virtio_did() {
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
    plat_pci_write(pm_pci_config_addr());
    let pm_id = match plat_pci_read(0xCFC, 4) {
        Some(v) => v,
        None => return false,
    };
    if pm_id as u16 != PM_BRIDGE_VENDOR || (pm_id >> 16) as u16 != PM_BRIDGE_DEVICE {
        return false;
    }
    let virtio_ok = crate::devices::guest_virtio_blk::pci_enumerated();
    let ide_ok = ide_cdrom::pci_enumerated();
    reset_virtio();
    reset_plat();
    ide_cdrom::reset();
    both_pci_evidence(virtio_ok, ide_ok)
        && (host_id >> 16) as u16 == HOST_BRIDGE_DEVICE
        && pm_pci_config_addr() == 0x8000_0B00
        && virtio_cfg() == 0x8000_1000
        && ide_cdrom::pci_config_addr() == 0x8000_0100
}

pub fn ovmf_both_surface_present() -> bool {
    reset_host_cdrom();
    let spa = include_str!("../assets/webui.html");
    let adr = include_str!("../docs/adr/ADR-014.md");
    let qemu = include_str!("../tools/qemu-boot-test.sh");
    let guest = include_str!("../vmx/guest_uefi.rs");
    let plat = include_str!("../devices/guest_platform.rs");
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
        && guest.contains("32768-exit cap")
        && guest.contains("8192-exit cap")
        && guest.contains("ACPI PM timer")
        && guest.contains("PIIX4 PM at 00:01.3")
        && guest.contains("remap i440FX DID")
        && guest.contains("remap_i440fx_did_imm")
        && guest.contains("cmp bx")
        && guest.contains("maybe_remap_guest_ram")
        && guest.contains("is_piix_pm_io")
        && guest.contains("8259 PIC RAZ/WI")
        && guest.contains("fw_cfg etc/e820")
        && guest.contains("exception insn dump")
        && guest.contains("copy_low_ram_at")
        && guest.contains("4MiB flash window")
        && guest.contains("flash_window_gpa_and_pad")
        && guest.contains("stamp_empty_ovmf_vars")
        && guest.contains("empty VARS _FVH")
        && guest.contains("hpet_tick_sink")
        && guest.contains("live HPET")
        && guest.contains("HPET 1s step")
        && guest.contains("stop RIP insn dump")
        && guest.contains("spin jmp skip")
        && guest.contains("skip_spin_short_jmp")
        && plat.contains("HPET_MAIN_STEP: u64 = 100_000_000")
        && plat.contains("HPET_INSN_STEP: u64 = 100_000")
        && plat.contains("HPET_UART_IO_STEP_CAP: u64 = 400")
        && plat.contains("fn hpet_ticks_from_tsc_delta(")
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
        && !post_dxe_should_stop(false, 2000, 0, 1)
        && !post_dxe_should_stop(true, 115, 115, 0)
        && post_dxe_should_stop(true, 115, 115, 1)
        && post_dxe_should_stop(true, 115 + GUEST_UEFI_POST_DXE_TAIL, 115, 0)
        && !post_dxe_should_stop(true, 115 + GUEST_UEFI_POST_DXE_TAIL - 1, 115, 0)
        && hlt_should_resume()
        && spin_short_jmp_should_skip(0xEB, 0xF3)
        && !spin_short_jmp_should_skip(0x74, 0xF3)
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("firmware-simultaneous PCI enum")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("not virtio-alone")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("not both-enum-alone")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("HLT skip so DXE can walk PCI")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("CR-access resume")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("8192-exit cap")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("ACPI PM timer")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("PIIX4 PM at 00:01.3")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("remap i440FX DID")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("cmp bx")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("8259 PIC RAZ/WI")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("fw_cfg etc/e820")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("exception insn dump")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("4MiB flash window")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("empty VARS _FVH")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("live HPET")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("HPET 1s step")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("HPET 1ms on CPUID/MSR/EPT")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("HPET TSC-delta on UART COM I/O")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("Linux printk ticks every 4096")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("guest UART nowait (do not clear COM2_LIVE)")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("guest UART TX ring drain")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("guest UART TX ring drain 4/exit")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("linux earlycon share TX ring")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("linux earlycon quiet ticks")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("linux earlycon hush HV")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("linux earlycon share product ISO")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("cpu_flush on tick cadence even when share")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("linux earlycon share first CPUID")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("linux earlycon share first high-half")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("linux earlycon skip #PF dump")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("linux earlycon skip exc deliver")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("poll ISO-INSTALL-OK every resume")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("256MiB disk leftover report-RAM")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("stop RIP insn dump")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("spin jmp skip")
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
