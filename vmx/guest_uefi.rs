//! Private guest-UEFI VMCS + EPT + VMLAUNCH of retained ESP `OVMF.fd`.
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** (ADR-002 / ADR-014 guest firmware path)
//! VERIFICATION: L1 (runtime assert + host tests; QEMU is the launch gate)
//!
//! After Stage 36 retain, this module allocates a **private** VMCS and EPT,
//! maps the retained bytes at the firmware-alias window, and issues a real
//! `VMLAUNCH` at reset vector `0xFFFF_FFF0`. It does not write the E4 SHELL
//! VMCS or EPT. Fixtures are refused. Host `cargo test` never executes the
//! instruction.

use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

use crate::boot::ovmf_esp::{self, MIN_REAL_OVMF_BYTES};
use crate::memory::frame_allocator::FrameAllocator;
use crate::vmx::launch::{
    alias_ept_covers_reset, GuestUefiLaunchError, GUEST_UEFI_FIRMWARE_TOP_GPA,
    GUEST_UEFI_PRIVATE_VMCS_ID,
};

#[cfg(target_os = "uefi")]
use crate::arch::cpu::{
    self, adjust_vmx_controls, true_ctl_msrs_supported, IA32_EFER, IA32_FS_BASE, IA32_GS_BASE,
    IA32_SYSENTER_CS, IA32_SYSENTER_EIP, IA32_SYSENTER_ESP, IA32_VMX_CR0_FIXED0,
    IA32_VMX_CR0_FIXED1, IA32_VMX_CR4_FIXED0, IA32_VMX_CR4_FIXED1, IA32_VMX_ENTRY_CTLS,
    IA32_VMX_EXIT_CTLS, IA32_VMX_PINBASED_CTLS, IA32_VMX_PROCBASED_CTLS, IA32_VMX_PROCBASED_CTLS2,
    IA32_VMX_TRUE_ENTRY_CTLS, IA32_VMX_TRUE_EXIT_CTLS, IA32_VMX_TRUE_PINBASED_CTLS,
    IA32_VMX_TRUE_PROCBASED_CTLS,
};
#[cfg(target_os = "uefi")]
use crate::audit::AuditEvent;
#[cfg(target_os = "uefi")]
use crate::audit_log;
#[cfg(target_os = "uefi")]
use crate::boot::serial;
#[cfg(target_os = "uefi")]
use crate::memory::ept_hw::{self, frames_required_firmware_alias, TWO_MIB};
#[cfg(target_os = "uefi")]
use crate::vmx::fields::*;
#[cfg(target_os = "uefi")]
use crate::vmx::launch::{
    install_host_tss, prepare_vmcs_region, LaunchError, GUEST_UEFI_RESET_CS, GUEST_UEFI_RESET_RIP,
    GUEST_UEFI_RESET_VECTOR_GPA, GUEST_UEFI_UNRESTRICTED_GUEST,
};
#[cfg(target_os = "uefi")]
use crate::vmx::ops;

/// QEMU / serial marker when VMLAUNCH entered retained OVMF (not VM-entry fail).
pub const M7_E5_OVMF_VMLAUNCH_OK_MARKER: &str = "RAYNU-V-M7-E5-OVMF-VMLAUNCH-OK";

/// Honest residual. First guest-UEFI entry is not Everest E5.
pub const E5_OVMF_VMLAUNCH_RESIDUAL_NOTE: &str =
    "residual: private guest-UEFI VMCS + EPT VMLAUNCH of retained ESP OVMF.fd; first entry only; not installer; attach_cdrom_uefi stays UnsupportedOnFirmware; not ISO-INSTALL-OK; no guest UEFI distro; VMLAUNCH insn issued only when presence is true";

static LAUNCH_ENTERED: AtomicBool = AtomicBool::new(false);
static MARKER_PRINTED: AtomicBool = AtomicBool::new(false);
static LAST_EXIT_REASON: AtomicU32 = AtomicU32::new(0);
static LAST_GUEST_RIP: AtomicU64 = AtomicU64::new(0);
static LAST_GUEST_PHYS: AtomicU64 = AtomicU64::new(0);
static LAST_INSN_ERROR: AtomicU32 = AtomicU32::new(0);

