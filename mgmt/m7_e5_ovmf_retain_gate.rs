//! E5 Stage 36 — real ESP OVMF retain + presence rule (outside Proven Core).
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** (ADR-002 / ADR-003 / ADR-014)
//! VERIFICATION: N/A
//!
//! Closes the Stage 25–35 `*Absent` bookkeeping ladder. This slice copies
//! a real ESP `\\EFI\\RayNu\\OVMF.fd` into a retained buffer and defines
//! when [`crate::vmx::launch::guest_uefi_live_esp_bytes_present`] is true.
//! QEMU proof uses a system `OVMF.fd` staged by `tools/run-qemu.sh`.
//! Heap fixtures are rejected. VMLAUNCH is not issued (no private VMCS).
//! No new SPA bookkeeping button.

use super::guest_fw::reset_guest_fw;
use super::iso::attach_cdrom_uefi;
use super::iso::IsoError;
use super::m7_e5_cdrom_attach_gate::e4_shell_launch_no_cdrom;
use super::m7_e5_live_hold_gate::run_m7_e5_live_hold_gate;
use crate::boot::ovmf_esp::{
    clear_retained, E5_OVMF_RETAIN_RESIDUAL_NOTE, M7_E5_LIVE_BYTES_PRESENT_OK_MARKER,
};

#[cfg(test)]
use crate::boot::ovmf_esp::{
    accept_real_ovmf_bytes, bytes_present, retain_ovmf_bytes, MIN_REAL_OVMF_BYTES,
};

#[cfg(test)]
use super::guest_fw::{
    admit_ovmf_live_esp, apply_ovmf_live_esp, arm_ovmf_esp_launch, arm_ovmf_firmware_alias,
    arm_ovmf_firmware_slot, arm_ovmf_live_issue, arm_ovmf_private_vmcs, arm_ovmf_real_launch,
    arm_ovmf_reset_vector, bind_ovmf_firmware_guest, box_guest_firmware, commit_ovmf_live_esp,
    copy_ovmf_live_esp, guest_fw_bytes, hold_ovmf_live_esp, install_ovmf_alias_ept,
    latch_ovmf_live_esp, load_guest_firmware, load_ovmf_from_esp, lock_ovmf_live_esp,
    map_live_esp_ovmf, place_ovmf_live_esp, prepare_ovmf_firmware_launch, present_ovmf_live_esp,
    probe_ovmf_firmware, probe_ovmf_live_bytes, program_ovmf_alias_ept, qualify_real_esp_ovmf,
    read_ovmf_live_esp, require_ovmf_live_esp, require_ovmf_live_fd, seal_ovmf_live_esp,
    stage_edk2_ovmf_firmware, stage_ovmf_firmware_floor, try_vmlaunch_ovmf_firmware,
    write_edk2_sized_fv, write_firmware_alias_fv, write_live_esp_ovmf_fv, write_mock_ovmf_fv,
    write_reset_vector_stub, write_size_floor_ovmf_fv, GuestFwError, MIN_EDK2_OVMF_BYTES,
    MIN_FIRMWARE_ALIAS_BYTES, MIN_LIVE_ESP_OVMF_BYTES, MOCK_OVMF_FV_BYTES, SIZE_FLOOR_FV_BYTES,
};

/// Host / CI / QEMU marker when real ESP `OVMF.fd` bytes are retained.
pub const M7_E5_OVMF_RETAIN_OK_MARKER: &str = M7_E5_LIVE_BYTES_PRESENT_OK_MARKER;

#[cfg(test)]
fn write_fvh(buf: &mut [u8]) {
    let len = buf.len() as u64;
    buf[0x20..0x28].copy_from_slice(&len.to_le_bytes());
    buf[0x28..0x2C].copy_from_slice(b"_FVH");
    buf[0x30..0x32].copy_from_slice(&0x38u16.to_le_bytes());
}

