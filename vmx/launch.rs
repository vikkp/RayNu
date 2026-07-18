//! M1.2 — minimal VMLAUNCH → one VMEXIT (guest HLT), no EPT.
//!
//! Pillar: [V]
//! Proven Core: **inside** (ADR-002)
//! VERIFICATION: L1 — control words adjusted from capability MSRs
//!
//! Guest shares the host address space (same CR3). Guest RIP points at a
//! `hlt` stub in the HV `.text` section (OVMF often maps bump frames NX).

use crate::arch::cpu::{
    self, adjust_vmx_controls, true_ctl_msrs_supported, IA32_EFER, IA32_FS_BASE, IA32_GS_BASE,
    IA32_PAT, IA32_SYSENTER_CS, IA32_SYSENTER_EIP, IA32_SYSENTER_ESP, IA32_VMX_BASIC,
    IA32_VMX_ENTRY_CTLS, IA32_VMX_EXIT_CTLS, IA32_VMX_PINBASED_CTLS, IA32_VMX_PROCBASED_CTLS,
    IA32_VMX_PROCBASED_CTLS2, IA32_VMX_TRUE_ENTRY_CTLS, IA32_VMX_TRUE_EXIT_CTLS,
    IA32_VMX_TRUE_PINBASED_CTLS, IA32_VMX_TRUE_PROCBASED_CTLS,
};
use crate::boot::serial;
use crate::vmx::fields::*;
use crate::vmx::hardware;
use crate::vmx::ops::{self, VmFailKind, VmcsOpError};

/// COM1 marker when the first guest HLT produces a VMEXIT (M1.2 gate).
pub const M1_VMEXIT_OK_MARKER: &str = "RAYNU-V-M1-VMEXIT-OK";

/// Exit-control bits for IA32_PAT load/save (SDM Vol. 3).
const VM_EXIT_SAVE_IA32_PAT: u32 = 1 << 18;
const VM_EXIT_LOAD_IA32_PAT: u32 = 1 << 19;
/// Exit-control: save debug controls (often forced in allowed0).
const VM_EXIT_SAVE_DEBUG_CONTROLS: u32 = 1 << 2;
/// Entry-control: load debug controls.
const VM_ENTRY_LOAD_DEBUG_CONTROLS: u32 = 1 << 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchError {
    PrepareFailed,
    ClearFailed,
    PtrldFailed,
    /// VMWRITE failed; `field` is the VMCS encoding that was rejected.
    VmwriteFailed { field: u64 },
    LaunchFailed { instruction_error: u32 },
}

impl From<VmcsOpError> for LaunchError {
    fn from(_: VmcsOpError) -> Self {
        // Prefer the typed `vw()` path which records the field encoding.
        Self::VmwriteFailed { field: 0xffff_ffff }
    }
}

/// Physical frames needed for the M1.2 HLT guest.
pub struct LaunchFrames {
    pub vmcs_phys: u64,
    pub guest_stack_phys: u64,
    pub host_stack_phys: u64,
    /// Optional MSR bitmap (only if primary controls force USE_MSR_BITMAPS).
    pub msr_bitmap_phys: Option<u64>,
    pub io_bitmap_a_phys: Option<u64>,
    pub io_bitmap_b_phys: Option<u64>,
}

/// Guest entry: execute HLT (exit reason 12), then spin if ever resumed.
///
/// Lives in HV `.text` so the page is executable under typical OVMF identity maps.
#[unsafe(naked)]
pub unsafe extern "C" fn guest_hlt_entry() {
    core::arch::naked_asm!(
        "hlt",
        "2: jmp 2b",
    );
}

/// Prepare VMCS revision ID in the first dword (same as VMXON region).
///
/// SAFETY: `region_phys` is a writable identity-mapped 4K frame.
pub unsafe fn prepare_vmcs_region(region_phys: u64) -> Result<(), LaunchError> {
    debug_assert_eq!(region_phys & 0xfff, 0);
    let basic = cpu::rdmsr(IA32_VMX_BASIC);
    let revision = (basic as u32) & 0x7fff_ffff;
    let ptr = region_phys as *mut u8;
    core::ptr::write_bytes(ptr, 0, 4096);
    core::ptr::write_volatile(ptr.cast::<u32>(), revision);
    Ok(())
}

