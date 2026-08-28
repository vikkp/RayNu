use super::{
    apply_guest_cr4_write, atapi_read_evidence, both_pci_evidence, copy_flash_at, copy_low_ram_at, dxe_or_cd_boot_evidence,
    xapic_fetch_miss_eax_fallback,
    guest_uefi_insn_linear,
    guest_uefi_mmio_peek_linear,
    guest_uefi_mmio_skip_len,
    eltorito_boot_evidence, eltorito_com_match_step, eltorito_payload_ran, exec_from_low_ram, flash_window_gpa_and_pad, guest_cr4_read_shadow, guest_uefi_alive, guest_uefi_atapi,
    guest_uefi_both, guest_uefi_com_bytes, guest_uefi_dxe, guest_uefi_eltorito, guest_uefi_non_tf_exits,
    guest_uefi_past_sec, guest_uefi_vmlaunch_entered, hlt_should_resume, io_port_from_qual,
    is_com_uart_port, is_pci_config_port, last_exit_reason, linear_left_sec_tail,
    live_firmware_alias_gpa, past_sec_evidence, pci_bdf_bit, post_atapi_should_stop,
    post_dxe_should_stop,
    run_retained_ovmf_vmlaunch, spin_short_jmp_should_skip, stamp_empty_ovmf_vars,
    cr_access_is_cr8,
    preempt_deadloop_should_skip, preempt_deadloop_skip_len, preempt_deadloop_is_assert_epilogue,
    preempt_deadloop_guarded_assert_skip_len, guest_uefi_assert_caller_is_dxe_ram,
    insn_fallthrough_is_leave_ret, assert_deadloop_return_gpa, guest_uefi_cpuid_leaf1_is_uniprocessor,
    guest_uefi_cpuid_has_hypervisor, guest_uefi_cpuid_is_kvm, guest_uefi_filter_cpuid,
    guest_uefi_xapic_is_not_sink, guest_uefi_is_mtrr_msr, guest_uefi_is_misc_enable,
    guest_uefi_misc_enable_read, guest_uefi_misc_enable_write,
    guest_uefi_mtrr_read, guest_uefi_mtrr_reset, guest_uefi_mtrr_write, guest_uefi_mtrr_pci_uc_hole,
    guest_uefi_mtrr_poweron_disabled, guest_uefi_mtrr_valid_var_pairs, guest_uefi_mtrr_uc_hole_live,
    guest_uefi_mtrr_set_admit_uc, guest_uefi_mtrr_uc_held,
    guest_uefi_mtrr_fixed_is_vga_hole, GUEST_UEFI_MTRR_UC_PACKED,
    guest_uefi_phys_bits, guest_uefi_gpa0_fixed_mtrr_split, guest_uefi_gpa0_split_now, guest_uefi_cpuid_80000008_eax, guest_uefi_mtrr_var_mask_sanitize,
    guest_uefi_flash_off, guest_uefi_gpa_to_hpa,
    try_alloc_product_iso_install_disk,
    guest_uefi_pf_should_identity_map, guest_uefi_pf_sec_cr3, guest_uefi_pf_should_load_sec_cr3, guest_uefi_pf_should_rebuild_sec_cr3, guest_uefi_pf_error_is_reserved, guest_uefi_pf_should_map_mmio, guest_uefi_pf_gpa32, guest_uefi_mmio_needs_scratch, guest_uefi_report_ram_should_map, guest_uefi_report_ram_gpa_2m, guest_uefi_report_ram_page_off, copy_report_ram_at, store_report_ram_at, load_report_ram_at, guest_uefi_ept_scratch_on_qual, guest_uefi_ept_qual_is_walk, guest_uefi_ept_qual_is_fetch, guest_uefi_ept_hole_ro_on_qual, guest_uefi_ept_hole_ro_allows_execute, guest_uefi_rip_is_hole_execute, guest_uefi_hole_ro_uses_dedicated_zero, guest_uefi_insn_is_poison_fill, guest_uefi_pf_should_split_ram_1g, guest_uefi_pde_is_large, guest_uefi_pde_is_poison, guest_uefi_pf_should_fix_ram_wp, guest_uefi_pf_split4k_resume_already_rw, guest_uefi_pf_error_is_present_write, guest_uefi_io_qual_is_string, guest_uefi_io_qual_is_rep, guest_uefi_io_string_count, guest_uefi_io_string_advance, guest_uefi_io_string_fills_ram, guest_uefi_io_addr_reg, store_low_ram_at, load_low_ram_at, guest_uefi_cs_ar_is_long, guest_uefi_cr0_is_paging, guest_uefi_efer_with_lma,
    guest_uefi_ia32e_entry_ctls, guest_uefi_is_pcd_database_sig, guest_uefi_is_ldri_sig, is_debugcon_port,
    ia32_pat_memory_type, IA32_PAT_RESET,
    ud_is_ud2, ud_xsave_family, xsetbv_accepts_xcr, xsetbv_masked_xcr0, e4_restore_xcr0_value, e4_restore_cr4_osxsave, E5_OVMF_SEC_CR4_VALUE, E5_OVMF_VMLAUNCH_RESIDUAL_NOTE, GUEST_UEFI_CR4_HOST_OWNED, GUEST_UEFI_CR4_OSXSAVE, GUEST_UEFI_CR4_VMXE, GUEST_UEFI_FEATURE_CONTROL_VALUE, GUEST_UEFI_FLASH_BASE,
    GUEST_UEFI_DEBUGCON_PORT, GUEST_UEFI_DXE_RAM_FLOOR, GUEST_UEFI_EFER_LMA, GUEST_UEFI_EFER_LME, GUEST_UEFI_EFER_NXE, GUEST_UEFI_CR0_PG,
    GUEST_UEFI_IRON_EPT_PCI_HOLE_GPA, GUEST_UEFI_IRON_PF_CR2, GUEST_UEFI_IRON_PF_RSVD_CR2, GUEST_UEFI_IRON_PF_HEAP_CR2, GUEST_UEFI_IRON_PF_HEAP_WR_CR2, GUEST_UEFI_IRON_PF_POISON_CR2, GUEST_UEFI_IRON_PF_POISON_PDE, GUEST_UEFI_IRON_PF_MTRR_UC_CR2, GUEST_UEFI_IRON_PF_SIGNEXT_CR2, GUEST_UEFI_IRON_PF_TRUNC32_CR2, GUEST_UEFI_IRON_MMIO_SCRATCH_GPA, GUEST_UEFI_IRON_SINK_PT_GPA, GUEST_UEFI_IRON_SCRATCH_CAP_GPA, GUEST_UEFI_IRON_SCRATCH_WALK_GPA, GUEST_UEFI_IRON_SCRATCH_FETCH_WALK_GPA, GUEST_UEFI_IRON_EPT_QUAL_FETCH_WALK, GUEST_UEFI_IRON_EPT_QUAL_AD_WALK, GUEST_UEFI_IRON_HOLE_RO_HPET_RIP, GUEST_UEFI_IRON_HOLE_X_RIP, GUEST_UEFI_IRON_ZERO_FILL_RIP, GUEST_UEFI_IRON_PF_WP_CR2, GUEST_UEFI_IRON_PF_WP_RIP, GUEST_UEFI_IRON_PF_WP_ERR, GUEST_UEFI_IRON_PF_WP_PDE, GUEST_UEFI_IRON_PF_WP_SPLIT_PDE, GUEST_UEFI_IRON_PF_WP_PML4E_RO, GUEST_UEFI_IRON_PF_XAPIC_CR2, GUEST_UEFI_IRON_PF_XAPIC_ERR, GUEST_UEFI_IRON_PF_XAPIC_PDPTE, GUEST_UEFI_IRON_PF_XAPIC_RIP, GUEST_UEFI_IO_QUAL_REP_INSW_1F0, GUEST_UEFI_IO_STRING_CAP, GUEST_UEFI_HV_PML4, GUEST_UEFI_MEMFD_BASE, GUEST_UEFI_MMIO_SCRATCH_SLOTS, GUEST_UEFI_REPORT_RAM_SLOTS, GUEST_UEFI_IRON_REPORT_RAM_GPA, GUEST_UEFI_EPT_MT_WB, GUEST_UEFI_IRON_HIGH_DEADLOOP_RIP, GUEST_UEFI_PF_IDENTITY_CAP, GUEST_UEFI_PF_ERR_RSVD,
    GUEST_UEFI_PCD_DATABASE_SIG, GUEST_UEFI_LDRI_SIG, GUEST_UEFI_LDRI_IMAGEBASE_OFF, GUEST_UEFI_VM_ENTRY_IA32E,
    CPUID_80000001_EDX_NX, CPUID_80000001_EDX_PAGE1GB, CPUID_LEAF7_ECX_TME_EN, CPUID_LEAF7_ECX_LA57,
    GUEST_UEFI_PHYS_BITS_MAX, GUEST_UEFI_PHYS_BITS_MIN, GUEST_UEFI_PHYS_BITS_IRON_CAP,
    GUEST_UEFI_FLASH_WINDOW, GUEST_UEFI_KVM_CPUID_LEAF, GUEST_UEFI_MISC_ENABLE_DEFAULT,
    GUEST_UEFI_MISC_ENABLE_MSR, GUEST_UEFI_MTRRCAP, GUEST_UEFI_MTRR_DEF_DEFAULT, GUEST_UEFI_MTRR_WB_PACKED, GUEST_UEFI_POST_ATAPI_TAIL, GUEST_UEFI_POST_DXE_TAIL, GUEST_UEFI_RESUME_CAP, GUEST_UEFI_NESTED_RESUME_CAP, GUEST_UEFI_PRODUCT_ISO_RESUME_CAP, guest_uefi_resume_cap, report_ram_return_to_e4, eltorito_stops_guest_uefi,
    GUEST_UEFI_SEC_TAIL_GPA, M7_E5_OVMF_ALIVE_OK_MARKER, M7_E5_OVMF_ATAPI_OK_MARKER,
    M7_E5_OVMF_BOTH_OK_MARKER, M7_E5_OVMF_CDROM_OK_MARKER, M7_E5_OVMF_DXE_OK_MARKER,
    M7_E5_OVMF_ELTORITO_OK_MARKER, M7_E5_OVMF_PAST_SEC_OK_MARKER, M7_E5_OVMF_VIRTIO_OK_MARKER,
    M7_E5_OVMF_VMLAUNCH_OK_MARKER,
    OVMF_VARS_EMPTY_PREFIX, OVMF_VARS_FV_BYTES,
};
use super::{
    guest_uefi_pt_pml4e_gpa, guest_uefi_pt_walk_pde, guest_uefi_pt_walk_pdpte, guest_uefi_pt_walk_pml4e,
    guest_uefi_pt_walk_pte, guest_uefi_pt_paint_live_uc_hole, guest_uefi_pt_pde_is_wb_hole,
    guest_uefi_pt_pde_pat_uc, guest_uefi_pt_split_gpa0, guest_uefi_pt_pde0_is_2m,
    guest_uefi_gpa0_split_pt_gpa, store_report_ram_u64,
    GUEST_UEFI_IRON_ASSERT_CALLER_RIP, GUEST_UEFI_ASSERT_PREHEX_BYTES, guest_uefi_assert_prehex_gpa,
    guest_uefi_assert_retcmp_gpa, guest_uefi_assert_retpre_word_gpa,
    GUEST_UEFI_IRON_HIGH_CR3, GUEST_UEFI_PT_ADDR_MASK,
    GUEST_UEFI_PT_PRESENT, GUEST_UEFI_IRON_PDE8000_WB, GUEST_UEFI_PT_LARGE_2M_UC,
    GUEST_UEFI_IRON_PDE0_2M, GUEST_UEFI_PT_LEAF_4K, GUEST_UEFI_PT_LEAF_4K_UC, GUEST_UEFI_PT_TABLE,
    guest_uefi_patch_cpu_flush_unsupported, guest_uefi_count_cpu_flush_jnz,
    guest_uefi_pt_paint_vga_uc, guest_uefi_pt_leaf_4k_for, guest_uefi_gpa_in_vga_fix_uc,
    GUEST_UEFI_CPU_FLUSH_UNSUPPORTED, GUEST_UEFI_CPU_FLUSH_JNZ_OFF, GUEST_UEFI_IRON_CPU_FLUSH_GPA,
    GUEST_UEFI_IRON_PTE_A0000_WB,
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
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("SectionAlignment 0x1000"));
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
    assert_eq!(GUEST_UEFI_RESUME_CAP, 262144);
    assert_eq!(GUEST_UEFI_NESTED_RESUME_CAP, 65536);
    assert_eq!(GUEST_UEFI_PRODUCT_ISO_RESUME_CAP, 16_777_216);
    assert_eq!(GUEST_UEFI_REPORT_RAM_SLOTS, 32);
    assert_eq!(super::GUEST_UEFI_REPORT_RAM_PRODUCT_EXTRA, 224);
    assert_eq!(guest_uefi_resume_cap(false), 262144);
    assert_eq!(guest_uefi_resume_cap(true), 65536);
    assert!(GUEST_UEFI_NESTED_RESUME_CAP > 30769);
    assert!(
        !report_ram_return_to_e4(true),
        "nested KVM must withhold report-RAM from E4 bzImage"
    );
    assert!(
        report_ram_return_to_e4(false),
        "iron still returns zeroed report-RAM to E4"
    );
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
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("KeyboardWaitForValue"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("c19b91f"));
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
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("empty virtio-blk at 00:02.0"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("product ISO virtio-pci queues gated on window"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("fw_cfg bootorder CD then disk"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("drive@0"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("ide@1,1"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("scsi@2 not scsi@0"));
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
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("Stage 46 product ISO PIC/IOAPIC inject"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("Stage 46 product ISO 16550 + ttyS0 cmdline"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("Stage 46 product ISO SOL RX to guest COM1"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("Stage 46 product ISO Alpine serial auto-answer"));
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
    assert_eq!(e4_restore_xcr0_value(0, false, 0x7), 1);
    assert_eq!(e4_restore_xcr0_value(0x7, true, 0x7), 0x7);
    assert_eq!(e4_restore_xcr0_value(0x4, true, 0x7), 0x7);
    assert_eq!(e4_restore_cr4_osxsave(0x640, false), 0x640);
    assert_eq!(
        e4_restore_cr4_osxsave(0x640, true),
        0x640 | GUEST_UEFI_CR4_OSXSAVE
    );
    assert_eq!(
        e4_restore_cr4_osxsave(0x640 | GUEST_UEFI_CR4_OSXSAVE, false),
        0x640
    );
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("73ed589"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("restore host XCR0"));
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
    assert!(pa >= GUEST_UEFI_PHYS_BITS_IRON_CAP && pa <= GUEST_UEFI_PHYS_BITS_MAX);
    assert_eq!(phys.eax >> 16, 0);
    assert_eq!(guest_uefi_phys_bits(32), 36);
    assert_eq!(guest_uefi_phys_bits(40), 40);
    assert_eq!(guest_uefi_phys_bits(46), GUEST_UEFI_PHYS_BITS_IRON_CAP);
    assert_eq!(guest_uefi_phys_bits(52), GUEST_UEFI_PHYS_BITS_IRON_CAP);
    assert_eq!(GUEST_UEFI_PHYS_BITS_MIN, 36);
    assert_eq!(GUEST_UEFI_IRON_PF_CR2, 0x80B000);
    assert_eq!(GUEST_UEFI_MEMFD_BASE, 0x800000);
    assert_eq!(GUEST_UEFI_HV_PML4, 0x200000);
    assert_eq!(
        GUEST_UEFI_HV_PML4,
        crate::devices::guest_platform::HV_IDENTITY_PML4
    );
    assert_eq!(GUEST_UEFI_PF_IDENTITY_CAP, 256);
    assert!(guest_uefi_pf_should_identity_map(0, GUEST_UEFI_IRON_PF_CR2));
    assert_eq!(guest_uefi_pf_sec_cr3(), GUEST_UEFI_HV_PML4);
    assert_ne!(guest_uefi_pf_sec_cr3(), GUEST_UEFI_MEMFD_BASE);
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("3311ff3"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("fail=alloc"));
    assert!(guest_uefi_pf_should_load_sec_cr3(0));
    assert!(!guest_uefi_pf_should_load_sec_cr3(GUEST_UEFI_MEMFD_BASE));
    assert!(guest_uefi_pf_should_rebuild_sec_cr3(GUEST_UEFI_HV_PML4));
    assert!(guest_uefi_pf_should_rebuild_sec_cr3(GUEST_UEFI_MEMFD_BASE));
    assert!(!guest_uefi_pf_should_rebuild_sec_cr3(0));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("7ea62ea"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("fail=present"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("13e8bd2"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("rebuild SEC 4G"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("hide LA57"));
    assert!(guest_uefi_pf_error_is_reserved(0x9));
    assert!(!guest_uefi_pf_error_is_reserved(0));
    assert_eq!(GUEST_UEFI_IRON_PF_RSVD_CR2, 0xA027C8);
    assert_eq!(GUEST_UEFI_PF_ERR_RSVD, 8);
    assert!(guest_uefi_pf_should_identity_map(0x9, GUEST_UEFI_IRON_PF_RSVD_CR2));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0xa027c8"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("err=0x9"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("101b8ec"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0x1ae7078"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0x30646870"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0x200000"));
    assert_eq!(GUEST_UEFI_IRON_PF_HEAP_CR2, 0x1AE7078);
    assert!(guest_uefi_pf_should_identity_map(0, GUEST_UEFI_IRON_PF_HEAP_CR2));
    assert!(!guest_uefi_pf_should_identity_map(1, GUEST_UEFI_IRON_PF_CR2));
    assert!(guest_uefi_pf_should_identity_map(0, 0xFFFF_0000));
    assert_eq!(GUEST_UEFI_IRON_PF_MTRR_UC_CR2, 0x8000_0008);
    assert!(guest_uefi_pf_error_is_reserved(0xb));
    assert!(!guest_uefi_pf_should_identity_map(0xb, GUEST_UEFI_IRON_PF_MTRR_UC_CR2));
    assert!(guest_uefi_pf_should_map_mmio(0xb, GUEST_UEFI_IRON_PF_MTRR_UC_CR2));
    assert!(!guest_uefi_pf_should_identity_map(0, GUEST_UEFI_IRON_PF_MTRR_UC_CR2));
    assert_eq!(GUEST_UEFI_IRON_PF_SIGNEXT_CR2, 0xFFFF_FFFF_9680_8086);
    assert!(guest_uefi_pf_should_map_mmio(0x2, GUEST_UEFI_IRON_PF_SIGNEXT_CR2));
    assert!(!guest_uefi_pf_should_identity_map(0x2, GUEST_UEFI_IRON_PF_SIGNEXT_CR2));
    assert_eq!(guest_uefi_pf_gpa32(GUEST_UEFI_IRON_PF_SIGNEXT_CR2), 0x9680_8086);
    assert_eq!(
        crate::devices::guest_platform::HV_IDENTITY_PML4_BYTES,
        crate::vmx::guest_pt::IDENTITY_RESERVED_BYTES
    );
    assert_eq!(crate::vmx::guest_pt::IDENTITY_4G_BYTES, 0xB000);
    assert_eq!(crate::vmx::guest_pt::IDENTITY_RESERVED_BYTES, 0x1B000);
    assert_eq!(crate::vmx::guest_pt::IDENTITY_SPLIT_PT_PAGES, 16);
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("eb4b27d"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0x80000008"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0xc0400083"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("73576cc"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("a428202"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("identity MMIO fail"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("124c1a8"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0xffffffff96808086"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("b25d75b"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0x301093"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("577c9eb"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0x9896808086"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0xc0200000"));
    assert_eq!(GUEST_UEFI_IRON_MMIO_SCRATCH_GPA, 0x8000_0000);
    assert_eq!(GUEST_UEFI_IRON_SINK_PT_GPA, 0xC020_0000);
    assert_eq!(GUEST_UEFI_IRON_PF_TRUNC32_CR2, 0x0000_0098_9680_8086);
    assert!(guest_uefi_mmio_needs_scratch(0x8000_0008));
    assert!(guest_uefi_mmio_needs_scratch(0xC020_0000));
    assert!(guest_uefi_mmio_needs_scratch(GUEST_UEFI_IRON_PF_SIGNEXT_CR2));
    assert!(guest_uefi_mmio_needs_scratch(GUEST_UEFI_IRON_PF_TRUNC32_CR2));
    assert!(!guest_uefi_mmio_needs_scratch(0xFED0_0000));
    assert!(!guest_uefi_mmio_needs_scratch(0xFEC0_0000));
    assert_eq!(guest_uefi_pf_gpa32(GUEST_UEFI_IRON_PF_TRUNC32_CR2), 0x9680_8086);
    assert!(guest_uefi_pf_should_map_mmio(0x2, GUEST_UEFI_IRON_PF_TRUNC32_CR2));
    assert!(guest_uefi_insn_is_poison_fill(0xAF, 0xAF, 0xAF, 0xAF));
    assert!(!guest_uefi_insn_is_poison_fill(0x4D, 0x85, 0xC9, 0x74));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("471391f"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("d757a0a"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0xafafafafafafafaf"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("identity_refill_low4g_pd"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0bad45d"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0xc0c00000"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0xCFFF9E"));
    assert_eq!(GUEST_UEFI_MMIO_SCRATCH_SLOTS, 32);
    assert_eq!(GUEST_UEFI_IRON_SCRATCH_CAP_GPA, 0xC0C0_0000);
    assert!(guest_uefi_mmio_needs_scratch(GUEST_UEFI_IRON_SCRATCH_CAP_GPA));
    assert_eq!(GUEST_UEFI_IRON_SCRATCH_WALK_GPA, 0xC3C0_0000);
    assert!(guest_uefi_mmio_needs_scratch(GUEST_UEFI_IRON_SCRATCH_WALK_GPA));
    assert!(guest_uefi_ept_scratch_on_qual(2));
    assert!(!guest_uefi_ept_scratch_on_qual(1));
    assert!(!guest_uefi_ept_scratch_on_qual(4));
    assert!(!guest_uefi_ept_scratch_on_qual(GUEST_UEFI_IRON_EPT_QUAL_FETCH_WALK));
    assert!(guest_uefi_ept_qual_is_walk(GUEST_UEFI_IRON_EPT_QUAL_FETCH_WALK));
    assert_eq!(GUEST_UEFI_IRON_SCRATCH_FETCH_WALK_GPA, 0xC3E0_0000);
    assert!(guest_uefi_mmio_needs_scratch(GUEST_UEFI_IRON_SCRATCH_FETCH_WALK_GPA));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("5837243"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0xc3c00000"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0x3d00001"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("guest_uefi_ept_scratch_on_qual"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("EPT hole ro"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("da2c9c4"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0xc3e00000"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0x184"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0x3dfffff"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("data-write only"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("f93caee"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0x1ab"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("dedicated zero"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("poison fill"));
    assert!(guest_uefi_hole_ro_uses_dedicated_zero(0x3C0_0000, 0x380_0000));
    assert!(!guest_uefi_hole_ro_uses_dedicated_zero(0x380_0000, 0x380_0000));
    assert!(!guest_uefi_hole_ro_uses_dedicated_zero(0, 0x380_0000));
    assert!(guest_uefi_ept_scratch_on_qual(GUEST_UEFI_IRON_EPT_QUAL_AD_WALK));
    assert_eq!(GUEST_UEFI_IRON_HOLE_RO_HPET_RIP, 0x300001);
    assert_eq!(GUEST_UEFI_IRON_PF_WP_CR2, 0x1D1_ABB8);
    assert_eq!(GUEST_UEFI_IRON_PF_WP_RIP, 0x1DE_592);
    assert_eq!(GUEST_UEFI_IRON_PF_WP_ERR, 0x3);
    assert_eq!(GUEST_UEFI_IRON_PF_WP_PDE, 0x1C0_00E7);
    assert_eq!(GUEST_UEFI_IRON_PF_WP_SPLIT_PDE, 0x219067);
    assert!(!guest_uefi_pf_split4k_resume_already_rw());
    assert!(guest_uefi_pf_error_is_present_write(GUEST_UEFI_IRON_PF_WP_ERR));
    assert!(guest_uefi_pf_should_fix_ram_wp(
        GUEST_UEFI_IRON_PF_WP_ERR,
        GUEST_UEFI_IRON_PF_WP_CR2
    ));
    assert!(!guest_uefi_pf_should_identity_map(
        GUEST_UEFI_IRON_PF_WP_ERR,
        GUEST_UEFI_IRON_PF_WP_CR2
    ));
    assert!(!guest_uefi_pf_should_fix_ram_wp(0x3, 0x8000_0008));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("06b011a"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0x1d1abb8"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0x1de592"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0x1c000e7"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("SPLIT4K"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("rep insw"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("ataio=236"));
    assert!(guest_uefi_io_qual_is_string(GUEST_UEFI_IO_QUAL_REP_INSW_1F0));
    assert!(guest_uefi_io_qual_is_rep(GUEST_UEFI_IO_QUAL_REP_INSW_1F0));
    assert_eq!(io_port_from_qual(GUEST_UEFI_IO_QUAL_REP_INSW_1F0), 0x1F0);
    assert_eq!(guest_uefi_io_string_count(GUEST_UEFI_IO_QUAL_REP_INSW_1F0, 256), 256);
    assert_eq!(guest_uefi_io_string_count(GUEST_UEFI_IO_QUAL_REP_INSW_1F0, 0), 0);
    assert_eq!(
        guest_uefi_io_string_count(GUEST_UEFI_IO_QUAL_REP_INSW_1F0, GUEST_UEFI_IO_STRING_CAP + 1),
        GUEST_UEFI_IO_STRING_CAP
    );
    assert_eq!(guest_uefi_io_string_count(0x1F00008, 256), 1);
    assert_eq!(guest_uefi_io_string_advance(0x1000, 2, false), 0x1002);
    assert_eq!(guest_uefi_io_string_advance(0x1000, 2, true), 0x0FFE);
    assert!(guest_uefi_io_string_fills_ram(0x1F0));
    assert!(guest_uefi_io_string_fills_ram(0x170));
    assert!(!guest_uefi_io_string_fills_ram(0x1F7));
    assert!(!guest_uefi_io_string_fills_ram(0x511));
    assert!(!guest_uefi_io_string_fills_ram(0xCF8));
    crate::devices::ide_cdrom::reset();
    assert!(crate::devices::ide_cdrom::present_placeholder());
    crate::devices::ide_cdrom::pci_write_addr(crate::devices::ide_cdrom::pci_config_addr() | 0x10);
    crate::devices::ide_cdrom::pci_write_data(0xCFC, 4, 0xC000);
    assert!(guest_uefi_io_string_fills_ram(0xC000));
    assert!(!guest_uefi_io_string_fills_ram(0xC007));
    crate::devices::ide_cdrom::reset();
    assert_eq!(guest_uefi_io_addr_reg(0x1_0000_1234, false), 0x1234);
    assert_eq!(guest_uefi_io_addr_reg(0x1_0000_1234, true), 0x1_0000_1234);
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("1e0f4a7"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0x511"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0x205f18"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("ATA-only"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0x1f21193"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0x1dd97d3"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0x3d2be4"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("54a8708"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0x219067"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("already-RW"));
    assert_eq!(GUEST_UEFI_IRON_HOLE_X_RIP, 0x27E_22D5);
    assert_eq!(GUEST_UEFI_IRON_ZERO_FILL_RIP, 0x3ED0_0001);
    assert!(guest_uefi_rip_is_hole_execute(GUEST_UEFI_IRON_HOLE_X_RIP));
    assert!(guest_uefi_rip_is_hole_execute(GUEST_UEFI_IRON_ZERO_FILL_RIP));
    assert!(!guest_uefi_rip_is_hole_execute(0x1DF1B7));
    assert!(!guest_uefi_ept_hole_ro_allows_execute());
    assert!(guest_uefi_ept_qual_is_fetch(GUEST_UEFI_IRON_EPT_QUAL_FETCH_WALK));
    assert!(guest_uefi_ept_hole_ro_on_qual(GUEST_UEFI_IRON_EPT_QUAL_FETCH_WALK));
    assert!(!guest_uefi_ept_hole_ro_on_qual(4));
    assert!(guest_uefi_ept_hole_ro_on_qual(1));
    assert!(!guest_uefi_pf_should_map_mmio(0, GUEST_UEFI_IRON_HOLE_X_RIP));
    assert!(!guest_uefi_pf_should_map_mmio(0, GUEST_UEFI_IRON_ZERO_FILL_RIP));
    assert!(!guest_uefi_pf_should_map_mmio(0, 0x3EE0_0000));
    assert!(guest_uefi_pf_should_map_mmio(0, GUEST_UEFI_IRON_EPT_PCI_HOLE_GPA));
    assert!(guest_uefi_pf_should_map_mmio(0xb, GUEST_UEFI_IRON_PF_MTRR_UC_CR2));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("19b0c11"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0x27e22d5"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0x3ed00001"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("R only"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("preemption while RIP"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("89c3731"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0x219027"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("walk R/W"));
    assert_eq!(GUEST_UEFI_IRON_PF_WP_PML4E_RO, 0x5A6D);
    assert_eq!(GUEST_UEFI_IRON_PF_XAPIC_CR2, 0xFEE0_0020);
    assert_eq!(GUEST_UEFI_IRON_PF_XAPIC_ERR, 0x9);
    assert_eq!(GUEST_UEFI_IRON_PF_XAPIC_PDPTE, 0xC060_0083);
    assert_eq!(GUEST_UEFI_IRON_PF_XAPIC_RIP, 0x1D8_4C7);
    assert!(guest_uefi_pf_error_is_reserved(GUEST_UEFI_IRON_PF_XAPIC_ERR));
    assert!(guest_uefi_pf_should_map_mmio(
        GUEST_UEFI_IRON_PF_XAPIC_ERR,
        GUEST_UEFI_IRON_PF_XAPIC_CR2
    ));
    assert!(!guest_uefi_pf_should_identity_map(
        GUEST_UEFI_IRON_PF_XAPIC_ERR,
        GUEST_UEFI_IRON_PF_XAPIC_CR2
    ));
    assert!(!guest_uefi_pf_should_fix_ram_wp(
        GUEST_UEFI_IRON_PF_XAPIC_ERR,
        GUEST_UEFI_IRON_PF_XAPIC_CR2
    ));
    assert!(guest_uefi_pde_is_large(GUEST_UEFI_IRON_PF_XAPIC_PDPTE));
    assert!(!guest_uefi_rip_is_hole_execute(GUEST_UEFI_IRON_PF_XAPIC_RIP));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("7413554"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0xfee00020"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0xc0600083"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0x5a6d"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("map_mmio xAPIC"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("32ee302"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("mtrr1=0x3fff80000800"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0x1bdd7d3"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("PAT-UC PCD+PWT"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("48c598a"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("ataio=1308"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("SET FEATURES"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("edc9c3"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("pdpte2"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("sibling 1GiB"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("73ed589"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("restore host XCR0"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("identity_split_mtrr_uc_hole"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("software-walks"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("8df2793"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("pde8000"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("PAT-UC 2-4GiB hole"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("d7bfb23"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("identity_sync_live_mtrr_uc_hole"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("pdpte3"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("1de9389"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0x205067"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("44c56db"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("pat=0x0"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0x0007040600070406"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("HOST_IA32_PAT"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("1a93cb8"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("OSFXSR+OSXMMEXCPT"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("ab25682"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0x8400276"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("emulate MOV CR4"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("PAT WB proved"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("guest PT WB"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("pde20"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0x7010600070406"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("28f42d2"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("identity_ensure_pdpt_2m"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("pde4000"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("be1b028"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("identity_clear_table_pwt_pcd"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("cap iron MAXPHYADDR 32"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("162809f"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("identity_refill_low4g_pd_keep_4k"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("1b587dd"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("identity_split_gpa0_fixed_mtrr"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("1MiB fixed-MTRR"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("TABLE_FLAGS USER"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("659e7de"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("mmio 2m keeps 4K tables"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("61f84c6"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("guest_uefi_gpa0_fixed_mtrr_split"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("84171aa"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0x83 to 0xE7"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("guest_uefi_gpa0_split_now"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("5811368"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("489d118"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("PCI UC [2GiB,4GiB)"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("22e0cb2"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("mid-gap"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("mixed MTRR disproved"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("f9a08c9"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("LowMemory 2GiB"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("fad19b2"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0x7bddd000"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("lazy 2MiB WB"));
    assert_eq!(GUEST_UEFI_REPORT_RAM_SLOTS, 32);
    assert_eq!(GUEST_UEFI_IRON_REPORT_RAM_GPA, 0x7BDD_D000);
    assert_eq!(GUEST_UEFI_EPT_MT_WB, 6);
    assert!(guest_uefi_report_ram_should_map(GUEST_UEFI_IRON_REPORT_RAM_GPA));
    assert_eq!(
        guest_uefi_report_ram_gpa_2m(GUEST_UEFI_IRON_REPORT_RAM_GPA),
        0x7BC0_0000
    );
    assert!(!guest_uefi_report_ram_should_map(0x1F0_0000));
    assert!(!guest_uefi_report_ram_should_map(0x8000_0000));
    assert_eq!(GUEST_UEFI_IRON_HIGH_DEADLOOP_RIP, 0x7F8E_21CA);
    assert!(guest_uefi_report_ram_should_map(GUEST_UEFI_IRON_HIGH_DEADLOOP_RIP));
    assert_eq!(
        guest_uefi_report_ram_page_off(GUEST_UEFI_IRON_HIGH_DEADLOOP_RIP),
        0xE21CA
    );
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0x7f8e21ca"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("peek report-RAM"));
    assert_eq!(GUEST_UEFI_IRON_ASSERT_CALLER_RIP, 0x7FD2_5193);
    assert_eq!(GUEST_UEFI_IRON_HIGH_CR3, 0x7FA0_1000);
    assert_eq!(
        guest_uefi_pt_pml4e_gpa(GUEST_UEFI_IRON_HIGH_CR3, 0),
        GUEST_UEFI_IRON_HIGH_CR3
    );
    assert_eq!(
        GUEST_UEFI_IRON_ASSERT_CALLER_RIP.wrapping_sub(0x1D2_5193),
        0x7E00_0000
    );
    {
        let pml4e = 0x7FA0_2003u64;
        let peek = |gpa: u64| {
            if gpa == GUEST_UEFI_IRON_HIGH_CR3 {
                pml4e
            } else if gpa == (pml4e & GUEST_UEFI_PT_ADDR_MASK) {
                0x7FA0_3003
            } else if gpa == 0x7FA0_3000 {
                0x7FA0_00E7
            } else {
                0
            }
        };
        assert_eq!(
            guest_uefi_pt_walk_pml4e(peek, GUEST_UEFI_IRON_HIGH_CR3, 0),
            pml4e
        );
        assert_eq!(
            guest_uefi_pt_walk_pdpte(peek, GUEST_UEFI_IRON_HIGH_CR3, 0) & GUEST_UEFI_PT_PRESENT,
            GUEST_UEFI_PT_PRESENT
        );
        assert_eq!(
            guest_uefi_pt_walk_pde(peek, GUEST_UEFI_IRON_HIGH_CR3, 0),
            0x7FA0_00E7
        );
    }
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0x7fd25193"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("MTRR UC admitted"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("c70768b"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0x80000083"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("guest_uefi_pt_paint_live_uc_hole"));
    assert_eq!(GUEST_UEFI_IRON_PDE8000_WB, 0x8000_0083);
    assert!(guest_uefi_pt_pde_is_wb_hole(GUEST_UEFI_IRON_PDE8000_WB));
    assert_eq!(guest_uefi_pt_pde_pat_uc(0x8000_0000), 0x8000_0000 | GUEST_UEFI_PT_LARGE_2M_UC);
    assert!(!guest_uefi_pt_pde_is_wb_hole(guest_uefi_pt_pde_pat_uc(0x8000_0000)));
    {
        use core::cell::RefCell;
        let page = RefCell::new([0u8; 0x7000]);
        assert!(store_report_ram_u64(
            &mut *page.borrow_mut(),
            GUEST_UEFI_IRON_HIGH_CR3,
            0x7FA0_2023
        ));
        assert!(store_report_ram_u64(
            &mut *page.borrow_mut(),
            0x7FA0_2010,
            0x7FA0_5003
        ));
        assert!(store_report_ram_u64(
            &mut *page.borrow_mut(),
            0x7FA0_2018,
            0x7FA0_6023
        ));
        assert!(store_report_ram_u64(
            &mut *page.borrow_mut(),
            0x7FA0_5000,
            GUEST_UEFI_IRON_PDE8000_WB
        ));
        let peek = |gpa: u64| {
            let p = page.borrow();
            let off = guest_uefi_report_ram_page_off(gpa) as usize;
            if off.saturating_add(8) > p.len() {
                0
            } else {
                let mut le = [0u8; 8];
                le.copy_from_slice(&p[off..off + 8]);
                u64::from_le_bytes(le)
            }
        };
        let poke = |gpa: u64, val: u64| store_report_ram_u64(&mut *page.borrow_mut(), gpa, val);
        assert_eq!(
            guest_uefi_pt_walk_pde(peek, GUEST_UEFI_IRON_HIGH_CR3, 0x8000_0000),
            GUEST_UEFI_IRON_PDE8000_WB
        );
        let n = guest_uefi_pt_paint_live_uc_hole(peek, poke, GUEST_UEFI_IRON_HIGH_CR3);
        assert!(n >= 1, "paint WB pde8000, n={n}");
        assert_eq!(
            guest_uefi_pt_walk_pde(peek, GUEST_UEFI_IRON_HIGH_CR3, 0x8000_0000),
            guest_uefi_pt_pde_pat_uc(0x8000_0000)
        );
    }
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("4ae87de"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("pde0=0xe3"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("guest_uefi_pt_split_gpa0"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("7e5d70f"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("c1476d3"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("PlatformMemMapInitialization"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("PEI never opened"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("f7620f6"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("pte_a0000"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("00:01.03"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("d6b012a"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0xa0067"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("CpuFlush"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("f0781bb"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("MTRR UC held after FIX WB (GCD)"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("flushjnz="));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("6334704"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("MTRR VGA FIX UC (GCD)"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("mtrr259="));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("ddbd866"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("guest_uefi_pt_paint_vga_uc"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("calltgt="));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("e368e86"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0x7f8e21a5"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("mtrr268="));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("coerce only FIX 0x259"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("fd041bb"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("MTRR VGA FIX WB held"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("prehex="));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("96ef961"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("retpre="));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("no DID flip"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("6f077a3"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("retcmp="));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("ASSERT(FALSE)"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("2cbf9e8"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("AcpiTimerLibConstructor"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("PIIX4_PMBA_VALUE"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("00:02.0"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("bf696ca"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("scsi=0x28"));
    assert_eq!(GUEST_UEFI_ASSERT_PREHEX_BYTES, 32);
    assert_eq!(
        guest_uefi_assert_prehex_gpa(GUEST_UEFI_IRON_ASSERT_CALLER_RIP),
        0x7FD2_5173
    );
    assert_eq!(guest_uefi_assert_retcmp_gpa(0x7F8E_2946), 0x7F8E_2906);
    assert_eq!(
        guest_uefi_assert_retpre_word_gpa(0x7F8E_2946, 0x8B3),
        0x7F8E_31F2
    );
    assert_eq!(guest_uefi_pt_leaf_4k_for(0xC_0000), 0xC_0000 | GUEST_UEFI_PT_LEAF_4K);
    assert!(!guest_uefi_gpa_in_vga_fix_uc(0xA_0000));
    assert!(!guest_uefi_gpa_in_vga_fix_uc(0xB_F000));
    assert!(!guest_uefi_gpa_in_vga_fix_uc(0xC_0000));
    assert!(!guest_uefi_gpa_in_vga_fix_uc(0xF_0000));
    assert!(!guest_uefi_gpa_in_vga_fix_uc(0x9_F000));
    assert!(!guest_uefi_gpa_in_vga_fix_uc(0x10_0000));
    assert_eq!(GUEST_UEFI_IRON_PTE_A0000_WB, 0xA_0067);
    assert_eq!(guest_uefi_pt_leaf_4k_for(0xA_0000), 0xA_0000 | GUEST_UEFI_PT_LEAF_4K);
    {
        let mut b = GUEST_UEFI_CPU_FLUSH_UNSUPPORTED.to_vec();
        assert_eq!(guest_uefi_count_cpu_flush_jnz(&b), 1);
        assert_eq!(guest_uefi_patch_cpu_flush_unsupported(&mut b), 1);
        assert_eq!(b[GUEST_UEFI_CPU_FLUSH_JNZ_OFF], 0x90);
        assert_eq!(guest_uefi_count_cpu_flush_jnz(&b), 0);
        assert_eq!(guest_uefi_patch_cpu_flush_unsupported(&mut b), 0);
        assert_eq!(GUEST_UEFI_IRON_CPU_FLUSH_GPA, 0x7EE6_8FA0);
        let pat = GUEST_UEFI_CPU_FLUSH_UNSUPPORTED;
        let mut two = vec![0u8; 64];
        two[..pat.len()].copy_from_slice(pat);
        two[32..32 + pat.len()].copy_from_slice(pat);
        assert_eq!(guest_uefi_count_cpu_flush_jnz(&two), 2);
        assert_eq!(guest_uefi_patch_cpu_flush_unsupported(&mut two), 2);
        assert_eq!(guest_uefi_count_cpu_flush_jnz(&two), 0);
    }
    assert_eq!(GUEST_UEFI_IRON_PDE0_2M, 0xE3);
    assert!(guest_uefi_pt_pde0_is_2m(GUEST_UEFI_IRON_PDE0_2M));
    assert!(!guest_uefi_pt_pde0_is_2m(0x7FA0_00E7));
    assert_eq!(guest_uefi_gpa0_split_pt_gpa(), 0x20B000);
    {
        // Iron 4ae87de: live CR3 GPA0 is still 2MiB (pde0=0xE3) spanning
        // the 1MiB fixed-MTRR boundary. Peek/poke fills HV SPLIT4K PT at
        // 0x20B000 and points PD[0] at it.
        use core::cell::RefCell;
        let high = RefCell::new([0u8; 0x4000]);
        let pt = RefCell::new([0u8; 4096]);
        let pt_gpa = guest_uefi_gpa0_split_pt_gpa();
        assert!(store_report_ram_u64(
            &mut *high.borrow_mut(),
            GUEST_UEFI_IRON_HIGH_CR3,
            0x7FA0_2023
        ));
        assert!(store_report_ram_u64(
            &mut *high.borrow_mut(),
            0x7FA0_2000,
            0x7FA0_3023
        ));
        assert!(store_report_ram_u64(
            &mut *high.borrow_mut(),
            0x7FA0_3000,
            GUEST_UEFI_IRON_PDE0_2M
        ));
        let peek = |gpa: u64| -> u64 {
            if gpa >= pt_gpa && gpa < pt_gpa + 4096 {
                let off = (gpa - pt_gpa) as usize;
                let p = pt.borrow();
                if off.saturating_add(8) > p.len() {
                    0
                } else {
                    let mut le = [0u8; 8];
                    le.copy_from_slice(&p[off..off + 8]);
                    u64::from_le_bytes(le)
                }
            } else {
                let off = guest_uefi_report_ram_page_off(gpa) as usize;
                let p = high.borrow();
                if off.saturating_add(8) > p.len() {
                    0
                } else {
                    let mut le = [0u8; 8];
                    le.copy_from_slice(&p[off..off + 8]);
                    u64::from_le_bytes(le)
                }
            }
        };
        let poke = |gpa: u64, val: u64| -> bool {
            if gpa >= pt_gpa && gpa < pt_gpa + 4096 {
                let off = (gpa - pt_gpa) as usize;
                let mut p = pt.borrow_mut();
                if off.saturating_add(8) > p.len() {
                    false
                } else {
                    p[off..off + 8].copy_from_slice(&val.to_le_bytes());
                    true
                }
            } else {
                store_report_ram_u64(&mut *high.borrow_mut(), gpa, val)
            }
        };
        assert_eq!(
            guest_uefi_pt_walk_pde(peek, GUEST_UEFI_IRON_HIGH_CR3, 0),
            GUEST_UEFI_IRON_PDE0_2M
        );
        let n = guest_uefi_pt_split_gpa0(peek, poke, GUEST_UEFI_IRON_HIGH_CR3, pt_gpa);
        assert_eq!(n, 513, "512 4K leaves + PD[0] table pointer, n={n}");
        assert_eq!(
            guest_uefi_pt_walk_pde(peek, GUEST_UEFI_IRON_HIGH_CR3, 0),
            pt_gpa | GUEST_UEFI_PT_TABLE
        );
        assert_eq!(
            guest_uefi_pt_walk_pte(peek, GUEST_UEFI_IRON_HIGH_CR3, 0),
            GUEST_UEFI_PT_LEAF_4K
        );
        assert_eq!(
            guest_uefi_pt_walk_pte(peek, GUEST_UEFI_IRON_HIGH_CR3, 0x10_0000),
            0x10_0000 | GUEST_UEFI_PT_LEAF_4K
        );
        assert_eq!(
            guest_uefi_pt_walk_pte(peek, GUEST_UEFI_IRON_HIGH_CR3, 0xA_0000),
            guest_uefi_pt_leaf_4k_for(0xA_0000)
        );
        assert_eq!(
            guest_uefi_pt_walk_pte(peek, GUEST_UEFI_IRON_HIGH_CR3, 0xC_0000),
            0xC_0000 | GUEST_UEFI_PT_LEAF_4K
        );
        assert_eq!(
            guest_uefi_pt_paint_vga_uc(peek, poke, GUEST_UEFI_IRON_HIGH_CR3),
            0
        );
        assert_eq!(
            guest_uefi_pt_split_gpa0(peek, poke, GUEST_UEFI_IRON_HIGH_CR3, pt_gpa),
            0
        );
    }
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("hide LA57"));
    {
        let mut page = [0u8; 0x20];
        page[0x10] = 0xEB;
        page[0x11] = 0xEC;
        page[0x12] = 0xC9;
        page[0x13] = 0xC3;
        let mut out = [0u8; 4];
        assert_eq!(copy_report_ram_at(&page, 0x7BC0_0010, &mut out), 4);
        assert!(preempt_deadloop_is_assert_epilogue(&out));
        assert_eq!(preempt_deadloop_skip_len(&out), 0);
        assert_eq!(store_report_ram_at(&mut page, 0x7BC0_0010, 0x90F3, 2), 2);
        assert_eq!(load_report_ram_at(&page, 0x7BC0_0010, 2), Some(0x90F3));
    }
    assert_eq!(
        (crate::memory::ept_hw::ept_leaf_large(0x200000, GUEST_UEFI_EPT_MT_WB) >> 3) & 7,
        6
    );
    assert!(guest_uefi_gpa0_fixed_mtrr_split(32));
    assert!(!guest_uefi_gpa0_fixed_mtrr_split(36));
    assert!(!guest_uefi_gpa0_fixed_mtrr_split(40));
    assert!(guest_uefi_gpa0_split_now(32, false));
    assert!(!guest_uefi_gpa0_split_now(32, true));
    assert!(!guest_uefi_gpa0_split_now(36, false));
    assert_eq!(IA32_PAT_RESET, 0x0007_0406_0007_0406);
    assert_eq!(ia32_pat_memory_type(IA32_PAT_RESET, 0), 6);
    assert_eq!(ia32_pat_memory_type(IA32_PAT_RESET, 3), 0);
    assert_eq!(ia32_pat_memory_type(0, 0), 0);
    {
        let mut buf = [0u8; 8];
        assert_eq!(store_low_ram_at(&mut buf, 2, 0x85C0, 2), 2);
        assert_eq!(load_low_ram_at(&buf, 2, 2), Some(0x85C0));
    }
    assert!(guest_uefi_pde_is_large(0xC000_0083));
    assert!(guest_uefi_pde_is_poison(GUEST_UEFI_IRON_PF_POISON_PDE));
    assert!(!guest_uefi_pde_is_poison(0xC000_0083));
    assert_eq!(GUEST_UEFI_IRON_PF_POISON_CR2, 0x1D1_E6CB);
    assert!(guest_uefi_pf_should_split_ram_1g(0x2, GUEST_UEFI_IRON_PF_HEAP_WR_CR2, 0xC000_0083));
    assert!(!guest_uefi_pf_should_split_ram_1g(0x2, GUEST_UEFI_IRON_PF_MTRR_UC_CR2, 0xC040_0083));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("d5fceb1"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0x80B000"));
    assert_eq!(guest_uefi_cpuid_80000008_eax(0x0030_2E2E), 0x2E20);
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
    assert_eq!(leaf7.ecx & CPUID_LEAF7_ECX_LA57, 0);
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
    assert_eq!(GUEST_UEFI_POST_DXE_TAIL, 32768);
    assert_eq!(GUEST_UEFI_POST_ATAPI_TAIL, 32768);
    assert!(GUEST_UEFI_RESUME_CAP >= GUEST_UEFI_POST_DXE_TAIL + GUEST_UEFI_POST_ATAPI_TAIL);
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
    assert_eq!(
        M7_E5_OVMF_ELTORITO_OK_MARKER,
        "RAYNU-V-M7-E5-OVMF-ELTORITO-OK"
    );
    assert!(!guest_uefi_alive());
    assert!(!guest_uefi_past_sec());
    assert!(!guest_uefi_dxe());
    assert!(!guest_uefi_both());
    assert!(!guest_uefi_atapi());
    assert!(!guest_uefi_eltorito());
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
    assert!(guest_uefi_mtrr_fixed_is_vga_hole(0x259));
    assert!(guest_uefi_mtrr_fixed_is_vga_hole(0x26F));
    assert!(guest_uefi_mtrr_fixed_is_vga_hole(0x268));
    assert!(!guest_uefi_mtrr_fixed_is_vga_hole(0x250));
    assert!(!guest_uefi_mtrr_fixed_is_vga_hole(0x258));
    assert!(guest_uefi_mtrr_write(0x259, GUEST_UEFI_MTRR_WB_PACKED));
    assert_eq!(guest_uefi_mtrr_read(0x259), Some(GUEST_UEFI_MTRR_WB_PACKED));
    assert!(guest_uefi_mtrr_write(0x259, GUEST_UEFI_MTRR_UC_PACKED));
    assert_eq!(guest_uefi_mtrr_read(0x259), Some(GUEST_UEFI_MTRR_WB_PACKED));
    assert!(guest_uefi_mtrr_write(0x268, GUEST_UEFI_MTRR_WB_PACKED));
    assert_eq!(guest_uefi_mtrr_read(0x268), Some(GUEST_UEFI_MTRR_WB_PACKED));
    assert!(guest_uefi_mtrr_write(0x268, GUEST_UEFI_MTRR_UC_PACKED));
    assert_eq!(guest_uefi_mtrr_read(0x268), Some(GUEST_UEFI_MTRR_WB_PACKED));
    assert!(guest_uefi_mtrr_write(0x258, GUEST_UEFI_MTRR_WB_PACKED));
    assert_eq!(guest_uefi_mtrr_read(0x258), Some(GUEST_UEFI_MTRR_WB_PACKED));
    assert!(guest_uefi_mtrr_write(0x2FF, 0xC00));
    assert_eq!(guest_uefi_mtrr_read(0x2FF), Some(0xC00));
    assert!(guest_uefi_mtrr_write(0x200, 6));
    assert_eq!(guest_uefi_mtrr_read(0x200), Some(6));
    assert!(guest_uefi_mtrr_write(0x201, 1 << 11));
    assert_eq!(guest_uefi_mtrr_valid_var_pairs(), 1);
    assert!(!guest_uefi_mtrr_uc_hole_live());
    assert!(!crate::vmx::guest_pt::identity_pat_uc_hole());
    assert!(guest_uefi_mtrr_write(0xFE, 0xFFFF));
    assert_eq!(guest_uefi_mtrr_read(0xFE), Some(GUEST_UEFI_MTRRCAP));
    assert!(guest_uefi_mtrr_write(0x200, 0x8000_0000));
    assert!(guest_uefi_mtrr_write(0x201, 0x8000_0800));
    assert!(!guest_uefi_mtrr_uc_hole_live(), "P1: hold UC until admit");
    assert!(!crate::vmx::guest_pt::identity_pat_uc_hole());
    assert!(guest_uefi_mtrr_uc_held() >= 1);
    guest_uefi_mtrr_set_admit_uc(true);
    assert!(guest_uefi_mtrr_write(0x200, 0x8000_0000));
    assert!(guest_uefi_mtrr_write(0x201, 0x8000_0800));
    assert!(guest_uefi_mtrr_uc_hole_live());
    assert!(crate::vmx::guest_pt::identity_pat_uc_hole());
    guest_uefi_mtrr_reset();
    assert_eq!(guest_uefi_mtrr_read(0x2FF), Some(GUEST_UEFI_MTRR_DEF_DEFAULT));
    assert!(!guest_uefi_mtrr_pci_uc_hole());
    assert!(!guest_uefi_mtrr_uc_hole_live());
    assert!(!crate::vmx::guest_pt::identity_pat_uc_hole());
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
    assert!(!eltorito_boot_evidence(true, true, false));
    assert!(!eltorito_boot_evidence(true, false, true));
    assert!(!eltorito_boot_evidence(false, true, true));
    assert!(eltorito_boot_evidence(true, true, true));
    let mut m = 0u8;
    for &b in b"RN-ELT" {
        m = eltorito_com_match_step(m, b);
    }
    assert!(eltorito_payload_ran(m));
    assert!(!eltorito_payload_ran(0));
    assert!(!post_atapi_should_stop(
        false, 2000, 0, 0, 1, false, false, false
    ));
    assert!(!post_atapi_should_stop(
        true, 115, 115, 0, 0, false, false, false
    ));
    assert!(
        !post_atapi_should_stop(true, 30769, 115, 30769, 1, false, false, false),
        "first ATAPI sector must not stop Stage 45"
    );
    assert!(
        !post_atapi_should_stop(
            true,
            30769 + GUEST_UEFI_POST_ATAPI_TAIL,
            115,
            30769,
            1,
            false,
            false,
            false
        ),
        "first ATAPI is often LBA 0 dummy; do not apply the 32768 post-ATAPI tail"
    );
    assert!(!post_atapi_should_stop(
        true,
        30769 + GUEST_UEFI_POST_ATAPI_TAIL - 1,
        115,
        30769,
        1,
        false,
        false,
        false
    ));
    assert!(
        !post_atapi_should_stop(
            true,
            30769 + GUEST_UEFI_POST_ATAPI_TAIL,
            115,
            30769,
            4,
            true,
            true,
            false
        ),
        "catalog+load READ must keep the VMCS until RN-ELT or the 131072-exit cap (262144 hard resume)"
    );
    assert!(post_atapi_should_stop(
        true, 200, 115, 180, 4, true, true, true
    ));
    assert!(eltorito_stops_guest_uefi(true));
    assert!(!eltorito_stops_guest_uefi(false));
    {
        let extra = crate::devices::ide_cdrom::MOCK_EFI_ISO_BYTES
            + crate::devices::ide_cdrom::ISO_SECTOR;
        let mut iso = vec![0u8; extra];
        crate::devices::ide_cdrom::write_placeholder_iso(
            &mut iso[..crate::devices::ide_cdrom::MOCK_EFI_ISO_BYTES],
        );
        assert!(crate::devices::ide_cdrom::present(&iso, 9));
        assert!(
            !eltorito_stops_guest_uefi(true),
            "product ISO must not stop on RN-ELT"
        );
        assert!(
            !post_atapi_should_stop(true, 200, 115, 180, 4, true, true, true),
            "Stage 46 product CD continues past El Torito"
        );
        assert_eq!(guest_uefi_resume_cap(false), GUEST_UEFI_PRODUCT_ISO_RESUME_CAP);
        assert_eq!(
            guest_uefi_resume_cap(true),
            GUEST_UEFI_PRODUCT_ISO_RESUME_CAP,
            "armed product ISO uses the product cap on nested too (QEMU PRODUCT_ISO=)"
        );
        crate::devices::ide_cdrom::reset();
    }
    assert!(eltorito_stops_guest_uefi(true));
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
    let mut thirty_two = [0u8; 32];
    let mut ram32 = [0u8; 40];
    for (i, b) in ram32.iter_mut().enumerate() {
        *b = i as u8;
    }
    assert_eq!(copy_low_ram_at(&ram32, 0, &mut thirty_two), 32);
    assert_eq!(thirty_two[0], 0);
    assert_eq!(thirty_two[31], 31);
}

#[test]
fn copy_flash_at_firmware_rip() {
    assert_eq!(guest_uefi_flash_off(0xfffc_fc86), Some(0x3c_fc86));
    assert_eq!(guest_uefi_flash_off(0xfee0_00f0), None);
    assert_eq!(guest_uefi_flash_off(GUEST_UEFI_FLASH_BASE), Some(0));
    assert_eq!(
        guest_uefi_flash_off(GUEST_UEFI_FLASH_BASE + GUEST_UEFI_FLASH_WINDOW),
        None
    );
    let mut flash = [0u8; 32];
    flash[0] = 0x8b;
    flash[1] = 0x04;
    flash[2] = 0x25;
    flash[3] = 0xf0;
    let mut out = [0u8; 4];
    assert_eq!(copy_flash_at(&flash, GUEST_UEFI_FLASH_BASE, &mut out), 4);
    assert_eq!(out, [0x8b, 0x04, 0x25, 0xf0]);
    assert_eq!(copy_flash_at(&flash, 0xfee0_00f0, &mut out), 0);
    assert_eq!(copy_flash_at(&flash, GUEST_UEFI_FLASH_BASE + 100, &mut out), 0);
    assert!(guest_uefi_gpa_to_hpa(0xfffc_fc86).is_none());
}

#[test]
fn xapic_fetch_miss_eax_fallback_when_skip_len_valid() {
    assert!(xapic_fetch_miss_eax_fallback(0, 6));
    assert!(xapic_fetch_miss_eax_fallback(0, 1));
    assert!(xapic_fetch_miss_eax_fallback(0, 15));
    assert!(
        xapic_fetch_miss_eax_fallback(4, 6),
        "decode-fail with peek still EAX when skip-len is 1-15"
    );
    assert!(!xapic_fetch_miss_eax_fallback(0, 0));
    assert!(!xapic_fetch_miss_eax_fallback(0, 16));
    assert!(!xapic_fetch_miss_eax_fallback(4, 0));
    assert!(!xapic_fetch_miss_eax_fallback(16, 16), "do not skip 16-byte peek");
}

#[test]
fn insn_linear_adds_cs_base_unless_long_mode() {
    assert_eq!(guest_uefi_insn_linear(0x3c_fc86, 0xFFC0_0000, false), 0xfffc_fc86);
    assert_eq!(guest_uefi_insn_linear(0xfffc_fc86, 0, false), 0xfffc_fc86);
    assert_eq!(guest_uefi_insn_linear(0xfffc_fc86, 0xFFFF_0000, true), 0xfffc_fc86);
}

#[test]
fn mmio_peek_uses_flash_rip_when_cs_base_plus_rip_misses_window() {
    // Leftover real-mode CS.base: GUEST_RIP is already the flash linear
    // (iron e3f56aa rip=0xfffcfc86). CS.base+RIP wraps out of the 4MiB window.
    assert_eq!(
        guest_uefi_mmio_peek_linear(0xfffc_fc86, 0xFFFF_0000, false),
        0xfffc_fc86
    );
    assert_eq!(
        guest_uefi_mmio_peek_linear(0xfffc_fc86, 0, false),
        0xfffc_fc86
    );
    assert_eq!(
        guest_uefi_mmio_peek_linear(0x3c_fc86, 0xFFC0_0000, false),
        0xfffc_fc86
    );
    assert_eq!(
        guest_uefi_mmio_peek_linear(0xfffc_fc86, 0xFFFF_0000, true),
        0xfffc_fc86
    );
    assert_eq!(guest_uefi_mmio_peek_linear(0x1000, 0, false), 0x1000);
}

#[test]
fn mmio_skip_len_uses_decoded_when_vmcs_len_is_zero() {
    assert_eq!(guest_uefi_mmio_skip_len(6, 0), 6);
    assert_eq!(guest_uefi_mmio_skip_len(0, 6), 6);
    assert_eq!(guest_uefi_mmio_skip_len(4, 6), 4, "prefer valid VMCS");
    assert_eq!(guest_uefi_mmio_skip_len(0, 0), 0);
    assert_eq!(guest_uefi_mmio_skip_len(0, 16), 0, "do not skip 16-byte peek");
    assert_eq!(guest_uefi_mmio_skip_len(99, 6), 6);
}

#[test]
fn greedy_report_ram_leaves_only_1mib_for_disk() {
    // Iron leftover after fw+ram+sink+zero+scratch in [1MiB,256MiB).
    const PAGES: u64 = (151 * 1024 * 1024) / 4096;
    let mut words = [0u64; 1024];
    let mut alloc = unsafe {
        FrameAllocator::new(0x10_0000, PAGES, words.as_mut_ptr() as u64).unwrap()
    };
    let mut slots = 0u64;
    while alloc.allocate_contiguous_aligned(512, 512).is_some() {
        slots += 1;
    }
    assert!(slots >= 32);
    assert!(alloc
        .allocate_contiguous((64 * 1024 * 1024) / 4096)
        .is_none());
    assert!(alloc.allocate_contiguous((1024 * 1024) / 4096).is_some());
}

#[test]
fn try_alloc_product_iso_install_disk_reserves_64mib() {
    const PAGES: u64 = (151 * 1024 * 1024) / 4096;
    let mut words = [0u64; 1024];
    let mut alloc = unsafe {
        FrameAllocator::new(0x10_0000, PAGES, words.as_mut_ptr() as u64).unwrap()
    };
    let (_frame, bytes) = try_alloc_product_iso_install_disk(&mut alloc, false).unwrap();
    assert_eq!(bytes, 64 * 1024 * 1024);
    assert!(alloc.allocate_contiguous_aligned(512, 512).is_some());
}

#[test]
fn try_alloc_product_iso_install_disk_256mib_when_pool_allows() {
    const PAGES: u64 = (400 * 1024 * 1024) / 4096;
    let mut words = vec![0u64; 2048];
    let mut alloc = unsafe {
        FrameAllocator::new(0x10_0000, PAGES, words.as_mut_ptr() as u64).unwrap()
    };
    let (_frame, bytes) = try_alloc_product_iso_install_disk(&mut alloc, false).unwrap();
    assert_eq!(bytes, 256 * 1024 * 1024);
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

#[test]
fn rep_insw_fills_identify_word0() {
    crate::devices::ide_cdrom::reset();
    assert!(crate::devices::ide_cdrom::present_placeholder());
    assert_eq!(
        crate::devices::ide_cdrom::host_identify_word0(),
        Some(0x85C0)
    );
    crate::devices::ide_cdrom::reset();
    assert!(crate::devices::ide_cdrom::present_placeholder());
    let _ = crate::devices::ide_cdrom::ata_io(0x01F7, false, 1, 0xA1);
    let mut buf = vec![0u8; 512];
    let mut addr = 0u64;
    let count = guest_uefi_io_string_count(GUEST_UEFI_IO_QUAL_REP_INSW_1F0, 256);
    assert_eq!(count, 256);
    for _ in 0..count {
        let word = crate::devices::ide_cdrom::ata_io(0x01F0, true, 2, 0);
        assert_eq!(store_low_ram_at(&mut buf, addr, word, 2), 2);
        addr = guest_uefi_io_string_advance(addr, 2, false);
    }
    assert_eq!(addr, 512);
    assert_eq!(load_low_ram_at(&buf, 0, 2), Some(0x85C0));
    crate::devices::ide_cdrom::reset();
}

#[test]
fn cr_access_qual_cr8() {
    assert!(cr_access_is_cr8(8));
    assert!(cr_access_is_cr8(8 | (1 << 4)));
    assert!(!cr_access_is_cr8(4));
    assert!(!cr_access_is_cr8(0));
}