#[cfg(test)]
fn dense_edk2_image() -> Vec<u8> {
    let mut bytes = vec![0u8; MIN_REAL_OVMF_BYTES];
    write_fvh(&mut bytes);
    for (i, b) in bytes.iter_mut().enumerate().skip(0x38) {
        *b = (i % 251) as u8 + 1;
    }
    bytes
}

/// Zero-padded 4 MiB alias fixture is not real OVMF and does not set presence.
pub fn prop_retain_rejects_alias_fixture() -> bool {
    #[cfg(not(test))]
    {
        return false;
    }
    #[cfg(test)]
    {
        reset_guest_fw();
        let mut fixture = vec![0u8; MIN_FIRMWARE_ALIAS_BYTES];
        if write_firmware_alias_fv(&mut fixture).is_err() {
            return false;
        }
        !accept_real_ovmf_bytes(&fixture)
            && retain_ovmf_bytes(&fixture).is_err()
            && !bytes_present()
            && !crate::vmx::launch::guest_uefi_live_esp_bytes_present()
    }
}

/// Dense EDK2-sized `_FVH` sets presence. VMLAUNCH stays refused.
pub fn prop_retain_sets_presence() -> bool {
    #[cfg(not(test))]
    {
        return false;
    }
    #[cfg(test)]
    {
        reset_guest_fw();
        let bytes = dense_edk2_image();
        match retain_ovmf_bytes(&bytes) {
            Ok(n) => {
                let ok = n == MIN_REAL_OVMF_BYTES
                    && bytes_present()
                    && crate::vmx::launch::guest_uefi_live_esp_bytes_present();
                reset_guest_fw();
                ok
            }
            Err(()) => false,
        }
    }
}

/// After live-hold + retain, try_vmlaunch is PrivateVmcsNotLaunched (no insn).
pub fn prop_retain_refuses_vmlaunch() -> bool {
    #[cfg(not(test))]
    {
        return false;
    }
    #[cfg(test)]
    {
        reset_guest_fw();
        if box_guest_firmware(guest_fw_bytes()).is_err()
            || load_guest_firmware(guest_fw_bytes()).is_err()
        {
            return false;
        }
        let mut mock = [0u8; MOCK_OVMF_FV_BYTES];
        if write_mock_ovmf_fv(&mut mock).is_err()
            || probe_ovmf_firmware(&mock).is_err()
            || load_ovmf_from_esp(&mock).is_err()
            || arm_ovmf_firmware_slot().is_err()
            || bind_ovmf_firmware_guest().is_err()
            || prepare_ovmf_firmware_launch().is_err()
        {
            return false;
        }
        let mut floor = [0u8; SIZE_FLOOR_FV_BYTES];
        if write_size_floor_ovmf_fv(&mut floor).is_err()
            || stage_ovmf_firmware_floor(&floor).is_err()
        {
            return false;
        }
        let mut edk2 = vec![0u8; MIN_EDK2_OVMF_BYTES];
        if write_edk2_sized_fv(&mut edk2).is_err() || stage_edk2_ovmf_firmware(&edk2).is_err() {
            return false;
        }
        if arm_ovmf_esp_launch().is_err() {
            return false;
        }
        let mut live = vec![0u8; MIN_LIVE_ESP_OVMF_BYTES];
        if write_live_esp_ovmf_fv(&mut live).is_err() || write_reset_vector_stub(&mut live).is_err()
        {
            return false;
        }
        if map_live_esp_ovmf(&live).is_err() || arm_ovmf_reset_vector(&live).is_err() {
            return false;
        }
        let mut alias = vec![0u8; MIN_FIRMWARE_ALIAS_BYTES];
        if write_firmware_alias_fv(&mut alias).is_err()
            || arm_ovmf_firmware_alias(&alias).is_err()
            || program_ovmf_alias_ept(&alias).is_err()
            || install_ovmf_alias_ept(&alias).is_err()
            || qualify_real_esp_ovmf(&alias).is_err()
            || arm_ovmf_real_launch(&alias).is_err()
            || require_ovmf_live_esp(&alias).is_err()
            || arm_ovmf_private_vmcs(&alias).is_err()
            || arm_ovmf_live_issue(&alias).is_err()
            || probe_ovmf_live_bytes(&alias).is_err()
            || require_ovmf_live_fd(&alias).is_err()
            || present_ovmf_live_esp(&alias).is_err()
            || admit_ovmf_live_esp(&alias).is_err()
            || read_ovmf_live_esp(&alias).is_err()
            || copy_ovmf_live_esp(&alias).is_err()
            || place_ovmf_live_esp(&alias).is_err()
            || apply_ovmf_live_esp(&alias).is_err()
            || commit_ovmf_live_esp(&alias).is_err()
            || latch_ovmf_live_esp(&alias).is_err()
            || seal_ovmf_live_esp(&alias).is_err()
            || lock_ovmf_live_esp(&alias).is_err()
            || hold_ovmf_live_esp(&alias).is_err()
        {
            return false;
        }
        if try_vmlaunch_ovmf_firmware() != Err(GuestFwError::LiveEspHoldAbsent) {
            return false;
        }
        let realish = dense_edk2_image();
        if retain_ovmf_bytes(&realish).is_err() {
            return false;
        }
        let ok = try_vmlaunch_ovmf_firmware() == Err(GuestFwError::PrivateVmcsNotLaunched)
            && crate::vmx::launch::guest_uefi_live_esp_bytes_present();
        reset_guest_fw();
        ok
    }
}

