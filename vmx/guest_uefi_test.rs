use super::{
    atapi_read_evidence, both_pci_evidence, copy_low_ram_at, dxe_or_cd_boot_evidence,
    exec_from_low_ram, flash_window_gpa_and_pad, guest_uefi_alive, guest_uefi_atapi,
    guest_uefi_both, guest_uefi_com_bytes, guest_uefi_dxe, guest_uefi_non_tf_exits,
    guest_uefi_past_sec, guest_uefi_vmlaunch_entered, hlt_should_resume, io_port_from_qual,
    is_com_uart_port, is_pci_config_port, last_exit_reason, linear_left_sec_tail,
    live_firmware_alias_gpa, past_sec_evidence, pci_bdf_bit, post_dxe_should_stop,
    run_retained_ovmf_vmlaunch, spin_short_jmp_should_skip, stamp_empty_ovmf_vars,
    E5_OVMF_SEC_CR4_VALUE, E5_OVMF_VMLAUNCH_RESIDUAL_NOTE, GUEST_UEFI_FLASH_BASE,
    GUEST_UEFI_FLASH_WINDOW, GUEST_UEFI_POST_DXE_TAIL, GUEST_UEFI_RESUME_CAP,
    GUEST_UEFI_SEC_TAIL_GPA, M7_E5_OVMF_ALIVE_OK_MARKER, M7_E5_OVMF_ATAPI_OK_MARKER,
    M7_E5_OVMF_BOTH_OK_MARKER, M7_E5_OVMF_CDROM_OK_MARKER, M7_E5_OVMF_DXE_OK_MARKER,
    M7_E5_OVMF_PAST_SEC_OK_MARKER, M7_E5_OVMF_VIRTIO_OK_MARKER, M7_E5_OVMF_VMLAUNCH_OK_MARKER,
    OVMF_VARS_EMPTY_PREFIX, OVMF_VARS_FV_BYTES,
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
    assert_eq!(GUEST_UEFI_RESUME_CAP, 8192);
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("CR4.VMXE host-owned"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("COM1/COM2 forwarded"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("past-SEC"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("GuestVisible"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("CMOS/fw_cfg/i440fx"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("IDE at 00:00.1"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("virtio Header Type is multifunction"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("i440FX host at 00:08.0"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("CF8|CFC byte offset"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("post-DXE spends the 8192-exit cap"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("past-PEI/DXE or CD boot attempt"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("empty virtio-blk at 00:00.0"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("fw_cfg bootorder CD then disk"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("firmware-simultaneous PCI enum"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("not virtio-alone"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("not both-enum-alone"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("ATAPI signature"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("PACKET interrupt-reason"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("HLT skip so DXE can walk PCI"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("CR-access resume"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("ACPI PM timer"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("PIIX4 PM at 00:01.3"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("remap i440FX DID"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("cmp bx"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("8259 PIC RAZ/WI"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("fw_cfg etc/e820"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("exception insn dump"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("4MiB flash window"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("VARS gap at 0xFFC00000"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("empty VARS _FVH"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("live HPET"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("HPET 1s step"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("stop RIP insn dump"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("spin jmp skip"));
    assert!(hlt_should_resume());
    assert!(spin_short_jmp_should_skip(0xEB, 0xF3));
    assert!(spin_short_jmp_should_skip(0xEB, 0xFE));
    assert!(!spin_short_jmp_should_skip(0xEB, 0x02));
    assert!(!spin_short_jmp_should_skip(0x74, 0xF3));
    assert_eq!(GUEST_UEFI_POST_DXE_TAIL, GUEST_UEFI_RESUME_CAP);
    assert_eq!(pci_bdf_bit(0, 0), Some((0, 1)));
    assert_eq!(pci_bdf_bit(1, 1), Some((0, 1u64 << 9)));
    assert_eq!(pci_bdf_bit(8, 0), Some((1, 1)));
    assert_eq!(pci_bdf_bit(16, 0), None);
    assert_eq!(M7_E5_OVMF_CDROM_OK_MARKER, "RAYNU-V-M7-E5-OVMF-CDROM-OK");
    assert_eq!(M7_E5_OVMF_DXE_OK_MARKER, "RAYNU-V-M7-E5-OVMF-DXE-OK");
    assert_eq!(M7_E5_OVMF_VIRTIO_OK_MARKER, "RAYNU-V-M7-E5-OVMF-VIRTIO-OK");
    assert_eq!(M7_E5_OVMF_BOTH_OK_MARKER, "RAYNU-V-M7-E5-OVMF-BOTH-OK");
    assert_eq!(M7_E5_OVMF_ATAPI_OK_MARKER, "RAYNU-V-M7-E5-OVMF-ATAPI-OK");
    assert!(!guest_uefi_alive());
    assert!(!guest_uefi_past_sec());
    assert!(!guest_uefi_dxe());
    assert!(!guest_uefi_both());
    assert!(!guest_uefi_atapi());
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
    assert!(!atapi_read_evidence(0));
    assert!(atapi_read_evidence(1));
    assert!(!post_dxe_should_stop(false, 2000, 0, 1));
    assert!(!post_dxe_should_stop(true, 115, 115, 0));
    assert!(post_dxe_should_stop(true, 115, 115, 1));
    assert!(post_dxe_should_stop(
        true,
        115 + GUEST_UEFI_POST_DXE_TAIL,
        115,
        0
    ));
    assert!(!post_dxe_should_stop(
        true,
        115 + GUEST_UEFI_POST_DXE_TAIL - 1,
        115,
        0
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

#[test]
fn copy_low_ram_at_identity_window() {
    let ram = [0u8, 1, 2, 0xEC, 0x90, 0xFA];
    let mut out = [0u8; 2];
    assert_eq!(copy_low_ram_at(&ram, 3, &mut out), 2);
    assert_eq!(out, [0xEC, 0x90]);
    assert_eq!(copy_low_ram_at(&ram, 100, &mut out), 0);
    assert_eq!(copy_low_ram_at(&ram, 5, &mut out), 1);
    assert_eq!(out[0], 0xFA);
    let mut sixteen = [0u8; 16];
    let ram16: [u8; 20] = [
        0x8b, 0x04, 0x25, 0xf0, 0x00, 0xd0, 0xfe, 0x48, 0x3b, 0xc1, 0x72, 0xf3, 0x90, 0x90, 0x90,
        0x90, 0xcc, 0xcc, 0xcc, 0xcc,
    ];
    assert_eq!(copy_low_ram_at(&ram16, 0, &mut sixteen), 16);
    assert_eq!(&sixteen[..4], &[0x8b, 0x04, 0x25, 0xf0]);
}

#[test]
fn flash_window_pads_code_only_image_to_4mib() {
    assert_eq!(GUEST_UEFI_FLASH_BASE, 0xFFC0_0000);
    assert_eq!(GUEST_UEFI_FLASH_WINDOW, 4 * 1024 * 1024);
    assert_eq!(
        flash_window_gpa_and_pad(3653632),
        Some((GUEST_UEFI_FLASH_BASE, 0x84000))
    );
    assert_eq!(
        flash_window_gpa_and_pad(GUEST_UEFI_FLASH_WINDOW),
        Some((GUEST_UEFI_FLASH_BASE, 0))
    );
    assert_eq!(
        flash_window_gpa_and_pad(1024 * 1024),
        Some((GUEST_UEFI_FLASH_BASE, 3 * 1024 * 1024))
    );
    assert!(flash_window_gpa_and_pad(4096).is_none());
    let (gpa, pad) = flash_window_gpa_and_pad(3653632).unwrap();
    assert!(alias_ept_covers_reset(gpa, GUEST_UEFI_FLASH_WINDOW));
    assert_eq!(gpa + pad, 0xFFC8_4000);
    assert!(frames_required_firmware_alias(gpa, GUEST_UEFI_FLASH_WINDOW) <= 8);
}

#[test]
fn stamp_empty_ovmf_vars_matches_debian_4m_template() {
    let mut wrong = vec![0xFFu8; 4096];
    assert!(!stamp_empty_ovmf_vars(&mut wrong));
    let mut pad = vec![0xFFu8; OVMF_VARS_FV_BYTES];
    assert!(stamp_empty_ovmf_vars(&mut pad));
    assert_eq!(&pad[0x28..0x2C], b"_FVH");
    assert_eq!(
        u64::from_le_bytes(pad[0x20..0x28].try_into().unwrap()),
        0x84000
    );
    assert_eq!(
        &pad[..OVMF_VARS_EMPTY_PREFIX.len()],
        &OVMF_VARS_EMPTY_PREFIX
    );
    assert!(pad[OVMF_VARS_EMPTY_PREFIX.len()..]
        .iter()
        .all(|&b| b == 0xFF));
    let hdrlen = u16::from_le_bytes(pad[0x30..0x32].try_into().unwrap()) as usize;
    assert_eq!(hdrlen, 0x48);
    let mut sum: u32 = 0;
    for i in (0..hdrlen).step_by(2) {
        sum = sum.wrapping_add(u16::from_le_bytes([pad[i], pad[i + 1]]) as u32);
    }
    assert_eq!(sum & 0xffff, 0);
    assert_eq!(pad[0x5C], 0x5A);
    assert_eq!(pad[0x5D], 0xFE);
    assert_eq!(
        u32::from_le_bytes(pad[0x58..0x5C].try_into().unwrap()),
        0x3ffb8
    );
}