fn ar_busy_tr(mut ar: u32) -> u32 {
    // Available 32/64-bit TSS (9) must be busy (B) for VMCS host/guest TR.
    if (ar & 0xF) == 0x9 {
        ar = (ar & !0xF) | 0xB;
    }
    ar
}

/// Flip GDT TSS type from available (9) to busy (B) so VMEXIT can reload TR.
///
/// SAFETY: `gdt_base` is the live GDTR base; `tr` selects a TSS descriptor.
unsafe fn ensure_tr_busy(gdt_base: u64, tr: u16) {
    if tr & 0xFFFC == 0 {
        return;
    }
    let index = (tr >> 3) as usize;
    let desc = (gdt_base as *mut u64).add(index);
    let d0 = core::ptr::read_unaligned(desc);
    let ty = ((d0 >> 40) & 0xF) as u8;
    if ty == 0x9 {
        core::ptr::write_unaligned(desc, d0 | (0x2u64 << 40));
    }
}

unsafe fn seg_ar(gdt_base: u64, sel: u16) -> u32 {
    if sel & 0xFFFC == 0 {
        return 1 << 16;
    }
    ar_busy_tr(cpu::segment_access_rights(gdt_base, sel))
}

fn fail_kind_name(k: VmFailKind) -> &'static str {
    match k {
        VmFailKind::Invalid => "Invalid(CF=no-current-VMCS)",
        VmFailKind::Valid => "Valid(ZF=insn-error)",
        VmFailKind::Both => "Both(CF+ZF)",
    }
}

unsafe fn report_vmwrite_fail(tag: &str, field: u64, kind: VmFailKind, expected_vmcs: u64) {
    serial::write_str("boot: ");
    serial::write_str(tag);
    serial::write_str(" failed field=0x");
    write_hex_u32(field as u32);
    serial::write_str(" kind=");
    serial::write_str(fail_kind_name(kind));
    serial::write_byte(b'\n');
    if let Ok(cur) = ops::vmptrst() {
        serial::write_str("boot: VMPTRST=0x");
        write_hex_u64(cur);
        serial::write_str(" expected=0x");
        write_hex_u64(expected_vmcs);
        serial::write_byte(b'\n');
    }
    // SDM App. C: 12 = unsupported field, 13 = write to read-only field.
    if let Ok(ierr) = ops::vmread(VM_INSTRUCTION_ERROR) {
        serial::write_str("boot: VM_INSTRUCTION_ERROR=");
        write_dec_u32(ierr as u32);
        serial::write_byte(b'\n');
        if ierr == 12 {
            serial::write_line(
                "boot: hint: error 12 = nested shadow VMCS rejected VMWRITE",
            );
            serial::write_line(
                "boot: fix on host: sudo ./tools/enable-nested-kvm.sh",
            );
        }
    }
}

unsafe fn vw(field: u64, value: u64) -> Result<(), LaunchError> {
    match ops::vmwrite_detailed(field, value) {
        Ok(()) => Ok(()),
        Err(kind) => {
            report_vmwrite_fail("VMWRITE", field, kind, 0);
            Err(LaunchError::VmwriteFailed { field })
        }
    }
}

fn write_dec_u32(mut n: u32) {
    let mut buf = [0u8; 10];
    let mut i = buf.len();
    if n == 0 {
        serial::write_byte(b'0');
        return;
    }
    while n > 0 {
        i -= 1;
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
    }
    for &b in &buf[i..] {
        serial::write_byte(b);
    }
}