static mut SAVED_VMCS: u64 = 0;
static mut E4_ALLOC: *mut FrameAllocator = core::ptr::null_mut();
static mut E4_LIFE: *mut crate::vmx::lifecycle::VmxLifecycle = core::ptr::null_mut();
static mut E4_RSP: u64 = 0;
static mut E4_RESUME: u64 = 0;

/// True after a real VMLAUNCH entered the guest (HOST_RIP reached).
pub fn guest_uefi_vmlaunch_entered() -> bool {
    LAUNCH_ENTERED.load(Ordering::Acquire)
}

/// Last recorded basic exit reason (0 if none).
pub fn last_exit_reason() -> u32 {
    LAST_EXIT_REASON.load(Ordering::Acquire)
}

/// Live alias window for a retained 1–4 MiB image: `4 GiB - len`.
///
/// Stage 15 [`crate::vmx::launch::firmware_alias_gpa`] stays 4 MiB-only
/// (bookkeeping contract). This helper is the real retain path.
pub fn live_firmware_alias_gpa(bytes_len: u64) -> Option<u64> {
    const MIN: u64 = MIN_REAL_OVMF_BYTES as u64;
    const MAX: u64 = 4 * 1024 * 1024;
    if bytes_len < MIN || bytes_len > MAX || (bytes_len & 0xfff) != 0 {
        return None;
    }
    let gpa = GUEST_UEFI_FIRMWARE_TOP_GPA - bytes_len;
    if alias_ept_covers_reset(gpa, bytes_len) {
        Some(gpa)
    } else {
        None
    }
}

/// Host-test / reset helper. Does not VMCLEAR a live VMCS.
pub fn reset_guest_uefi_launch() {
    LAUNCH_ENTERED.store(false, Ordering::Release);
    MARKER_PRINTED.store(false, Ordering::Release);
    LAST_EXIT_REASON.store(0, Ordering::Release);
    LAST_GUEST_RIP.store(0, Ordering::Release);
    LAST_GUEST_PHYS.store(0, Ordering::Release);
    LAST_INSN_ERROR.store(0, Ordering::Release);
}

#[cfg(target_os = "uefi")]
fn alloc_phys(alloc: &mut FrameAllocator) -> Option<u64> {
    alloc.allocate_frame().map(|f| {
        crate::audit::integrity::record_event(AuditEvent::FrameAllocated { frame: f.0 });
        f.to_phys()
    })
}

/// Launch retained ESP `OVMF.fd` on a private VMCS + EPT.
///
/// INVARIANTS:
/// - Refuses when [`ovmf_esp::bytes_present`] is false (fixtures stay out)
/// - Does not write the E4 SHELL VMCS or EPT
/// - Host `cargo test` never executes `ops::vmlaunch`
/// - On VMLAUNCH instruction failure, logs and returns (host stays alive)
///
/// On successful entry, HOST_RIP logs the first VMEXIT, prints the marker
/// when the exit is not a VM-entry failure, VMCLEARs the private VMCS, and
/// resumes the E4 SHELL path on the original stack.
pub unsafe fn run_retained_ovmf_vmlaunch(
    alloc: &mut FrameAllocator,
) -> Result<(), GuestUefiLaunchError> {
    if !ovmf_esp::bytes_present() {
        return Err(GuestUefiLaunchError::MissingEspFirmware);
    }
    let Some(bytes) = ovmf_esp::retained_bytes() else {
        return Err(GuestUefiLaunchError::MissingEspFirmware);
    };
    if bytes.len() < MIN_REAL_OVMF_BYTES || !ovmf_esp::accept_real_ovmf_bytes(bytes) {
        return Err(GuestUefiLaunchError::MissingEspFirmware);
    }
    let fw_len = bytes.len() as u64;
    let map_len = (fw_len + 0xfff) & !0xfff;
    let Some(gpa) = live_firmware_alias_gpa(map_len) else {
        return Err(GuestUefiLaunchError::LaunchSetupFailed);
    };
    if !alias_ept_covers_reset(gpa, map_len) {
        return Err(GuestUefiLaunchError::LaunchSetupFailed);
    }

    #[cfg(not(target_os = "uefi"))]
    {
        let _ = (
            alloc,
            GUEST_UEFI_PRIVATE_VMCS_ID,
            E5_OVMF_VMLAUNCH_RESIDUAL_NOTE,
        );
        return Err(GuestUefiLaunchError::PrivateVmcsNotLaunched);
    }

    #[cfg(target_os = "uefi")]
    {
        match launch_uefi(alloc, bytes, gpa, map_len) {
            Ok(()) => Ok(()),
            Err(e) => Err(e),
        }
    }
}

