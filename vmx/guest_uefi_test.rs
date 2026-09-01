use super::{
    guest_uefi_report_ram_premap_gpa, guest_uefi_report_ram_should_premap,
    GUEST_UEFI_REPORT_RAM_PRODUCT_EXTRA,
};
use super::{
    apply_guest_cr4_write, atapi_read_evidence, both_pci_evidence, copy_flash_at, copy_low_ram_at, dxe_or_cd_boot_evidence,
    xapic_fetch_miss_eax_fallback,
    xapic_eax_fallback_skip_len,
    guest_uefi_insn_linear,
    guest_uefi_mmio_peek_linear,
    guest_uefi_mmio_skip_len,
    guest_uefi_linux_fixed_skip_len,
    guest_uefi_linux_invlpg_len,
    guest_uefi_linux_cpuid_msr_skip,
    guest_uefi_linux_cpuid_force_skip,
    guest_uefi_linux_cpuid_exit_skip,
    guest_uefi_linux_cpuid_should_log,
    guest_uefi_linux_hlt_skip,
    eltorito_boot_evidence, eltorito_com_match_step, eltorito_payload_ran, guest_uefi_tick_should_print, guest_uefi_linux_earlycon_drain, guest_uefi_linux_earlycon_share_on_linux_deliver, guest_uefi_linux_earlycon_share_on_vmexit, guest_uefi_linux_earlycon_share_on_bootimg, guest_uefi_poll_iso_install_ok, guest_uefi_post_cd_non_io, exec_from_low_ram, flash_window_gpa_and_pad, guest_cr4_read_shadow, guest_uefi_alive, guest_uefi_atapi,
    guest_uefi_both, guest_uefi_com_bytes, guest_uefi_dxe, guest_uefi_eltorito, guest_uefi_non_tf_exits,
    guest_uefi_past_sec, guest_uefi_vmlaunch_entered, hlt_should_resume, io_port_from_qual,
    is_com_uart_port, is_pci_config_port, last_exit_reason, linear_left_sec_tail,
    live_firmware_alias_gpa, past_sec_evidence, pci_bdf_bit, post_atapi_should_stop,
    post_dxe_should_stop,
    run_retained_ovmf_vmlaunch, spin_short_jmp_should_skip, stamp_empty_ovmf_vars,
    cr_access_is_cr8,
    preempt_deadloop_should_skip, preempt_deadloop_skip_len, preempt_deadloop_is_assert_epilogue,
    preempt_deadloop_delay_loop_skip_len, preempt_deadloop_delay_loop_sets_rax_one,
    preempt_deadloop_guarded_assert_skip_len, guest_uefi_assert_caller_is_dxe_ram,
    insn_fallthrough_is_leave_ret, assert_deadloop_return_gpa, guest_uefi_cpuid_leaf1_is_uniprocessor,
    guest_uefi_cpuid_has_hypervisor, guest_uefi_cpuid_is_kvm, guest_uefi_cpuid_leaf_is_hypervisor_scan,
    guest_uefi_filter_cpuid, guest_uefi_filter_cpuid_for_linux,
    guest_uefi_cpuid_is_genuine_intel,
    guest_uefi_linux_hypervisor_scan_bump_gpr, guest_uefi_linux_hypervisor_scan_bump_gprs,
    GUEST_UEFI_LINUX_HYPERVISOR_SCAN_LAST,
    guest_uefi_hpet_step_for_exit, guest_uefi_hpet_uart_tsc_step,
    guest_uefi_xapic_is_not_sink, guest_uefi_is_mtrr_msr, guest_uefi_is_misc_enable,
    guest_uefi_misc_enable_read, guest_uefi_misc_enable_write,
    guest_uefi_mtrr_read, guest_uefi_mtrr_reset, guest_uefi_mtrr_write, guest_uefi_mtrr_pci_uc_hole,
    guest_uefi_mtrr_poweron_disabled, guest_uefi_mtrr_valid_var_pairs, guest_uefi_mtrr_uc_hole_live,
    guest_uefi_mtrr_set_admit_uc, guest_uefi_mtrr_uc_held,
    guest_uefi_mtrr_fixed_is_vga_hole, GUEST_UEFI_MTRR_UC_PACKED,
    guest_uefi_phys_bits, guest_uefi_gpa0_fixed_mtrr_split, guest_uefi_gpa0_split_now, guest_uefi_cpuid_80000008_eax, guest_uefi_mtrr_var_mask_sanitize,
    guest_uefi_flash_off, guest_uefi_gpa_to_hpa,
    try_alloc_product_iso_install_disk,
    guest_uefi_pf_should_identity_map, guest_uefi_pf_should_deliver_to_guest, guest_uefi_pf_is_linux_direct_map, guest_uefi_linux_pf_entry_info, guest_uefi_linux_pf_blocks_irq, guest_uefi_linux_exc_blocks_irq, guest_uefi_linux_exception_bitmap, guest_uefi_hw_exception_entry_info, GUEST_UEFI_LINUX_PF_ENTRY_INFO, GUEST_UEFI_INTR_TYPE_HW_EXCEPTION, GUEST_UEFI_INTR_DELIVER_CODE, GUEST_UEFI_INTR_INFO_VALID, guest_uefi_pf_sec_cr3, guest_uefi_pf_should_load_sec_cr3, guest_uefi_pf_should_rebuild_sec_cr3, guest_uefi_pf_error_is_reserved, guest_uefi_pf_should_map_mmio, guest_uefi_pf_gpa32, guest_uefi_mmio_needs_scratch, guest_uefi_report_ram_should_map, guest_uefi_string_ins_needs_report_ram_map, guest_uefi_report_ram_gpa_2m, guest_uefi_report_ram_page_off, copy_report_ram_at, store_report_ram_at, load_report_ram_at, guest_uefi_ept_scratch_on_qual, guest_uefi_ept_qual_is_walk, guest_uefi_ept_qual_is_fetch, guest_uefi_ept_hole_ro_on_qual, guest_uefi_ept_hole_ro_allows_execute, guest_uefi_rip_is_hole_execute, guest_uefi_hole_ro_uses_dedicated_zero, guest_uefi_insn_is_poison_fill, guest_uefi_pf_should_split_ram_1g, guest_uefi_pde_is_large, guest_uefi_pde_is_poison, guest_uefi_pf_should_fix_ram_wp, guest_uefi_pf_split4k_resume_already_rw, guest_uefi_pf_error_is_present_write, guest_uefi_io_qual_is_string, guest_uefi_io_qual_is_rep, guest_uefi_io_string_count, guest_uefi_io_string_advance, guest_uefi_io_string_fills_ram, guest_uefi_fwcfg_string_fills_ram, guest_uefi_io_string_dest_ok, GUEST_UEFI_FWCFG_SKIP_HV_IDENTITY_PREFIX, guest_uefi_fwcfg_identity_overlay, GUEST_UEFI_FWCFG_IDENTITY_OVERLAY_PREFIX, GUEST_UEFI_FWCFG_IDENTITY_OVERLAY_CAP, guest_uefi_fwcfg_dest_ok_fill, guest_uefi_fwcfg_dest_ok_fill_should_log, GUEST_UEFI_FWCFG_DEST_OK_FILL_PREFIX, GUEST_UEFI_FWCFG_DEST_OK_FILL_LOG_CAP, copy_low_ram_bytes, write_low_ram_bytes, guest_uefi_fwcfg_identity_overlay_apply, guest_uefi_fwcfg_identity_overlay_restore, guest_uefi_io_addr_reg, store_low_ram_at, load_low_ram_at, guest_uefi_cs_ar_is_long, guest_uefi_cr0_is_paging, guest_uefi_efer_with_lma, guest_uefi_efer_with_lma_allow_nx, guest_uefi_efer_allow_nx,
    guest_uefi_ia32e_entry_ctls, guest_uefi_is_pcd_database_sig, guest_uefi_is_ldri_sig, is_debugcon_port,
    ia32_pat_memory_type, IA32_PAT_RESET,
    ud_is_ud2, ud_xsave_family, xsetbv_accepts_xcr, xsetbv_masked_xcr0, e4_restore_xcr0_value, e4_restore_cr4_osxsave, E5_OVMF_SEC_CR4_VALUE, E5_OVMF_VMLAUNCH_RESIDUAL_NOTE, GUEST_UEFI_CR4_HOST_OWNED, GUEST_UEFI_CR4_OSXSAVE, GUEST_UEFI_CR4_VMXE, GUEST_UEFI_FEATURE_CONTROL_VALUE, GUEST_UEFI_FLASH_BASE,
    GUEST_UEFI_DEBUGCON_PORT, GUEST_UEFI_DXE_RAM_FLOOR, GUEST_UEFI_EFER_LMA, GUEST_UEFI_EFER_LME, GUEST_UEFI_EFER_NXE, GUEST_UEFI_CR0_PG,
    GUEST_UEFI_IRON_EPT_PCI_HOLE_GPA, GUEST_UEFI_IRON_PF_CR2, GUEST_UEFI_IRON_LINUX_PF_CR2, GUEST_UEFI_IRON_LINUX_PF_RIP, GUEST_UEFI_IRON_LINUX_CPUID_RIP, GUEST_UEFI_LINUX_DIRECT_MAP, GUEST_UEFI_IRON_PF_RSVD_CR2, GUEST_UEFI_IRON_PF_HEAP_CR2, GUEST_UEFI_IRON_PF_HEAP_WR_CR2, GUEST_UEFI_IRON_PF_POISON_CR2, GUEST_UEFI_IRON_PF_POISON_PDE, GUEST_UEFI_IRON_PF_MTRR_UC_CR2, GUEST_UEFI_IRON_PF_SIGNEXT_CR2, GUEST_UEFI_IRON_PF_TRUNC32_CR2, GUEST_UEFI_IRON_MMIO_SCRATCH_GPA, GUEST_UEFI_IRON_SINK_PT_GPA, GUEST_UEFI_IRON_SCRATCH_CAP_GPA, GUEST_UEFI_IRON_SCRATCH_WALK_GPA, GUEST_UEFI_IRON_SCRATCH_FETCH_WALK_GPA, GUEST_UEFI_IRON_EPT_QUAL_FETCH_WALK, GUEST_UEFI_IRON_EPT_QUAL_AD_WALK, GUEST_UEFI_IRON_HOLE_RO_HPET_RIP, GUEST_UEFI_IRON_HOLE_X_RIP, GUEST_UEFI_IRON_ZERO_FILL_RIP, GUEST_UEFI_IRON_PF_WP_CR2, GUEST_UEFI_IRON_PF_WP_RIP, GUEST_UEFI_IRON_PF_WP_ERR, GUEST_UEFI_IRON_PF_WP_PDE, GUEST_UEFI_IRON_PF_WP_SPLIT_PDE, GUEST_UEFI_IRON_PF_WP_PML4E_RO, GUEST_UEFI_IRON_PF_XAPIC_CR2, GUEST_UEFI_IRON_PF_XAPIC_ERR, GUEST_UEFI_IRON_PF_XAPIC_PDPTE, GUEST_UEFI_IRON_PF_XAPIC_RIP, GUEST_UEFI_IO_QUAL_REP_INSW_1F0, GUEST_UEFI_IO_STRING_CAP, GUEST_UEFI_HV_PML4, GUEST_UEFI_MEMFD_BASE, GUEST_UEFI_MMIO_SCRATCH_SLOTS, GUEST_UEFI_REPORT_RAM_SLOTS, GUEST_UEFI_IRON_REPORT_RAM_GPA, GUEST_UEFI_EPT_MT_WB, GUEST_UEFI_IRON_HIGH_DEADLOOP_RIP, GUEST_UEFI_PF_IDENTITY_CAP, GUEST_UEFI_PF_ERR_RSVD,
    GUEST_UEFI_PCD_DATABASE_SIG, GUEST_UEFI_LDRI_SIG, GUEST_UEFI_LDRI_IMAGEBASE_OFF, GUEST_UEFI_VM_ENTRY_IA32E,
    CPUID_80000001_EDX_NX, CPUID_80000001_EDX_PAGE1GB, CPUID_LEAF7_ECX_TME_EN, CPUID_LEAF7_ECX_LA57,
    CPUID_LEAF7_EBX_CLFLUSHOPT, CPUID_LEAF7_EBX_CLWB,
    GUEST_UEFI_CPUID_LEAF4_LAST_SUB, GUEST_UEFI_CPUID_LEAF0_MAX,
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
    guest_uefi_cpu_flush_skip_mapped,
    guest_uefi_cpu_flush_tick_scans_mapped,
    guest_uefi_linux_guest_active, guest_uefi_linux_unhandled_should_skip,
    guest_uefi_linux_unhandled_try_skip, guest_uefi_linux_exc_error_code, guest_uefi_nmi_entry_info,
    guest_uefi_linux_nmi_should_inject, virtio_mmio_eax_fallback, virtio_mmio_eax_fallback_len,
    virtio_mmio_eax_fallback_size,
    virtio_mmio_retry_decode_len, guest_uefi_linux_mov_dr_len,
    guest_uefi_virtio_bar_overlaps_scratch, guest_uefi_virtio_bar_should_trap,
    guest_uefi_virtio_mmio_raises_pit, guest_uefi_virtio_mmio_polls_lapic,
    guest_uefi_linux_io_raises_pit, guest_uefi_linux_preempt_deadloop_noskip,
    guest_uefi_linux_pic_before_lapic, guest_uefi_pic_before_lapic,
    guest_uefi_firmware_hlt_ignores_tpr,
    guest_uefi_firmware_hlt_wait_for_irq,
    guest_uefi_firmware_hlt_skip_after_inject,
    guest_uefi_firmware_hlt_skip_only_after_inject,
    guest_uefi_firmware_hlt_skip_without_inject,
    guest_uefi_firmware_skip_pit_inject,
    guest_uefi_firmware_leftover_timer_vec,
    guest_uefi_firmware_hlt_skip_len,
    guest_uefi_firmware_hlt_insn_len0_skip,
    guest_uefi_nested_iso0_firmware_hlt_pit,
    guest_uefi_nested_iso0_firmware_lapic_timer,
    guest_uefi_nested_iso0_inject_vec,
    guest_uefi_nested_iso0_firmware_hlt_ata,
    guest_uefi_nested_iso0_ata_inject_vec,
    guest_uefi_nested_iso0_ata_lapic,
    guest_uefi_product_firmware_hlt_wake,
    guest_uefi_product_firmware_hlt_ata,
    guest_uefi_product_firmware_hlt_ata_inject_vec,
    guest_uefi_product_firmware_hlt_ata_lapic,
    guest_uefi_product_firmware_hlt_wake_lapic,
    guest_uefi_product_firmware_hlt_wake_lapic_timer,
    guest_uefi_firmware_hlt_ataio0_wake_vec,
    guest_uefi_firmware_hlt_activity_active,
    guest_uefi_firmware_lapic_timer_expiry,
    guest_uefi_ioapic_io_over_pit,
    guest_uefi_firmware_virtual_wire_pic,
    guest_uefi_product_iso_pci_ready,
    guest_uefi_firmware_hlt_force_if,
    guest_uefi_firmware_force_if_for_inject,
    guest_uefi_firmware_arm_ata_gsi14,
    guest_uefi_firmware_prefer_ata_irr,
    guest_uefi_firmware_ata_over_pic,
    guest_uefi_firmware_ata_irr_only,
    guest_uefi_firmware_take_ioapic_ata,
    guest_uefi_firmware_pic_ata,
    guest_uefi_hlt_stall_quiet_tick, guest_uefi_linux_pic_irq0_vec,
    guest_uefi_linux_gsi2_before_pic,
    guest_uefi_pit_skips_ioapic_pin0,
    guest_uefi_virtio_drain_every_resume,
    GUEST_UEFI_INTR_TYPE_NMI,
    guest_uefi_pt_paint_vga_uc, guest_uefi_pt_leaf_4k_for, guest_uefi_gpa_in_vga_fix_uc,
    GUEST_UEFI_CPU_FLUSH_UNSUPPORTED, GUEST_UEFI_CPU_FLUSH_JNZ_OFF, GUEST_UEFI_IRON_CPU_FLUSH_GPA,
    GUEST_UEFI_CPU_FLUSH_HEAP_GPA, GUEST_UEFI_CPU_FLUSH_LEFTOVER_PER_WALK,
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
    assert_eq!(super::GUEST_UEFI_REPORT_RAM_PRODUCT_EXTRA, 976);
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
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("HPET 1ms on CPUID/MSR/EPT"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("HPET TSC-delta on UART COM I/O"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("Linux printk ticks every 4096"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("guest UART nowait (do not clear COM2_LIVE)"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("Linux CPUID GenuineIntel + NX"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("guest UART TX ring drain"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("guest UART TX ring drain 4/exit"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("linux earlycon share TX ring"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("linux earlycon quiet ticks"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("linux earlycon hush HV"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("linux earlycon share product ISO"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("cpu_flush on tick cadence even when share"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("linux earlycon share first CPUID"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("linux earlycon share first high-half"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("linux earlycon share first bootimg"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("guest UART TX drain COM2 independent"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("linux earlycon pace LSR THRE"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("linux earlycon skip #PF dump"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("linux earlycon skip exc deliver"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("poll ISO-INSTALL-OK every resume"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("256MiB disk leftover report-RAM"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("report-RAM EPT pre-map"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("cpu_flush skip leftover pre-map"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("cpu_flush leftover per walk"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("linux unhandled nowait stop"));
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
    assert_eq!(
        preempt_deadloop_skip_len(&[0x48, 0xFF, 0xC8, 0x75, 0xFB, 0x48, 0xFF, 0xC8]),
        5
    );
    assert_eq!(
        preempt_deadloop_skip_len(&[0x48, 0xFF, 0xC8, 0x75, 0xFB, 0x48, 0xFF, 0xC8, 0x75, 0xE0]),
        10
    );
    assert_eq!(preempt_deadloop_skip_len(&[0x48, 0xFF, 0xC8, 0x75, 0xE0]), 5);
    assert_eq!(
        preempt_deadloop_delay_loop_skip_len(&[0x48, 0xFF, 0xC8, 0x75, 0xFB]),
        Some(5)
    );
    assert!(preempt_deadloop_delay_loop_sets_rax_one(&[
        0x48, 0xFF, 0xC8, 0x75, 0xFB
    ]));
    assert!(!preempt_deadloop_delay_loop_sets_rax_one(&[0x48, 0xFF, 0xC8]));
    assert_eq!(preempt_deadloop_skip_len(&[0x48, 0xFF, 0xC8]), 0);
    assert_eq!(preempt_deadloop_skip_len(&[0x75, 0xFB]), 2);
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
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0x40003d00"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0x4000bd00"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("push rbx RSP slot"));
    assert_eq!(GUEST_UEFI_FEATURE_CONTROL_VALUE, 1);
    let leaf1 = guest_uefi_filter_cpuid(1, 0);
    assert_eq!(leaf1.ecx & crate::arch::cpu::CPUID_ECX_VMX, 0);
    assert_eq!(leaf1.ecx & crate::arch::cpu::CPUID_ECX_X2APIC, 0);
    assert!(guest_uefi_cpuid_has_hypervisor(leaf1.ecx));
    assert!(guest_uefi_cpuid_leaf1_is_uniprocessor(leaf1.ebx, leaf1.edx));
    let kvm = guest_uefi_filter_cpuid(GUEST_UEFI_KVM_CPUID_LEAF, 0);
    assert!(guest_uefi_cpuid_is_kvm(kvm.ebx, kvm.ecx, kvm.edx));
    assert_eq!(kvm.eax, GUEST_UEFI_KVM_CPUID_LEAF + 1);
    let linux1 = guest_uefi_filter_cpuid_for_linux(1, 0);
    assert!(!guest_uefi_cpuid_has_hypervisor(linux1.ecx));
    assert!(guest_uefi_cpuid_leaf1_is_uniprocessor(linux1.ebx, linux1.edx));
    let linux_kvm = guest_uefi_filter_cpuid_for_linux(GUEST_UEFI_KVM_CPUID_LEAF, 0);
    assert_eq!(linux_kvm.eax, 0);
    assert_eq!(linux_kvm.ebx, 0);
    assert_eq!(linux_kvm.ecx, 0);
    assert_eq!(linux_kvm.edx, 0);
    assert!(!guest_uefi_cpuid_is_kvm(linux_kvm.ebx, linux_kvm.ecx, linux_kvm.edx));
    assert!(guest_uefi_cpuid_leaf_is_hypervisor_scan(0x4000_3d00));
    assert!(guest_uefi_cpuid_leaf_is_hypervisor_scan(0x4000_bd00));
    assert!(guest_uefi_cpuid_leaf_is_hypervisor_scan(GUEST_UEFI_KVM_CPUID_LEAF));
    assert!(!guest_uefi_cpuid_leaf_is_hypervisor_scan(1));
    assert!(!guest_uefi_cpuid_leaf_is_hypervisor_scan(0x4001_0000));
    let linux_scan = guest_uefi_filter_cpuid_for_linux(0x4000_3d00, 0);
    assert_eq!(linux_scan.eax | linux_scan.ebx | linux_scan.ecx | linux_scan.edx, 0);
    let linux0 = guest_uefi_filter_cpuid_for_linux(0, 0);
    assert!(guest_uefi_cpuid_is_genuine_intel(
        linux0.ebx, linux0.edx, linux0.ecx
    ));
    let linux_nx = guest_uefi_filter_cpuid_for_linux(0x8000_0001, 0);
    assert_ne!(linux_nx.edx & CPUID_80000001_EDX_NX, 0);
    assert_eq!(linux_nx.edx & CPUID_80000001_EDX_PAGE1GB, 0);
    let fw_nx = guest_uefi_filter_cpuid(0x8000_0001, 0);
    assert_eq!(fw_nx.edx & CPUID_80000001_EDX_NX, 0);
    assert_eq!(fw_nx.edx & CPUID_80000001_EDX_PAGE1GB, 0);
    assert_eq!(GUEST_UEFI_LINUX_HYPERVISOR_SCAN_LAST, 0x4000_ff00);
    assert_eq!(
        guest_uefi_linux_hypervisor_scan_bump_gpr(0x4000_3d00, 0x4000_3d00),
        u64::from(GUEST_UEFI_LINUX_HYPERVISOR_SCAN_LAST)
    );
    assert_eq!(
        guest_uefi_linux_hypervisor_scan_bump_gpr(0x4000_bd00, 0x4000_bd00),
        u64::from(GUEST_UEFI_LINUX_HYPERVISOR_SCAN_LAST)
    );
    // alpine-virt native_cpuid: push %rbx with base in EBX (not R12).
    assert_eq!(
        guest_uefi_linux_hypervisor_scan_bump_gpr(0x4000_3d00, 0x7fff_ffff_8abc_def0),
        0x7fff_ffff_8abc_def0
    );
    // Iron 73c2cab: do not snap a direct-map pointer at GPA 0x40000000.
    assert_eq!(
        guest_uefi_linux_hypervisor_scan_bump_gpr(0x4000_0000, 0xffff_8880_4000_0000),
        0xffff_8880_4000_0000
    );
    assert_eq!(guest_uefi_linux_hypervisor_scan_bump_gpr(1, 1), 1);
    let mut gprs = [0x4000_0000u64, 0x7, 0x4000_0000];
    assert!(guest_uefi_linux_hypervisor_scan_bump_gprs(0x4000_0000, &mut gprs));
    assert_eq!(gprs[0], u64::from(GUEST_UEFI_LINUX_HYPERVISOR_SCAN_LAST));
    assert_eq!(gprs[1], 0x7);
    assert_eq!(gprs[2], u64::from(GUEST_UEFI_LINUX_HYPERVISOR_SCAN_LAST));
    let mut none = [0x10u64, 0x20];
    assert!(!guest_uefi_linux_hypervisor_scan_bump_gprs(0x4000_0000, &mut none));
    assert_eq!(none, [0x10, 0x20]);
    assert_eq!(
        guest_uefi_hpet_step_for_exit(10, false, false),
        crate::devices::guest_platform::HPET_INSN_STEP
    );
    assert_eq!(
        guest_uefi_hpet_step_for_exit(31, false, false),
        crate::devices::guest_platform::HPET_INSN_STEP
    );
    assert_eq!(
        guest_uefi_hpet_step_for_exit(32, false, false),
        crate::devices::guest_platform::HPET_INSN_STEP
    );
    assert_eq!(
        guest_uefi_hpet_step_for_exit(12, false, false),
        crate::devices::guest_platform::HPET_MAIN_STEP
    );
    assert_eq!(guest_uefi_hpet_step_for_exit(30, false, false), 0);
    assert_eq!(
        guest_uefi_hpet_step_for_exit(30, false, true),
        crate::devices::guest_platform::HPET_MAIN_STEP
    );
    assert_eq!(guest_uefi_hpet_uart_tsc_step(0), 0);
    assert_eq!(
        guest_uefi_hpet_uart_tsc_step(crate::devices::guest_platform::TSC_PER_HPET_TICK),
        1
    );
    assert_eq!(
        guest_uefi_hpet_uart_tsc_step(
            crate::devices::guest_platform::TSC_PER_HPET_TICK
                * crate::devices::guest_platform::HPET_UART_IO_STEP_CAP
        ),
        crate::devices::guest_platform::HPET_UART_IO_STEP_CAP
    );
    assert_eq!(
        guest_uefi_hpet_uart_tsc_step(u64::MAX),
        crate::devices::guest_platform::HPET_UART_IO_STEP_CAP
    );
    assert!(
        guest_uefi_hpet_uart_tsc_step(u64::MAX)
            < crate::devices::guest_platform::HPET_INSN_STEP
    );
    assert_eq!(
        guest_uefi_hpet_step_for_exit(48, false, false),
        crate::devices::guest_platform::HPET_INSN_STEP
    );
    assert_eq!(
        guest_uefi_hpet_step_for_exit(48, true, false),
        crate::devices::guest_platform::HPET_MAIN_STEP
    );
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
    assert_eq!(GUEST_UEFI_HV_PML4, 0x400000);
    assert_eq!(
        GUEST_UEFI_HV_PML4,
        crate::devices::guest_platform::HV_IDENTITY_PML4
    );
    assert_eq!(GUEST_UEFI_PF_IDENTITY_CAP, 256);
    assert_eq!(GUEST_UEFI_IRON_LINUX_PF_CR2, 0xffff_8880_7e2a_3000);
    assert_eq!(GUEST_UEFI_IRON_LINUX_PF_RIP, 0xffff_ffff_bee1_9755);
    assert_eq!(GUEST_UEFI_IRON_LINUX_CPUID_RIP, 0xffff_ffff_8408_1783);
    assert!(guest_uefi_pf_should_deliver_to_guest(GUEST_UEFI_IRON_LINUX_CPUID_RIP));
    assert!(guest_uefi_pf_is_linux_direct_map(GUEST_UEFI_IRON_LINUX_PF_CR2));
    assert!(guest_uefi_pf_is_linux_direct_map(GUEST_UEFI_LINUX_DIRECT_MAP));
    assert!(!guest_uefi_pf_is_linux_direct_map(GUEST_UEFI_IRON_PF_SIGNEXT_CR2));
    assert!(guest_uefi_pf_should_deliver_to_guest(GUEST_UEFI_IRON_LINUX_PF_RIP));
    assert!(!guest_uefi_pf_should_deliver_to_guest(0x7ee5_dbe4));
    assert!(!guest_uefi_pf_should_deliver_to_guest(GUEST_UEFI_IRON_HOLE_X_RIP));
    assert_eq!(GUEST_UEFI_LINUX_PF_ENTRY_INFO, 0x8000_0B0E);
    assert_eq!(guest_uefi_linux_pf_entry_info(), 0x8000_0B0E);
    assert_eq!(guest_uefi_linux_pf_entry_info() & 0xff, 14);
    assert_eq!(
        (guest_uefi_linux_pf_entry_info() >> 8) & 7,
        GUEST_UEFI_INTR_TYPE_HW_EXCEPTION
    );
    assert_ne!(
        guest_uefi_linux_pf_entry_info() & GUEST_UEFI_INTR_DELIVER_CODE,
        0
    );
    assert_ne!(
        guest_uefi_linux_pf_entry_info() & GUEST_UEFI_INTR_INFO_VALID,
        0
    );
    assert!(guest_uefi_linux_pf_blocks_irq(GUEST_UEFI_IRON_LINUX_PF_CR2));
    assert!(!guest_uefi_linux_pf_blocks_irq(0));
    assert!(guest_uefi_linux_exc_blocks_irq(0, true));
    assert!(!guest_uefi_linux_exc_blocks_irq(0, false));
    assert_eq!(
        guest_uefi_linux_exception_bitmap(),
        crate::vmx::fields::LINUX_EXCEPTION_BITMAP
    );
    assert_eq!(guest_uefi_linux_exception_bitmap() & (1 << 14), 0);
    assert_eq!(guest_uefi_linux_exception_bitmap() & (1 << 6), 0);
    assert_eq!(guest_uefi_linux_exception_bitmap() & (1 << 13), 0);
    assert_ne!(guest_uefi_linux_exception_bitmap() & (1 << 8), 0);
    assert_eq!(guest_uefi_hw_exception_entry_info(6, false), 0x8000_0306);
    assert_eq!(guest_uefi_hw_exception_entry_info(13, true), 0x8000_0B0D);
    assert_eq!(GUEST_UEFI_INTR_TYPE_NMI, 2);
    assert_eq!(guest_uefi_nmi_entry_info(), 0x8000_0202);
    assert_ne!(guest_uefi_nmi_entry_info(), guest_uefi_hw_exception_entry_info(2, false));
    assert!(guest_uefi_linux_nmi_should_inject(true, 2));
    assert!(!guest_uefi_linux_nmi_should_inject(false, 2));
    assert!(!guest_uefi_linux_nmi_should_inject(true, 8));
    assert!(!guest_uefi_pf_should_identity_map(0, GUEST_UEFI_IRON_LINUX_PF_CR2));
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
    assert!(guest_uefi_io_string_fills_ram(0x511));
    assert!(guest_uefi_fwcfg_string_fills_ram(0x511));
    assert!(!guest_uefi_fwcfg_string_fills_ram(0x510));
    assert!(guest_uefi_io_string_dest_ok(0x205f18));
    assert!(GUEST_UEFI_FWCFG_SKIP_HV_IDENTITY_PREFIX.contains("fw_cfg string skip HV identity dest="));
    assert!(GUEST_UEFI_FWCFG_IDENTITY_OVERLAY_PREFIX.contains("fw_cfg identity overlay dest="));
    assert_eq!(GUEST_UEFI_FWCFG_IDENTITY_OVERLAY_CAP, 16);
    let overlay_dest = GUEST_UEFI_HV_PML4 + 0x5f18;
    assert!(!guest_uefi_io_string_dest_ok(overlay_dest));
    assert!(guest_uefi_fwcfg_identity_overlay(0x511, overlay_dest, 4, false));
    assert!(!guest_uefi_fwcfg_identity_overlay(0x511, overlay_dest, 4, true));
    assert!(!guest_uefi_fwcfg_identity_overlay(0x511, overlay_dest, 17, false));
    assert!(!guest_uefi_fwcfg_identity_overlay(0x1F0, overlay_dest, 4, false));
    assert!(!guest_uefi_fwcfg_identity_overlay(0x511, 0x205f18, 4, false));
    assert!(
        u64::from(crate::devices::guest_acpi::ACPI_TABLES_LEN)
            > GUEST_UEFI_FWCFG_IDENTITY_OVERLAY_CAP
    );
    assert!(!guest_uefi_fwcfg_identity_overlay(
        0x511,
        0x205f18,
        u64::from(crate::devices::guest_acpi::ACPI_TABLES_LEN),
        false
    ));
    assert!(GUEST_UEFI_FWCFG_DEST_OK_FILL_PREFIX.contains("fw_cfg dest_ok fill dest="));
    assert_eq!(GUEST_UEFI_FWCFG_DEST_OK_FILL_LOG_CAP, 8);
    assert!(guest_uefi_fwcfg_dest_ok_fill_should_log(0));
    assert!(guest_uefi_fwcfg_dest_ok_fill_should_log(7));
    assert!(!guest_uefi_fwcfg_dest_ok_fill_should_log(8));
    assert!(guest_uefi_fwcfg_dest_ok_fill(
        0x511,
        0x205f18,
        u64::from(crate::devices::guest_acpi::ACPI_TABLES_LEN),
        false
    ));
    assert!(guest_uefi_fwcfg_dest_ok_fill(
        0x511,
        0x205f18,
        u64::from(crate::devices::guest_acpi::ACPI_LOADER_LEN),
        false
    ));
    assert!(!guest_uefi_fwcfg_dest_ok_fill(0x511, 0x205f18, 4, false));
    assert!(!guest_uefi_fwcfg_dest_ok_fill(
        0x511,
        overlay_dest,
        u64::from(crate::devices::guest_acpi::ACPI_TABLES_LEN),
        false
    ));
    assert!(!guest_uefi_fwcfg_dest_ok_fill(
        0x1F0,
        0x205f18,
        u64::from(crate::devices::guest_acpi::ACPI_TABLES_LEN),
        false
    ));
    assert!(!guest_uefi_fwcfg_dest_ok_fill(
        0x511,
        0x205f18,
        u64::from(crate::devices::guest_acpi::ACPI_TABLES_LEN),
        true
    ));
    {
        // Nested 1e0f4a7 dest is now ordinary RAM. Overlay cannot hold
        // etc/acpi/tables; dest_ok fill must. PEI dest holds ACPI tables.
        crate::devices::ide_cdrom::reset();
        crate::devices::guest_platform::reset();
        let extra = crate::devices::ide_cdrom::MOCK_EFI_ISO_BYTES
            + crate::devices::ide_cdrom::ISO_SECTOR;
        let mut iso = vec![0u8; extra];
        crate::devices::ide_cdrom::write_placeholder_iso(
            &mut iso[..crate::devices::ide_cdrom::MOCK_EFI_ISO_BYTES],
        );
        assert!(crate::devices::ide_cdrom::present(&iso, 9));
        let n = crate::devices::guest_acpi::ACPI_TABLES_LEN as usize;
        let dest = 0x205f18usize;
        let mut blob = vec![0u8; n];
        let _ = crate::devices::guest_platform::io(
            0x510,
            false,
            2,
            u64::from(crate::devices::guest_acpi::FW_CFG_ACPI_TABLES_SEL),
        );
        for i in 0..n {
            blob[i] = crate::devices::guest_platform::io(0x511, true, 1, 0) as u8;
        }
        assert_eq!(&blob[..4], b"RSDT");
        let mut ram = vec![0u8; dest + n];
        assert!(write_low_ram_bytes(&mut ram, dest as u64, &blob));
        assert_eq!(&ram[dest..dest + 4], b"RSDT");
        assert_eq!(
            ram[dest],
            crate::devices::guest_acpi::acpi_tables_byte(0)
        );
        crate::devices::ide_cdrom::reset();
        crate::devices::guest_platform::reset();
    }
    {
        // ZONE_FSEG AllocateMaxAddress dest is conventional 640KiB, not
        // PEI stack 0x205f18 and not ZONE_HIGH leftover. FSEG dest holds
        // ACPI tables.
        crate::devices::ide_cdrom::reset();
        crate::devices::guest_platform::reset();
        let extra = crate::devices::ide_cdrom::MOCK_EFI_ISO_BYTES
            + crate::devices::ide_cdrom::ISO_SECTOR;
        let mut iso = vec![0u8; extra];
        crate::devices::ide_cdrom::write_placeholder_iso(
            &mut iso[..crate::devices::ide_cdrom::MOCK_EFI_ISO_BYTES],
        );
        assert!(crate::devices::ide_cdrom::present(&iso, 9));
        let n = crate::devices::guest_acpi::ACPI_TABLES_LEN as usize;
        let dest = 0x9E000usize;
        assert!(dest < crate::devices::guest_platform::E820_VGA_BASE as usize);
        assert!(dest + n <= crate::devices::guest_platform::E820_VGA_BASE as usize);
        assert!(guest_uefi_io_string_dest_ok(dest as u64));
        assert!(guest_uefi_fwcfg_dest_ok_fill(
            0x511,
            dest as u64,
            n as u64,
            false
        ));
        assert!(!guest_uefi_fwcfg_identity_overlay(
            0x511,
            dest as u64,
            n as u64,
            false
        ));
        let mut blob = vec![0u8; n];
        let _ = crate::devices::guest_platform::io(
            0x510,
            false,
            2,
            u64::from(crate::devices::guest_acpi::FW_CFG_ACPI_TABLES_SEL),
        );
        for i in 0..n {
            blob[i] = crate::devices::guest_platform::io(0x511, true, 1, 0) as u8;
        }
        assert_eq!(&blob[..4], b"RSDT");
        let mut ram = vec![0u8; dest + n];
        assert!(write_low_ram_bytes(&mut ram, dest as u64, &blob));
        assert_eq!(&ram[dest..dest + 4], b"RSDT");
        crate::devices::ide_cdrom::reset();
        crate::devices::guest_platform::reset();
    }
    assert!(!guest_uefi_fwcfg_identity_overlay(0x511, 0x100000, 4, false));
    {
        let mut ram = vec![0u8; (GUEST_UEFI_HV_PML4
            + crate::devices::guest_platform::HV_IDENTITY_PML4_BYTES)
            as usize];
        let d = overlay_dest as usize;
        ram[d] = 0xAA;
        ram[d + 1] = 0xBB;
        ram[d + 2] = 0xCC;
        ram[d + 3] = 0xDD;
        let mut saved = [0u8; 4];
        assert!(guest_uefi_fwcfg_identity_overlay_apply(
            &mut ram,
            overlay_dest,
            b"QEMU",
            &mut saved
        ));
        assert_eq!(&saved, &[0xAA, 0xBB, 0xCC, 0xDD]);
        assert_eq!(&ram[d..d + 4], b"QEMU");
        assert!(guest_uefi_fwcfg_identity_overlay_restore(
            &mut ram, overlay_dest, &saved
        ));
        assert_eq!(&ram[d..d + 4], &[0xAA, 0xBB, 0xCC, 0xDD]);
        let mut tmp = [0u8; 4];
        assert!(copy_low_ram_bytes(&ram, overlay_dest, &mut tmp));
        assert!(write_low_ram_bytes(&mut ram, overlay_dest, b"QEMU"));
    }
    assert!(guest_uefi_io_string_dest_ok(0x100000));
    assert!(guest_uefi_io_string_dest_ok(0x7bddd000));
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
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("fw_cfg IoReadFifo8"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("skip HV identity PML4 dest"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("fw_cfg string skip HV identity dest="));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("fw_cfg identity overlay"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("HV identity PML4 0x400000"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("PEI dest holds ACPI tables"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("fw_cfg dest_ok fill dest="));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("dest_ok fill log cap 8"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("ACPI tables ZONE_FSEG"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("FSEG dest holds ACPI tables"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("linux-line ata_piix blacklist"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("linux-line piix_init blacklist"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("FADT FACS"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("flashcruzer reject 2d6b109 dest skip"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("auto-answer / # without login"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("product ISO POST_DXE_TAIL skip"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("emergency mount+exit"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("linux-line usbdelay"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("io string (rep insb)"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0xAF00 PM timer"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("tick port="));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("flash 084430f"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("flash 5c0f7a2"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("flash 2ae4544"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("do not F11 2ae4544"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0xB000 dword timer"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("firmware PIC before GSI 2"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("HLT stall quiet tick"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("HLT stall quiet tick print-only"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("firmware HLT ignores TPR"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("firmware HLT stall waits for IRQ"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("firmware virtual-wire PIC"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("firmware virtual-wire AEOI"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("firmware virtual-wire GSI 2"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("firmware HLT force IF"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("firmware HLT skip after inject"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("firmware HLT activity active"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("firmware LAPIC timer expiry"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("IOAPIC I/O over PIT"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("firmware virtual-wire GSI 14"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("product ISO fw_cfg bootorder virtio-iso scsi@3 first"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("product ISO fw_cfg bootorder El Torito ide@ first"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("do not F11 56f31d3"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("flash d61dc7e"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("do not F11 5c0f7a2"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("flash ea30da1"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("flash 56f31d3"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("flash 90da03d"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("flash e70a295"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("flash 77f5866"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("flash 5227ad9"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("firmware arm ATA GSI 14"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("flash 489d938"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("flash bce5bbb"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("flash eaa580d"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("flash 12926eb"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("do not F11 eaa580d"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("flash 0bb06a2"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("flash 30b78a0"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("flash 8e581c7"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("flash d7d63ca"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("flash e4faceb"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("flash a14223f"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("flash b5c3a9c"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("do not F11 a14223f"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("flash 3b7bbac"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("do not F11 3b7bbac"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("do not F11 e4faceb"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("do not F11 d7d63ca"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("firmware OVMF ATA vector"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("do not clobber IOAPIC ATA vector"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("do not inject leftover 0x2E"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("do not clobber PIC ICW2"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("PIC ATA vector follows ICW2"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("firmware HLT insn_len 0 skip"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("nested iso=0 firmware HLT PIT"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("do not F11 8e581c7"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("do not F11 30b78a0"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("do not F11 0bb06a2"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("do not F11 12926eb"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("do not F11 bce5bbb"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("do not F11 489d938"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("firmware prefer ATA IRR"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("firmware ATA over PIC"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("firmware force IF for inject"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("do not F11 77f5866"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("firmware arm ATA GSI 14"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("do not F11 b824789"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("flash b824789"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("do not F11 d61dc7e"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("skip-after-inject uses pci_ready"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("product ISO HLT stall before n=16384"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("firmware HLT skip without inject"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("firmware HLT skip after ataio"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("do not F11 90da03d"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("do not F11 ea30da1"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("product ISO hides PIIX IDE"));
    assert!(!guest_uefi_product_iso_pci_ready(false, true));
    assert!(guest_uefi_product_iso_pci_ready(true, false));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("do not F11 daf3195"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("do not F11 b26c86a"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("iron COM2 eac424b IRET-to-HLT"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("do not F11 8e81c2e"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("iron COM2 eac424b"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("do not F11 eac424b"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("do not F11 c08a13d"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("do not F11 9ce65ae"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("PIIX4 PM1 SCI_EN"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("PM1 SCI_EN at reset"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("DSDT PCI0 _PRT"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("DSDT PCI0 _CRS"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("linux hides duplicate slot0 IDE"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("linux hides PIIX IDE"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("linux high-half hides PIIX"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("linux ATA floating bus"));
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
    assert!(guest_uefi_string_ins_needs_report_ram_map(GUEST_UEFI_IRON_REPORT_RAM_GPA));
    assert!(!guest_uefi_string_ins_needs_report_ram_map(0x1000));
    assert!(!guest_uefi_string_ins_needs_report_ram_map(0x1F0_0000));
    assert!(guest_uefi_tick_should_print(256, false, false, false));
    assert!(guest_uefi_tick_should_print(16384, false, false, false));
    assert!(!guest_uefi_tick_should_print(16640, false, false, false));
    assert!(!guest_uefi_tick_should_print(17408, false, false, false));
    assert!(guest_uefi_tick_should_print(17408, true, false, false));
    assert!(guest_uefi_tick_should_print(20480, false, false, false));
    // Iron 115e5ee: linux=true used to print every 256 and split printk.
    assert!(!guest_uefi_tick_should_print(437248, true, true, false));
    assert!(!guest_uefi_tick_should_print(16640, false, true, false));
    assert!(!guest_uefi_tick_should_print(256, false, true, false));
    assert!(guest_uefi_tick_should_print(4096, false, true, false));
    assert!(guest_uefi_tick_should_print(438272, true, true, false));
    // linux earlycon quiet ticks: share suppresses every-4096 Linux ticks.
    assert!(!guest_uefi_tick_should_print(4096, false, true, true));
    assert!(!guest_uefi_tick_should_print(438272, true, true, true));
    // linux earlycon share product ISO: iso=0 must not latch share.
    assert!(!guest_uefi_linux_earlycon_share_on_linux_deliver(true, false));
    assert!(!guest_uefi_linux_earlycon_share_on_linux_deliver(false, true));
    assert!(guest_uefi_linux_earlycon_share_on_linux_deliver(true, true));
    // linux earlycon share first CPUID: iso=0 still must not latch.
    // linux earlycon share first high-half: UART printk / e820 I/O, not only CPUID.
    assert!(!guest_uefi_linux_earlycon_share_on_vmexit(0xfffc_f000, true));
    assert!(!guest_uefi_linux_earlycon_share_on_vmexit(0xffff_8880_7e2a_3000, false));
    assert!(guest_uefi_linux_earlycon_share_on_vmexit(0xffff_8880_7e2a_3000, true));
    // linux earlycon share first bootimg: identity-map `[` (iron b983ef8
    // OVMF RIP 0x7ee5dbe4) is not bit 63; iso=0 still must not latch.
    assert!(!guest_uefi_linux_earlycon_share_on_bootimg(true, false));
    assert!(!guest_uefi_linux_earlycon_share_on_bootimg(false, true));
    assert!(guest_uefi_linux_earlycon_share_on_bootimg(true, true));
    assert!(!guest_uefi_linux_earlycon_share_on_vmexit(0x7ee5_dbe4, true));
    // linux earlycon skip #PF dump: same predicate.
    // linux earlycon skip exc deliver: same predicate.
    // poll ISO-INSTALL-OK every resume: iron only.
    assert!(guest_uefi_poll_iso_install_ok(false));
    assert!(!guest_uefi_poll_iso_install_ok(true));
    assert!(guest_uefi_virtio_drain_every_resume(true), "virtio drain every resume");
    assert!(!guest_uefi_virtio_drain_every_resume(false), "iso=0 no virtio drain");
    assert_eq!(guest_uefi_linux_earlycon_drain(false), 4);
    assert_eq!(guest_uefi_linux_earlycon_drain(true), 4);
    assert_eq!(guest_uefi_linux_fixed_skip_len(&[0xF4]), 1);
    assert_eq!(guest_uefi_linux_fixed_skip_len(&[0x0F, 0xA2]), 2);
    assert_eq!(guest_uefi_linux_fixed_skip_len(&[0x0F, 0x32]), 2);
    assert_eq!(guest_uefi_linux_fixed_skip_len(&[0x0F, 0x30]), 2);
    assert_eq!(guest_uefi_linux_fixed_skip_len(&[0x0F, 0x31]), 2);
    assert_eq!(guest_uefi_linux_fixed_skip_len(&[0x0F, 0x08]), 2);
    assert_eq!(guest_uefi_linux_fixed_skip_len(&[0x0F, 0x09]), 2);
    assert_eq!(guest_uefi_linux_fixed_skip_len(&[0xF3, 0x90]), 2);
    assert_eq!(guest_uefi_linux_fixed_skip_len(&[0x0F, 0x01]), 0);
    assert_eq!(guest_uefi_linux_fixed_skip_len(&[0x0F, 0x01, 0x38]), 3);
    assert_eq!(guest_uefi_linux_fixed_skip_len(&[0x90]), 0);
    assert_eq!(guest_uefi_linux_invlpg_len(&[]), 0);
    assert_eq!(guest_uefi_linux_invlpg_len(&[0x0F, 0x01]), 0);
    assert_eq!(guest_uefi_linux_invlpg_len(&[0x0F, 0x01, 0x38]), 3);
    assert_eq!(guest_uefi_linux_invlpg_len(&[0x0F, 0x01, 0x3F]), 3);
    assert_eq!(guest_uefi_linux_invlpg_len(&[0x0F, 0x01, 0x3C, 0x24]), 4);
    assert_eq!(
        guest_uefi_linux_invlpg_len(&[0x0F, 0x01, 0x3D, 0, 0, 0, 0]),
        7
    );
    assert_eq!(guest_uefi_linux_invlpg_len(&[0x41, 0x0F, 0x01, 0x38]), 4);
    assert_eq!(guest_uefi_linux_invlpg_len(&[0x0F, 0x01, 0x78, 0x10]), 4);
    assert_eq!(guest_uefi_linux_invlpg_len(&[0x0F, 0x01, 0xF8]), 0);
    assert_eq!(guest_uefi_linux_invlpg_len(&[0x0F, 0x01, 0x10]), 0);
    assert_eq!(
        guest_uefi_linux_cpuid_msr_skip(0xffff_ffff_b808_1783, 0, &[]),
        2
    );
    assert_eq!(
        guest_uefi_linux_cpuid_msr_skip(0xffff_ffff_b808_1783, 2, &[]),
        0
    );
    assert_eq!(guest_uefi_linux_cpuid_msr_skip(0x7ee8_7e18, 0, &[]), 0);
    assert_eq!(
        guest_uefi_linux_cpuid_msr_skip(0xffff_ffff_b808_1783, 0, &[0x0F, 0xA2]),
        2
    );
    assert_eq!(
        guest_uefi_linux_cpuid_force_skip(GUEST_UEFI_IRON_LINUX_CPUID_RIP, GUEST_UEFI_IRON_LINUX_CPUID_RIP),
        2
    );
    assert_eq!(
        guest_uefi_linux_cpuid_force_skip(
            GUEST_UEFI_IRON_LINUX_CPUID_RIP,
            GUEST_UEFI_IRON_LINUX_CPUID_RIP.wrapping_add(2)
        ),
        0
    );
    assert_eq!(guest_uefi_linux_cpuid_force_skip(0x7ee8_7e18, 0x7ee8_7e18), 0);
    assert_eq!(
        guest_uefi_linux_cpuid_exit_skip(GUEST_UEFI_IRON_LINUX_CPUID_RIP),
        2
    );
    assert_eq!(guest_uefi_linux_cpuid_exit_skip(0x7ee8_7e18), 0);
    assert_eq!(GUEST_UEFI_CPUID_LEAF4_LAST_SUB, 4);
    assert_eq!(guest_uefi_filter_cpuid(4, GUEST_UEFI_CPUID_LEAF4_LAST_SUB).eax, 0);
    assert_eq!(guest_uefi_filter_cpuid(4, 0xc000_0101).eax, 0);
    assert!(guest_uefi_filter_cpuid(0, 0).eax <= GUEST_UEFI_CPUID_LEAF0_MAX);
    assert!(guest_uefi_linux_cpuid_should_log(1));
    assert!(guest_uefi_linux_cpuid_should_log(8));
    assert!(!guest_uefi_linux_cpuid_should_log(9));
    assert!(guest_uefi_linux_cpuid_should_log(16));
    assert!(guest_uefi_linux_cpuid_should_log(256));
    assert!(!guest_uefi_linux_cpuid_should_log(257));
    assert!(!guest_uefi_linux_cpuid_should_log(0));
    assert_eq!(guest_uefi_linux_hlt_skip(0xffff_ffff_b808_1783, 0, &[]), 1);
    assert_eq!(guest_uefi_linux_hlt_skip(0xffff_ffff_b808_1783, 1, &[]), 0);
    assert_eq!(guest_uefi_linux_hlt_skip(0x7ee8_7e18, 0, &[]), 0);
    assert_eq!(guest_uefi_linux_hlt_skip(0xffff_ffff_b808_1783, 0, &[0xF4]), 1);
    assert!(guest_uefi_post_cd_non_io(true, false, false));
    assert!(!guest_uefi_post_cd_non_io(true, false, true));
    assert!(!guest_uefi_post_cd_non_io(false, false, false));
    assert!(!guest_uefi_post_cd_non_io(true, true, false));
    assert_eq!(
        guest_uefi_report_ram_gpa_2m(GUEST_UEFI_IRON_REPORT_RAM_GPA),
        0x7BC0_0000
    );
    // report-RAM EPT pre-map: [32MiB, 2GiB) is 1008×2MiB (iron 113a08a PAT then quiet).
    let pre_n = GUEST_UEFI_REPORT_RAM_SLOTS + GUEST_UEFI_REPORT_RAM_PRODUCT_EXTRA;
    assert_eq!(pre_n, 1008);
    assert_eq!(guest_uefi_report_ram_premap_gpa(0, pre_n), Some(0x0200_0000));
    assert_eq!(
        guest_uefi_report_ram_premap_gpa(pre_n - 1, pre_n),
        Some(0x7FE0_0000)
    );
    assert!(guest_uefi_report_ram_premap_gpa(pre_n, pre_n).is_none());
    assert!(guest_uefi_report_ram_should_premap(true));
    assert!(!guest_uefi_report_ram_should_premap(false));
    assert_eq!(
        guest_uefi_report_ram_premap_gpa(0, GUEST_UEFI_REPORT_RAM_SLOTS),
        Some(0x0200_0000)
    );
    // cpu_flush leftover per walk (iron abfb008 skip n=944 then 64 heap slots hung).
    assert_eq!(GUEST_UEFI_CPU_FLUSH_HEAP_GPA, 0x7800_0000);
    assert_eq!(GUEST_UEFI_CPU_FLUSH_LEFTOVER_PER_WALK, 2);
    assert!(!guest_uefi_cpu_flush_skip_mapped(0x0200_0000, false));
    assert!(guest_uefi_cpu_flush_skip_mapped(0x0200_0000, true));
    assert!(!guest_uefi_cpu_flush_skip_mapped(0x1000, false));
    let flush_gpa = guest_uefi_report_ram_gpa_2m(GUEST_UEFI_IRON_CPU_FLUSH_GPA);
    assert!(!guest_uefi_cpu_flush_skip_mapped(flush_gpa, false));
    assert!(guest_uefi_cpu_flush_skip_mapped(flush_gpa, true));
    assert!(!guest_uefi_cpu_flush_skip_mapped(
        guest_uefi_report_ram_gpa_2m(GUEST_UEFI_IRON_REPORT_RAM_GPA),
        false,
    ));
    assert!(guest_uefi_cpu_flush_skip_mapped(
        guest_uefi_report_ram_gpa_2m(GUEST_UEFI_IRON_REPORT_RAM_GPA),
        true,
    ));
    assert!(guest_uefi_cpu_flush_tick_scans_mapped(32));
    assert!(!guest_uefi_cpu_flush_tick_scans_mapped(1008));
    // linux unhandled nowait stop (iron 1a2544d Freeing initrd then xcr0 restore).
    assert!(!guest_uefi_linux_guest_active(false, false, false));
    assert!(guest_uefi_linux_guest_active(true, false, false));
    assert!(guest_uefi_linux_guest_active(false, true, false));
    assert!(guest_uefi_linux_guest_active(false, false, true));
    assert!(!guest_uefi_linux_unhandled_should_skip(false, 3));
    assert!(!guest_uefi_linux_unhandled_should_skip(true, 0));
    assert!(!guest_uefi_linux_unhandled_should_skip(true, 16));
    assert!(guest_uefi_linux_unhandled_should_skip(true, 3));
    assert!(guest_uefi_linux_unhandled_try_skip(true, 0, 29));
    assert!(!guest_uefi_linux_unhandled_try_skip(true, 0, 2), "triple fault still stops");
    assert!(!guest_uefi_linux_unhandled_try_skip(false, 0, 29));
    assert_eq!(guest_uefi_linux_mov_dr_len(&[0x0F, 0x23, 0xC0]), 3);
    assert_eq!(guest_uefi_linux_mov_dr_len(&[0x0F, 0x21, 0xC0]), 3);
    assert_eq!(guest_uefi_linux_mov_dr_len(&[0x4C, 0x0F, 0x23, 0xC0]), 4);
    assert_eq!(guest_uefi_linux_mov_dr_len(&[]), 0);
    assert_eq!(guest_uefi_linux_fixed_skip_len(&[0x0F, 0x23, 0xC0]), 3);
    assert_eq!(virtio_mmio_retry_decode_len(16, 0), 15, "linux MMIO decode retry");
    assert_eq!(virtio_mmio_retry_decode_len(4, 7), 4, "insn_len longer than peek");
    assert_eq!(virtio_mmio_retry_decode_len(8, 3), 0, "first decode already had len");
    assert_eq!(virtio_mmio_retry_decode_len(0, 0), 0);
    assert_eq!(virtio_mmio_eax_fallback_len(true, 0, 0), 3, "linux EAX fallback skip 3");
    assert_eq!(virtio_mmio_eax_fallback_len(false, 0, 0), 0, "iso=0 decode fail still stops");
    assert_eq!(virtio_mmio_eax_fallback_len(true, 0, 6), 6);
    assert_eq!(virtio_mmio_eax_fallback_len(true, 4, 0), 4);
    assert_eq!(virtio_mmio_eax_fallback_len(true, 16, 0), 0, "do not skip 16-byte peek");
    assert_eq!(virtio_mmio_eax_fallback_size(0x14), 1, "virtio MMIO eax fallback size");
    assert_eq!(virtio_mmio_eax_fallback_size(0x18), 2);
    assert_eq!(virtio_mmio_eax_fallback_size(0x00), 4);
    assert!(virtio_mmio_eax_fallback(true, 0, 3));
    assert!(virtio_mmio_eax_fallback(true, 4, 6));
    assert!(virtio_mmio_eax_fallback(true, 0, 0), "linux EAX fallback skip 3");
    assert!(!virtio_mmio_eax_fallback(false, 0, 3), "iso=0 decode fail still stops");
    assert!(!virtio_mmio_eax_fallback(false, 0, 0), "iso=0 decode fail still stops");
    assert!(guest_uefi_virtio_bar_overlaps_scratch(0x8000_1000), "virtio BAR trap over scratch");
    assert!(guest_uefi_virtio_bar_overlaps_scratch(0x8000_0000));
    assert!(!guest_uefi_virtio_bar_overlaps_scratch(0xFE00_0000));
    assert!(guest_uefi_virtio_bar_should_trap(0x8000_1000));
    assert!(!guest_uefi_virtio_bar_should_trap(0));
    assert!(guest_uefi_virtio_mmio_raises_pit(true, true), "virtio MMIO raises PIT");
    assert!(!guest_uefi_virtio_mmio_raises_pit(false, true), "iso=0 firmware no extra PIT");
    assert!(!guest_uefi_virtio_mmio_raises_pit(true, false));
    assert!(guest_uefi_virtio_mmio_polls_lapic(true, true), "virtio MMIO polls lapic");
    assert!(!guest_uefi_virtio_mmio_polls_lapic(false, true), "iso=0 firmware no extra lapic poll");
    assert!(!guest_uefi_virtio_mmio_polls_lapic(true, false));
    assert!(!guest_uefi_linux_io_raises_pit(true, true), "linux I/O does not raise PIT (iron MADT stop)");
    assert!(!guest_uefi_linux_io_raises_pit(false, true), "iso=0 firmware no extra I/O PIT");
    assert!(!guest_uefi_linux_io_raises_pit(true, false));
    assert!(
        guest_uefi_linux_preempt_deadloop_noskip(true, true),
        "linux preempt deadloop noskip"
    );
    assert!(
        !guest_uefi_linux_preempt_deadloop_noskip(false, true),
        "iso=0 firmware still skips CpuDeadLoop"
    );
    assert!(!guest_uefi_linux_preempt_deadloop_noskip(true, false));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("virtio BAR trap over scratch"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("PIIX3 ISA BAR RAZ"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("packed virtio common cfg"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("virtio MMIO raises PIT"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("virtio MMIO off="));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("virtio MMIO eax fallback size"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("packed virtio common cfg write"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("virtio MMIO polls lapic"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("linux I/O does not raise PIT (iron MADT stop)"));
    assert!(include_str!("guest_uefi.rs").contains("linux I/O does not raise PIT (iron MADT stop)"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("linux xAPIC EPT insn_len 0"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("linux preempt deadloop noskip"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("linux PIT prefer once"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("linux PIT prefer until DRIVER_OK"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("UART reassert RX not THRE"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("virtio drain every resume"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("linux virtio DRIVER_OK"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("product ISO fw_cfg ACPI MADT (iso=0 named files stay 3)"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("linux PIC before LAPIC"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("linux PIC IRQ0"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("MADT IRQ0 ISO GSI 2"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("PIT skips IOAPIC pin 0"));
    assert!(E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("linux GSI 2 before PIC"));
    assert!(guest_uefi_pit_skips_ioapic_pin0());
    assert!(guest_uefi_linux_gsi2_before_pic(true));
    assert!(!guest_uefi_linux_gsi2_before_pic(false));
    assert!(guest_uefi_linux_pic_before_lapic(true, false));
    assert!(!guest_uefi_linux_pic_before_lapic(true, true));
    assert!(!guest_uefi_linux_pic_before_lapic(false, false));
    assert!(guest_uefi_pic_before_lapic(true, true, false));
    assert!(!guest_uefi_pic_before_lapic(true, true, true));
    assert!(!guest_uefi_pic_before_lapic(false, false, false));
    assert!(guest_uefi_firmware_hlt_ignores_tpr(false, true, 0));
    assert!(!guest_uefi_firmware_hlt_ignores_tpr(true, true, 0));
    assert!(!guest_uefi_firmware_hlt_ignores_tpr(false, false, 0));
    assert!(!guest_uefi_firmware_hlt_ignores_tpr(false, true, 1));
    crate::devices::ide_cdrom::reset();
    assert!(
        !guest_uefi_firmware_hlt_wait_for_irq(true, 16385, 12, true, 0),
        "firmware HLT skip without inject: do not arm virtual-wire on product ISO HLT"
    );
    assert!(!guest_uefi_firmware_hlt_wait_for_irq(false, 16385, 12, true, 0));
    assert!(!guest_uefi_firmware_hlt_wait_for_irq(true, 16384, 12, true, 0));
    assert!(!guest_uefi_firmware_hlt_wait_for_irq(true, 16385, 12, false, 0));
    assert!(!guest_uefi_firmware_hlt_wait_for_irq(true, 16385, 12, true, 1));
    assert!(guest_uefi_firmware_hlt_skip_after_inject(true, 16385, 12, true, 0));
    assert!(
        guest_uefi_firmware_hlt_skip_only_after_inject(true),
        "firmware HLT skip only after inject"
    );
    assert!(
        !guest_uefi_firmware_hlt_skip_only_after_inject(false),
        "firmware HLT skip only after inject; iron COM2 b5c3a9c inj=0"
    );
    assert!(
        guest_uefi_firmware_hlt_skip_after_inject(true, 16385, 12, true, 1),
        "firmware HLT skip after ataio"
    );
    assert!(!guest_uefi_firmware_hlt_skip_after_inject(false, 16385, 12, true, 0));
    assert!(
        guest_uefi_nested_iso0_firmware_hlt_pit(false, false, true, 0, 12),
        "nested iso=0 firmware HLT PIT"
    );
    assert!(
        !guest_uefi_nested_iso0_firmware_hlt_pit(true, false, true, 0, 12),
        "product ISO keeps skip_pit; nested PIT is iso=0 only"
    );
    assert!(!guest_uefi_nested_iso0_firmware_hlt_pit(false, true, true, 0, 12));
    assert!(!guest_uefi_nested_iso0_firmware_hlt_pit(false, false, false, 0, 12));
    assert!(!guest_uefi_nested_iso0_firmware_hlt_pit(false, false, true, 1, 12));
    assert!(!guest_uefi_nested_iso0_firmware_hlt_pit(false, false, true, 0, 0x1e));
    assert!(
        guest_uefi_nested_iso0_firmware_hlt_ata(false, false, true, 1, 12),
        "nested iso=0 firmware HLT ATA"
    );
    assert!(!guest_uefi_nested_iso0_firmware_hlt_ata(false, false, true, 0, 12));
    assert!(!guest_uefi_nested_iso0_firmware_hlt_ata(true, false, true, 1, 12));
    assert_eq!(
        crate::devices::guest_irq::NESTED_ISO0_EDK2_IRQ14,
        0x76,
        "nested iso=0 firmware HLT ATA"
    );
    assert_eq!(
        guest_uefi_nested_iso0_ata_inject_vec(None),
        0x76,
        "do not inject leftover 0x2E"
    );
    assert_eq!(
        guest_uefi_nested_iso0_ata_inject_vec(Some(0x2E)),
        0x76,
        "do not inject leftover 0x2E"
    );
    assert_eq!(
        guest_uefi_nested_iso0_ata_inject_vec(Some(0xEF)),
        0x76,
        "nested iso=0 firmware HLT ATA LAPIC; do not inject leftover 0xEF"
    );
    assert_eq!(guest_uefi_nested_iso0_ata_inject_vec(Some(0x76)), 0x76);
    assert!(
        guest_uefi_nested_iso0_ata_lapic(None),
        "nested iso=0 firmware HLT ATA LAPIC"
    );
    assert!(!guest_uefi_nested_iso0_ata_lapic(Some(0x76)));
    assert_eq!(
        crate::devices::guest_irq::NESTED_ISO0_EDK2_IRQ0,
        0x68,
        "nested iso=0 EDK2 IRQ0"
    );
    assert_eq!(
        guest_uefi_nested_iso0_inject_vec(Some(0x68), Some(0x20)),
        0x68,
        "PIC take beats LAPIC"
    );
    assert_eq!(
        guest_uefi_nested_iso0_inject_vec(None, Some(0x20)),
        0x20,
        "nested iso=0 firmware LAPIC timer"
    );
    assert_eq!(
        guest_uefi_nested_iso0_inject_vec(None, None),
        0x68,
        "nested iso=0 EDK2 IRQ0 when both empty"
    );
    assert!(
        guest_uefi_nested_iso0_firmware_lapic_timer(false, false, true, 0, 12),
        "nested iso=0 firmware LAPIC timer"
    );
    assert!(!guest_uefi_nested_iso0_firmware_lapic_timer(true, false, true, 0, 12));
    assert!(
        guest_uefi_firmware_skip_pit_inject(false, 0x20),
        "product skip_pit still drops 0x20"
    );
    assert!(
        guest_uefi_firmware_skip_pit_inject(false, 0xEF),
        "product skip_pit still drops leftover LVT 0xEF"
    );
    assert!(
        !guest_uefi_firmware_skip_pit_inject(false, 0x68),
        "product ISO firmware HLT wake: remapped 0x68 injects"
    );
    assert!(
        guest_uefi_product_firmware_hlt_wake(false, true, 0, 12),
        "product ISO firmware HLT wake"
    );
    assert!(!guest_uefi_product_firmware_hlt_wake(true, true, 0, 12));
    assert!(!guest_uefi_product_firmware_hlt_wake(false, true, 1, 12));
    assert!(!guest_uefi_product_firmware_hlt_wake(false, false, 0, 12));
    assert!(!guest_uefi_product_firmware_hlt_wake(false, true, 0, 0x1e));
    assert!(guest_uefi_firmware_leftover_timer_vec(0x20));
    assert!(guest_uefi_firmware_leftover_timer_vec(0xEF));
    assert!(!guest_uefi_firmware_leftover_timer_vec(0x68));
    assert_eq!(
        guest_uefi_firmware_hlt_ataio0_wake_vec(None, None),
        0x68,
        "product ISO firmware HLT wake"
    );
    assert_eq!(
        guest_uefi_firmware_hlt_ataio0_wake_vec(Some(0x20), None),
        0x68
    );
    assert_eq!(
        guest_uefi_firmware_hlt_ataio0_wake_vec(None, Some(0xEF)),
        0x68
    );
    assert_eq!(
        guest_uefi_firmware_hlt_ataio0_wake_vec(Some(0x76), None),
        0x76
    );
    assert_eq!(
        guest_uefi_firmware_hlt_ataio0_wake_vec(Some(0x68), Some(0x20)),
        0x68
    );
    assert!(
        guest_uefi_product_firmware_hlt_wake_lapic(None),
        "product ISO firmware HLT wake LAPIC"
    );
    assert!(!guest_uefi_product_firmware_hlt_wake_lapic(Some(0x68)));
    assert!(
        guest_uefi_product_firmware_hlt_wake_lapic_timer(None),
        "product ISO firmware HLT wake LAPIC timer"
    );
    assert!(!guest_uefi_product_firmware_hlt_wake_lapic_timer(Some(0x68)));
    assert_eq!(
        guest_uefi_firmware_hlt_ataio0_wake_vec(None, Some(0x27)),
        0x27,
        "product ISO firmware HLT wake LAPIC timer; unmasked LVT injects"
    );
    assert!(
        guest_uefi_product_firmware_hlt_ata(false, true, 1, 12),
        "product ISO firmware HLT ATA"
    );
    assert!(!guest_uefi_product_firmware_hlt_ata(false, true, 0, 12));
    assert!(!guest_uefi_product_firmware_hlt_ata(true, true, 1, 12));
    assert_eq!(
        guest_uefi_product_firmware_hlt_ata_inject_vec(None),
        0x76,
        "product ISO firmware HLT ATA; do not inject leftover 0x2E"
    );
    assert_eq!(
        guest_uefi_product_firmware_hlt_ata_inject_vec(Some(0x2E)),
        0x76,
        "product ISO firmware HLT ATA IOAPIC; do not inject leftover 0x2E"
    );
    assert_eq!(
        guest_uefi_product_firmware_hlt_ata_inject_vec(Some(0xEF)),
        0x76,
        "product ISO firmware HLT ATA LAPIC; do not inject leftover 0xEF"
    );
    assert_eq!(guest_uefi_product_firmware_hlt_ata_inject_vec(Some(0x76)), 0x76);
    assert!(
        guest_uefi_product_firmware_hlt_ata_lapic(None),
        "product ISO firmware HLT ATA LAPIC"
    );
    assert!(!guest_uefi_product_firmware_hlt_ata_lapic(Some(0x76)));
    assert_eq!(
        guest_uefi_firmware_hlt_insn_len0_skip(false),
        1,
        "firmware HLT insn_len 0 skip"
    );
    assert_eq!(guest_uefi_firmware_hlt_insn_len0_skip(true), 0);
    assert!(!guest_uefi_firmware_hlt_skip_after_inject(true, 16384, 12, true, 0));
    assert!(!guest_uefi_firmware_hlt_skip_after_inject(true, 16385, 12, false, 1));
    assert!(
        guest_uefi_firmware_hlt_skip_without_inject(false),
        "firmware HLT skip without inject"
    );
    assert!(!guest_uefi_firmware_hlt_skip_without_inject(true));
    assert!(
        guest_uefi_firmware_skip_pit_inject(false, 0x20),
        "firmware skip PIT inject"
    );
    assert!(
        !guest_uefi_firmware_skip_pit_inject(false, 0x68),
        "product ISO firmware HLT wake: remapped 0x68 injects"
    );
    assert!(
        !guest_uefi_firmware_skip_pit_inject(false, 0x2E),
        "firmware skip PIT inject: ATA 14 still injects"
    );
    assert!(
        !guest_uefi_firmware_skip_pit_inject(true, 0x20),
        "linux still injects PIT 0x20"
    );
    assert_eq!(
        guest_uefi_firmware_force_if_for_inject(false, 0x2),
        0x2 | (1 << 9),
        "firmware force IF for inject"
    );
    assert_eq!(
        guest_uefi_firmware_force_if_for_inject(true, 0x2),
        0x2,
        "linux keeps guest IF"
    );
    assert!(
        guest_uefi_firmware_arm_ata_gsi14(false),
        "firmware arm ATA GSI 14"
    );
    assert!(
        !guest_uefi_firmware_arm_ata_gsi14(true),
        "linux programs its own RTEs"
    );
    assert!(
        guest_uefi_firmware_prefer_ata_irr(false, 1),
        "firmware prefer ATA IRR after PACKET"
    );
    assert!(
        guest_uefi_firmware_prefer_ata_irr(false, 0),
        "firmware prefer ATA IRR before PACKET when 0x2E is latched"
    );
    assert!(
        !guest_uefi_firmware_prefer_ata_irr(true, 1),
        "linux keeps TPR"
    );
    assert!(
        guest_uefi_firmware_ata_over_pic(false, true),
        "firmware ATA over PIC"
    );
    assert!(
        !guest_uefi_firmware_ata_over_pic(true, true),
        "linux keeps PIC-first"
    );
    assert!(
        !guest_uefi_firmware_ata_over_pic(false, false),
        "firmware ATA over PIC only when pin 14 or latched 0x2E is ready"
    );
    assert!(
        guest_uefi_firmware_ata_irr_only(false),
        "firmware ATA IRR only"
    );
    assert!(
        !guest_uefi_firmware_ata_irr_only(true),
        "linux still takes LVT"
    );
    assert!(
        guest_uefi_firmware_take_ioapic_ata(false),
        "firmware take IOAPIC ATA"
    );
    assert!(
        guest_uefi_firmware_pic_ata(false, true),
        "firmware PIC ATA"
    );
    assert!(
        !guest_uefi_firmware_pic_ata(true, true),
        "linux keeps PIC-first / GSI 2"
    );
    assert!(
        !guest_uefi_firmware_pic_ata(false, false),
        "firmware PIC ATA only when PIC peek is 0x2E"
    );
    assert!(
        !guest_uefi_firmware_take_ioapic_ata(true),
        "linux still takes any IOAPIC pin"
    );
    assert_eq!(
        guest_uefi_firmware_hlt_force_if(false, true, 1, 0x2),
        0x2,
        "virtual-wire force IF stays ataio==0"
    );
    assert_eq!(guest_uefi_firmware_hlt_skip_len(true), 1, "firmware HLT skip after inject");
    assert_eq!(guest_uefi_firmware_hlt_skip_len(false), 0, "nested iso=0 keeps skip_hlt");
    assert_eq!(
        guest_uefi_firmware_hlt_activity_active(),
        0,
        "firmware HLT activity active"
    );
    assert!(guest_uefi_firmware_lapic_timer_expiry(false, true, 0));
    assert!(!guest_uefi_firmware_lapic_timer_expiry(true, true, 0));
    assert!(!guest_uefi_firmware_lapic_timer_expiry(false, false, 0));
    assert!(!guest_uefi_firmware_lapic_timer_expiry(false, true, 1));
    assert!(
        guest_uefi_ioapic_io_over_pit(),
        "IOAPIC I/O over PIT"
    );
    assert!(guest_uefi_firmware_virtual_wire_pic(false, true, 0));
    assert!(!guest_uefi_firmware_virtual_wire_pic(true, true, 0));
    assert!(!guest_uefi_firmware_virtual_wire_pic(false, false, 0));
    assert!(!guest_uefi_firmware_virtual_wire_pic(false, true, 1));
    assert_eq!(
        guest_uefi_firmware_hlt_force_if(false, true, 0, 0x2),
        0x2 | (1 << 9),
        "firmware HLT force IF"
    );
    assert_eq!(
        guest_uefi_firmware_hlt_force_if(true, true, 0, 0x2),
        0x2,
        "linux keeps guest IF"
    );
    assert!(guest_uefi_pic_before_lapic(true, true, false));
    crate::devices::guest_irq::reset();
    assert!(guest_uefi_hlt_stall_quiet_tick(16385, 12, true, 0));
    assert!(!guest_uefi_hlt_stall_quiet_tick(16384, 12, true, 0));
    assert!(!guest_uefi_hlt_stall_quiet_tick(16385, 12, false, 0));
    assert!(!guest_uefi_hlt_stall_quiet_tick(16385, 12, true, 1));
    assert!(!guest_uefi_hlt_stall_quiet_tick(16385, 0x1e, true, 0));
    assert!(guest_uefi_linux_pic_irq0_vec(0x20));
    assert!(!guest_uefi_linux_pic_irq0_vec(0x24));
    assert!(guest_uefi_linux_exc_error_code(8));
    assert!(guest_uefi_linux_exc_error_code(14));
    assert!(!guest_uefi_linux_exc_error_code(6));
    assert!(!guest_uefi_linux_exc_error_code(0));
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
    assert_eq!(
        guest_uefi_gpa0_split_pt_gpa(),
        GUEST_UEFI_HV_PML4 + crate::vmx::guest_pt::IDENTITY_4G_BYTES
    );
    {
        // Iron 4ae87de: live CR3 GPA0 is still 2MiB (pde0=0xE3) spanning
        // the 1MiB fixed-MTRR boundary. Peek/poke fills HV SPLIT4K PT at
        // GUEST_UEFI_HV_PML4+0xB000 and points PD[0] at it.
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
    assert_eq!(leaf7.ebx & CPUID_LEAF7_EBX_CLFLUSHOPT, 0);
    assert_eq!(leaf7.ebx & CPUID_LEAF7_EBX_CLWB, 0);
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
    assert!(guest_uefi_efer_allow_nx(true));
    assert!(!guest_uefi_efer_allow_nx(false));
    assert_eq!(
        guest_uefi_efer_with_lma_allow_nx(
            GUEST_UEFI_EFER_LME | GUEST_UEFI_EFER_NXE,
            true,
            true
        ),
        GUEST_UEFI_EFER_LME | GUEST_UEFI_EFER_LMA | GUEST_UEFI_EFER_NXE
    );
    assert_eq!(
        guest_uefi_efer_with_lma_allow_nx(
            GUEST_UEFI_EFER_LME | GUEST_UEFI_EFER_NXE,
            true,
            false
        ),
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
        post_atapi_should_stop(
            true,
            115 + GUEST_UEFI_POST_DXE_TAIL,
            115,
            0,
            0,
            false,
            false,
            false
        ),
        "lab stub still applies POST_DXE_TAIL when sectors==0"
    );
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
        assert!(
            !post_atapi_should_stop(
                true,
                115 + GUEST_UEFI_POST_DXE_TAIL,
                115,
                0,
                0,
                false,
                false,
                false
            ),
            "product ISO POST_DXE_TAIL skip (iron 2d6b109 stop n=33297 sectors=0)"
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
    assert!(
        xapic_fetch_miss_eax_fallback(0, 0),
        "linux xAPIC EPT insn_len 0"
    );
    assert_eq!(xapic_eax_fallback_skip_len(0), 3);
    assert_eq!(xapic_eax_fallback_skip_len(6), 6);
    assert_eq!(xapic_eax_fallback_skip_len(16), 0);
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
    // 256 MiB disk + 64 MiB scratch leave (leftover DRAM fills report-RAM).
    const PAGES: u64 = (512 * 1024 * 1024) / 4096;
    let mut words = vec![0u64; 4096];
    let mut alloc = unsafe {
        FrameAllocator::new(0x10_0000, PAGES, words.as_mut_ptr() as u64).unwrap()
    };
    let (_frame, bytes) = try_alloc_product_iso_install_disk(&mut alloc, false).unwrap();
    assert_eq!(bytes, 256 * 1024 * 1024);
}

#[test]
fn try_alloc_256mib_when_leftover_backs_report_ram() {
    // Iron after fw/sink/hole is ~480 MiB. Scratch-only leave (64 MiB)
    // lets 256 MiB land; leftover DRAM extra=846 fills report-RAM.
    const PAGES: u64 = (480 * 1024 * 1024) / 4096;
    let mut words = vec![0u64; 4096];
    let mut alloc = unsafe {
        FrameAllocator::new(0x10_0000, PAGES, words.as_mut_ptr() as u64).unwrap()
    };
    let (_frame, bytes) = try_alloc_product_iso_install_disk(&mut alloc, false).unwrap();
    assert_eq!(bytes, 256 * 1024 * 1024);
    assert_eq!(super::PRODUCT_ISO_DISK_LEAVE_2M_SLOTS, 32);
    assert_eq!(super::product_iso_disk_leave_pages(), 32 * 512);
}

#[test]
fn try_alloc_skips_256mib_when_scratch_would_starve() {
    // ~280 MiB precise pool: 256 MiB disk would leave ~24 MiB (< 64 MiB
    // scratch). 64 MiB still fits Alpine GPT. Do not steal leftover.
    const PAGES: u64 = (280 * 1024 * 1024) / 4096;
    let mut words = vec![0u64; 4096];
    let mut alloc = unsafe {
        FrameAllocator::new(0x10_0000, PAGES, words.as_mut_ptr() as u64).unwrap()
    };
    let (_frame, bytes) = try_alloc_product_iso_install_disk(&mut alloc, false).unwrap();
    assert_eq!(bytes, 64 * 1024 * 1024);
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
fn product_iso_pci_ready_arms_on_virtio_enum_not_ide() {
    crate::devices::ide_cdrom::reset();
    assert!(!guest_uefi_product_iso_pci_ready(false, true));
    let extra = crate::devices::ide_cdrom::MOCK_EFI_ISO_BYTES + crate::devices::ide_cdrom::ISO_SECTOR;
    let iso = vec![0u8; extra];
    assert!(crate::devices::ide_cdrom::present(&iso, 9));
    assert!(guest_uefi_product_iso_pci_ready(false, true));
    assert!(guest_uefi_firmware_hlt_skip_after_inject(true, 16385, 12, true, 0));
    assert!(!guest_uefi_firmware_hlt_skip_after_inject(true, 16385, 12, false, 0));
    assert!(
        guest_uefi_firmware_hlt_skip_after_inject(true, 1, 12, true, 0),
        "product ISO HLT stall before n=16384: skip CpuSleep without PIT inject"
    );
    assert!(
        !guest_uefi_firmware_hlt_wait_for_irq(true, 1, 12, true, 0),
        "firmware HLT skip without inject"
    );
    assert!(
        guest_uefi_firmware_skip_pit_inject(false, 0x20),
        "firmware skip PIT inject"
    );
    assert!(!guest_uefi_firmware_skip_pit_inject(false, 0x68));
    assert!(!guest_uefi_firmware_skip_pit_inject(false, 0x2E));
    assert_eq!(
        guest_uefi_firmware_force_if_for_inject(false, 0),
        1 << 9,
        "firmware force IF for inject after ataio"
    );
    assert!(guest_uefi_firmware_arm_ata_gsi14(false));
    assert!(guest_uefi_firmware_prefer_ata_irr(false, 1));
    assert!(guest_uefi_firmware_ata_irr_only(false));
    assert!(!guest_uefi_firmware_ata_irr_only(true));
    assert!(guest_uefi_firmware_take_ioapic_ata(false));
    assert!(!guest_uefi_firmware_take_ioapic_ata(true));
    assert!(guest_uefi_firmware_pic_ata(false, true));
    assert!(!guest_uefi_firmware_pic_ata(true, true));
    assert!(!guest_uefi_firmware_pic_ata(false, false));
    assert!(
        guest_uefi_hlt_stall_quiet_tick(1, 12, true, 0),
        "product ISO quiet tick arms with the window, not n>16384"
    );
    assert!(!guest_uefi_product_iso_pci_ready(false, false));
    crate::devices::ide_cdrom::reset();
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