/// attach_cdrom_uefi stays closed. No new SPA bookkeeping button.
pub fn ovmf_retain_surface_present() -> bool {
    let spa = include_str!("../assets/webui.html");
    let adr = include_str!("../docs/adr/ADR-014.md");
    let launch = include_str!("../vmx/launch.rs");
    attach_cdrom_uefi(1) == Err(IsoError::UnsupportedOnFirmware)
        && !spa.contains("Retain OVMF")
        && !spa.contains("Load real OVMF")
        && !spa.contains("btn-rt")
        && adr.contains("Presence rule")
        && adr.contains("accept_real_ovmf_bytes")
        && adr.contains("no further *Absent bookkeeping")
        && launch.contains("fn guest_uefi_live_esp_bytes_present")
        && launch.contains("boot::ovmf_esp::bytes_present")
        && !launch.contains("super::ops::vmlaunch()")
        && e4_shell_launch_no_cdrom()
}

/// Full E5 retain package. Host gate + QEMU marker. Not iron. Not Everest E5.
pub fn run_m7_e5_ovmf_retain_gate() -> bool {
    let _ = (
        M7_E5_OVMF_RETAIN_OK_MARKER,
        E5_OVMF_RETAIN_RESIDUAL_NOTE,
        clear_retained as fn(),
    );
    reset_guest_fw();
    let ok = prop_retain_rejects_alias_fixture()
        && prop_retain_sets_presence()
        && prop_retain_refuses_vmlaunch()
        && ovmf_retain_surface_present()
        && run_m7_e5_live_hold_gate()
        && E5_OVMF_RETAIN_RESIDUAL_NOTE.contains("UnsupportedOnFirmware")
        && E5_OVMF_RETAIN_RESIDUAL_NOTE.contains("no guest UEFI VMLAUNCH")
        && E5_OVMF_RETAIN_RESIDUAL_NOTE.contains("VMLAUNCH insn not issued")
        && E5_OVMF_RETAIN_RESIDUAL_NOTE.contains("not allocated")
        && M7_E5_OVMF_RETAIN_OK_MARKER == "RAYNU-V-M7-E5-LIVE-BYTES-PRESENT-OK";
    reset_guest_fw();
    ok
}

#[cfg(test)]
#[path = "m7_e5_ovmf_retain_gate_test.rs"]
mod m7_e5_ovmf_retain_gate_test;