#[cfg(target_os = "uefi")]
unsafe fn launch_uefi(
    alloc: &mut FrameAllocator,
    bytes: &[u8],
    gpa: u64,
    fw_len: u64,
) -> Result<(), GuestUefiLaunchError> {
    let pages = (fw_len + 4095) / 4096;
    let Some(fw_frame) = alloc.allocate_contiguous(pages) else {
        serial::write_line("boot: guest-UEFI no frames for retained OVMF copy");
        return Err(GuestUefiLaunchError::LaunchSetupFailed);
    };
    let fw_hpa = fw_frame.to_phys();
    core::ptr::write_bytes(fw_hpa as *mut u8, 0, (pages * 4096) as usize);
    core::ptr::copy_nonoverlapping(bytes.as_ptr(), fw_hpa as *mut u8, bytes.len());

    let Some(ram_frame) = alloc.allocate_contiguous_aligned(512, 512) else {
        serial::write_line("boot: guest-UEFI no 2MiB RAM slab");
        return Err(GuestUefiLaunchError::LaunchSetupFailed);
    };
    let ram_hpa = ram_frame.to_phys();
    core::ptr::write_bytes(ram_hpa as *mut u8, 0, TWO_MIB as usize);

    let ept_need = frames_required_firmware_alias(gpa, fw_len);
    if ept_need > 8 {
        return Err(GuestUefiLaunchError::LaunchSetupFailed);
    }
    let mut ept_frames = [0u64; 8];
    for slot in ept_frames.iter_mut().take(ept_need) {
        let Some(f) = alloc_phys(alloc) else {
            serial::write_line("boot: guest-UEFI no EPT table frames");
            return Err(GuestUefiLaunchError::LaunchSetupFailed);
        };
        *slot = f;
    }
    let eptp = match ept_hw::build_firmware_alias_ept(
        gpa,
        fw_hpa,
        fw_len,
        ram_hpa,
        &mut ept_frames[..ept_need],
    ) {
        Ok(v) => v,
        Err(_) => {
            serial::write_line("boot: guest-UEFI alias EPT build failed");
            return Err(GuestUefiLaunchError::LaunchSetupFailed);
        }
    };
    let pml4 = ept_frames[0];
    if !ept_hw::gpa_is_mapped(pml4, GUEST_UEFI_RESET_VECTOR_GPA)
        || !ept_hw::gpa_is_mapped(pml4, gpa)
        || !ept_hw::gpa_is_mapped(pml4, 0)
    {
        serial::write_line("boot: guest-UEFI alias EPT walk failed");
        return Err(GuestUefiLaunchError::LaunchSetupFailed);
    }

    let Some(vmcs) = alloc_phys(alloc) else {
        return Err(GuestUefiLaunchError::LaunchSetupFailed);
    };
    let Some(stack_frame) = alloc.allocate_contiguous(4) else {
        return Err(GuestUefiLaunchError::LaunchSetupFailed);
    };
    let host_stack = stack_frame.to_phys();
    core::ptr::write_bytes(host_stack as *mut u8, 0, 4 * 4096);
    let host_rsp = (host_stack + 4 * 4096) & !0xFu64;
    let Some(gdt) = alloc_phys(alloc) else {
        return Err(GuestUefiLaunchError::LaunchSetupFailed);
    };
    let Some(tss) = alloc_phys(alloc) else {
        return Err(GuestUefiLaunchError::LaunchSetupFailed);
    };
    let io_a = alloc_phys(alloc);
    let io_b = alloc_phys(alloc);
    let msr_bmp = alloc_phys(alloc);

    SAVED_VMCS = vmcs;
    let _ = GUEST_UEFI_PRIVATE_VMCS_ID;

    serial::write_str("boot: guest-UEFI private VMCS=0x");
    write_hex(vmcs);
    serial::write_str(" EPTP=0x");
    write_hex(eptp);
    serial::write_str(" alias_gpa=0x");
    write_hex(gpa);
    serial::write_str(" fw_hpa=0x");
    write_hex(fw_hpa);
    serial::write_str(" bytes=");
    write_dec(fw_len);
    serial::write_byte(b'\n');

    if let Err(e) = setup_guest_uefi_vmcs(vmcs, host_rsp, gdt, tss, eptp, io_a, io_b, msr_bmp) {
        serial::write_str("boot: guest-UEFI VMCS setup failed: ");
        serial::write_line(match e {
            LaunchError::EptUnsupported => "EptUnsupported",
            LaunchError::PrepareFailed => "PrepareFailed",
            LaunchError::ClearFailed => "ClearFailed",
            LaunchError::PtrldFailed => "PtrldFailed",
            LaunchError::VmwriteFailed { .. } => "VmwriteFailed",
            LaunchError::LaunchFailed { .. } => "LaunchFailed",
            LaunchError::CpuidExitingUnsupported => "CpuidExitingUnsupported",
        });
        return Err(GuestUefiLaunchError::LaunchSetupFailed);
    }

    serial::write_line("boot: guest-UEFI VMLAUNCH → reset vector 0xFFFF_FFF0");
    match ops::vmlaunch() {
        Ok(()) => {
            serial::write_line("boot: ERROR — guest-UEFI VMLAUNCH returned Ok");
            Err(GuestUefiLaunchError::LaunchSetupFailed)
        }
        Err(_) => {
            let ierr = ops::vmread(VM_INSTRUCTION_ERROR).unwrap_or(0xFFFF) as u32;
            LAST_INSN_ERROR.store(ierr, Ordering::Release);
            serial::write_str("boot: guest-UEFI VMLAUNCH failed insn_error=0x");
            write_hex_u32(ierr);
            serial::write_byte(b'\n');
            if ierr == 7 {
                serial::write_line("boot: hint: error 7 = invalid VMX control field(s)");
            } else if ierr == 8 {
                serial::write_line("boot: hint: error 8 = invalid host-state");
            }
            let _ = ops::vmclear(vmcs);
            Err(GuestUefiLaunchError::LaunchSetupFailed)
        }
    }
}

