//! E5 Stage 45 — firmware El Torito CD boot so BDS runs the CD EFI.
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** (ADR-002 / ADR-014)
//! VERIFICATION: N/A
//!
//! Stage 44 closed ATAPI-OK on iron COM2 `bf696ca` then stopped the private
//! VMCS on the first sector (`n=30769` `sectors=1`). Keep running after
//! that READ until OVMF mounts the El Torito FAT ESP, StartImages
//! `\EFI\BOOT\BOOTX64.EFI`, and the payload writes `RN-ELT` to COM1, or
//! the 65536-exit cap. Nested VT-x or iron COM2
//! closes the marker. Not `sectors>0` alone. Not installer. Not
//! `ISO-INSTALL-OK`. Not P0-60 G1 EPT. Do not skip `ebecc9c3`. Do not
//! move virtio to `00:00.0`.

use super::guest_fw::reset_guest_fw;
use super::iso::{attach_cdrom_uefi, reset_host_cdrom, IsoError};
use super::m7_e5_ovmf_atapi_gate::run_m7_e5_ovmf_atapi_gate;
use crate::devices::ide_cdrom::{
    eltorito_boot_image_read, eltorito_catalog_read, eltorito_validation_checksum_ok, host_read10,
    present_placeholder, reset, write_eltorito_efi_pe, write_eltorito_fat12, ELTORITO_BOOTX64_OFF,
    ELTORITO_PAYLOAD_MAGIC,
};
use crate::vmx::guest_uefi::{
    eltorito_boot_evidence, eltorito_com_match_step, eltorito_payload_ran, post_atapi_should_stop,
    post_dxe_should_stop, E5_OVMF_VMLAUNCH_RESIDUAL_NOTE, GUEST_UEFI_POST_ATAPI_TAIL,
    GUEST_UEFI_POST_DXE_TAIL, GUEST_UEFI_RESUME_CAP, M7_E5_OVMF_ELTORITO_OK_MARKER,
};

/// Host / CI / QEMU marker when firmware ran the El Torito CD EFI.
pub const M7_E5_OVMF_ELTORITO_GATE_MARKER: &str = M7_E5_OVMF_ELTORITO_OK_MARKER;

pub fn prop_eltorito_payload_is_pe() -> bool {
    let mut pe = [0u8; 0x400];
    if write_eltorito_efi_pe(&mut pe) == 0 {
        return false;
    }
    if &pe[0..2] != b"MZ" || &pe[0x80..0x84] != b"PE\0\0" {
        return false;
    }
    if pe[0x98 + 0x44] != 10 {
        return false;
    }
    let mut fat = [0u8; 8192];
    if write_eltorito_fat12(&mut fat) != 8192 {
        return false;
    }
    if fat[510] != 0x55 || fat[511] != 0xAA {
        return false;
    }
    if &fat[ELTORITO_BOOTX64_OFF..ELTORITO_BOOTX64_OFF + 2] != b"MZ" {
        return false;
    }
    pe.windows(ELTORITO_PAYLOAD_MAGIC.len())
        .any(|w| w == ELTORITO_PAYLOAD_MAGIC)
}

pub fn prop_catalog_and_load_reads() -> bool {
    reset();
    if !present_placeholder() {
        return false;
    }
    if eltorito_catalog_read() || eltorito_boot_image_read() {
        reset();
        return false;
    }
    if host_read10(16).is_none() {
        reset();
        return false;
    }
    if eltorito_catalog_read() || eltorito_boot_image_read() {
        reset();
        return false;
    }
    let cat = match host_read10(20) {
        Some(s) => s,
        None => {
            reset();
            return false;
        }
    };
    if cat[0] != 0x01 || cat[30] != 0x55 || cat[31] != 0xAA || !eltorito_catalog_read() {
        reset();
        return false;
    }
    if !eltorito_validation_checksum_ok(&cat[..32]) || cat[32] != 0x88 {
        reset();
        return false;
    }
    let img = match host_read10(22) {
        Some(s) => s,
        None => {
            reset();
            return false;
        }
    };
    if img[510] != 0x55 || img[511] != 0xAA || !eltorito_boot_image_read() {
        reset();
        return false;
    }
    let file_sec = match host_read10(23) {
        Some(s) => s,
        None => {
            reset();
            return false;
        }
    };
    let pe_in = ELTORITO_BOOTX64_OFF - 2048;
    let ok = &file_sec[pe_in..pe_in + 2] == b"MZ";
    reset();
    ok
}