/// Program a minimal long-mode guest that executes HLT (no EPT).
///
/// On success, does not return — VMEXIT lands in [`vmexit_landing`].
///
/// SAFETY: CPU in VMX root; frames exclusively owned; identity map.
pub unsafe fn run_hlt_guest(frames: &LaunchFrames) -> Result<(), LaunchError> {
    prepare_vmcs_region(frames.vmcs_phys)?;

    ops::vmclear(frames.vmcs_phys).map_err(|_| LaunchError::ClearFailed)?;
    // Nested VT-x has been observed to disturb the revision dword across
    // VMCLEAR; rewrite it before any VMPTRLD.
    prepare_vmcs_region(frames.vmcs_phys)?;
    // VMPTRLD is deferred until after all RDMSR/serial gather work inside
    // setup_vmcs (nested VT-x can drop current-VMCS across those exits).

    setup_vmcs(frames)?;

    serial::write_line("boot: VMLAUNCH → guest HLT");
    match ops::vmlaunch() {
        Ok(()) => {
            // Architecturally unreachable: success transfers to HOST_RIP.
            serial::write_line("boot: ERROR — VMLAUNCH returned Ok");
            Err(LaunchError::LaunchFailed {
                instruction_error: 0xffff,
            })
        }
        Err(_) => {
            let ierr = ops::vmread(VM_INSTRUCTION_ERROR).unwrap_or(0xFFFF) as u32;
            serial::write_str("boot: ERROR — VMLAUNCH failed insn_error=0x");
            write_hex_u32(ierr);
            serial::write_byte(b'\n');
            Err(LaunchError::LaunchFailed {
                instruction_error: ierr,
            })
        }
    }
}

