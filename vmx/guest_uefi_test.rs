use super::{
    guest_uefi_vmlaunch_entered, last_exit_reason, live_firmware_alias_gpa,
    run_retained_ovmf_vmlaunch, E5_OVMF_VMLAUNCH_RESIDUAL_NOTE, M7_E5_OVMF_VMLAUNCH_OK_MARKER,
};
use crate::boot::ovmf_esp::{
    accept_real_ovmf_bytes, clear_retained, retain_ovmf_bytes, MIN_REAL_OVMF_BYTES,
};
use crate::memory::ept_hw::{firmware_alias_pt_count, frames_required_firmware_alias};
use crate::memory::frame_allocator::FrameAllocator;
use crate::vmx::launch::{
    alias_ept_covers_reset, firmware_alias_gpa, GuestUefiLaunchError, GUEST_UEFI_RESET_VECTOR_GPA,
    MIN_FIRMWARE_ALIAS_BYTES,
};

fn write_fvh(buf: &mut [u8]) {
    let len = buf.len() as u64;
    buf[0x20..0x28].copy_from_slice(&len.to_le_bytes());
    buf[0x28..0x2C].copy_from_slice(b"_FVH");
    buf[0x30..0x32].copy_from_slice(&0x38u16.to_le_bytes());
}

fn dense_edk2_image() -> Vec<u8> {
    let mut bytes = vec![0u8; MIN_REAL_OVMF_BYTES];
    write_fvh(&mut bytes);
    for (i, b) in bytes.iter_mut().enumerate().skip(0x38) {
        *b = (i % 251) as u8 + 1;
    }
    bytes
}

#[test]
fn marker_and_residual_honest() {
    assert_eq!(
        M7_E5_OVMF_VMLAUNCH_OK_MARKER,
        "RAYNU-V-M7-E5-OVMF-VMLAUNCH-OK"
    );
    assert!(
        E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("VMLAUNCH insn issued only when presence is true")
    );
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("not ISO-INSTALL-OK"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("UnsupportedOnFirmware"));
    assert!(!guest_uefi_vmlaunch_entered());
    assert_eq!(last_exit_reason(), 0);
}

#[test]
fn alias_window_covers_reset_for_real_sizes() {
    assert_eq!(
        firmware_alias_gpa(MIN_FIRMWARE_ALIAS_BYTES as u64),
        Some(0xFFC0_0000),
        "Stage 15 contract stays 4 MiB-only"
    );
    assert_eq!(firmware_alias_gpa(1024 * 1024), None);
    assert_eq!(firmware_alias_gpa(2 * 1024 * 1024), None);
    for &len in &[1024 * 1024u64, 2 * 1024 * 1024, 4 * 1024 * 1024] {
        let gpa = live_firmware_alias_gpa(len).expect("live alias");
        assert!(alias_ept_covers_reset(gpa, len));
        assert!(gpa <= GUEST_UEFI_RESET_VECTOR_GPA);
        assert!(GUEST_UEFI_RESET_VECTOR_GPA < gpa + len);
        assert!(frames_required_firmware_alias(gpa, len) <= 8);
        assert!(firmware_alias_pt_count(gpa, len) >= 1);
    }
}

#[test]
fn host_retain_does_not_issue_vmlaunch() {
    clear_retained();
    let bytes = dense_edk2_image();
    assert!(accept_real_ovmf_bytes(&bytes));
    assert_eq!(retain_ovmf_bytes(&bytes), Ok(MIN_REAL_OVMF_BYTES));
    let mut words = [0u64; 1];
    let mut alloc = unsafe { FrameAllocator::new(0x1000, 8, words.as_mut_ptr() as u64).unwrap() };
    assert_eq!(
        unsafe { run_retained_ovmf_vmlaunch(&mut alloc) },
        Err(GuestUefiLaunchError::PrivateVmcsNotLaunched)
    );
    assert!(!guest_uefi_vmlaunch_entered());
    clear_retained();
}

#[test]
fn host_without_presence_refuses() {
    clear_retained();
    let mut words = [0u64; 1];
    let mut alloc = unsafe { FrameAllocator::new(0x1000, 8, words.as_mut_ptr() as u64).unwrap() };
    assert_eq!(
        unsafe { run_retained_ovmf_vmlaunch(&mut alloc) },
        Err(GuestUefiLaunchError::MissingEspFirmware)
    );
}
