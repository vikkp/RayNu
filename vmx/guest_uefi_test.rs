use super::{
    both_pci_evidence, dxe_or_cd_boot_evidence, exec_from_low_ram, guest_uefi_alive,
    guest_uefi_both, guest_uefi_com_bytes, guest_uefi_dxe, guest_uefi_non_tf_exits,
    guest_uefi_past_sec, guest_uefi_vmlaunch_entered, hlt_should_resume, io_port_from_qual,
    is_com_uart_port, is_pci_config_port, last_exit_reason, linear_left_sec_tail,
    live_firmware_alias_gpa, past_sec_evidence, pci_bdf_bit, post_dxe_should_stop,
    run_retained_ovmf_vmlaunch, E5_OVMF_SEC_CR4_VALUE, E5_OVMF_VMLAUNCH_RESIDUAL_NOTE,
    GUEST_UEFI_POST_DXE_TAIL, GUEST_UEFI_RESUME_CAP, GUEST_UEFI_SEC_TAIL_GPA,
    M7_E5_OVMF_ALIVE_OK_MARKER, M7_E5_OVMF_BOTH_OK_MARKER, M7_E5_OVMF_CDROM_OK_MARKER,
    M7_E5_OVMF_DXE_OK_MARKER, M7_E5_OVMF_PAST_SEC_OK_MARKER, M7_E5_OVMF_VIRTIO_OK_MARKER,
    M7_E5_OVMF_VMLAUNCH_OK_MARKER,
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
    assert_eq!(M7_E5_OVMF_ALIVE_OK_MARKER, "RAYNU-V-M7-E5-OVMF-ALIVE-OK");
    assert_eq!(
        M7_E5_OVMF_PAST_SEC_OK_MARKER,
        "RAYNU-V-M7-E5-OVMF-PAST-SEC-OK"
    );
    assert_eq!(E5_OVMF_SEC_CR4_VALUE, 0x640);
    assert_eq!(GUEST_UEFI_SEC_TAIL_GPA, 0xFFFF_0000);
    assert_eq!(GUEST_UEFI_RESUME_CAP, 2048);
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("CR4.VMXE host-owned"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("COM1/COM2 forwarded"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("past-SEC"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("GuestVisible"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("CMOS/fw_cfg/i440fx"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("IDE at 00:00.1"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("virtio Header Type is multifunction"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("i440FX host at 00:08.0"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("CF8|CFC byte offset"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("post-DXE spends the 2048-exit cap"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("past-PEI/DXE or CD boot attempt"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("empty virtio-blk at 00:00.0"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("fw_cfg bootorder CD then disk"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("firmware-simultaneous PCI enum"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("not virtio-alone"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("HLT skip so DXE can walk PCI"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("CR-access resume"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("ACPI PM timer"));
    assert!(hlt_should_resume());
    assert_eq!(GUEST_UEFI_POST_DXE_TAIL, GUEST_UEFI_RESUME_CAP);
    assert_eq!(pci_bdf_bit(0, 0), Some((0, 1)));
    assert_eq!(pci_bdf_bit(1, 1), Some((0, 1u64 << 9)));
    assert_eq!(pci_bdf_bit(8, 0), Some((1, 1)));
    assert_eq!(pci_bdf_bit(16, 0), None);
    assert_eq!(M7_E5_OVMF_CDROM_OK_MARKER, "RAYNU-V-M7-E5-OVMF-CDROM-OK");
    assert_eq!(M7_E5_OVMF_DXE_OK_MARKER, "RAYNU-V-M7-E5-OVMF-DXE-OK");
    assert_eq!(M7_E5_OVMF_VIRTIO_OK_MARKER, "RAYNU-V-M7-E5-OVMF-VIRTIO-OK");
    assert_eq!(M7_E5_OVMF_BOTH_OK_MARKER, "RAYNU-V-M7-E5-OVMF-BOTH-OK");
    assert!(!guest_uefi_alive());
    assert!(!guest_uefi_past_sec());
    assert!(!guest_uefi_dxe());
    assert!(!guest_uefi_both());
    assert_eq!(guest_uefi_non_tf_exits(), 0);
    assert_eq!(guest_uefi_com_bytes(), 0);
}

#[test]
fn past_sec_predicates_are_honest() {
    assert_eq!(io_port_from_qual(0x00cf_8000_b), 0xCF8);
    assert_eq!(io_port_from_qual(0x00cf_c000_3), 0xCFC);
    assert!(is_pci_config_port(0xCF8));
    assert!(is_pci_config_port(0xCFC));
    assert!(!is_pci_config_port(0x3F8));
    assert!(is_com_uart_port(0x3F8));
    assert!(is_com_uart_port(0x3FD));
    assert!(is_com_uart_port(0x2F8));
    assert!(!is_com_uart_port(0x80));
    assert!(!linear_left_sec_tail(0xFFFF_FBC6));
    assert!(!linear_left_sec_tail(0xFFFF_0000));
    assert!(linear_left_sec_tail(0xFFFC_DF76));
    assert!(linear_left_sec_tail(0xFFFD_37A9));
    assert!(!past_sec_evidence(false, true, 1, true));
    assert!(!past_sec_evidence(true, false, 0, false));
    assert!(past_sec_evidence(true, true, 0, false));
    assert!(past_sec_evidence(true, false, 1, false));
    assert!(past_sec_evidence(true, false, 0, true));
    assert!(exec_from_low_ram(0x0010_0000));
    assert!(!exec_from_low_ram(0xFFFD_3759));
    assert!(!dxe_or_cd_boot_evidence(false, 1, true, true));
    assert!(!dxe_or_cd_boot_evidence(true, 0, false, false));
    assert!(!dxe_or_cd_boot_evidence(true, 0, true, false));
    assert!(dxe_or_cd_boot_evidence(true, 1, false, false));
    assert!(dxe_or_cd_boot_evidence(true, 0, true, true));
    assert!(!both_pci_evidence(true, false));
    assert!(!both_pci_evidence(false, true));
    assert!(both_pci_evidence(true, true));
    assert!(!post_dxe_should_stop(false, 2000, 0, true, true));
    assert!(!post_dxe_should_stop(true, 115, 115, false, false));
    assert!(!post_dxe_should_stop(true, 115, 115, true, false));
    assert!(!post_dxe_should_stop(true, 115, 115, false, true));
    assert!(post_dxe_should_stop(true, 115, 115, true, true));
    assert!(post_dxe_should_stop(
        true,
        115 + GUEST_UEFI_POST_DXE_TAIL,
        115,
        false,
        false
    ));
    assert!(!post_dxe_should_stop(
        true,
        115 + GUEST_UEFI_POST_DXE_TAIL - 1,
        115,
        true,
        false
    ));
    assert!(hlt_should_resume());
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
