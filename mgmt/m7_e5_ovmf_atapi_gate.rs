//! E5 Stage 44 — firmware ATAPI READ so `sectors>0`.
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** (ADR-002 / ADR-014)
//! VERIFICATION: N/A
//!
//! Stage 43 closed both PCI enums then stopped the private VMCS
//! (`1b07692` n=1111 `sectors=0`). Keep running after BOTH-OK until
//! firmware issues PACKET/`READ(10)` (honest `sectors>0`) or the 2048
//! cap. ATAPI signature after reset (`LBA mid=0x14` high=`0xEB`),
//! PACKET interrupt-reason (CDB `0x01`, data-in `0x02`, complete `0x03`),
//! and cylinder byte count. Placeholder ISO has PVD `CD001` plus a
//! minimal EFI El Torito catalog. Marker after past-SEC and a real
//! sector read. Not firmware El Torito boot. Not installer. No new
//! `*Absent` enum. No TLS.

use super::guest_fw::reset_guest_fw;
use super::iso::{attach_cdrom_uefi, reset_host_cdrom, IsoError};
use super::m7_e5_cdrom_attach_gate::e4_shell_launch_no_cdrom;
use super::m7_e5_ovmf_both_gate::run_m7_e5_ovmf_both_gate;
use crate::devices::ide_cdrom;
use crate::vmx::guest_uefi::{
    atapi_read_evidence, hlt_should_resume, post_dxe_should_stop, spin_short_jmp_should_skip,
    E5_OVMF_VMLAUNCH_RESIDUAL_NOTE, GUEST_UEFI_POST_DXE_TAIL, M7_E5_OVMF_ATAPI_OK_MARKER,
};

/// Host / CI / QEMU marker when firmware read an ATAPI sector.
pub const M7_E5_OVMF_ATAPI_GATE_MARKER: &str = M7_E5_OVMF_ATAPI_OK_MARKER;

pub fn prop_atapi_signature_and_read10() -> bool {
    ide_cdrom::reset();
    if !ide_cdrom::present_placeholder() {
        return false;
    }
    // After reset/present, ATAPI signature so firmware sends PACKET (0xA0).
    let mid = ide_cdrom::ata_io(0x01F4, true, 1, 0) as u8;
    let high = ide_cdrom::ata_io(0x01F5, true, 1, 0) as u8;
    if mid != 0x14 || high != 0xEB {
        ide_cdrom::reset();
        return false;
    }
    let _ = ide_cdrom::ata_io(0x01F7, false, 1, 0xA0);
    let reason = ide_cdrom::ata_io(0x01F2, true, 1, 0) as u8;
    if reason != 0x01 {
        ide_cdrom::reset();
        return false;
    }
    let pvd = match ide_cdrom::host_read10(16) {
        Some(s) => s,
        None => {
            ide_cdrom::reset();
            return false;
        }
    };
    let br = match ide_cdrom::host_read10(17) {
        Some(s) => s,
        None => {
            ide_cdrom::reset();
            return false;
        }
    };
    let sectors = ide_cdrom::sectors_read();
    let packets = ide_cdrom::packet_commands();
    let scsi = ide_cdrom::last_scsi();
    ide_cdrom::reset();
    mid == 0x14
        && high == 0xEB
        && &pvd[1..6] == b"CD001"
        && &br[7..30] == b"EL TORITO SPECIFICATION"
        && atapi_read_evidence(sectors)
        && packets >= 2
        && scsi == 0x28
}

pub fn ovmf_atapi_surface_present() -> bool {
    reset_host_cdrom();
    let spa = include_str!("../assets/webui.html");
    let adr = include_str!("../docs/adr/ADR-014.md");
    let qemu = include_str!("../tools/qemu-boot-test.sh");
    let guest = include_str!("../vmx/guest_uefi.rs");
    let ide = include_str!("../devices/ide_cdrom.rs");
    attach_cdrom_uefi(1) == Err(IsoError::UnsupportedOnFirmware)
        && !spa.contains("Launch OVMF")
        && !spa.contains("btn-vl")
        && adr.contains("RAYNU-V-M7-E5-OVMF-ATAPI-OK")
        && qemu.contains("RAYNU-V-M7-E5-OVMF-ATAPI-OK")
        && guest.contains("maybe_print_atapi")
        && guest.contains("atapi_read_evidence")
        && guest.contains("not both-enum-alone")
        && guest.contains("ATAPI signature")
        && guest.contains("PACKET interrupt-reason")
        && ide.contains("ATAPI_INT_CD")
        && ide.contains("ATAPI_SIG_LBA")
        && ide.contains("EL TORITO SPECIFICATION")
        && ide.contains("SCSI_REQUEST_SENSE")
        && e4_shell_launch_no_cdrom()
}

/// Full E5 Stage 44 package. Host gate + QEMU marker — not iron, not Everest E5.
pub fn run_m7_e5_ovmf_atapi_gate() -> bool {
    reset_guest_fw();
    reset_host_cdrom();
    ide_cdrom::reset();
    let ok = ovmf_atapi_surface_present()
        && prop_atapi_signature_and_read10()
        && run_m7_e5_ovmf_both_gate()
        && !atapi_read_evidence(0)
        && atapi_read_evidence(1)
        && !post_dxe_should_stop(false, 2000, 0, 1)
        && !post_dxe_should_stop(true, 115, 115, 0)
        && post_dxe_should_stop(true, 115, 115, 1)
        && post_dxe_should_stop(true, 115 + GUEST_UEFI_POST_DXE_TAIL, 115, 0)
        && !post_dxe_should_stop(true, 115 + GUEST_UEFI_POST_DXE_TAIL - 1, 115, 0)
        && hlt_should_resume()
        && spin_short_jmp_should_skip(0xEB, 0xF3)
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("not both-enum-alone")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("ATAPI signature")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("PACKET interrupt-reason")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("not firmware El Torito boot")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("not ISO-INSTALL-OK")
        && M7_E5_OVMF_ATAPI_GATE_MARKER == "RAYNU-V-M7-E5-OVMF-ATAPI-OK";
    ide_cdrom::reset();
    reset_host_cdrom();
    reset_guest_fw();
    ok
}

#[cfg(test)]
#[path = "m7_e5_ovmf_atapi_gate_test.rs"]
mod m7_e5_ovmf_atapi_gate_test;