#[cfg(target_os = "uefi")]
unsafe fn setup_guest_uefi_vmcs(
    vmcs: u64,
    host_rsp: u64,
    gdt: u64,
    tss: u64,
    eptp: u64,
    io_a: Option<u64>,
    io_b: Option<u64>,
    msr_bmp: Option<u64>,
) -> Result<(), LaunchError> {
    let (gdt_base, gdt_limit, tr, tr_base) = install_host_tss(gdt, tss)?;
    let use_true = true_ctl_msrs_supported();
    let pin_msr = if use_true {
        IA32_VMX_TRUE_PINBASED_CTLS
    } else {
        IA32_VMX_PINBASED_CTLS
    };
    let proc_msr = if use_true {
        IA32_VMX_TRUE_PROCBASED_CTLS
    } else {
        IA32_VMX_PROCBASED_CTLS
    };
    let exit_msr = if use_true {
        IA32_VMX_TRUE_EXIT_CTLS
    } else {
        IA32_VMX_EXIT_CTLS
    };
    let entry_msr = if use_true {
        IA32_VMX_TRUE_ENTRY_CTLS
    } else {
        IA32_VMX_ENTRY_CTLS
    };

    let pin = adjust_vmx_controls(PIN_BASED_EXTERNAL_INTERRUPT_EXITING, pin_msr);
    // Same wanted bits as E4, then drop unconditional I/O if bitmaps won
    // (SDM: the two I/O-exit controls must not both be 1).
    let mut primary = adjust_vmx_controls(
        CPU_BASED_HLT_EXITING
            | CPU_BASED_USE_IO_BITMAPS
            | CPU_BASED_UNCONDITIONAL_IO
            | CPU_BASED_USE_MSR_BITMAPS
            | CPU_BASED_ACTIVATE_SECONDARY,
        proc_msr,
    );
    if primary & CPU_BASED_USE_IO_BITMAPS != 0 {
        primary &= !CPU_BASED_UNCONDITIONAL_IO;
    }
    if primary & CPU_BASED_USE_TPR_SHADOW != 0 {
        serial::write_line("boot: guest-UEFI WARN — TPR shadow forced (no virt-APIC)");
    }
    if primary & CPU_BASED_ACTIVATE_SECONDARY == 0 {
        serial::write_line("boot: guest-UEFI secondary controls not allowed");
        return Err(LaunchError::EptUnsupported);
    }
    let exit_wanted = VM_EXIT_HOST_ADDR_SPACE_SIZE
        | VM_EXIT_ACK_INTERRUPT_ON_EXIT
        | VM_EXIT_SAVE_IA32_EFER
        | VM_EXIT_LOAD_IA32_EFER;
    let entry_wanted = VM_ENTRY_LOAD_IA32_EFER; // no IA-32e — real mode
    let exit_ctls = adjust_vmx_controls(exit_wanted, exit_msr);
    let entry_ctls = adjust_vmx_controls(entry_wanted, entry_msr);
    if entry_ctls & VM_ENTRY_IA32E_MODE != 0 {
        serial::write_line(
            "boot: guest-UEFI ERROR — IA-32e entry forced (need unrestricted real mode)",
        );
        return Err(LaunchError::EptUnsupported);
    }
    let secondary = adjust_vmx_controls(
        SECONDARY_ENABLE_EPT | GUEST_UEFI_UNRESTRICTED_GUEST,
        IA32_VMX_PROCBASED_CTLS2,
    );
    if secondary & SECONDARY_ENABLE_EPT == 0 {
        serial::write_line("boot: guest-UEFI EPT not allowed");
        return Err(LaunchError::EptUnsupported);
    }
    if secondary & GUEST_UEFI_UNRESTRICTED_GUEST == 0 {
        serial::write_line("boot: guest-UEFI unrestricted guest not allowed");
        return Err(LaunchError::EptUnsupported);
    }

    if primary & CPU_BASED_USE_IO_BITMAPS != 0 {
        let (Some(a), Some(b)) = (io_a, io_b) else {
            return Err(LaunchError::PrepareFailed);
        };
        core::ptr::write_bytes(a as *mut u8, 0xFF, 4096);
        core::ptr::write_bytes(b as *mut u8, 0xFF, 4096);
    }
    if primary & CPU_BASED_USE_MSR_BITMAPS != 0 {
        let Some(bmp) = msr_bmp else {
            return Err(LaunchError::PrepareFailed);
        };
        core::ptr::write_bytes(bmp as *mut u8, 0xFF, 4096);
    }

    let host_cr0 = cpu::read_cr0();
    let host_cr3 = cpu::read_cr3();
    let host_cr4 = cpu::read_cr4();
    let efer = cpu::rdmsr(IA32_EFER);
    let idtr = cpu::sidt();
    let cs = cpu::read_cs();
    let ss = cpu::read_ss();
    let ds = cpu::read_ds();
    let es = cpu::read_es();
    let fs = cpu::read_fs();
    let gs = cpu::read_gs();
    let fs_base = cpu::rdmsr(IA32_FS_BASE);
    let gs_base = cpu::rdmsr(IA32_GS_BASE);
    let sysenter_cs = cpu::rdmsr(IA32_SYSENTER_CS) as u32;
    let sysenter_esp = cpu::rdmsr(IA32_SYSENTER_ESP);
    let sysenter_eip = cpu::rdmsr(IA32_SYSENTER_EIP);

    let guest_cr0 = guest_cr0_real();
    let guest_cr4 = guest_cr4_real();

    prepare_vmcs_region(vmcs)?;
    ops::vmclear(vmcs).map_err(|_| LaunchError::ClearFailed)?;
    prepare_vmcs_region(vmcs)?;

    let host_rip = guest_uefi_vmexit as *const () as u64;

    match ops::vmptrld_and_vmwrite(vmcs, VMCS_LINK_POINTER, !0u64) {
        Ok(()) => {}
        Err(_) => {
            return Err(LaunchError::VmwriteFailed {
                field: VMCS_LINK_POINTER,
            })
        }
    }

    vw(PIN_BASED_VM_EXEC_CONTROL, pin as u64)?;
    vw(PRIMARY_PROC_BASED_VM_EXEC_CONTROL, primary as u64)?;
    vw(VM_EXIT_CONTROLS, exit_ctls as u64)?;
    vw(VM_ENTRY_CONTROLS, entry_ctls as u64)?;
    vw(EXCEPTION_BITMAP, 0)?;
    vw(PAGE_FAULT_ERROR_CODE_MASK, 0)?;
    vw(PAGE_FAULT_ERROR_CODE_MATCH, 0)?;
    vw(CR3_TARGET_COUNT, 0)?;
    vw(VM_EXIT_MSR_STORE_COUNT, 0)?;
    vw(VM_EXIT_MSR_LOAD_COUNT, 0)?;
    vw(VM_ENTRY_MSR_LOAD_COUNT, 0)?;
    vw(VM_ENTRY_INTERRUPTION_INFO, 0)?;
    vw(CR0_GUEST_HOST_MASK, 0)?;
    vw(CR4_GUEST_HOST_MASK, 0)?;
    vw(CR0_READ_SHADOW, 0)?;
    vw(CR4_READ_SHADOW, 0)?;
    vw(SECONDARY_VM_EXEC_CONTROL, secondary as u64)?;
    vw(EPT_POINTER, eptp)?;
    if primary & CPU_BASED_USE_MSR_BITMAPS != 0 {
        if let Some(bmp) = msr_bmp {
            vw(MSR_BITMAP, bmp)?;
        }
    }
    if primary & CPU_BASED_USE_IO_BITMAPS != 0 {
        if let (Some(a), Some(b)) = (io_a, io_b) {
            vw(IO_BITMAP_A, a)?;
            vw(IO_BITMAP_B, b)?;
        }
    }

    // Real-mode reset (SDM 9.1.4) + unrestricted guest.
    const AR_CODE16: u64 = 0x009B;
    const AR_DATA16: u64 = 0x0093;
    const AR_TR16: u64 = 0x008B;
    const AR_LDTR_UNUSABLE: u64 = 1 << 16;

    vw(GUEST_CS_SELECTOR, GUEST_UEFI_RESET_CS as u64)?;
    vw(GUEST_SS_SELECTOR, 0)?;
    vw(GUEST_DS_SELECTOR, 0)?;
    vw(GUEST_ES_SELECTOR, 0)?;
    vw(GUEST_FS_SELECTOR, 0)?;
    vw(GUEST_GS_SELECTOR, 0)?;
    vw(GUEST_LDTR_SELECTOR, 0)?;
    vw(GUEST_TR_SELECTOR, 0)?;

    vw(GUEST_CS_BASE, 0xFFFF_0000)?;
    vw(GUEST_SS_BASE, 0)?;
    vw(GUEST_DS_BASE, 0)?;
    vw(GUEST_ES_BASE, 0)?;
    vw(GUEST_FS_BASE, 0)?;
    vw(GUEST_GS_BASE, 0)?;
    vw(GUEST_LDTR_BASE, 0)?;
    vw(GUEST_TR_BASE, 0)?;
    vw(GUEST_GDTR_BASE, 0)?;
    vw(GUEST_IDTR_BASE, 0)?;

    vw(GUEST_CS_LIMIT, 0xFFFF)?;
    vw(GUEST_SS_LIMIT, 0xFFFF)?;
    vw(GUEST_DS_LIMIT, 0xFFFF)?;
    vw(GUEST_ES_LIMIT, 0xFFFF)?;
    vw(GUEST_FS_LIMIT, 0xFFFF)?;
    vw(GUEST_GS_LIMIT, 0xFFFF)?;
    vw(GUEST_LDTR_LIMIT, 0xFFFF)?;
    vw(GUEST_TR_LIMIT, 0xFFFF)?;
    vw(GUEST_GDTR_LIMIT, 0xFFFF)?;
    vw(GUEST_IDTR_LIMIT, 0xFFFF)?;

    vw(GUEST_CS_ACCESS_RIGHTS, AR_CODE16)?;
    vw(GUEST_SS_ACCESS_RIGHTS, AR_DATA16)?;
    vw(GUEST_DS_ACCESS_RIGHTS, AR_DATA16)?;
    vw(GUEST_ES_ACCESS_RIGHTS, AR_DATA16)?;
    vw(GUEST_FS_ACCESS_RIGHTS, AR_DATA16)?;
    vw(GUEST_GS_ACCESS_RIGHTS, AR_DATA16)?;
    vw(GUEST_LDTR_ACCESS_RIGHTS, AR_LDTR_UNUSABLE)?;
    vw(GUEST_TR_ACCESS_RIGHTS, AR_TR16)?;

    vw(GUEST_CR0, guest_cr0)?;
    vw(GUEST_CR3, 0)?;
    vw(GUEST_CR4, guest_cr4)?;
    vw(GUEST_DR7, 0x400)?;
    vw(GUEST_IA32_EFER, 0)?;
    vw(GUEST_RSP, 0)?;
    vw(GUEST_RIP, GUEST_UEFI_RESET_RIP)?;
    vw(GUEST_RFLAGS, 0x2)?;
    vw(GUEST_ACTIVITY_STATE, 0)?;
    vw(GUEST_INTERRUPTIBILITY_STATE, 0)?;
    vw(GUEST_PENDING_DBG_EXCEPTIONS, 0)?;
    vw(GUEST_IA32_SYSENTER_CS, 0)?;
    vw(GUEST_IA32_SYSENTER_ESP, 0)?;
    vw(GUEST_IA32_SYSENTER_EIP, 0)?;

    vw(HOST_ES_SELECTOR, (es & 0xF8) as u64)?;
    vw(HOST_CS_SELECTOR, (cs & 0xF8) as u64)?;
    vw(HOST_SS_SELECTOR, (ss & 0xF8) as u64)?;
    vw(HOST_DS_SELECTOR, (ds & 0xF8) as u64)?;
    vw(HOST_FS_SELECTOR, (fs & 0xF8) as u64)?;
    vw(HOST_GS_SELECTOR, (gs & 0xF8) as u64)?;
    vw(HOST_TR_SELECTOR, (tr & 0xF8) as u64)?;
    vw(HOST_CR0, host_cr0)?;
    vw(HOST_CR3, host_cr3)?;
    vw(HOST_CR4, host_cr4)?;
    vw(HOST_FS_BASE, fs_base)?;
    vw(HOST_GS_BASE, gs_base)?;
    vw(HOST_TR_BASE, tr_base)?;
    vw(HOST_GDTR_BASE, gdt_base)?;
    vw(HOST_IDTR_BASE, idtr.base)?;
    vw(HOST_IA32_SYSENTER_CS, sysenter_cs as u64)?;
    vw(HOST_IA32_SYSENTER_ESP, sysenter_esp)?;
    vw(HOST_IA32_SYSENTER_EIP, sysenter_eip)?;
    vw(HOST_IA32_EFER, efer)?;
    vw(HOST_RSP, host_rsp)?;
    vw(HOST_RIP, host_rip)?;

    let _ = gdt_limit;
    Ok(())
}