pub fn ovmf_eltorito_surface_present() -> bool {
    reset_host_cdrom();
    let adr = include_str!("../docs/adr/ADR-014.md");
    let qemu = include_str!("../tools/qemu-boot-test.sh");
    let guest = include_str!("../vmx/guest_uefi.rs");
    let ide = include_str!("../devices/ide_cdrom.rs");
    let plan = include_str!("../docs/m7_plan.md");
    attach_cdrom_uefi(1) == Err(IsoError::UnsupportedOnFirmware)
        && adr.contains("RAYNU-V-M7-E5-OVMF-ELTORITO-OK")
        && qemu.contains("RAYNU-V-M7-E5-OVMF-ELTORITO-OK")
        && qemu.contains("RAYNU-V-M7-E5-OVMF-ATAPI-OK")
        && qemu.contains("RAYNU-V-M7-E5-OVMF-BOTH-OK")
        && qemu.contains("RAYNU-V-M3-LINUX-EARLY-OK")
        && guest.contains("maybe_print_eltorito")
        && guest.contains("post_atapi_should_stop")
        && guest.contains("eltorito_boot_evidence")
        && guest.contains("65536-exit cap")
        && ide.contains("write_eltorito_efi_pe")
        && ide.contains("write_eltorito_fat12")
        && ide.contains("BOOTX64")
        && ide.contains("eltorito_set_validation_checksum")
        && ide.contains("ELTORITO_PAYLOAD_MAGIC")
        && guest.contains("eltorito-progress")
        && plan.contains("OVMF-ELTORITO-OK")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("not firmware El Torito boot")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("catalog+load READ")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("RN-ELT")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("not ISO-INSTALL-OK")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("ebecc9c3")
}

/// Full E5 Stage 45 package. Host gate + QEMU marker. Nested VT-x or iron
/// COM2 closes `OVMF-ELTORITO-OK`. Not installer. Not Everest E5.
pub fn run_m7_e5_ovmf_eltorito_gate() -> bool {
    reset_guest_fw();
    reset_host_cdrom();
    reset();
    let mut magic = 0u8;
    for &b in ELTORITO_PAYLOAD_MAGIC {
        magic = eltorito_com_match_step(magic, b);
    }
    let ok = ovmf_eltorito_surface_present()
        && run_m7_e5_ovmf_atapi_gate()
        && prop_eltorito_payload_is_pe()
        && prop_catalog_and_load_reads()
        && GUEST_UEFI_RESUME_CAP >= 65536
        && GUEST_UEFI_POST_DXE_TAIL == 32768
        && GUEST_UEFI_POST_ATAPI_TAIL == 32768
        && post_dxe_should_stop(true, 115, 115, 1)
        && !post_atapi_should_stop(true, 30769, 115, 30769, 1, false)
        && post_atapi_should_stop(
            true,
            30769 + GUEST_UEFI_POST_ATAPI_TAIL,
            115,
            30769,
            1,
            false,
        )
        && post_atapi_should_stop(true, 200, 115, 180, 4, true)
        && !eltorito_boot_evidence(true, true, false)
        && eltorito_boot_evidence(true, true, true)
        && eltorito_payload_ran(magic)
        && M7_E5_OVMF_ELTORITO_GATE_MARKER == "RAYNU-V-M7-E5-OVMF-ELTORITO-OK";
    reset();
    reset_host_cdrom();
    reset_guest_fw();
    ok
}

#[cfg(test)]
#[path = "m7_e5_ovmf_eltorito_gate_test.rs"]
mod m7_e5_ovmf_eltorito_gate_test;