unsafe fn setup_vmcs(frames: &LaunchFrames) -> Result<(), LaunchError> {
    // ── Phase 1: gather everything that may VM-exit under nested VT-x ──
    // (RDMSR, serial, GDT walks). No current-VMCS required yet.
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

    let pin = adjust_vmx_controls(0, pin_msr);
    let primary = adjust_vmx_controls(CPU_BASED_HLT_EXITING, proc_msr);
    let exit_wanted =
        VM_EXIT_HOST_ADDR_SPACE_SIZE | VM_EXIT_SAVE_IA32_EFER | VM_EXIT_LOAD_IA32_EFER;
    let entry_wanted = VM_ENTRY_IA32E_MODE | VM_ENTRY_LOAD_IA32_EFER;
    let exit_ctls = adjust_vmx_controls(exit_wanted, exit_msr);
    let entry_ctls = adjust_vmx_controls(entry_wanted, entry_msr);

    let secondary = if primary & CPU_BASED_ACTIVATE_SECONDARY != 0 {
        Some(adjust_vmx_controls(0, IA32_VMX_PROCBASED_CTLS2))
    } else {
        None
    };

    let msr_bitmap = if primary & CPU_BASED_USE_MSR_BITMAPS != 0 {
        let bmp = frames.msr_bitmap_phys.ok_or(LaunchError::PrepareFailed)?;
        core::ptr::write_bytes(bmp as *mut u8, 0, 4096);
        Some(bmp)
    } else {
        None
    };

    let io_bitmaps = if primary & CPU_BASED_USE_IO_BITMAPS != 0 {
        let a = frames.io_bitmap_a_phys.ok_or(LaunchError::PrepareFailed)?;
        let b = frames.io_bitmap_b_phys.ok_or(LaunchError::PrepareFailed)?;
        core::ptr::write_bytes(a as *mut u8, 0, 4096);
        core::ptr::write_bytes(b as *mut u8, 0, 4096);
        Some((a, b))
    } else {
        None
    };

    let cr0 = cpu::read_cr0();
    let cr3 = cpu::read_cr3();
    let cr4 = cpu::read_cr4();
    let efer = cpu::rdmsr(IA32_EFER);
    let pat = cpu::rdmsr(IA32_PAT);
    let dr7 = cpu::read_dr7();
    let gdtr = cpu::sgdt();
    let idtr = cpu::sidt();
    let gdt_base = gdtr.base;

    let cs = cpu::read_cs();
    let ss = cpu::read_ss();
    let ds = cpu::read_ds();
    let es = cpu::read_es();
    let fs = cpu::read_fs();
    let gs = cpu::read_gs();
    let tr = cpu::read_tr();
    let ldtr = cpu::read_ldtr();

    let fs_base = cpu::rdmsr(IA32_FS_BASE);
    let gs_base = cpu::rdmsr(IA32_GS_BASE);
    let sysenter_cs = cpu::rdmsr(IA32_SYSENTER_CS) as u32;
    let sysenter_esp = cpu::rdmsr(IA32_SYSENTER_ESP);
    let sysenter_eip = cpu::rdmsr(IA32_SYSENTER_EIP);

    let es_base = cpu::segment_base(gdt_base, es);
    let cs_base = cpu::segment_base(gdt_base, cs);
    let ss_base = cpu::segment_base(gdt_base, ss);
    let ds_base = cpu::segment_base(gdt_base, ds);
    let ldtr_base = cpu::segment_base(gdt_base, ldtr);
    let tr_base = cpu::segment_base(gdt_base, tr);

    let es_limit = cpu::segment_limit(es) as u64;
    let cs_limit = cpu::segment_limit(cs) as u64;
    let ss_limit = cpu::segment_limit(ss) as u64;
    let ds_limit = cpu::segment_limit(ds) as u64;
    let fs_limit = cpu::segment_limit(fs) as u64;
    let gs_limit = cpu::segment_limit(gs) as u64;
    let ldtr_limit = cpu::segment_limit(ldtr) as u64;
    let tr_limit = cpu::segment_limit(tr) as u64;

    let es_ar = seg_ar(gdt_base, es) as u64;
    let cs_ar = seg_ar(gdt_base, cs) as u64;
    let ss_ar = seg_ar(gdt_base, ss) as u64;
    let ds_ar = seg_ar(gdt_base, ds) as u64;
    let fs_ar = seg_ar(gdt_base, fs) as u64;
    let gs_ar = seg_ar(gdt_base, gs) as u64;
    let ldtr_ar = seg_ar(gdt_base, ldtr) as u64;
    let tr_ar = seg_ar(gdt_base, tr) as u64;

    let need_efer = (exit_ctls & (VM_EXIT_SAVE_IA32_EFER | VM_EXIT_LOAD_IA32_EFER)) != 0
        || (entry_ctls & VM_ENTRY_LOAD_IA32_EFER) != 0;
    let need_pat = (exit_ctls & (VM_EXIT_SAVE_IA32_PAT | VM_EXIT_LOAD_IA32_PAT)) != 0;
    let need_debugctl = (exit_ctls & VM_EXIT_SAVE_DEBUG_CONTROLS) != 0
        || (entry_ctls & VM_ENTRY_LOAD_DEBUG_CONTROLS) != 0;

    let guest_rip = guest_hlt_entry as *const () as u64;
    let guest_rsp = frames.guest_stack_phys + 4096;
    let host_rsp = (frames.host_stack_phys + 4096) & !0xFu64;
    let host_rip = vmexit_landing as *const () as u64;

    // Host TR load on VMEXIT requires a busy TSS descriptor in the GDT.
    ensure_tr_busy(gdt_base, tr);

    serial::write_str("boot: VMCS ctls pin=0x");
    write_hex_u32(pin);
    serial::write_str(" primary=0x");
    write_hex_u32(primary);
    serial::write_str(" exit=0x");
    write_hex_u32(exit_ctls);
    serial::write_str(" entry=0x");
    write_hex_u32(entry_ctls);
    serial::write_byte(b'\n');

    // ── Phase 2: VMPTRLD + VMWRITE burst (no RDMSR / serial / I/O) ──
    // Canary: VMCS link pointer is a universally supported RW field.
    match ops::vmptrld_and_vmwrite(frames.vmcs_phys, VMCS_LINK_POINTER, !0u64) {
        Ok(()) => {}
        Err(kind) => {
            report_vmwrite_fail("VMPTRLD+VMWRITE(link)", VMCS_LINK_POINTER, kind, frames.vmcs_phys);
            return Err(LaunchError::VmwriteFailed {
                field: VMCS_LINK_POINTER,
            });
        }
    }

    match ops::vmwrite_detailed(PIN_BASED_VM_EXEC_CONTROL, pin as u64) {
        Ok(()) => {}
        Err(kind) => {
            report_vmwrite_fail("VMWRITE(pin)", PIN_BASED_VM_EXEC_CONTROL, kind, frames.vmcs_phys);
            return Err(LaunchError::VmwriteFailed {
                field: PIN_BASED_VM_EXEC_CONTROL,
            });
        }
    }

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
    // VMCS_LINK_POINTER already written as the VMPTRLD canary above.

    if let Some(sec) = secondary {
        vw(SECONDARY_VM_EXEC_CONTROL, sec as u64)?;
    }
    if let Some(bmp) = msr_bitmap {
        vw(MSR_BITMAP, bmp)?;
    }
    if let Some((a, b)) = io_bitmaps {
        vw(IO_BITMAP_A, a)?;
        vw(IO_BITMAP_B, b)?;
    }

    vw(GUEST_ES_SELECTOR, es as u64)?;
    vw(GUEST_CS_SELECTOR, cs as u64)?;
    vw(GUEST_SS_SELECTOR, ss as u64)?;
    vw(GUEST_DS_SELECTOR, ds as u64)?;
    vw(GUEST_FS_SELECTOR, fs as u64)?;
    vw(GUEST_GS_SELECTOR, gs as u64)?;
    vw(GUEST_LDTR_SELECTOR, ldtr as u64)?;
    vw(GUEST_TR_SELECTOR, tr as u64)?;

    vw(GUEST_ES_BASE, es_base)?;
    vw(GUEST_CS_BASE, cs_base)?;
    vw(GUEST_SS_BASE, ss_base)?;
    vw(GUEST_DS_BASE, ds_base)?;
    vw(GUEST_FS_BASE, fs_base)?;
    vw(GUEST_GS_BASE, gs_base)?;
    vw(GUEST_LDTR_BASE, ldtr_base)?;
    vw(GUEST_TR_BASE, tr_base)?;
    vw(GUEST_GDTR_BASE, gdtr.base)?;
    vw(GUEST_IDTR_BASE, idtr.base)?;

    vw(GUEST_ES_LIMIT, es_limit)?;
    vw(GUEST_CS_LIMIT, cs_limit)?;
    vw(GUEST_SS_LIMIT, ss_limit)?;
    vw(GUEST_DS_LIMIT, ds_limit)?;
    vw(GUEST_FS_LIMIT, fs_limit)?;
    vw(GUEST_GS_LIMIT, gs_limit)?;
    vw(GUEST_LDTR_LIMIT, ldtr_limit)?;
    vw(GUEST_TR_LIMIT, tr_limit)?;
    vw(GUEST_GDTR_LIMIT, gdtr.limit as u64)?;
    vw(GUEST_IDTR_LIMIT, idtr.limit as u64)?;

    vw(GUEST_ES_ACCESS_RIGHTS, es_ar)?;
    vw(GUEST_CS_ACCESS_RIGHTS, cs_ar)?;
    vw(GUEST_SS_ACCESS_RIGHTS, ss_ar)?;
    vw(GUEST_DS_ACCESS_RIGHTS, ds_ar)?;
    vw(GUEST_FS_ACCESS_RIGHTS, fs_ar)?;
    vw(GUEST_GS_ACCESS_RIGHTS, gs_ar)?;
    vw(GUEST_LDTR_ACCESS_RIGHTS, ldtr_ar)?;
    vw(GUEST_TR_ACCESS_RIGHTS, tr_ar)?;

    vw(GUEST_CR0, cr0)?;
    vw(GUEST_CR3, cr3)?;
    vw(GUEST_CR4, cr4)?;
    vw(GUEST_DR7, dr7)?;

    if need_efer {
        vw(GUEST_IA32_EFER, efer)?;
    }
    if need_pat {
        vw(GUEST_IA32_PAT, pat)?;
    }
    if need_debugctl {
        vw(GUEST_IA32_DEBUGCTL, 0)?;
    }

    vw(GUEST_RSP, guest_rsp)?;
    vw(GUEST_RIP, guest_rip)?;
    vw(GUEST_RFLAGS, 0x2)?;
    vw(GUEST_ACTIVITY_STATE, 0)?;
    vw(GUEST_INTERRUPTIBILITY_STATE, 0)?;
    vw(GUEST_PENDING_DBG_EXCEPTIONS, 0)?;
    vw(GUEST_IA32_SYSENTER_CS, sysenter_cs as u64)?;
    vw(GUEST_IA32_SYSENTER_ESP, sysenter_esp)?;
    vw(GUEST_IA32_SYSENTER_EIP, sysenter_eip)?;

    vw(HOST_ES_SELECTOR, (es & 0xF8) as u64)?;
    vw(HOST_CS_SELECTOR, (cs & 0xF8) as u64)?;
    vw(HOST_SS_SELECTOR, (ss & 0xF8) as u64)?;
    vw(HOST_DS_SELECTOR, (ds & 0xF8) as u64)?;
    vw(HOST_FS_SELECTOR, (fs & 0xF8) as u64)?;
    vw(HOST_GS_SELECTOR, (gs & 0xF8) as u64)?;
    vw(HOST_TR_SELECTOR, (tr & 0xF8) as u64)?;

    vw(HOST_CR0, cr0)?;
    vw(HOST_CR3, cr3)?;
    vw(HOST_CR4, cr4)?;
    vw(HOST_FS_BASE, fs_base)?;
    vw(HOST_GS_BASE, gs_base)?;
    vw(HOST_TR_BASE, tr_base)?;
    vw(HOST_GDTR_BASE, gdtr.base)?;
    vw(HOST_IDTR_BASE, idtr.base)?;
    vw(HOST_IA32_SYSENTER_CS, sysenter_cs as u64)?;
    vw(HOST_IA32_SYSENTER_ESP, sysenter_esp)?;
    vw(HOST_IA32_SYSENTER_EIP, sysenter_eip)?;

    if need_efer {
        vw(HOST_IA32_EFER, efer)?;
    }
    if need_pat {
        vw(HOST_IA32_PAT, pat)?;
    }

    vw(HOST_RSP, host_rsp)?;
    vw(HOST_RIP, host_rip)?;

    Ok(())
}

