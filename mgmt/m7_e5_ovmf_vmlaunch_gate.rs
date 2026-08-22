//! E5 Stage 37 — private guest-UEFI VMLAUNCH of retained OVMF (outside Proven Core).
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** (ADR-002 / ADR-014)
//! VERIFICATION: N/A
//!
//! Host gate: fixtures stay refused; presence + host `run_retained_ovmf_vmlaunch`
//! does not execute VMLAUNCH (`PrivateVmcsNotLaunched`). QEMU (VMX) is the
//! launch proof. No new `*Absent` bookkeeping enum. No new SPA button.

use super::guest_fw::reset_guest_fw;
use super::iso::attach_cdrom_uefi;
use super::iso::IsoError;
use super::m7_e5_cdrom_attach_gate::e4_shell_launch_no_cdrom;
use super::m7_e5_ovmf_retain_gate::run_m7_e5_ovmf_retain_gate;
use crate::boot::ovmf_esp::E5_OVMF_RETAIN_RESIDUAL_NOTE;
use crate::vmx::guest_uefi::{E5_OVMF_VMLAUNCH_RESIDUAL_NOTE, M7_E5_OVMF_VMLAUNCH_OK_MARKER};

#[cfg(test)]
use crate::boot::ovmf_esp::{
    accept_real_ovmf_bytes, clear_retained, retain_ovmf_bytes, MIN_REAL_OVMF_BYTES,
};
#[cfg(test)]
use crate::memory::frame_allocator::FrameAllocator;
#[cfg(test)]
use crate::vmx::guest_uefi::guest_uefi_vmlaunch_entered;
#[cfg(test)]
use crate::vmx::guest_uefi::run_retained_ovmf_vmlaunch;
#[cfg(test)]
use crate::vmx::launch::GuestUefiLaunchError;

/// Host / CI / QEMU marker when VMLAUNCH entered retained OVMF.
pub const M7_E5_OVMF_VMLAUNCH_GATE_MARKER: &str = M7_E5_OVMF_VMLAUNCH_OK_MARKER;

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

pub fn prop_host_retain_does_not_vmlaunch() -> bool {
    #[cfg(not(test))]
    {
        return false;
    }
    #[cfg(test)]
    {
        reset_guest_fw();
        clear_retained();
        let bytes = dense_edk2_image();
        if !accept_real_ovmf_bytes(&bytes) || retain_ovmf_bytes(&bytes).is_err() {
            return false;
        }
        let mut words = [0u64; 1];
        let mut alloc =
            unsafe { FrameAllocator::new(0x1000, 8, words.as_mut_ptr() as u64).unwrap() };
        let ok = unsafe { run_retained_ovmf_vmlaunch(&mut alloc) }
            == Err(GuestUefiLaunchError::PrivateVmcsNotLaunched)
            && !guest_uefi_vmlaunch_entered();
        reset_guest_fw();
        ok
    }
}

pub fn ovmf_vmlaunch_surface_present() -> bool {
    let spa = include_str!("../assets/webui.html");
    let adr = include_str!("../docs/adr/ADR-014.md");
    let qemu = include_str!("../tools/qemu-boot-test.sh");
    attach_cdrom_uefi(1) == Err(IsoError::UnsupportedOnFirmware)
        && !spa.contains("Launch OVMF")
        && !spa.contains("btn-vl")
        && adr.contains("private guest-UEFI VMCS")
        && adr.contains("RAYNU-V-M7-E5-OVMF-VMLAUNCH-OK")
        && qemu.contains("RAYNU-V-M7-E5-OVMF-VMLAUNCH-OK")
        && e4_shell_launch_no_cdrom()
}

pub fn run_m7_e5_ovmf_vmlaunch_gate() -> bool {
    let _ = (
        M7_E5_OVMF_VMLAUNCH_GATE_MARKER,
        E5_OVMF_VMLAUNCH_RESIDUAL_NOTE,
        E5_OVMF_RETAIN_RESIDUAL_NOTE,
    );
    reset_guest_fw();
    let ok = prop_host_retain_does_not_vmlaunch()
        && ovmf_vmlaunch_surface_present()
        && run_m7_e5_ovmf_retain_gate()
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("UnsupportedOnFirmware")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("not ISO-INSTALL-OK")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE
            .contains("VMLAUNCH insn issued only when presence is true")
        && M7_E5_OVMF_VMLAUNCH_GATE_MARKER == "RAYNU-V-M7-E5-OVMF-VMLAUNCH-OK";
    reset_guest_fw();
    ok
}

#[cfg(test)]
#[path = "m7_e5_ovmf_vmlaunch_gate_test.rs"]
mod m7_e5_ovmf_vmlaunch_gate_test;
