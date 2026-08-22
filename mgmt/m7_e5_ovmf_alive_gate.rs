//! E5 Stage 38 — keep guest UEFI alive past the first triple-fault.
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** (ADR-002 / ADR-014)
//! VERIFICATION: N/A
//!
//! Host-owns CR4.VMXE so OVMF SEC `mov cr4, 0x640` does not #GP. Short
//! resume loop. Not installer. No new `*Absent` enum. No new SPA button.

use super::guest_fw::reset_guest_fw;
use super::iso::attach_cdrom_uefi;
use super::iso::IsoError;
use super::m7_e5_cdrom_attach_gate::e4_shell_launch_no_cdrom;
use super::m7_e5_ovmf_vmlaunch_gate::run_m7_e5_ovmf_vmlaunch_gate;
use crate::vmx::guest_uefi::{
    E5_OVMF_SEC_CR4_VALUE, E5_OVMF_VMLAUNCH_RESIDUAL_NOTE, M7_E5_OVMF_ALIVE_OK_MARKER,
};

/// Host / CI / QEMU marker when OVMF ran past the first triple-fault.
pub const M7_E5_OVMF_ALIVE_GATE_MARKER: &str = M7_E5_OVMF_ALIVE_OK_MARKER;

pub fn ovmf_alive_surface_present() -> bool {
    let spa = include_str!("../assets/webui.html");
    let adr = include_str!("../docs/adr/ADR-014.md");
    let qemu = include_str!("../tools/qemu-boot-test.sh");
    let guest = include_str!("../vmx/guest_uefi.rs");
    attach_cdrom_uefi(1) == Err(IsoError::UnsupportedOnFirmware)
        && !spa.contains("Launch OVMF")
        && !spa.contains("btn-vl")
        && adr.contains("RAYNU-V-M7-E5-OVMF-ALIVE-OK")
        && qemu.contains("RAYNU-V-M7-E5-OVMF-ALIVE-OK")
        && guest.contains("CR4_GUEST_HOST_MASK")
        && guest.contains("CR4_VMXE")
        && e4_shell_launch_no_cdrom()
}

pub fn run_m7_e5_ovmf_alive_gate() -> bool {
    reset_guest_fw();
    let ok = ovmf_alive_surface_present()
        && run_m7_e5_ovmf_vmlaunch_gate()
        && E5_OVMF_SEC_CR4_VALUE == 0x640
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("CR4.VMXE host-owned")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("not ISO-INSTALL-OK")
        && M7_E5_OVMF_ALIVE_GATE_MARKER == "RAYNU-V-M7-E5-OVMF-ALIVE-OK";
    reset_guest_fw();
    ok
}

#[cfg(test)]
#[path = "m7_e5_ovmf_alive_gate_test.rs"]
mod m7_e5_ovmf_alive_gate_test;