/// HOST_RIP target — runs after the first VMEXIT with host RSP restored.
///
/// GPRs still hold guest values; only use stack locals / known addresses.
pub unsafe extern "C" fn vmexit_landing() -> ! {
    let reason = ops::vmread(EXIT_REASON).unwrap_or(0xFFFF) as u32;
    let basic = reason & 0xFFFF;
    let qual = ops::vmread(EXIT_QUALIFICATION).unwrap_or(0);

    serial::write_str("boot: VMEXIT reason=0x");
    write_hex_u32(basic);
    serial::write_str(" qual=0x");
    write_hex_u64(qual);
    serial::write_byte(b'\n');

    if basic == EXIT_REASON_HLT {
        serial::write_line(M1_VMEXIT_OK_MARKER);
    } else {
        serial::write_line("boot: ERROR — expected HLT exit (reason 12)");
    }

    match hardware::vmxoff() {
        Ok(()) => serial::write_line("boot: VMXOFF ok"),
        Err(_) => serial::write_line("boot: ERROR — VMXOFF failed"),
    }

    serial::write_line("boot: M1.2 complete");
    if basic == EXIT_REASON_HLT {
        serial::qemu_exit_success();
    } else {
        serial::qemu_exit_failure();
    }

    loop {
        core::hint::spin_loop();
    }
}

fn write_hex_u32(mut n: u32) {
    let mut buf = [b'0'; 8];
    for i in (0..8).rev() {
        let d = (n & 0xf) as u8;
        buf[i] = if d < 10 { b'0' + d } else { b'a' + (d - 10) };
        n >>= 4;
    }
    for &b in &buf {
        serial::write_byte(b);
    }
}

fn write_hex_u64(mut n: u64) {
    let mut buf = [b'0'; 16];
    for i in (0..16).rev() {
        let d = (n & 0xf) as u8;
        buf[i] = if d < 10 { b'0' + d } else { b'a' + (d - 10) };
        n >>= 4;
    }
    for &b in &buf {
        serial::write_byte(b);
    }
}

#[cfg(test)]
mod launch_test {
    use super::*;

    #[test]
    fn marker_stable() {
        assert_eq!(M1_VMEXIT_OK_MARKER, "RAYNU-V-M1-VMEXIT-OK");
        assert_eq!(EXIT_REASON_HLT, 12);
    }
}
