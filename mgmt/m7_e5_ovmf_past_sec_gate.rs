//! E5 Stage 39 — OVMF past-SEC on the private guest-UEFI VMCS.
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** (ADR-002 / ADR-014)
//! VERIFICATION: N/A
//!
//! COM1/COM2 forwarded. PAST-SEC after leaving the last 64 KiB with PEI
//! PCI, firmware serial, or HLT. Not installer. No new `*Absent` enum.

use super::guest_fw::reset_guest_fw;
use super::iso::attach_cdrom_uefi;
use super::iso::IsoError;
use super::m7_e5_cdrom_attach_gate::e4_shell_launch_no_cdrom;
use super::m7_e5_ovmf_alive_gate::run_m7_e5_ovmf_alive_gate;
use crate::vmx::guest_uefi::{
    past_sec_evidence, E5_OVMF_VMLAUNCH_RESIDUAL_NOTE, GUEST_UEFI_RESUME_CAP,
    GUEST_UEFI_SEC_TAIL_GPA, M7_E5_OVMF_PAST_SEC_OK_MARKER,
};

/// Host / CI / QEMU marker when OVMF left the SEC tail with PEI-style evidence.
pub const M7_E5_OVMF_PAST_SEC_GATE_MARKER: &str = M7_E5_OVMF_PAST_SEC_OK_MARKER;

pub fn ovmf_past_sec_surface_present() -> bool {
    let spa = include_str!("../assets/webui.html");
    let adr = include_str!("../docs/adr/ADR-014.md");
    let qemu = include_str!("../tools/qemu-boot-test.sh");
    let guest = include_str!("../vmx/guest_uefi.rs");
    attach_cdrom_uefi(1) == Err(IsoError::UnsupportedOnFirmware)
        && !spa.contains("Launch OVMF")
        && !spa.contains("btn-vl")
        && adr.contains("RAYNU-V-M7-E5-OVMF-PAST-SEC-OK")
        && qemu.contains("RAYNU-V-M7-E5-OVMF-PAST-SEC-OK")
        && guest.contains("COM1/COM2")
        && guest.contains("handle_uart")
        && guest.contains("GUEST_UEFI_SEC_TAIL_GPA")
        && e4_shell_launch_no_cdrom()
}

pub fn run_m7_e5_ovmf_past_sec_gate() -> bool {
    reset_guest_fw();
    let ok = ovmf_past_sec_surface_present()
        && run_m7_e5_ovmf_alive_gate()
        && GUEST_UEFI_SEC_TAIL_GPA == 0xFFFF_0000
        && GUEST_UEFI_RESUME_CAP == 256
        && past_sec_evidence(true, true, 0, false)
        && !past_sec_evidence(false, true, 1, true)
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("past-SEC")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("COM1/COM2 forwarded")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("not ISO-INSTALL-OK")
        && M7_E5_OVMF_PAST_SEC_GATE_MARKER == "RAYNU-V-M7-E5-OVMF-PAST-SEC-OK";
    reset_guest_fw();
    ok
}

#[cfg(test)]
#[path = "m7_e5_ovmf_past_sec_gate_test.rs"]
mod m7_e5_ovmf_past_sec_gate_test;