#[cfg(target_os = "uefi")]
unsafe fn guest_cr0_real() -> u64 {
    let fixed0 = cpu::rdmsr(IA32_VMX_CR0_FIXED0);
    let fixed1 = cpu::rdmsr(IA32_VMX_CR0_FIXED1);
    // Unrestricted guest: PE and PG in FIXED0 are not enforced.
    let mut cr0 = 0x6000_0010u64; // CD | NW | ET
    cr0 |= fixed0 & !0x8000_0001;
    cr0 &= fixed1;
    cr0
}

#[cfg(target_os = "uefi")]
unsafe fn guest_cr4_real() -> u64 {
    let fixed0 = cpu::rdmsr(IA32_VMX_CR4_FIXED0);
    let fixed1 = cpu::rdmsr(IA32_VMX_CR4_FIXED1);
    fixed0 & fixed1
}

#[cfg(target_os = "uefi")]
unsafe fn vw(field: u64, value: u64) -> Result<(), LaunchError> {
    ops::vmwrite(field, value).map_err(|_| LaunchError::VmwriteFailed { field })
}

/// HOST_RIP for the private guest-UEFI VMCS. Not the E4 SHELL landing.
#[cfg(target_os = "uefi")]
pub unsafe extern "C" fn guest_uefi_vmexit() -> ! {
    LAUNCH_ENTERED.store(true, Ordering::Release);
    let reason = ops::vmread(EXIT_REASON).unwrap_or(0xFFFF) as u32;
    let qual = ops::vmread(EXIT_QUALIFICATION).unwrap_or(0);
    let rip = ops::vmread(GUEST_RIP).unwrap_or(0);
    let cs_base = ops::vmread(GUEST_CS_BASE).unwrap_or(0);
    let gpa = ops::vmread(GUEST_PHYSICAL_ADDRESS).unwrap_or(0);
    LAST_EXIT_REASON.store(reason, Ordering::Release);
    LAST_GUEST_RIP.store(rip, Ordering::Release);
    LAST_GUEST_PHYS.store(gpa, Ordering::Release);

    serial::write_str("boot: guest-UEFI VMEXIT reason=0x");
    write_hex_u32(reason);
    serial::write_str(" rip=0x");
    write_hex(rip);
    serial::write_str(" cs_base=0x");
    write_hex(cs_base);
    serial::write_str(" qual=0x");
    write_hex(qual);
    serial::write_str(" gpa=0x");
    write_hex(gpa);
    serial::write_byte(b'\n');

    let basic = reason & 0xFFFF;
    let entry_fail = (reason & 0x8000_0000) != 0
        || basic == EXIT_REASON_VMENTRY_GUEST_STATE
        || basic == EXIT_REASON_VMENTRY_MSR_LOAD;
    let linear = cs_base.wrapping_add(rip);
    let fetch_fail = basic == EXIT_REASON_EPT_VIOLATION && gpa == GUEST_UEFI_RESET_VECTOR_GPA;

    if !entry_fail && !fetch_fail {
        if !MARKER_PRINTED.swap(true, Ordering::AcqRel) {
            serial::write_line(M7_E5_OVMF_VMLAUNCH_OK_MARKER);
            serial::write_str("boot: guest-UEFI linear=0x");
            write_hex(linear);
            serial::write_byte(b'\n');
        }
        audit_log!(AuditEvent::OvmfGuestUefiVmlaunched {
            exit_reason: reason as u64,
            guest_rip: rip,
        });
    } else {
        serial::write_line("boot: guest-UEFI VM-entry/fetch failed — marker not claimed");
    }

    let vmcs = SAVED_VMCS;
    if vmcs != 0 {
        let _ = ops::vmclear(vmcs);
    }

    let rsp = E4_RSP;
    let rip_cont = E4_RESUME;
    if rsp != 0 && rip_cont != 0 {
        core::arch::asm!(
            "mov rsp, {rsp}",
            "jmp {rip}",
            rsp = in(reg) rsp,
            rip = in(reg) rip_cont,
            options(noreturn),
        );
    }
    loop {
        core::hint::spin_loop();
    }
}

