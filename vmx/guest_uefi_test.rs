use super::{
    apply_guest_cr4_write, atapi_read_evidence, both_pci_evidence, copy_low_ram_at, dxe_or_cd_boot_evidence,
    exec_from_low_ram, flash_window_gpa_and_pad, guest_cr4_read_shadow, guest_uefi_alive, guest_uefi_atapi,
    guest_uefi_both, guest_uefi_com_bytes, guest_uefi_dxe, guest_uefi_non_tf_exits,
    guest_uefi_past_sec, guest_uefi_vmlaunch_entered, hlt_should_resume, io_port_from_qual,
    is_com_uart_port, is_pci_config_port, last_exit_reason, linear_left_sec_tail,
    live_firmware_alias_gpa, past_sec_evidence, pci_bdf_bit, post_dxe_should_stop,
    run_retained_ovmf_vmlaunch, spin_short_jmp_should_skip, stamp_empty_ovmf_vars,
    preempt_deadloop_should_skip, preempt_deadloop_skip_len, preempt_deadloop_is_assert_epilogue,
    preempt_deadloop_guarded_assert_skip_len, guest_uefi_assert_caller_is_dxe_ram,
    insn_fallthrough_is_leave_ret, assert_deadloop_return_gpa, guest_uefi_cpuid_leaf1_is_uniprocessor,
    guest_uefi_cpuid_has_hypervisor, guest_uefi_cpuid_is_kvm, guest_uefi_filter_cpuid,
    guest_uefi_xapic_is_not_sink, guest_uefi_is_mtrr_msr, guest_uefi_is_misc_enable,
    guest_uefi_misc_enable_read, guest_uefi_misc_enable_write,
    guest_uefi_mtrr_read, guest_uefi_mtrr_reset, guest_uefi_mtrr_write, guest_uefi_mtrr_pci_uc_hole,
    guest_uefi_mtrr_poweron_disabled, guest_uefi_mtrr_valid_var_pairs,
    guest_uefi_phys_bits, guest_uefi_cpuid_80000008_eax, guest_uefi_mtrr_var_mask_sanitize,
    guest_uefi_pf_should_identity_map, guest_uefi_pf_sec_cr3, guest_uefi_cs_ar_is_long, guest_uefi_cr0_is_paging, guest_uefi_efer_with_lma,
    guest_uefi_ia32e_entry_ctls, guest_uefi_is_pcd_database_sig, guest_uefi_is_ldri_sig, is_debugcon_port,
    ud_is_ud2, ud_xsave_family, xsetbv_accepts_xcr, xsetbv_masked_xcr0, E5_OVMF_SEC_CR4_VALUE, E5_OVMF_VMLAUNCH_RESIDUAL_NOTE, GUEST_UEFI_CR4_HOST_OWNED, GUEST_UEFI_CR4_OSXSAVE, GUEST_UEFI_CR4_VMXE, GUEST_UEFI_FEATURE_CONTROL_VALUE, GUEST_UEFI_FLASH_BASE,
    GUEST_UEFI_DEBUGCON_PORT, GUEST_UEFI_DXE_RAM_FLOOR, GUEST_UEFI_EFER_LMA, GUEST_UEFI_EFER_LME, GUEST_UEFI_EFER_NXE, GUEST_UEFI_CR0_PG,
    GUEST_UEFI_IRON_PF_CR2, GUEST_UEFI_MEMFD_BASE, GUEST_UEFI_PF_IDENTITY_CAP,
    GUEST_UEFI_PCD_DATABASE_SIG, GUEST_UEFI_LDRI_SIG, GUEST_UEFI_LDRI_IMAGEBASE_OFF, GUEST_UEFI_VM_ENTRY_IA32E,
    CPUID_80000001_EDX_NX, CPUID_80000001_EDX_PAGE1GB, CPUID_LEAF7_ECX_TME_EN,
    GUEST_UEFI_PHYS_BITS_MAX, GUEST_UEFI_PHYS_BITS_MIN,
    GUEST_UEFI_FLASH_WINDOW, GUEST_UEFI_KVM_CPUID_LEAF, GUEST_UEFI_MISC_ENABLE_DEFAULT,
    GUEST_UEFI_MISC_ENABLE_MSR, GUEST_UEFI_MTRRCAP, GUEST_UEFI_MTRR_DEF_DEFAULT, GUEST_UEFI_MTRR_WB_PACKED, GUEST_UEFI_POST_DXE_TAIL, GUEST_UEFI_RESUME_CAP,
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
    assert_eq!(GUEST_UEFI_RESUME_CAP, 32768);
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("CR4.VMXE host-owned"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("CR4.OSXSAVE host-owned"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0ca02e6"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("#UD intercept XSAVE retry"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("COM1/COM2 forwarded"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("past-SEC"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("GuestVisible"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("CMOS/fw_cfg/i440fx"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("IDE at 00:00.1"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("virtio Header Type is multifunction"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("i440FX host at 00:08.0"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("CF8|CFC byte offset"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("post-DXE spends the 32768-exit cap"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("8192-exit cap ended on CF8"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("HPET 1s on preemption/HLT not PCI I/O"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("8042 KBC"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("ACPI PM 1s step"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("INVPCID/RDTSCP/XSAVES"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("XSETBV executes XCR0"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("etc/boot-menu-wait"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0x109D"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0x6e81ca"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("pause CpuDeadLoop"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("preempt pause/jcc skip"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("preempt eb/jcc32 skip"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("preempt noskip dump"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("891eb5b"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("leave; ret"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("ebecc9c3"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("17449e2"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("ebf3c9c3"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("uniprocessor"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("FEATURE_CONTROL"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("2674629"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("acpi=16612"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("PIIX3 ISA PIRQ"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0x80000838"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("past-PEI/DXE or CD boot attempt"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("empty virtio-blk at 00:00.0"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("fw_cfg bootorder CD then disk"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("drive@0"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("ide@1,1"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("scsi-first skipped IDE Start"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("insn=ebec"));
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
    assert!(!spin_short_jmp_should_skip(0xEB, 0xE0));
    assert!(!spin_short_jmp_should_skip(0xEB, 0x02));
    assert!(!spin_short_jmp_should_skip(0x74, 0xF3));
    assert!(preempt_deadloop_should_skip(0xF3, 0x90));
    assert!(preempt_deadloop_should_skip(0x74, 0xEC));
    assert!(preempt_deadloop_should_skip(0xEB, 0xF3));
    assert!(preempt_deadloop_should_skip(0xEB, 0xFC));
    assert!(preempt_deadloop_should_skip(0xEB, 0xEC));
    assert!(!spin_short_jmp_should_skip(0xEB, 0xFC));
    assert!(!spin_short_jmp_should_skip(0xEB, 0xEC));
    assert!(!preempt_deadloop_should_skip(0x74, 0x02));
    assert!(!preempt_deadloop_should_skip(0x90, 0x90));
    assert!(!spin_short_jmp_should_skip(0xF3, 0x90));
    assert_eq!(preempt_deadloop_skip_len(&[0xF3, 0x90]), 2);
    assert_eq!(preempt_deadloop_skip_len(&[0xEB, 0xFC]), 2);
    assert_eq!(preempt_deadloop_skip_len(&[0xEB, 0xEC, 0xC9, 0xC3]), 0);
    assert_eq!(preempt_deadloop_skip_len(&[0xEB, 0xF3, 0xC9, 0xC3]), 2);
    assert_eq!(GUEST_UEFI_DXE_RAM_FLOOR, 0x10_0000);
    assert!(guest_uefi_assert_caller_is_dxe_ram(0x6e81ca));
    assert!(guest_uefi_assert_caller_is_dxe_ram(0x1d25193));
    assert!(!guest_uefi_assert_caller_is_dxe_ram(0x109D));
    assert!(!guest_uefi_assert_caller_is_dxe_ram(0xffff_fff0));
    assert_eq!(
        preempt_deadloop_guarded_assert_skip_len(&[0xEB, 0xEC, 0xC9, 0xC3], 0x6e81ca, 0x1d25193),
        0
    );
    assert_eq!(
        preempt_deadloop_guarded_assert_skip_len(&[0xEB, 0xEC, 0xC9, 0xC3], 0x109D, 0x1d25193),
        0
    );
    assert_eq!(
        preempt_deadloop_guarded_assert_skip_len(&[0xEB, 0xEC, 0xC9, 0xC3], 0x6e81ca, 0x109D),
        0
    );
    assert_eq!(
        preempt_deadloop_guarded_assert_skip_len(&[0xEB, 0xF3, 0xC9, 0xC3], 0x6e81ca, 0x1d25193),
        0
    );
    assert!(preempt_deadloop_is_assert_epilogue(&[0xEB, 0xEC, 0xC9, 0xC3]));
    assert!(!preempt_deadloop_is_assert_epilogue(&[0xEB, 0xF3, 0xC9, 0xC3]));
    assert!(insn_fallthrough_is_leave_ret(&[0xEB, 0xEC, 0xC9, 0xC3], 2));
    assert!(!preempt_deadloop_is_assert_epilogue(&[0xEB, 0xFC, 0x90, 0x90]));
    assert_eq!(assert_deadloop_return_gpa(0x2000, true), 0x2008);
    assert_eq!(assert_deadloop_return_gpa(0x2000, false), 0x2004);
    assert_eq!(
        preempt_deadloop_skip_len(&[0x0F, 0x84, 0xE8, 0xFF, 0xFF, 0xFF]),
        6
    );
    assert_eq!(
        preempt_deadloop_skip_len(&[0x0F, 0x84, 0xE8, 0xFF, 0xFF, 0xFF, 0xC9, 0xC3]),
        6
    );
    assert_eq!(preempt_deadloop_skip_len(&[0x0F, 0x84, 0x10, 0, 0, 0]), 0);
    assert_eq!(preempt_deadloop_skip_len(&[0x90, 0x90]), 0);
    assert!(xsetbv_accepts_xcr(0));
    assert!(!xsetbv_accepts_xcr(1));
    assert_eq!(xsetbv_masked_xcr0(0, 0x7), 1);
    assert_eq!(xsetbv_masked_xcr0(0x4, 0x7), 0x7);
    assert_eq!(xsetbv_masked_xcr0(0x7, 0x3), 0x3);
    assert_eq!(GUEST_UEFI_FEATURE_CONTROL_VALUE, 1);
    let leaf1 = guest_uefi_filter_cpuid(1, 0);
    assert_eq!(leaf1.ecx & crate::arch::cpu::CPUID_ECX_VMX, 0);
    assert_eq!(leaf1.ecx & crate::arch::cpu::CPUID_ECX_X2APIC, 0);
    assert!(guest_uefi_cpuid_has_hypervisor(leaf1.ecx));
    assert!(guest_uefi_cpuid_leaf1_is_uniprocessor(leaf1.ebx, leaf1.edx));
    let kvm = guest_uefi_filter_cpuid(GUEST_UEFI_KVM_CPUID_LEAF, 0);
    assert!(guest_uefi_cpuid_is_kvm(kvm.ebx, kvm.ecx, kvm.edx));
    assert_eq!(kvm.eax, GUEST_UEFI_KVM_CPUID_LEAF + 1);
    let phys = guest_uefi_filter_cpuid(0x8000_0008, 0);
    let pa = phys.eax & 0xFF;
    assert!(pa >= GUEST_UEFI_PHYS_BITS_MIN && pa <= GUEST_UEFI_PHYS_BITS_MAX);
    assert_eq!(phys.eax >> 16, 0);
    assert_eq!(guest_uefi_phys_bits(32), 36);
    assert_eq!(guest_uefi_phys_bits(46), 46);
    assert_eq!(guest_uefi_phys_bits(52), 48);
    assert_eq!(GUEST_UEFI_IRON_PF_CR2, 0x80B000);
    assert_eq!(GUEST_UEFI_MEMFD_BASE, 0x800000);
    assert_eq!(GUEST_UEFI_PF_IDENTITY_CAP, 256);
    assert!(guest_uefi_pf_should_identity_map(0, GUEST_UEFI_IRON_PF_CR2));
    assert_eq!(guest_uefi_pf_sec_cr3(), GUEST_UEFI_MEMFD_BASE);
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("3311ff3"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("fail=alloc"));
    assert!(!guest_uefi_pf_should_identity_map(1, GUEST_UEFI_IRON_PF_CR2));
    assert!(!guest_uefi_pf_should_identity_map(0, 0xFFFF_0000));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("d5fceb1"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0x80B000"));
    assert_eq!(guest_uefi_cpuid_80000008_eax(0x0030_2E2E), 0x2E2E);
    assert_eq!(
        guest_uefi_mtrr_var_mask_sanitize(0xFFFF_FFFF_F800_0800, 46),
        0x0000_3FFF_F800_0800
    );
    let phys_ecx = guest_uefi_filter_cpuid(0x8000_0008, 0x8000_0008);
    assert_eq!(phys_ecx.eax & 0xFF, phys.eax & 0xFF);
    let ext = guest_uefi_filter_cpuid(0x8000_0001, 0);
    assert_eq!(ext.edx & CPUID_80000001_EDX_NX, 0);
    assert_eq!(ext.edx & CPUID_80000001_EDX_PAGE1GB, 0);
    let leaf7 = guest_uefi_filter_cpuid(7, 0);
    assert_eq!(leaf7.ecx & CPUID_LEAF7_ECX_TME_EN, 0);
    let top = guest_uefi_filter_cpuid(0xB, 0);
    assert_eq!(top.eax, 0);
    assert_eq!(top.ebx, 0);
    assert_eq!(GUEST_UEFI_CR4_VMXE, 1 << 13);
    assert_eq!(GUEST_UEFI_CR4_OSXSAVE, 1 << 18);
    assert_eq!(
        GUEST_UEFI_CR4_HOST_OWNED,
        GUEST_UEFI_CR4_VMXE | GUEST_UEFI_CR4_OSXSAVE
    );
    // Iron 0ca02e6 dump CR4=0x668 (DE+PAE+MCE+OSFXSR+OSXMMEXCPT), no OSXSAVE.
    assert_eq!(apply_guest_cr4_write(0x640) & GUEST_UEFI_CR4_HOST_OWNED, GUEST_UEFI_CR4_HOST_OWNED);
    assert_eq!(apply_guest_cr4_write(0x668) & GUEST_UEFI_CR4_OSXSAVE, GUEST_UEFI_CR4_OSXSAVE);
    assert_eq!(apply_guest_cr4_write(0x668) & GUEST_UEFI_CR4_VMXE, GUEST_UEFI_CR4_VMXE);
    assert_eq!(guest_cr4_read_shadow(apply_guest_cr4_write(0x668)) & GUEST_UEFI_CR4_VMXE, 0);
    assert_ne!(guest_cr4_read_shadow(apply_guest_cr4_write(0x668)) & GUEST_UEFI_CR4_OSXSAVE, 0);
    assert!(ud_xsave_family(&[0x0F, 0xAE, 0x20])); // xsave [rax]  /4
    assert!(ud_xsave_family(&[0x48, 0x0F, 0xAE, 0x21])); // rex.w xsave [rcx]
    assert!(ud_xsave_family(&[0x0F, 0xAE, 0x28])); // xrstor [rax] /5
    assert!(ud_xsave_family(&[0x0F, 0xAE, 0x30])); // xsaveopt [rax] /6
    assert!(ud_xsave_family(&[0x0F, 0xC7, 0x28])); // xsaves [rax] /5
    assert!(ud_xsave_family(&[0x0F, 0xC7, 0x20])); // xsavec [rax] /4
    assert!(ud_xsave_family(&[0x0F, 0xC7, 0x18])); // xrstors [rax] /3
    assert!(!ud_xsave_family(&[0x0F, 0xAE, 0x00])); // fxsave /0
    assert!(!ud_xsave_family(&[0x0F, 0xAE, 0x08])); // fxrstor /1
    assert!(ud_is_ud2(&[0x0F, 0x0B]));
    assert!(!ud_is_ud2(&[0x0F, 0xAE, 0x20]));
    assert!(!ud_xsave_family(&[0x0F, 0x0B]));
    assert_eq!(GUEST_UEFI_POST_DXE_TAIL, GUEST_UEFI_RESUME_CAP);
    assert!(guest_uefi_xapic_is_not_sink());
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("ad78f12"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("xAPIC 4K"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("3f417ca"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("MTRR shadow"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("408788c"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("KVMKVMKVM"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("callerrip"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("8700cbb"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("VCNT=32"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("bootorder NUL"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0b7d647"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("EFER.LMA"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("CR0.PG"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("IA-32e entry"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("debugcon 0x402"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("b4b4847"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("gPcdDataBaseSignatureGuid"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("c40f4a8"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("aee545f"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("10cb881"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("power-on E=0"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("DXE assert skip"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("pcdsig=1"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("a9ffaa5"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("ldri ImageBase"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("EFER.NXE"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("5f59c86"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("lastmsr=0x23f"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("MAXPHYADDR"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("clip-36"));
    assert!(guest_uefi_is_pcd_database_sig(&GUEST_UEFI_PCD_DATABASE_SIG));
    assert!(!guest_uefi_is_pcd_database_sig(&[0u8; 16]));
    assert_eq!(GUEST_UEFI_DEBUGCON_PORT, 0x402);
    assert!(is_debugcon_port(0x402));
    assert!(!is_debugcon_port(0x3f8));
    assert!(!guest_uefi_cs_ar_is_long(0x009B));
    assert!(guest_uefi_cs_ar_is_long(1 << 13));
    assert!(!guest_uefi_cr0_is_paging(1));
    assert!(guest_uefi_cr0_is_paging(GUEST_UEFI_CR0_PG));
    assert_eq!(
        guest_uefi_efer_with_lma(GUEST_UEFI_EFER_LME, false),
        GUEST_UEFI_EFER_LME
    );
    assert_eq!(
        guest_uefi_efer_with_lma(GUEST_UEFI_EFER_LME, true),
        GUEST_UEFI_EFER_LME | GUEST_UEFI_EFER_LMA
    );
    assert_eq!(
        guest_uefi_efer_with_lma(GUEST_UEFI_EFER_LME | GUEST_UEFI_EFER_LMA, false),
        GUEST_UEFI_EFER_LME
    );
    assert_eq!(
        guest_uefi_efer_with_lma(GUEST_UEFI_EFER_LME | GUEST_UEFI_EFER_NXE, true),
        GUEST_UEFI_EFER_LME | GUEST_UEFI_EFER_LMA
    );
    assert_eq!(GUEST_UEFI_EFER_NXE, 1 << 11);
    assert!(guest_uefi_is_ldri_sig(&GUEST_UEFI_LDRI_SIG));
    assert!(!guest_uefi_is_ldri_sig(b"ptal"));
    assert_eq!(GUEST_UEFI_LDRI_IMAGEBASE_OFF, 0x68);
    assert_eq!(
        guest_uefi_ia32e_entry_ctls(0, true),
        GUEST_UEFI_VM_ENTRY_IA32E
    );
    assert_eq!(
        guest_uefi_ia32e_entry_ctls(GUEST_UEFI_VM_ENTRY_IA32E, false),
        0
    );
    assert_eq!(pci_bdf_bit(0, 0), Some((0, 1)));
    assert_eq!(pci_bdf_bit(1, 1), Some((0, 1u64 << 9)));
    assert_eq!(pci_bdf_bit(8, 0), Some((1, 1)));
    assert_eq!(pci_bdf_bit(16, 0), Some((2, 1)));
    assert_eq!(pci_bdf_bit(32, 0), None);
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
fn mtrr_shadow_is_guest_not_host() {
    guest_uefi_mtrr_reset();
    assert!(guest_uefi_is_mtrr_msr(0xFE));
    assert!(guest_uefi_is_mtrr_msr(0x250));
    assert!(guest_uefi_is_mtrr_msr(0x26B));
    assert!(guest_uefi_is_mtrr_msr(0x23F));
    assert!(!guest_uefi_is_mtrr_msr(0x240));
    assert!(!guest_uefi_is_mtrr_msr(0x277));
    assert!(!guest_uefi_is_mtrr_msr(0x1B));
    assert_eq!(guest_uefi_mtrr_read(0xFE), Some(GUEST_UEFI_MTRRCAP));
    assert_eq!(GUEST_UEFI_MTRRCAP & 0xFF, 32u64);
    assert_eq!(guest_uefi_mtrr_read(0x2FF), Some(GUEST_UEFI_MTRR_DEF_DEFAULT));
    assert_eq!(guest_uefi_mtrr_read(0x250), Some(0));
    assert_eq!(guest_uefi_mtrr_read(0x259), Some(0));
    assert!(!guest_uefi_mtrr_pci_uc_hole());
    assert!(guest_uefi_mtrr_poweron_disabled());
    assert_eq!(guest_uefi_mtrr_valid_var_pairs(), 0);
    assert!(guest_uefi_mtrr_write(0x250, GUEST_UEFI_MTRR_WB_PACKED));
    assert_eq!(guest_uefi_mtrr_read(0x250), Some(GUEST_UEFI_MTRR_WB_PACKED));
    assert!(guest_uefi_mtrr_write(0x2FF, 0xC00));
    assert_eq!(guest_uefi_mtrr_read(0x2FF), Some(0xC00));
    assert!(guest_uefi_mtrr_write(0x200, 6));
    assert_eq!(guest_uefi_mtrr_read(0x200), Some(6));
    assert!(guest_uefi_mtrr_write(0x201, 1 << 11));
    assert_eq!(guest_uefi_mtrr_valid_var_pairs(), 1);
    assert!(guest_uefi_mtrr_write(0xFE, 0xFFFF));
    assert_eq!(guest_uefi_mtrr_read(0xFE), Some(GUEST_UEFI_MTRRCAP));
    guest_uefi_mtrr_reset();
    assert_eq!(guest_uefi_mtrr_read(0x2FF), Some(GUEST_UEFI_MTRR_DEF_DEFAULT));
    assert!(!guest_uefi_mtrr_pci_uc_hole());
    assert!(guest_uefi_mtrr_poweron_disabled());
    assert!(guest_uefi_is_misc_enable(GUEST_UEFI_MISC_ENABLE_MSR));
    assert!(!guest_uefi_is_misc_enable(0xFE));
    assert_eq!(
        guest_uefi_misc_enable_read(GUEST_UEFI_MISC_ENABLE_MSR),
        Some(GUEST_UEFI_MISC_ENABLE_DEFAULT)
    );
    assert!(guest_uefi_misc_enable_write(GUEST_UEFI_MISC_ENABLE_MSR, 1));
    assert_eq!(guest_uefi_misc_enable_read(GUEST_UEFI_MISC_ENABLE_MSR), Some(1));
    guest_uefi_mtrr_reset();
    assert_eq!(
        guest_uefi_misc_enable_read(GUEST_UEFI_MISC_ENABLE_MSR),
        Some(GUEST_UEFI_MISC_ENABLE_DEFAULT)
    );
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