/// Remember the E4 SHELL continuation (original UEFI stack).
pub unsafe fn arm_e4_resume(
    alloc: *mut FrameAllocator,
    life: *mut crate::vmx::lifecycle::VmxLifecycle,
    resume: extern "C" fn() -> !,
) {
    E4_ALLOC = alloc;
    E4_LIFE = life;
    E4_RESUME = resume as usize as u64;
    let mut rsp: u64;
    core::arch::asm!("mov {rsp}, rsp", rsp = out(reg) rsp);
    E4_RSP = rsp;
}

pub fn e4_alloc() -> *mut FrameAllocator {
    unsafe { E4_ALLOC }
}

pub fn e4_life() -> *mut crate::vmx::lifecycle::VmxLifecycle {
    unsafe { E4_LIFE }
}

#[cfg(target_os = "uefi")]
fn write_hex_inner(mut n: u64) {
    let mut buf = [0u8; 16];
    let mut i = 16;
    if n == 0 {
        serial::write_byte(b'0');
        return;
    }
    while n > 0 && i > 0 {
        i -= 1;
        let d = (n & 0xF) as u8;
        buf[i] = if d < 10 { b'0' + d } else { b'a' + (d - 10) };
        n >>= 4;
    }
    for &b in &buf[i..] {
        serial::write_byte(b);
    }
}

#[cfg(target_os = "uefi")]
fn write_hex(n: u64) {
    write_hex_inner(n);
}

#[cfg(target_os = "uefi")]
fn write_hex_u32(n: u32) {
    write_hex_inner(n as u64);
}

#[cfg(target_os = "uefi")]
fn write_dec(mut n: u64) {
    if n == 0 {
        serial::write_byte(b'0');
        return;
    }
    let mut buf = [0u8; 20];
    let mut i = 20;
    while n > 0 && i > 0 {
        i -= 1;
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
    }
    for &b in &buf[i..] {
        serial::write_byte(b);
    }
}

#[cfg(test)]
#[path = "guest_uefi_test.rs"]
mod guest_uefi_test;
