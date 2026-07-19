//! M1.2 / M2.x — VMLAUNCH under EPT: store + loop + HLT + IRQ inject.
//!
//! Pillar: [V]
//! Proven Core: **inside** (ADR-002, ADR-004)
//! VERIFICATION: L1 — control words + EPTP + ownership + inject firewall
//!
//! Guest shares host CR3 (UEFI identity paging). EPT identity-maps the first
//! precise window so GPA→HPA is 1:1. Guest RIP points at an owned page that stores a
//! magic value, runs a short increment loop, then `hlt`. On the first HLT
//! exit the host injects vector [`crate::sched::M2_IRQ_VECTOR`] via VM-entry
//! interruption-info and VMRESUMEs; the guest ISR acks and HLTs again.
//! M2.5 arms a host LAPIC one-shot; guest waits in real HLT; external-interrupt
//! VMEXIT → EOI → re-inject → `RAYNU-V-M2-TIMER-OK`.
//! M3.0: guest COM1 `out` → I/O VMEXIT → host UART → `RAYNU-V-M3-IO-OK`.
//! M3.1: guest CPUID → filter (hide VMX) → `RAYNU-V-M3-CPUID-OK`.
//! M3.3: after timer path, enter proto-kernel (RSI=`boot_params`) → early OK.
//! M3.4: post-proto guest timer → ext-IRQ → EOI → inject → `RAYNU-V-M3-GTIMER-OK`.
//! M3.5: proto-init OUT shell marker → `RAYNU-V-M3-SHELL-OK` (closes synthetic M3).
//! M3.6: after SHELL-OK, continuous HLT resume loop → `RAYNU-V-M3-LOOP-OK`.
//! M3.7: bzImage PM+0x200 entry via [`set_linux_load`] → `RAYNU-V-M3-BZIMAGE-OK`.
//! M3.8: real Linux earlyprintk banner → `RAYNU-V-M3-LINUX-EARLY-OK`.
//! M3.9: MSR allow-list emulate + post-banner host LAPIC → `RAYNU-V-M3-GTIMER2-OK`.
//! M3.10: real `/init` on initrd prints shell magic → `RAYNU-V-M3-SHELL-OK`.
//! M3.11: EPT hole + virtual LAPIC timer → `RAYNU-V-M3-GTIMER3-OK` (drop `nolapic`).
//! M3.12: IRR/ISR LVT inject → `RAYNU-V-M3-APIC-OK`; drop host→IRQ0 after APIC-OK.
//! M3.19: drop ISA IRQ4 COM1 TX inject; SHELL via CPUID; no `console=ttyS0`.
//! IRQ0 retained only until SHELL (APIC calibrate jiffies). → `RAYNU-V-M3-NOIRQ-OK`.
//! At Linux entry, host-own CR4.VMXE (mask + shadow) so `startup_64` can clear
//! guest-visible CR4 without #GP.
//! Markers: …/BZIMAGE/LINUX-EARLY/GTIMER2/GTIMER3/APIC/SHELL/NOIRQ (real).

use crate::arch::apic;
use crate::arch::cpu::{
    self, adjust_vmx_controls, true_ctl_msrs_supported, IA32_EFER, IA32_FS_BASE, IA32_GS_BASE,
    IA32_PAT, IA32_SYSENTER_CS, IA32_SYSENTER_EIP, IA32_SYSENTER_ESP, IA32_VMX_BASIC,
    IA32_VMX_ENTRY_CTLS, IA32_VMX_EXIT_CTLS, IA32_VMX_PINBASED_CTLS, IA32_VMX_PROCBASED_CTLS,
    IA32_VMX_PROCBASED_CTLS2, IA32_VMX_TRUE_ENTRY_CTLS, IA32_VMX_TRUE_EXIT_CTLS,
    IA32_VMX_TRUE_PINBASED_CTLS, IA32_VMX_TRUE_PROCBASED_CTLS,
};
use crate::boot::serial;
use crate::devices::lapic_virt::{self, M3_APIC_OK_MARKER, M3_GTIMER3_OK_MARKER};
use crate::devices::serial_pio::{
    self, M3_EARLY_OK_MARKER, M3_IO_OK_MARKER, M3_LINUX_EARLY_OK_MARKER, M3_SHELL_OK_MARKER,
    SHELL_CPUID_LEAF, SHELL_CPUID_SUBLEAF,
};

/// Finish marker when IRQ4 inject is gone and IRQ0 stops at SHELL (M3.19).
pub const M3_NOIRQ_OK_MARKER: &str = "RAYNU-V-M3-NOIRQ-OK";
use crate::vmx::{guest_pt, mmio_decode};
use crate::memory::ept::{self, M2_OWN_OK_MARKER};
use crate::memory::ept_hw::{self, GUEST_ISR_OFF, M2_EPT_OK_MARKER, M2_GUEST_OK_MARKER};
use crate::memory::frame_allocator::{self, M2_ALLOC_OK_MARKER};
use crate::sched::interrupt::{
    self, M2_IRQ_OK_MARKER, M2_IRQ_VECTOR, M2_TIMER_OK_MARKER, M3_GTIMER2_OK_MARKER,
    M3_GTIMER_OK_MARKER,
};
use crate::sched::msr_firewall::{self, MsrAccess, MsrAction, M3_CPUID_OK_MARKER};
use crate::vmx::fields::*;
use crate::vmx::hardware;
use crate::vmx::ops::{self, VmFailKind, VmcsOpError};

/// Exit-phase state machine (M2.4 / M2.5 / M3.3–M3.6):
/// 0 = first HLT → software inject
/// 1 = ISR HLT → IRQ-OK, arm LAPIC, wait (HLT exiting off)
/// 2 = external-interrupt VMEXIT → EOI → re-inject
/// 3 = ISR HLT after timer path → TIMER-OK (+ M3.0/M3.1) → enter proto-kernel
/// 4 = proto-kernel HLT → EARLY-OK → arm guest timer
/// 5 = post-proto external-interrupt → EOI → inject
/// 6 = ISR HLT → GTIMER-OK → enter proto-init
/// 7 = proto-init HLT → SHELL-OK → enter continuous loop
/// 8 = durable HLT resume loop → LOOP-OK
static mut EXIT_PHASE: u8 = 0;

/// Bring-up guest code page (store/ISR); ack slot lives here across M3.4 inject.
static mut BRINGUP_GUEST_CODE_PHYS: u64 = 0;

/// Guest GPRs saved by the naked VMEXIT trampoline before Rust clobbers them.
/// RSP/RIP/RFLAGS live in the VMCS; general regs must be saved here for Linux.
static mut SAVED_GUEST_RAX: u64 = 0;
static mut SAVED_GUEST_RBX: u64 = 0;
static mut SAVED_GUEST_RCX: u64 = 0;
static mut SAVED_GUEST_RDX: u64 = 0;
static mut SAVED_GUEST_RSI: u64 = 0;
static mut SAVED_GUEST_RDI: u64 = 0;
static mut SAVED_GUEST_RBP: u64 = 0;
static mut SAVED_GUEST_R8: u64 = 0;
static mut SAVED_GUEST_R9: u64 = 0;
static mut SAVED_GUEST_R10: u64 = 0;
static mut SAVED_GUEST_R11: u64 = 0;
static mut SAVED_GUEST_R12: u64 = 0;
static mut SAVED_GUEST_R13: u64 = 0;
static mut SAVED_GUEST_R14: u64 = 0;
static mut SAVED_GUEST_R15: u64 = 0;

/// HLT VMEXITs counted in phase 8 after SHELL-OK.
static mut LOOP_HLT_COUNT: u32 = 0;

/// Resumes required in the continuous loop before [`M3_LOOP_OK_MARKER`].
pub const LOOP_HLT_TARGET: u32 = 4;

/// COM1 marker when the post-shell exit loop survives [`LOOP_HLT_TARGET`] HLTs.
pub const M3_LOOP_OK_MARKER: &str = "RAYNU-V-M3-LOOP-OK";

/// M3.2–M3.5 load addresses (set before [`run_hlt_guest`]).
static mut LOAD_KERNEL_PHYS: u64 = 0;
static mut LOAD_BOOT_PARAMS_PHYS: u64 = 0;
static mut LOAD_INIT_PHYS: u64 = 0;
/// When set, phase 4+ follows the real-Linux early path (skip GTIMER/SHELL/LOOP).
static mut REAL_LINUX_GUEST: bool = false;

/// M3.9: host LAPIC armed after LINUX-EARLY; waiting for ext-IRQ → GTIMER2-OK.
static mut LINUX_GTIMER2_ARMED: bool = false;
/// M3.9 done; M3.10 waits for real init `RAYNU-V-M3-SHELL` magic.
static mut LINUX_GTIMER2_DONE: bool = false;

/// Record kernel entry / boot_params / proto-init for later VMRESUME.
///
/// `entry_phys` is the 64-bit entry RIP (bzImage: PM base + 0x200).
pub fn set_linux_load(entry_phys: u64, boot_params_phys: u64, init_phys: u64) {
    // SAFETY: single-threaded boot before VMLAUNCH.
    unsafe {
        LOAD_KERNEL_PHYS = entry_phys;
        LOAD_BOOT_PARAMS_PHYS = boot_params_phys;
        LOAD_INIT_PHYS = init_phys;
    }
}

/// Select real-Linux post-entry handling (M3.8+) vs synthetic proto path.
pub fn set_real_linux(real: bool) {
    // SAFETY: single-threaded boot before VMLAUNCH.
    unsafe {
        REAL_LINUX_GUEST = real;
        LINUX_GTIMER2_ARMED = false;
        LINUX_GTIMER2_DONE = false;
    }
}

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
    /// Secondary controls / EPT capability missing.
    EptUnsupported,
    /// Primary CPUID exiting not allowed by capability MSRs.
    CpuidExitingUnsupported,
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

/// Physical frames needed for the M1.2/M2.x HLT + IRQ guest under EPT.
pub struct LaunchFrames {
    pub vmcs_phys: u64,
    pub guest_stack_phys: u64,
    pub host_stack_phys: u64,
    /// Zeroed page for a 64-bit TSS (OVMF often has TR=0 — invalid host state).
    pub tss_phys: u64,
    /// Page to hold a copy of the GDT plus a TSS descriptor.
    pub gdt_phys: u64,
    /// Packed EPTP (PML4 already built).
    pub eptp: u64,
    /// Guest code page (store/loop/HLT + ISR); identity-mapped via EPT + host CR3.
    pub guest_code_phys: u64,
    /// Guest IDT page (one interrupt gate for the inject vector).
    pub guest_idt_phys: u64,
    /// Optional MSR bitmap (only if primary controls force USE_MSR_BITMAPS).
    pub msr_bitmap_phys: Option<u64>,
    pub io_bitmap_a_phys: Option<u64>,
    pub io_bitmap_b_phys: Option<u64>,
}

/// Minimal IA-32e TSS size (SDM Vol. 3A §7.7) — enough for LTR / host TR.
const TSS_BYTES: usize = 0x68;

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

/// Build a host TSS + GDT and load them (LGDT/LTR).
///
/// UEFI/OVMF commonly leaves TR=0. Host-state checks then fail VMLAUNCH with
/// insn error 8 (invalid host-state). We always install our own TSS.
///
/// Returns `(new_gdtr_base, new_gdtr_limit, tr_selector, tr_base)`.
///
/// SAFETY: `gdt_phys`/`tss_phys` are owned zeroable frames; interrupts off.
unsafe fn install_host_tss(
    gdt_phys: u64,
    tss_phys: u64,
) -> Result<(u64, u16, u16, u64), LaunchError> {
    let old = cpu::sgdt();
    let old_size = (old.limit as usize) + 1;
    // Need room for a 16-byte system descriptor after the existing table.
    if old_size + 16 > 4096 || old_size < 8 {
        return Err(LaunchError::PrepareFailed);
    }

    core::ptr::write_bytes(gdt_phys as *mut u8, 0, 4096);
    core::ptr::write_bytes(tss_phys as *mut u8, 0, 4096);
    core::ptr::copy_nonoverlapping(old.base as *const u8, gdt_phys as *mut u8, old_size);

    // Append available 64-bit TSS descriptor at the next 8-byte aligned slot.
    let tss_index = (old_size + 7) / 8; // qword index; may skip a pad entry
    let tss_off = tss_index * 8;
    if tss_off + 16 > 4096 {
        return Err(LaunchError::PrepareFailed);
    }

    let base = tss_phys;
    let limit = (TSS_BYTES - 1) as u64;
    // Low qword: limit[15:0] | base[23:0]<<16 | type/S/DPL/P | limit/flags | base[31:24]
    // Type 0x9 = available 64-bit TSS; S=0; DPL=0; P=1.
    let d0 = (limit & 0xFFFF)
        | ((base & 0xFF_FFFF) << 16)
        | (0x89u64 << 40) // P=1, DPL=0, S=0, type=9
        | (((limit >> 16) & 0xF) << 48)
        | (((base >> 24) & 0xFF) << 56);
    let d1 = (base >> 32) & 0xFFFF_FFFF;
    let desc = (gdt_phys as *mut u64).add(tss_index);
    core::ptr::write_unaligned(desc, d0);
    core::ptr::write_unaligned(desc.add(1), d1);

    let new_limit = (tss_off + 16 - 1) as u16;
    let gdtr = cpu::DescriptorTablePtr {
        limit: new_limit,
        base: gdt_phys,
    };
    cpu::lgdt(&gdtr);

    let tr_sel = (tss_off as u16) & 0xFFF8;
    cpu::load_tr(tr_sel);

    serial::write_str("boot: host TSS sel=0x");
    write_hex_u32(tr_sel as u32);
    serial::write_str(" base=0x");
    write_hex_u64(tss_phys);
    serial::write_str(" gdtr=0x");
    write_hex_u64(gdt_phys);
    serial::write_byte(b'\n');

    Ok((gdt_phys, new_limit, tr_sel, tss_phys))
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
            // SDM: 12 = unsupported VMCS component. Common causes under QEMU:
            // swapped AT&T vmwrite operands, or host kvm_intel shadow VMCS.
            serial::write_line(
                "boot: hint: error 12 = unsupported VMCS field (check VMWRITE operands / shadow VMCS)",
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
    BRINGUP_GUEST_CODE_PHYS = frames.guest_code_phys;
    prepare_vmcs_region(frames.vmcs_phys)?;

    ops::vmclear(frames.vmcs_phys).map_err(|_| LaunchError::ClearFailed)?;
    // Nested VT-x has been observed to disturb the revision dword across
    // VMCLEAR; rewrite it before any VMPTRLD.
    prepare_vmcs_region(frames.vmcs_phys)?;
    // VMPTRLD is deferred until after all RDMSR/serial gather work inside
    // setup_vmcs (nested VT-x can drop current-VMCS across those exits).

    setup_vmcs(frames)?;

    serial::write_line("boot: VMLAUNCH → guest store+loop+HLT + IRQ inject (EPT)");
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
            if ierr == 8 {
                serial::write_line(
                    "boot: hint: error 8 = invalid host-state (TR/CS/CR/EFER/canonical)",
                );
            } else if ierr == 7 {
                serial::write_line("boot: hint: error 7 = invalid VMX control field(s)");
            }
            Err(LaunchError::LaunchFailed {
                instruction_error: ierr,
            })
        }
    }
}

unsafe fn setup_vmcs(frames: &LaunchFrames) -> Result<(), LaunchError> {
    // ── Phase 0: host TSS (before any TR-dependent gather) ──
    // OVMF often has TR=0 → VMLAUNCH fails with insn error 8.
    let (gdt_base, gdt_limit, tr, tr_base) =
        install_host_tss(frames.gdt_phys, frames.tss_phys)?;

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

    // Ext-IRQ (M2.5) + I/O (M3.0) + CPUID (M3.1) exiting.
    // Prefer I/O bitmaps (COM1 only): unconditional I/O makes every Linux
    // `io_delay` (port 0x80) a VMEXIT and stalls mid mem-init.
    let pin = adjust_vmx_controls(PIN_BASED_EXTERNAL_INTERRUPT_EXITING, pin_msr);
    let primary = adjust_vmx_controls(
        CPU_BASED_HLT_EXITING
            | CPU_BASED_CPUID_EXITING
            | CPU_BASED_USE_IO_BITMAPS
            | CPU_BASED_UNCONDITIONAL_IO
            | CPU_BASED_USE_MSR_BITMAPS
            | CPU_BASED_ACTIVATE_SECONDARY,
        proc_msr,
    );
    if primary & CPU_BASED_ACTIVATE_SECONDARY == 0 {
        serial::write_line("boot: ERROR — secondary controls not allowed (need EPT)");
        return Err(LaunchError::EptUnsupported);
    }
    if primary & CPU_BASED_CPUID_EXITING == 0 {
        serial::write_line("boot: ERROR — CPUID exiting not allowed by PROCBASED_CTLS");
        return Err(LaunchError::CpuidExitingUnsupported);
    }
    let exit_wanted = VM_EXIT_HOST_ADDR_SPACE_SIZE
        | VM_EXIT_ACK_INTERRUPT_ON_EXIT
        | VM_EXIT_SAVE_IA32_EFER
        | VM_EXIT_LOAD_IA32_EFER;
    let entry_wanted = VM_ENTRY_IA32E_MODE | VM_ENTRY_LOAD_IA32_EFER;
    let exit_ctls = adjust_vmx_controls(exit_wanted, exit_msr);
    let entry_ctls = adjust_vmx_controls(entry_wanted, entry_msr);
    if exit_ctls & VM_EXIT_ACK_INTERRUPT_ON_EXIT == 0 {
        serial::write_line("boot: WARN — ack-interrupt-on-exit not allowed; EOI still used");
    }

    // RDTSCP: without secondary bit 3, guest `rdtscp` #UD (Linux tsc clocksource).
    let secondary = adjust_vmx_controls(
        SECONDARY_ENABLE_EPT | SECONDARY_ENABLE_RDTSCP,
        IA32_VMX_PROCBASED_CTLS2,
    );
    if secondary & SECONDARY_ENABLE_EPT == 0 {
        serial::write_line("boot: ERROR — enable-EPT not allowed by PROCBASED_CTLS2");
        return Err(LaunchError::EptUnsupported);
    }
    if secondary & SECONDARY_ENABLE_RDTSCP == 0 {
        serial::write_line("boot: WARN — enable-RDTSCP not allowed; Linux may #UD on rdtscp");
    }

    let msr_bitmap = if primary & CPU_BASED_USE_MSR_BITMAPS != 0 {
        let bmp = frames.msr_bitmap_phys.ok_or(LaunchError::PrepareFailed)?;
        core::ptr::write_bytes(bmp as *mut u8, 0, 4096);
        // M3.11: trap x2APIC + APIC_BASE for virtual LAPIC.
        crate::devices::lapic_virt::trap_x2apic_msrs(bmp);
        Some(bmp)
    } else {
        serial::write_line("boot: WARN — MSR bitmaps unavailable; x2APIC may hit host");
        None
    };

    let io_bitmaps = if primary & CPU_BASED_USE_IO_BITMAPS != 0 {
        // Bitmaps override unconditional I/O exiting — trap COM1 explicitly.
        let a = frames.io_bitmap_a_phys.ok_or(LaunchError::PrepareFailed)?;
        let b = frames.io_bitmap_b_phys.ok_or(LaunchError::PrepareFailed)?;
        core::ptr::write_bytes(a as *mut u8, 0, 4096);
        core::ptr::write_bytes(b as *mut u8, 0, 4096);
        serial_pio::trap_com1_in_bitmap_a(a);
        serial::write_line("boot: I/O exiting via COM1 bitmaps");
        Some((a, b))
    } else if primary & CPU_BASED_UNCONDITIONAL_IO == 0 {
        serial::write_line("boot: ERROR — neither unconditional I/O nor I/O bitmaps available");
        return Err(LaunchError::PrepareFailed);
    } else {
        serial::write_line("boot: WARN — I/O bitmaps unavailable; unconditional I/O (slow)");
        None
    };

    let cr0 = cpu::read_cr0();
    let cr3 = cpu::read_cr3();
    let cr4 = cpu::read_cr4();
    let efer = cpu::rdmsr(IA32_EFER);
    let pat = cpu::rdmsr(IA32_PAT);
    let dr7 = cpu::read_dr7();
    let idtr = cpu::sidt();

    let cs = cpu::read_cs();
    let ss = cpu::read_ss();
    let ds = cpu::read_ds();
    let es = cpu::read_es();
    let fs = cpu::read_fs();
    let gs = cpu::read_gs();
    let ldtr = cpu::read_ldtr();
    // `tr` / `tr_base` / `gdt_base` come from install_host_tss (LTR already done).

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

    let es_limit = cpu::segment_limit(es) as u64;
    let cs_limit = cpu::segment_limit(cs) as u64;
    let ss_limit = cpu::segment_limit(ss) as u64;
    let ds_limit = cpu::segment_limit(ds) as u64;
    let fs_limit = cpu::segment_limit(fs) as u64;
    let gs_limit = cpu::segment_limit(gs) as u64;
    let ldtr_limit = cpu::segment_limit(ldtr) as u64;
    let tr_limit = (TSS_BYTES - 1) as u64;

    let es_ar = seg_ar(gdt_base, es) as u64;
    let cs_ar = seg_ar(gdt_base, cs) as u64;
    let ss_ar = seg_ar(gdt_base, ss) as u64;
    let ds_ar = seg_ar(gdt_base, ds) as u64;
    let fs_ar = seg_ar(gdt_base, fs) as u64;
    let gs_ar = seg_ar(gdt_base, gs) as u64;
    let ldtr_ar = seg_ar(gdt_base, ldtr) as u64;
    // After LTR the GDT type is busy (B); AR must reflect that for guest TR.
    let tr_ar = seg_ar(gdt_base, tr) as u64;

    let need_efer = (exit_ctls & (VM_EXIT_SAVE_IA32_EFER | VM_EXIT_LOAD_IA32_EFER)) != 0
        || (entry_ctls & VM_ENTRY_LOAD_IA32_EFER) != 0;
    let need_pat = (exit_ctls & (VM_EXIT_SAVE_IA32_PAT | VM_EXIT_LOAD_IA32_PAT)) != 0;
    let need_debugctl = (exit_ctls & VM_EXIT_SAVE_DEBUG_CONTROLS) != 0
        || (entry_ctls & VM_ENTRY_LOAD_DEBUG_CONTROLS) != 0;

    // Guest IDT: one gate → ISR on the code page (M2.4).
    ept_hw::write_guest_idt(
        frames.guest_idt_phys,
        frames.guest_code_phys + GUEST_ISR_OFF,
        cs,
        M2_IRQ_VECTOR as u8,
    );

    let guest_rip = frames.guest_code_phys;
    let guest_rsp = frames.guest_stack_phys + 4096;
    let host_rsp = (frames.host_stack_phys + 4096) & !0xFu64;
    let host_rip = vmexit_landing as *const () as u64;

    // IA-32e interrupt delivery always loads RSP from the TSS (RSP0 when IST=0).
    // Point RSP0 at the guest stack so the injected ISR has a valid stack.
    core::ptr::write_volatile((frames.tss_phys + 4) as *mut u64, guest_rsp);

    serial::write_str("boot: VMCS ctls pin=0x");
    write_hex_u32(pin);
    serial::write_str(" primary=0x");
    write_hex_u32(primary);
    serial::write_str(" secondary=0x");
    write_hex_u32(secondary);
    serial::write_str(" exit=0x");
    write_hex_u32(exit_ctls);
    serial::write_str(" entry=0x");
    write_hex_u32(entry_ctls);
    serial::write_byte(b'\n');
    serial::write_str("boot: EPTP=0x");
    write_hex_u64(frames.eptp);
    serial::write_str(" guest_code=0x");
    write_hex_u64(guest_rip);
    serial::write_byte(b'\n');
    serial::write_str("boot: host CS=0x");
    write_hex_u32(cs as u32);
    serial::write_str(" SS=0x");
    write_hex_u32(ss as u32);
    serial::write_str(" TR=0x");
    write_hex_u32(tr as u32);
    serial::write_str(" EFER=0x");
    write_hex_u64(efer);
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

    vw(SECONDARY_VM_EXEC_CONTROL, secondary as u64)?;
    vw(EPT_POINTER, frames.eptp)?;
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
    vw(GUEST_GDTR_BASE, gdt_base)?;
    vw(GUEST_IDTR_BASE, frames.guest_idt_phys)?;

    vw(GUEST_ES_LIMIT, es_limit)?;
    vw(GUEST_CS_LIMIT, cs_limit)?;
    vw(GUEST_SS_LIMIT, ss_limit)?;
    vw(GUEST_DS_LIMIT, ds_limit)?;
    vw(GUEST_FS_LIMIT, fs_limit)?;
    vw(GUEST_GS_LIMIT, gs_limit)?;
    vw(GUEST_LDTR_LIMIT, ldtr_limit)?;
    vw(GUEST_TR_LIMIT, tr_limit)?;
    vw(GUEST_GDTR_LIMIT, gdt_limit as u64)?;
    vw(GUEST_IDTR_LIMIT, 4095)?;

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
    vw(HOST_GDTR_BASE, gdt_base)?;
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

/// HOST_RIP trampoline — save guest GPRs before Rust clobbers them.
///
/// Guest GPRs are not in the VMCS; they live in host registers across VMEXIT.
#[unsafe(naked)]
pub unsafe extern "C" fn vmexit_landing() -> ! {
    core::arch::naked_asm!(
        "mov [{slot_rax}], rax",
        "mov [{slot_rbx}], rbx",
        "mov [{slot_rcx}], rcx",
        "mov [{slot_rdx}], rdx",
        "mov [{slot_rsi}], rsi",
        "mov [{slot_rdi}], rdi",
        "mov [{slot_rbp}], rbp",
        "mov [{slot_r8}], r8",
        "mov [{slot_r9}], r9",
        "mov [{slot_r10}], r10",
        "mov [{slot_r11}], r11",
        "mov [{slot_r12}], r12",
        "mov [{slot_r13}], r13",
        "mov [{slot_r14}], r14",
        "mov [{slot_r15}], r15",
        "jmp {cont}",
        slot_rax = sym SAVED_GUEST_RAX,
        slot_rbx = sym SAVED_GUEST_RBX,
        slot_rcx = sym SAVED_GUEST_RCX,
        slot_rdx = sym SAVED_GUEST_RDX,
        slot_rsi = sym SAVED_GUEST_RSI,
        slot_rdi = sym SAVED_GUEST_RDI,
        slot_rbp = sym SAVED_GUEST_RBP,
        slot_r8 = sym SAVED_GUEST_R8,
        slot_r9 = sym SAVED_GUEST_R9,
        slot_r10 = sym SAVED_GUEST_R10,
        slot_r11 = sym SAVED_GUEST_R11,
        slot_r12 = sym SAVED_GUEST_R12,
        slot_r13 = sym SAVED_GUEST_R13,
        slot_r14 = sym SAVED_GUEST_R14,
        slot_r15 = sym SAVED_GUEST_R15,
        cont = sym vmexit_continue,
    );
}

/// Restore saved GPRs and VMRESUME (CPUID / I/O / loop / entry).
#[unsafe(naked)]
unsafe extern "C" fn vmresume_with_gprs() -> ! {
    core::arch::naked_asm!(
        "mov rax, [{slot_rax}]",
        "mov rbx, [{slot_rbx}]",
        "mov rcx, [{slot_rcx}]",
        "mov rdx, [{slot_rdx}]",
        "mov rsi, [{slot_rsi}]",
        "mov rdi, [{slot_rdi}]",
        "mov rbp, [{slot_rbp}]",
        "mov r8, [{slot_r8}]",
        "mov r9, [{slot_r9}]",
        "mov r10, [{slot_r10}]",
        "mov r11, [{slot_r11}]",
        "mov r12, [{slot_r12}]",
        "mov r13, [{slot_r13}]",
        "mov r14, [{slot_r14}]",
        "mov r15, [{slot_r15}]",
        "vmresume",
        "jmp {fail}",
        slot_rax = sym SAVED_GUEST_RAX,
        slot_rbx = sym SAVED_GUEST_RBX,
        slot_rcx = sym SAVED_GUEST_RCX,
        slot_rdx = sym SAVED_GUEST_RDX,
        slot_rsi = sym SAVED_GUEST_RSI,
        slot_rdi = sym SAVED_GUEST_RDI,
        slot_rbp = sym SAVED_GUEST_RBP,
        slot_r8 = sym SAVED_GUEST_R8,
        slot_r9 = sym SAVED_GUEST_R9,
        slot_r10 = sym SAVED_GUEST_R10,
        slot_r11 = sym SAVED_GUEST_R11,
        slot_r12 = sym SAVED_GUEST_R12,
        slot_r13 = sym SAVED_GUEST_R13,
        slot_r14 = sym SAVED_GUEST_R14,
        slot_r15 = sym SAVED_GUEST_R15,
        fail = sym vmresume_gprs_failed,
    );
}

/// HOST_RIP continuation after [`vmexit_landing`] saves GPRs.
///
/// See [`EXIT_PHASE`] for the M2.4/M2.5 state machine. I/O (M3.0) and CPUID
/// (M3.1) exits are handled before the phase dispatch and always VMRESUME.
pub unsafe extern "C" fn vmexit_continue() -> ! {
    let guest_rax = SAVED_GUEST_RAX;
    let reason = ops::vmread(EXIT_REASON).unwrap_or(0xFFFF) as u32;
    let basic = reason & 0xFFFF;
    let qual = ops::vmread(EXIT_QUALIFICATION).unwrap_or(0);
    let guest_rip = ops::vmread(GUEST_RIP).unwrap_or(0);
    let guest_page = guest_rip & !0xfff;
    let phase = EXIT_PHASE;

    if basic == EXIT_REASON_IO_INSTRUCTION {
        handle_io_and_resume(qual, guest_rax, guest_rip);
    }
    if basic == EXIT_REASON_EPT_VIOLATION {
        handle_ept_violation_and_resume(qual, guest_rip);
    }
    if basic == EXIT_REASON_CPUID {
        handle_cpuid_and_resume(guest_rip);
    }
    // M3.9: emulate allow-listed MSRs without phase-log spam (real Linux).
    if REAL_LINUX_GUEST
        && (basic == EXIT_REASON_MSR_READ || basic == EXIT_REASON_MSR_WRITE)
    {
        handle_msr_and_resume(basic);
    }
    // M3.10: XSETBV always exits; Linux enables XSAVE early after fpu init.
    if REAL_LINUX_GUEST && basic == EXIT_REASON_XSETBV {
        handle_xsetbv_and_resume(guest_rip);
    }
    // M3.10: after GTIMER2, quiet-dispatch EXT_INT / HLT — logging every tick
    // on COM1 interleaves with Linux printk and starves guest progress.
    if REAL_LINUX_GUEST && LINUX_GTIMER2_DONE && phase == 4 {
        phase4_linux_early(basic);
    }

    serial::write_str("boot: VMEXIT phase=");
    write_hex_u32(phase as u32);
    serial::write_str(" reason=0x");
    write_hex_u32(basic);
    serial::write_str(" qual=0x");
    write_hex_u64(qual);
    serial::write_str(" rip=0x");
    write_hex_u64(guest_rip);
    serial::write_byte(b'\n');

    match phase {
        0 => phase0_first_hlt(basic, guest_page),
        1 => phase1_irq_ok_arm_timer(basic, guest_page),
        2 => phase2_external_irq(basic),
        3 => phase3_timer_ok(basic, guest_page),
        4 => phase4_early_ok(basic, guest_page),
        5 => phase5_guest_timer_irq(basic),
        6 => phase6_gtimer_ok(basic, guest_page),
        7 => phase7_shell_ok(basic, guest_page),
        8 => phase8_exit_loop(basic),
        _ => {
            serial::write_line("boot: ERROR — bad EXIT_PHASE");
            finish_boot(false);
        }
    }
}

/// Zero general regs; set RSI (proto-kernel / proto-init `boot_params`).
unsafe fn reset_saved_gprs(rsi: u64) {
    SAVED_GUEST_RAX = 0;
    SAVED_GUEST_RBX = 0;
    SAVED_GUEST_RCX = 0;
    SAVED_GUEST_RDX = 0;
    SAVED_GUEST_RSI = rsi;
    SAVED_GUEST_RDI = 0;
    SAVED_GUEST_RBP = 0;
    SAVED_GUEST_R8 = 0;
    SAVED_GUEST_R9 = 0;
    SAVED_GUEST_R10 = 0;
    SAVED_GUEST_R11 = 0;
    SAVED_GUEST_R12 = 0;
    SAVED_GUEST_R13 = 0;
    SAVED_GUEST_R14 = 0;
    SAVED_GUEST_R15 = 0;
}

unsafe fn handle_io_and_resume(qual: u64, guest_rax: u64, guest_rip: u64) -> ! {
    let info = serial_pio::parse_qualification(qual);
    match serial_pio::handle_pio(&info, guest_rax) {
        Ok(None) => {}
        Ok(Some(new_rax)) => {
            SAVED_GUEST_RAX = new_rax;
        }
        Err(()) => {
            // Should be rare after misc-port stubs; keep as hard fail.
            serial::write_str("boot: ERROR — unhandled PIO port=0x");
            write_hex_u32(info.port as u32);
            serial::write_byte(b'\n');
            finish_boot(false);
        }
    }

    if serial_pio::guest_io_ok() {
        // Emit once when magic completes (may appear before TIMER-OK).
        static mut IO_MARKED: bool = false;
        if !IO_MARKED {
            IO_MARKED = true;
            serial::write_byte(b'\n');
            serial::write_line(M3_IO_OK_MARKER);
        }
    }
    if serial_pio::guest_early_ok() {
        static mut EARLY_MARKED: bool = false;
        if !EARLY_MARKED {
            EARLY_MARKED = true;
            serial::write_byte(b'\n');
            serial::write_line(M3_EARLY_OK_MARKER);
        }
    }
    if serial_pio::guest_shell_ok() {
        static mut SHELL_MARKED: bool = false;
        if !SHELL_MARKED {
            SHELL_MARKED = true;
            serial::write_byte(b'\n');
            serial::write_line(M3_SHELL_OK_MARKER);
        }
        maybe_finish_m312();
    }
    if serial_pio::guest_linux_early_ok() {
        static mut LINUX_EARLY_MARKED: bool = false;
        if !LINUX_EARLY_MARKED {
            LINUX_EARLY_MARKED = true;
            serial::write_byte(b'\n');
            serial::write_line(M3_LINUX_EARLY_OK_MARKER);
        }
    }

    let insn_len = ops::vmread(VM_EXIT_INSTRUCTION_LEN).unwrap_or(2);
    let _ = ops::vmwrite(GUEST_RIP, guest_rip.wrapping_add(insn_len));
    // M3.9: after real banner, arm host LAPIC once (not again after GTIMER2-OK).
    if REAL_LINUX_GUEST
        && serial_pio::guest_linux_early_ok()
        && !LINUX_GTIMER2_ARMED
        && !LINUX_GTIMER2_DONE
    {
        let _ = ops::vmwrite(VM_ENTRY_INTERRUPTION_INFO, 0);
        arm_linux_gtimer2();
    }
    // M3.19: no IRQ4 COM1 TX inject — SHELL latches via CPUID hypercall.
    if REAL_LINUX_GUEST && LINUX_GTIMER2_DONE {
        if try_inject_guest_apic_timer() {
            vmresume_with_gprs();
        }
        maybe_arm_interrupt_window_for_apic();
    }
    let _ = ops::vmwrite(VM_ENTRY_INTERRUPTION_INFO, 0);
    // Preserve RSI across OUT storms in the proto-kernel.
    vmresume_with_gprs();
}

/// Emulate APIC MMIO at the EPT hole (GPA 0xFEE00000).
unsafe fn handle_ept_violation_and_resume(qual: u64, guest_rip: u64) -> ! {
    let gpa = ops::vmread(GUEST_PHYSICAL_ADDRESS).unwrap_or(0);
    if !(lapic_virt::APIC_GPA..lapic_virt::APIC_GPA + 0x1000).contains(&gpa) {
        serial::write_str("boot: ERROR — EPT violation GPA=0x");
        write_hex_u64(gpa);
        serial::write_byte(b'\n');
        dump_linux_guest_state();
        finish_boot(false);
    }
    let is_write = (qual & 0x2) != 0;
    // Guest RIP is a linear address (high kernel VA after Linux paging).
    // Walk guest CR3 → GPA, then read via identity EPT — never deref GVA as HVA.
    let guest_cr3 = ops::vmread(GUEST_CR3).unwrap_or(0);
    let mut insn = [0u8; 15];
    if guest_pt::copy_from_guest_va(guest_cr3, guest_rip, &mut insn).is_err() {
        serial::write_line("boot: ERROR — APIC MMIO insn fetch (guest PT walk)");
        serial::write_str("boot: guest cr3=0x");
        write_hex_u64(guest_cr3);
        serial::write_str(" rip=0x");
        write_hex_u64(guest_rip);
        serial::write_byte(b'\n');
        dump_linux_guest_state();
        finish_boot(false);
    }
    let Some(mov) = mmio_decode::decode_mov_mmio(&insn) else {
        serial::write_line("boot: ERROR — APIC MMIO undecoded insn");
        serial::write_str("boot: insn=");
        for &b in insn.iter().take(8) {
            let hi = b >> 4;
            let lo = b & 0xf;
            serial::write_byte(if hi < 10 { b'0' + hi } else { b'a' + (hi - 10) });
            serial::write_byte(if lo < 10 { b'0' + lo } else { b'a' + (lo - 10) });
            serial::write_byte(b' ');
        }
        serial::write_byte(b'\n');
        dump_linux_guest_state();
        finish_boot(false);
    };
    if mov.is_write != is_write {
        serial::write_line("boot: WARN — APIC mov direction ≠ EPT qual");
    }
    // Intel GPR order in ModRM: RAX RCX RDX RBX RSP RBP RSI RDI R8…R15
    let mut gprs = [
        SAVED_GUEST_RAX,
        SAVED_GUEST_RCX,
        SAVED_GUEST_RDX,
        SAVED_GUEST_RBX,
        ops::vmread(GUEST_RSP).unwrap_or(0),
        SAVED_GUEST_RBP,
        SAVED_GUEST_RSI,
        SAVED_GUEST_RDI,
        SAVED_GUEST_R8,
        SAVED_GUEST_R9,
        SAVED_GUEST_R10,
        SAVED_GUEST_R11,
        SAVED_GUEST_R12,
        SAVED_GUEST_R13,
        SAVED_GUEST_R14,
        SAVED_GUEST_R15,
    ];
    if mmio_decode::apply_apic_mov(mov, gpa, &mut gprs).is_err() {
        serial::write_line("boot: ERROR — APIC MMIO apply failed");
        finish_boot(false);
    }
    SAVED_GUEST_RAX = gprs[0];
    SAVED_GUEST_RCX = gprs[1];
    SAVED_GUEST_RDX = gprs[2];
    SAVED_GUEST_RBX = gprs[3];
    SAVED_GUEST_RBP = gprs[5];
    SAVED_GUEST_RSI = gprs[6];
    SAVED_GUEST_RDI = gprs[7];
    SAVED_GUEST_R8 = gprs[8];
    SAVED_GUEST_R9 = gprs[9];
    SAVED_GUEST_R10 = gprs[10];
    SAVED_GUEST_R11 = gprs[11];
    SAVED_GUEST_R12 = gprs[12];
    SAVED_GUEST_R13 = gprs[13];
    SAVED_GUEST_R14 = gprs[14];
    SAVED_GUEST_R15 = gprs[15];
    emit_lapic_markers();
    if lapic_virt::host_timer_armed_for_guest() {
        let _ = apic::arm_oneshot_timer(M2_IRQ_VECTOR as u8, LINUX_TICK_COUNT);
    }
    let _ = ops::vmwrite(GUEST_RIP, guest_rip.wrapping_add(mov.len as u64));
    if try_inject_guest_apic_timer() {
        vmresume_with_gprs();
    }
    maybe_arm_interrupt_window_for_apic();
    let _ = ops::vmwrite(VM_ENTRY_INTERRUPTION_INFO, 0);
    vmresume_with_gprs();
}

/// Guest can accept a VM-entry external interrupt (IF=1, no STI/MOV-SS block).
/// Injecting with IF=0 → VM-entry failure reason 33 (`0x80000021`).
unsafe fn guest_can_accept_extint() -> bool {
    let rflags = ops::vmread(GUEST_RFLAGS).unwrap_or(0);
    if (rflags & (1 << 9)) == 0 {
        return false;
    }
    let int_state = ops::vmread(GUEST_INTERRUPTIBILITY_STATE).unwrap_or(0);
    // Bit 0: blocking by STI; bit 1: blocking by MOV SS.
    (int_state & 0x3) == 0
}

fn emit_lapic_markers() {
    if lapic_virt::take_gtimer3_latch() {
        serial::write_line(M3_GTIMER3_OK_MARKER);
    }
    if lapic_virt::take_apic_ok_latch() {
        serial::write_line(M3_APIC_OK_MARKER);
    }
}

/// M3.12/M3.19 gate: real `/init` SHELL + APIC-OK (no IRQ4; IRQ0 already stopped).
unsafe fn maybe_finish_m312() {
    if REAL_LINUX_GUEST
        && LINUX_GTIMER2_DONE
        && serial_pio::guest_shell_ok()
        && lapic_virt::apic_ok()
    {
        static mut NOIRQ_MARKED: bool = false;
        if !NOIRQ_MARKED {
            NOIRQ_MARKED = true;
            serial::write_line(M3_NOIRQ_OK_MARKER);
        }
        finish_boot(true);
    }
}

/// Deliver a pending virtual APIC IRR vector when the guest can accept it.
/// Moves IRR→ISR inside [`lapic_virt::take_deliverable_vector`].
unsafe fn try_inject_guest_apic_timer() -> bool {
    if !lapic_virt::has_deliverable_irr() {
        return false;
    }
    if !guest_can_accept_extint() {
        let _ = set_interrupt_window_exiting(true);
        return false;
    }
    let Some(vec) = lapic_virt::take_deliverable_vector() else {
        return false;
    };
    emit_lapic_markers();
    if let Ok(info) = interrupt::prepare_external_inject(vec) {
        let _ = set_interrupt_window_exiting(false);
        let _ = ops::vmwrite(VM_ENTRY_INTERRUPTION_INFO, info as u64);
        let _ = ops::vmwrite(GUEST_INTERRUPTIBILITY_STATE, 0);
        let _ = ops::vmwrite(GUEST_ACTIVITY_STATE, 0);
        return true;
    }
    false
}

/// Arm interrupt-window when APIC IRR is pending but guest IF=0.
unsafe fn maybe_arm_interrupt_window_for_apic() {
    if lapic_virt::has_deliverable_irr() {
        let _ = set_interrupt_window_exiting(true);
    }
}

/// Emulate guest XSETBV (exit reason 55). Only XCR0 is accepted.
unsafe fn handle_xsetbv_and_resume(guest_rip: u64) -> ! {
    let xcr = SAVED_GUEST_RCX as u32;
    let value =
        (SAVED_GUEST_RAX & 0xffff_ffff) | ((SAVED_GUEST_RDX & 0xffff_ffff) << 32);
    if xcr != 0 {
        inject_gp0();
        vmresume_with_gprs();
    }
    // Mask to host-supported XCR0 features (CPUID.0D:0).
    let host_mask = {
        let r = cpu::cpuid(0xD, 0);
        ((r.edx as u64) << 32) | (r.eax as u64)
    };
    // XCR0 bit 0 (x87) must stay set.
    let mut v = (value & host_mask) | 1;
    if v & 0x6 == 0x4 {
        // AVX (bit 2) requires SSE (bit 1).
        v |= 0x2;
    }
    // Host CR4 often lacks OSXSAVE after UEFI bring-up; xsetbv #UD without it
    // (Latitude crash: RIP in r640_hypervisor, CR4=0x2668).
    // SAFETY: VMX root; OSXSAVE is not CR4-fixed0-forbidden on this CPU.
    let cr4 = cpu::read_cr4();
    if cr4 & cpu::CR4_OSXSAVE == 0 {
        cpu::write_cr4(cr4 | cpu::CR4_OSXSAVE);
    }
    cpu::xsetbv(0, v);
    let insn_len = ops::vmread(VM_EXIT_INSTRUCTION_LEN).unwrap_or(3);
    if insn_len == 0 || insn_len > 15 {
        serial::write_line("boot: ERROR — XSETBV bad insn len");
        finish_boot(false);
    }
    let _ = ops::vmwrite(GUEST_RIP, guest_rip.wrapping_add(insn_len));
    let _ = ops::vmwrite(VM_ENTRY_INTERRUPTION_INFO, 0);
    vmresume_with_gprs();
}

unsafe fn handle_cpuid_and_resume(guest_rip: u64) -> ! {
    let leaf = SAVED_GUEST_RAX as u32;
    let subleaf = SAVED_GUEST_RCX as u32;

    // M3.10: real `/init` SHELL hypercall (before any UART TX that may stall).
    // M3.19: latch shell on CPUID — no IRQ4 COM1 TX inject required.
    // M3.12: do not close until APIC-OK as well (maybe_finish_m312).
    if leaf == SHELL_CPUID_LEAF
        && subleaf == SHELL_CPUID_SUBLEAF
        && REAL_LINUX_GUEST
        && LINUX_GTIMER2_DONE
    {
        serial_pio::note_shell_cpuid();
        static mut SHELL_CPUID_MARKED: bool = false;
        if !SHELL_CPUID_MARKED {
            SHELL_CPUID_MARKED = true;
            serial::write_byte(b'\n');
            serial::write_line(M3_SHELL_OK_MARKER);
        }
        maybe_finish_m312();
    }

    let regs = msr_firewall::filter_cpuid(leaf, subleaf);
    SAVED_GUEST_RAX = regs.eax as u64;
    SAVED_GUEST_RBX = regs.ebx as u64;
    SAVED_GUEST_RCX = regs.ecx as u64;
    SAVED_GUEST_RDX = regs.edx as u64;

    if leaf == 1 && msr_firewall::cpuid_filter_ok() {
        static mut CPUID_MARKED: bool = false;
        if !CPUID_MARKED {
            CPUID_MARKED = true;
            serial::write_line(M3_CPUID_OK_MARKER);
        }
    }

    let insn_len = ops::vmread(VM_EXIT_INSTRUCTION_LEN).unwrap_or(2);
    let _ = ops::vmwrite(GUEST_RIP, guest_rip.wrapping_add(insn_len));
    // M3.19: APIC IRR may need the interrupt window (no COM1 TX IRQ inject).
    if REAL_LINUX_GUEST && LINUX_GTIMER2_DONE {
        if try_inject_guest_apic_timer() {
            vmresume_with_gprs();
        }
        maybe_arm_interrupt_window_for_apic();
    }
    let _ = ops::vmwrite(VM_ENTRY_INTERRUPTION_INFO, 0);
    vmresume_with_gprs();
}

unsafe extern "C" fn vmresume_gprs_failed() -> ! {
    let ierr = ops::vmread(VM_INSTRUCTION_ERROR).unwrap_or(0xFFFF) as u32;
    serial::write_str("boot: ERROR — VMRESUME(gprs) failed insn_error=0x");
    write_hex_u32(ierr);
    serial::write_byte(b'\n');
    finish_boot(false);
}

unsafe fn phase0_first_hlt(basic: u32, guest_page: u64) -> ! {
    if basic != EXIT_REASON_HLT {
        serial::write_line("boot: ERROR — phase0 expected HLT");
        finish_boot(false);
    }
    serial::write_line(M1_VMEXIT_OK_MARKER);
    serial::write_line(M2_EPT_OK_MARKER);
    let mut ok = true;
    if ept_hw::verify_guest_store(guest_page) {
        serial::write_line(M2_GUEST_OK_MARKER);
    } else {
        serial::write_line("boot: ERROR — guest store/loop verify failed");
        ok = false;
    }
    if ept::ownership_selftest_ok() {
        serial::write_line(M2_OWN_OK_MARKER);
    } else {
        serial::write_line("boot: ERROR — ADR-004 ownership latch clear");
        ok = false;
    }
    if frame_allocator::allocator_selftest_ok() {
        serial::write_line(M2_ALLOC_OK_MARKER);
    } else {
        serial::write_line("boot: ERROR — frame allocator latch clear");
        ok = false;
    }
    if !ok {
        finish_boot(false);
    }

    EXIT_PHASE = 1;
    inject_and_resume("software inject");
}

unsafe fn phase1_irq_ok_arm_timer(basic: u32, guest_page: u64) -> ! {
    if basic != EXIT_REASON_HLT {
        serial::write_line("boot: ERROR — phase1 expected HLT");
        finish_boot(false);
    }
    if !ept_hw::verify_guest_irq(guest_page) {
        serial::write_line("boot: ERROR — guest IRQ ack missing");
        finish_boot(false);
    }
    serial::write_line(M2_IRQ_OK_MARKER);

    // Clear ack so the timer-path ISR must write it again.
    ept_hw::clear_guest_irq(guest_page);

    if apic::arm_bringup_timer(M2_IRQ_VECTOR as u8).is_err() {
        serial::write_line("boot: ERROR — LAPIC timer arm failed");
        finish_boot(false);
    }
    serial::write_line("boot: LAPIC one-shot armed; waiting in guest HLT");

    // Drop HLT exiting so the guest actually waits; timer → reason 1.
    if set_hlt_exiting(false).is_err() {
        serial::write_line("boot: ERROR — clear HLT exiting failed");
        finish_boot(false);
    }
    let _ = ops::vmwrite(VM_ENTRY_INTERRUPTION_INFO, 0);
    let _ = ops::vmwrite(GUEST_INTERRUPTIBILITY_STATE, 0);
    let _ = ops::vmwrite(GUEST_ACTIVITY_STATE, 0);
    let _ = ops::vmwrite(GUEST_RFLAGS, 0x2 | (1 << 9));

    EXIT_PHASE = 2;
    resume_or_die();
}

unsafe fn phase2_external_irq(basic: u32) -> ! {
    if basic != EXIT_REASON_EXTERNAL_INTERRUPT {
        serial::write_line("boot: ERROR — phase2 expected external-interrupt exit");
        finish_boot(false);
    }

    let exit_info = ops::vmread(VM_EXIT_INTR_INFO).unwrap_or(0) as u32;
    if (exit_info & (1 << 31)) != 0 {
        let vec = exit_info & 0xff;
        serial::write_str("boot: external IRQ vector=0x");
        write_hex_u32(vec);
        serial::write_byte(b'\n');
        if vec != M2_IRQ_VECTOR {
            serial::write_line("boot: ERROR — unexpected exit vector");
            finish_boot(false);
        }
    } else {
        serial::write_line("boot: external IRQ (no ack-info); assuming LAPIC timer");
    }

    if apic::eoi().is_err() {
        serial::write_line("boot: ERROR — APIC EOI failed");
        finish_boot(false);
    }
    serial::write_line("boot: APIC EOI ok");

    // Re-enable HLT exiting so the re-injected ISR's HLT exits to phase 3.
    if set_hlt_exiting(true).is_err() {
        serial::write_line("boot: ERROR — restore HLT exiting failed");
        finish_boot(false);
    }

    EXIT_PHASE = 3;
    inject_and_resume("timer re-inject");
}

unsafe fn phase3_timer_ok(basic: u32, guest_page: u64) -> ! {
    if basic != EXIT_REASON_HLT {
        serial::write_line("boot: ERROR — phase3 expected HLT");
        finish_boot(false);
    }
    let mut ok = true;
    if ept_hw::verify_guest_irq(guest_page) {
        serial::write_line(M2_TIMER_OK_MARKER);
    } else {
        serial::write_line("boot: ERROR — timer-path IRQ ack missing");
        ok = false;
    }
    if serial_pio::guest_io_ok() {
        // Marker may already have been printed when magic completed.
    } else {
        serial::write_line("boot: ERROR — guest COM1 I/O magic missing");
        ok = false;
    }
    if msr_firewall::cpuid_filter_ok() {
        // Marker printed on the CPUID exit path.
    } else {
        serial::write_line("boot: ERROR — guest CPUID filter missing");
        ok = false;
    }
    if !ept_hw::verify_guest_cpuid_filtered(guest_page) {
        serial::write_line("boot: ERROR — guest CPUID ECX store still has VMX");
        ok = false;
    }
    if !ok {
        finish_boot(false);
    }
    enter_proto_kernel();
}

unsafe fn enter_proto_kernel() -> ! {
    let kernel = LOAD_KERNEL_PHYS;
    let boot_params = LOAD_BOOT_PARAMS_PHYS;
    if kernel == 0 || boot_params == 0 {
        serial::write_line("boot: ERROR — missing load info for proto-kernel");
        finish_boot(false);
    }
    if REAL_LINUX_GUEST {
        serial::write_str("boot: entering 64-bit Linux rip=0x");
    } else {
        serial::write_str("boot: entering 64-bit proto-kernel rip=0x");
    }
    write_hex_u64(kernel);
    serial::write_str(" rsi=0x");
    write_hex_u64(boot_params);
    serial::write_byte(b'\n');

    if ops::vmwrite(GUEST_RIP, kernel).is_err() {
        serial::write_line("boot: ERROR — VMWRITE guest RIP for kernel entry failed");
        finish_boot(false);
    }
    if REAL_LINUX_GUEST {
        // Bring-up IDT is not a Linux IDT — intercept faults before triple-fault.
        if ops::vmwrite(EXCEPTION_BITMAP, LINUX_EXCEPTION_BITMAP as u64).is_err()
            || ops::vmwrite(PAGE_FAULT_ERROR_CODE_MASK, 0).is_err()
            || ops::vmwrite(PAGE_FAULT_ERROR_CODE_MATCH, 0).is_err()
        {
            serial::write_line("boot: ERROR — exception bitmap VMWRITE failed");
            finish_boot(false);
        }
        serial::write_line("boot: Linux exception bitmap armed");

        // Host-own CR4.VMXE. `startup_64` does `cr4 &= 0x1060` then `mov %rax,%cr4`,
        // which clears VMXE. Under VMX that write is #GP(0). Keep VMXE set in the
        // VMCS guest CR4 and hide it via mask + read-shadow (guest sees VMXE=0).
        let guest_cr4 = ops::vmread(GUEST_CR4).unwrap_or(0) | cpu::CR4_VMXE;
        let shadow = guest_cr4 & !cpu::CR4_VMXE;
        if ops::vmwrite(GUEST_CR4, guest_cr4).is_err()
            || ops::vmwrite(CR4_GUEST_HOST_MASK, cpu::CR4_VMXE).is_err()
            || ops::vmwrite(CR4_READ_SHADOW, shadow).is_err()
        {
            serial::write_line("boot: ERROR — CR4.VMXE mask VMWRITE failed");
            finish_boot(false);
        }
        serial::write_line("boot: Linux CR4.VMXE host-owned");
    }
    let _ = ops::vmwrite(VM_ENTRY_INTERRUPTION_INFO, 0);
    let _ = ops::vmwrite(GUEST_INTERRUPTIBILITY_STATE, 0);
    let _ = ops::vmwrite(GUEST_ACTIVITY_STATE, 0);
    let _ = ops::vmwrite(GUEST_RFLAGS, 0x2);

    reset_saved_gprs(boot_params);
    EXIT_PHASE = 4;
    vmresume_with_gprs();
}

unsafe fn phase4_early_ok(basic: u32, guest_page: u64) -> ! {
    if REAL_LINUX_GUEST {
        phase4_linux_early(basic);
    }
    if basic != EXIT_REASON_HLT {
        serial::write_line("boot: ERROR — phase4 expected HLT");
        finish_boot(false);
    }
    let kernel_page = LOAD_KERNEL_PHYS & !0xfff;
    let mut ok = true;
    if guest_page != kernel_page {
        serial::write_line("boot: ERROR — proto-kernel HLT on unexpected page");
        ok = false;
    }
    if serial_pio::guest_early_ok() {
        // Marker may already have been printed when magic completed.
    } else {
        serial::write_line("boot: ERROR — proto-kernel early magic missing");
        ok = false;
    }
    if !ok {
        finish_boot(false);
    }

    // M3.4: arm a second host LAPIC one-shot while RIP is still the proto-kernel.
    // Distinct from M2.5 by lifecycle (post-EARLY) and marker, not by a new API.
    let bringup = BRINGUP_GUEST_CODE_PHYS;
    if bringup == 0 {
        serial::write_line("boot: ERROR — missing bring-up guest code for GTIMER");
        finish_boot(false);
    }
    ept_hw::clear_guest_irq(bringup);

    if apic::arm_bringup_timer(M2_IRQ_VECTOR as u8).is_err() {
        serial::write_line("boot: ERROR — guest timer arm failed");
        finish_boot(false);
    }
    serial::write_line("boot: guest timer armed (post-proto); waiting in guest HLT");

    if set_hlt_exiting(false).is_err() {
        serial::write_line("boot: ERROR — clear HLT exiting for guest timer failed");
        finish_boot(false);
    }
    let _ = ops::vmwrite(VM_ENTRY_INTERRUPTION_INFO, 0);
    let _ = ops::vmwrite(GUEST_INTERRUPTIBILITY_STATE, 0);
    let _ = ops::vmwrite(GUEST_ACTIVITY_STATE, 0);
    let _ = ops::vmwrite(GUEST_RFLAGS, 0x2 | (1 << 9));

    EXIT_PHASE = 5;
    resume_or_die();
}

/// Linux ISA IRQ0 vector — jiffies during APIC calibrate only (dropped after SHELL).
const LINUX_IRQ0_VECTOR: u32 = 0x30;
/// Faster host one-shot for post-GTIMER2 guest ticks (ONESHOT_COUNT / 16).
const LINUX_TICK_COUNT: u32 = 0x0010_0000;

unsafe fn arm_linux_tick() {
    let _ = apic::arm_oneshot_timer(M2_IRQ_VECTOR as u8, LINUX_TICK_COUNT);
}

/// Keep host one-shots running for IRQ0/APIC until SHELL.
unsafe fn arm_linux_tick_if_needed() {
    if !serial_pio::guest_shell_ok()
        || lapic_virt::host_timer_armed_for_guest()
        || lapic_virt::has_deliverable_irr()
    {
        arm_linux_tick();
    }
}

/// Inject ISA IRQ0 (jiffies) until SHELL — needed so APIC calibrate verify sees
/// jiffies advance. M3.19: no IRQ4; IRQ0 stops once `guest_shell_ok()`.
unsafe fn try_inject_linux_irq0() -> bool {
    if serial_pio::guest_shell_ok() {
        return false;
    }
    if !guest_can_accept_extint() {
        return false;
    }
    let cs = ops::vmread(GUEST_CS_SELECTOR).unwrap_or(0);
    if (cs & 3) != 0 {
        return false;
    }
    if let Ok(info) = interrupt::prepare_external_inject(LINUX_IRQ0_VECTOR) {
        let _ = ops::vmwrite(VM_ENTRY_INTERRUPTION_INFO, info as u64);
        let _ = ops::vmwrite(GUEST_INTERRUPTIBILITY_STATE, 0);
        let _ = ops::vmwrite(GUEST_ACTIVITY_STATE, 0);
        return true;
    }
    false
}

/// Real Linux post-entry loop: banner → MSR → GTIMER2 → wait for init SHELL.
unsafe fn phase4_linux_early(basic: u32) -> ! {
    // Close once both SHELL and APIC-OK are latched (either order).
    maybe_finish_m312();

    // Post-banner: host LAPIC one-shot → ext-IRQ → GTIMER2-OK (M3.9), then
    // keep running until real `/init` SHELL CPUID (M3.10 / M3.19).
    if LINUX_GTIMER2_ARMED && !LINUX_GTIMER2_DONE && basic == EXIT_REASON_EXTERNAL_INTERRUPT {
        let _ = apic::eoi();
        LINUX_GTIMER2_DONE = true;
        LINUX_GTIMER2_ARMED = false;
        serial::write_line(M3_GTIMER2_OK_MARKER);
        if msr_firewall::msr_firewall_ok() {
            serial::write_line("boot: MSR firewall exercised");
        }
        serial::write_line("boot: waiting for real init SHELL marker");
        // Quiet path: host ticks → IRQ0 (jiffies) + APIC IRR until SHELL.
        let _ = set_hlt_exiting(false);
        let _ = ops::vmwrite(EXCEPTION_BITMAP, 0);
        arm_linux_tick();
        let _ = ops::vmwrite(VM_ENTRY_INTERRUPTION_INFO, 0);
        let _ = ops::vmwrite(GUEST_INTERRUPTIBILITY_STATE, 0);
        let _ = ops::vmwrite(GUEST_ACTIVITY_STATE, 0);
        vmresume_with_gprs();
    }

    match basic {
        EXIT_REASON_HLT => {
            let _ = ops::vmwrite(VM_ENTRY_INTERRUPTION_INFO, 0);
            vmresume_with_gprs();
        }
        EXIT_REASON_INTERRUPT_WINDOW => {
            let _ = set_interrupt_window_exiting(false);
            // Window after IRQ0: deliver deferred APIC IRR (calibrate verify).
            if try_inject_guest_apic_timer() {
                arm_linux_tick_if_needed();
                vmresume_with_gprs();
            }
            maybe_arm_interrupt_window_for_apic();
            let _ = ops::vmwrite(VM_ENTRY_INTERRUPTION_INFO, 0);
            vmresume_with_gprs();
        }
        EXIT_REASON_EXTERNAL_INTERRUPT => {
            let _ = apic::eoi();
            if LINUX_GTIMER2_DONE {
                // Host one-shot → virtual APIC IRR (M3.12).
                let _ = lapic_virt::on_host_timer_fire();
                emit_lapic_markers();
                // Until SHELL: IRQ0 for jiffies (APIC calibrate), then APIC LVT.
                // M3.19: no IRQ4 COM1 TX inject (earlyprintk + CPUID SHELL).
                if try_inject_linux_irq0() {
                    maybe_arm_interrupt_window_for_apic();
                    arm_linux_tick_if_needed();
                    vmresume_with_gprs();
                }
                if try_inject_guest_apic_timer() {
                    arm_linux_tick_if_needed();
                    vmresume_with_gprs();
                }
                maybe_arm_interrupt_window_for_apic();
                arm_linux_tick_if_needed();
            }
            let _ = ops::vmwrite(VM_ENTRY_INTERRUPTION_INFO, 0);
            let _ = ops::vmwrite(GUEST_INTERRUPTIBILITY_STATE, 0);
            let _ = ops::vmwrite(GUEST_ACTIVITY_STATE, 0);
            vmresume_with_gprs();
        }
        EXIT_REASON_EXCEPTION_NMI => {
            dump_linux_exception_exit();
            finish_boot(false);
        }
        EXIT_REASON_TRIPLE_FAULT => {
            serial::write_line("boot: ERROR — Linux triple fault");
            dump_linux_guest_state();
            finish_boot(false);
        }
        EXIT_REASON_MSR_READ | EXIT_REASON_MSR_WRITE => handle_msr_and_resume(basic),
        EXIT_REASON_XSETBV => handle_xsetbv_and_resume(ops::vmread(GUEST_RIP).unwrap_or(0)),
        EXIT_REASON_CR_ACCESS => {
            serial::write_line("boot: ERROR — unexpected CR-access exit");
            dump_linux_guest_state();
            finish_boot(false);
        }
        _ => {
            let full = ops::vmread(EXIT_REASON).unwrap_or(basic as u64) as u32;
            serial::write_str("boot: linux unhandled exit reason=0x");
            write_hex_u32(full);
            if (full & (1 << 31)) != 0 {
                serial::write_str(" (VM-entry failure)");
            }
            serial::write_byte(b'\n');
            dump_linux_guest_state();
            finish_boot(false);
        }
    }
}

/// Arm host LAPIC after LINUX-EARLY; next ext-IRQ closes M3.9.
unsafe fn arm_linux_gtimer2() -> ! {
    LINUX_GTIMER2_ARMED = true;
    if apic::arm_bringup_timer(M2_IRQ_VECTOR as u8).is_err() {
        serial::write_line("boot: ERROR — Linux GTIMER2 arm failed");
        finish_boot(false);
    }
    serial::write_line("boot: Linux GTIMER2 armed; waiting for host LAPIC");
    let _ = ops::vmwrite(VM_ENTRY_INTERRUPTION_INFO, 0);
    let _ = ops::vmwrite(GUEST_INTERRUPTIBILITY_STATE, 0);
    let _ = ops::vmwrite(GUEST_ACTIVITY_STATE, 0);
    vmresume_with_gprs();
}

/// Emulate guest RDMSR/WRMSR via allow-list (M3.9).
unsafe fn handle_msr_and_resume(basic: u32) -> ! {
    let index = SAVED_GUEST_RCX as u32;
    let write_val = (SAVED_GUEST_RAX & 0xffff_ffff) | ((SAVED_GUEST_RDX & 0xffff_ffff) << 32);
    let is_write = basic == EXIT_REASON_MSR_WRITE;

    // M3.11/M3.12: x2APIC MSRs → virtual LAPIC (+ host arm / IRR inject).
    if lapic_virt::is_x2apic_msr(index) {
        if is_write {
            if let Some(true) = lapic_virt::wrmsr(index, write_val) {
                let _ = apic::arm_oneshot_timer(M2_IRQ_VECTOR as u8, LINUX_TICK_COUNT);
            }
        } else if let Some(v) = lapic_virt::rdmsr(index) {
            SAVED_GUEST_RAX = v as u32 as u64;
            SAVED_GUEST_RDX = v >> 32;
        }
        emit_lapic_markers();
        let guest_rip = ops::vmread(GUEST_RIP).unwrap_or(0);
        let insn_len = ops::vmread(VM_EXIT_INSTRUCTION_LEN).unwrap_or(2);
        let _ = ops::vmwrite(GUEST_RIP, guest_rip.wrapping_add(insn_len));
        if try_inject_guest_apic_timer() {
            vmresume_with_gprs();
        }
        maybe_arm_interrupt_window_for_apic();
        let _ = ops::vmwrite(VM_ENTRY_INTERRUPTION_INFO, 0);
        vmresume_with_gprs();
    }

    let access = if is_write {
        MsrAccess::Write
    } else {
        MsrAccess::Read
    };
    let action = msr_firewall::classify_msr(index, access);

    match action {
        MsrAction::InjectGp => {
            serial::write_str("boot: MSR #GP index=0x");
            write_hex_u32(index);
            serial::write_byte(b'\n');
            inject_gp0();
            vmresume_with_gprs();
        }
        MsrAction::HostPassthrough => {
            if is_write {
                // SAFETY: allow-listed host MSR write.
                cpu::wrmsr(index, write_val);
            } else {
                // SAFETY: allow-listed host MSR read.
                let v = cpu::rdmsr(index);
                SAVED_GUEST_RAX = v as u32 as u64;
                SAVED_GUEST_RDX = v >> 32;
            }
            msr_firewall::note_msr_emulated();
        }
        MsrAction::VmcsEfer => msr_vmcs_u64(GUEST_IA32_EFER, is_write, write_val),
        MsrAction::VmcsPat => msr_vmcs_u64(GUEST_IA32_PAT, is_write, write_val),
        MsrAction::VmcsSysenterCs => msr_vmcs_u64(GUEST_IA32_SYSENTER_CS, is_write, write_val),
        MsrAction::VmcsSysenterEsp => msr_vmcs_u64(GUEST_IA32_SYSENTER_ESP, is_write, write_val),
        MsrAction::VmcsSysenterEip => msr_vmcs_u64(GUEST_IA32_SYSENTER_EIP, is_write, write_val),
        MsrAction::VmcsFsBase => msr_vmcs_u64(GUEST_FS_BASE, is_write, write_val),
        MsrAction::VmcsGsBase => msr_vmcs_u64(GUEST_GS_BASE, is_write, write_val),
        MsrAction::Shadow => {
            if is_write {
                msr_firewall::shadow_write(index, write_val);
            } else {
                let v = msr_firewall::shadow_read(index);
                SAVED_GUEST_RAX = v as u32 as u64;
                SAVED_GUEST_RDX = v >> 32;
            }
            msr_firewall::note_msr_emulated();
        }
        MsrAction::ReadZero => {
            SAVED_GUEST_RAX = 0;
            SAVED_GUEST_RDX = 0;
        }
        MsrAction::IgnoreWrite => {}
    }

    let guest_rip = ops::vmread(GUEST_RIP).unwrap_or(0);
    let insn_len = ops::vmread(VM_EXIT_INSTRUCTION_LEN).unwrap_or(2);
    if insn_len == 0 || insn_len > 15 {
        serial::write_line("boot: ERROR — MSR exit with bad insn len");
        dump_linux_guest_state();
        finish_boot(false);
    }
    let _ = ops::vmwrite(GUEST_RIP, guest_rip.wrapping_add(insn_len));
    let _ = ops::vmwrite(VM_ENTRY_INTERRUPTION_INFO, 0);
    vmresume_with_gprs();
}

unsafe fn msr_vmcs_u64(field: u64, is_write: bool, write_val: u64) {
    if is_write {
        if ops::vmwrite(field, write_val).is_err() {
            serial::write_line("boot: ERROR — MSR VMCS write failed");
            finish_boot(false);
        }
    } else {
        let v = ops::vmread(field).unwrap_or(0);
        SAVED_GUEST_RAX = v as u32 as u64;
        SAVED_GUEST_RDX = v >> 32;
    }
    msr_firewall::note_msr_emulated();
}

/// Inject `#GP(0)` on the next VM-entry (do not advance RIP).
unsafe fn inject_gp0() {
    // vector=13, type=hardware exception (3), error-code valid, valid.
    let info: u64 = 13 | (3 << 8) | (1 << 11) | (1 << 31);
    let _ = ops::vmwrite(VM_ENTRY_INTERRUPTION_INFO, info);
    let _ = ops::vmwrite(VM_ENTRY_EXCEPTION_ERROR_CODE, 0);
}

unsafe fn dump_linux_exception_exit() {
    let info = ops::vmread(VM_EXIT_INTR_INFO).unwrap_or(0);
    let vector = (info & 0xff) as u32;
    let typ = ((info >> 8) & 7) as u32;
    let ec_valid = (info & (1 << 11)) != 0;
    let valid = (info & (1 << 31)) != 0;
    serial::write_str("boot: Linux exception valid=");
    serial::write_byte(if valid { b'1' } else { b'0' });
    serial::write_str(" type=0x");
    write_hex_u32(typ);
    serial::write_str(" vec=0x");
    write_hex_u32(vector);
    if ec_valid {
        let ec = ops::vmread(VM_EXIT_INTR_ERROR_CODE).unwrap_or(0) as u32;
        serial::write_str(" err=0x");
        write_hex_u32(ec);
    }
    if vector == 14 {
        // Intercepted #PF: fault address is EXIT_QUALIFICATION (CR2 may be stale).
        let addr = ops::vmread(EXIT_QUALIFICATION).unwrap_or(0);
        serial::write_str(" pfaddr=0x");
        write_hex_u64(addr);
    }
    serial::write_byte(b'\n');
    dump_linux_guest_state();
}

unsafe fn dump_linux_guest_state() {
    let rip = ops::vmread(GUEST_RIP).unwrap_or(0);
    let rsp = ops::vmread(GUEST_RSP).unwrap_or(0);
    let cr0 = ops::vmread(GUEST_CR0).unwrap_or(0);
    let cr3 = ops::vmread(GUEST_CR3).unwrap_or(0);
    let cr4 = ops::vmread(GUEST_CR4).unwrap_or(0);
    let qual = ops::vmread(EXIT_QUALIFICATION).unwrap_or(0);
    serial::write_str("boot: guest rip=0x");
    write_hex_u64(rip);
    serial::write_str(" rsp=0x");
    write_hex_u64(rsp);
    serial::write_str(" cr0=0x");
    write_hex_u64(cr0);
    serial::write_str(" cr3=0x");
    write_hex_u64(cr3);
    serial::write_str(" cr4=0x");
    write_hex_u64(cr4);
    serial::write_str(" qual=0x");
    write_hex_u64(qual);
    serial::write_byte(b'\n');
}

unsafe fn phase5_guest_timer_irq(basic: u32) -> ! {
    if basic != EXIT_REASON_EXTERNAL_INTERRUPT {
        serial::write_line("boot: ERROR — phase5 expected external-interrupt exit");
        finish_boot(false);
    }

    let exit_info = ops::vmread(VM_EXIT_INTR_INFO).unwrap_or(0) as u32;
    if (exit_info & (1 << 31)) != 0 {
        let vec = exit_info & 0xff;
        serial::write_str("boot: guest-timer IRQ vector=0x");
        write_hex_u32(vec);
        serial::write_byte(b'\n');
        if vec != M2_IRQ_VECTOR {
            serial::write_line("boot: ERROR — unexpected guest-timer exit vector");
            finish_boot(false);
        }
    } else {
        serial::write_line("boot: guest-timer IRQ (no ack-info); assuming LAPIC");
    }

    if apic::eoi().is_err() {
        serial::write_line("boot: ERROR — APIC EOI failed (guest timer)");
        finish_boot(false);
    }
    serial::write_line("boot: guest-timer APIC EOI ok");
    // Stop further LAPIC timer IRQs before proto-init OUT storm (M3.5).
    let _ = apic::mask_timer();

    if set_hlt_exiting(true).is_err() {
        serial::write_line("boot: ERROR — restore HLT exiting after guest timer failed");
        finish_boot(false);
    }

    EXIT_PHASE = 6;
    inject_and_resume("guest timer re-inject");
}

unsafe fn phase6_gtimer_ok(basic: u32, guest_page: u64) -> ! {
    if basic != EXIT_REASON_HLT {
        serial::write_line("boot: ERROR — phase6 expected HLT");
        finish_boot(false);
    }
    let mut ok = true;
    let bringup = BRINGUP_GUEST_CODE_PHYS;
    let ack_page = if guest_page == (bringup & !0xfff) {
        guest_page
    } else {
        bringup
    };
    if ept_hw::verify_guest_irq(ack_page) {
        serial::write_line(M3_GTIMER_OK_MARKER);
    } else {
        serial::write_line("boot: ERROR — guest-timer IRQ ack missing");
        ok = false;
    }
    if !serial_pio::guest_early_ok() {
        serial::write_line("boot: ERROR — early marker cleared before GTIMER");
        ok = false;
    }
    if !ok {
        finish_boot(false);
    }
    enter_proto_init();
}

unsafe fn enter_proto_init() -> ! {
    let init = LOAD_INIT_PHYS;
    if init == 0 {
        serial::write_line("boot: ERROR — missing proto-init load address");
        finish_boot(false);
    }
    // Defensive: must HLT-exit out of proto-init (phase4 may have cleared this).
    if set_hlt_exiting(true).is_err() {
        serial::write_line("boot: ERROR — HLT exiting off before proto-init");
        finish_boot(false);
    }
    let _ = apic::mask_timer();

    serial::write_str("boot: entering proto-init rip=0x");
    write_hex_u64(init);
    serial::write_byte(b'\n');

    if ops::vmwrite(GUEST_RIP, init).is_err() {
        serial::write_line("boot: ERROR — VMWRITE guest RIP for proto-init failed");
        finish_boot(false);
    }
    let _ = ops::vmwrite(VM_ENTRY_INTERRUPTION_INFO, 0);
    let _ = ops::vmwrite(GUEST_INTERRUPTIBILITY_STATE, 0);
    let _ = ops::vmwrite(GUEST_ACTIVITY_STATE, 0);
    let _ = ops::vmwrite(GUEST_RFLAGS, 0x2);

    reset_saved_gprs(LOAD_BOOT_PARAMS_PHYS);
    EXIT_PHASE = 7;
    vmresume_with_gprs();
}

unsafe fn phase7_shell_ok(basic: u32, guest_page: u64) -> ! {
    if basic != EXIT_REASON_HLT {
        serial::write_line("boot: ERROR — phase7 expected HLT");
        finish_boot(false);
    }
    let init_page = LOAD_INIT_PHYS & !0xfff;
    let mut ok = true;
    if guest_page != init_page {
        serial::write_line("boot: ERROR — proto-init HLT on unexpected page");
        ok = false;
    }
    if serial_pio::guest_shell_ok() {
        // Marker may already have been printed when magic completed.
    } else {
        serial::write_line("boot: ERROR — proto-init shell magic missing");
        ok = false;
    }
    if !serial_pio::guest_early_ok() {
        serial::write_line("boot: ERROR — early marker missing at shell");
        ok = false;
    }
    if !ok {
        finish_boot(false);
    }
    enter_exit_loop();
}

/// After SHELL-OK: keep HLT exiting and resume into a durable HLT loop.
unsafe fn enter_exit_loop() -> ! {
    if set_hlt_exiting(true).is_err() {
        serial::write_line("boot: ERROR — HLT exiting off before exit loop");
        finish_boot(false);
    }
    let _ = apic::mask_timer();
    let _ = ops::vmwrite(VM_ENTRY_INTERRUPTION_INFO, 0);
    LOOP_HLT_COUNT = 0;
    EXIT_PHASE = 8;
    serial::write_line("boot: entering continuous exit loop");
    // Re-execute the proto-init HLT (RIP unchanged) for LOOP_HLT_TARGET exits.
    vmresume_with_gprs();
}

unsafe fn phase8_exit_loop(basic: u32) -> ! {
    // I/O / CPUID already resumed above. HLT proves the durable loop.
    if basic != EXIT_REASON_HLT {
        serial::write_str("boot: loop stub — unexpected exit reason=0x");
        write_hex_u32(basic);
        serial::write_byte(b'\n');
        // Safe halt for M3.6: do not resume unknown reasons yet (MSR/EPT later).
        finish_boot(false);
    }

    LOOP_HLT_COUNT = LOOP_HLT_COUNT.saturating_add(1);
    if LOOP_HLT_COUNT >= LOOP_HLT_TARGET {
        if !serial_pio::guest_shell_ok() {
            serial::write_line("boot: ERROR — shell latch cleared during loop");
            finish_boot(false);
        }
        serial::write_line(M3_LOOP_OK_MARKER);
        finish_boot(true);
    }

    let _ = ops::vmwrite(VM_ENTRY_INTERRUPTION_INFO, 0);
    vmresume_with_gprs();
}

unsafe fn inject_and_resume(tag: &str) -> ! {
    let info = match interrupt::prepare_external_inject(M2_IRQ_VECTOR) {
        Ok(v) => v,
        Err(_) => {
            serial::write_line("boot: ERROR — inject vector rejected by firewall");
            finish_boot(false);
        }
    };
    if ops::vmwrite(VM_ENTRY_INTERRUPTION_INFO, info as u64).is_err() {
        serial::write_line("boot: ERROR — VMWRITE interrupt-info failed");
        finish_boot(false);
    }
    let _ = ops::vmwrite(GUEST_INTERRUPTIBILITY_STATE, 0);
    let _ = ops::vmwrite(GUEST_ACTIVITY_STATE, 0);
    let _ = ops::vmwrite(GUEST_RFLAGS, 0x2 | (1 << 9));

    serial::write_str("boot: ");
    serial::write_str(tag);
    serial::write_str(" vector 0x");
    write_hex_u32(M2_IRQ_VECTOR);
    serial::write_line(" + VMRESUME");
    resume_or_die();
}

unsafe fn set_hlt_exiting(on: bool) -> Result<(), ()> {
    let cur = ops::vmread(PRIMARY_PROC_BASED_VM_EXEC_CONTROL).map_err(|_| ())? as u32;
    let next = if on {
        cur | CPU_BASED_HLT_EXITING
    } else {
        cur & !CPU_BASED_HLT_EXITING
    };
    ops::vmwrite(PRIMARY_PROC_BASED_VM_EXEC_CONTROL, next as u64).map_err(|_| ())
}

unsafe fn set_interrupt_window_exiting(on: bool) -> Result<(), ()> {
    let cur = ops::vmread(PRIMARY_PROC_BASED_VM_EXEC_CONTROL).map_err(|_| ())? as u32;
    let next = if on {
        cur | CPU_BASED_INTERRUPT_WINDOW_EXITING
    } else {
        cur & !CPU_BASED_INTERRUPT_WINDOW_EXITING
    };
    ops::vmwrite(PRIMARY_PROC_BASED_VM_EXEC_CONTROL, next as u64).map_err(|_| ())
}

unsafe fn resume_or_die() -> ! {
    match ops::vmresume() {
        Ok(()) => {
            serial::write_line("boot: ERROR — VMRESUME returned Ok");
            finish_boot(false);
        }
        Err(_) => {
            let ierr = ops::vmread(VM_INSTRUCTION_ERROR).unwrap_or(0xFFFF) as u32;
            serial::write_str("boot: ERROR — VMRESUME failed insn_error=0x");
            write_hex_u32(ierr);
            serial::write_byte(b'\n');
            finish_boot(false);
        }
    }
}

fn finish_boot(ok: bool) -> ! {
    // SAFETY: still in VMX root after VMEXIT; tear down before QEMU exit.
    match unsafe { hardware::vmxoff() } {
        Ok(()) => serial::write_line("boot: VMXOFF ok"),
        Err(_) => serial::write_line("boot: ERROR — VMXOFF failed"),
    }

    if ok {
        // SAFETY: boot single-threaded; flag set before VMLAUNCH.
        if unsafe { REAL_LINUX_GUEST } {
            if lapic_virt::apic_ok() && crate::memory::precise_ranges_ok() {
                serial::write_line(
                    "boot: M3.19 complete — no ISA IRQ crutches + precise EPT + APIC + SHELL OK",
                );
            } else if lapic_virt::apic_ok() {
                serial::write_line("boot: M3.12 complete — Linux APIC inject + SHELL OK");
            } else if lapic_virt::gtimer3_ok() {
                serial::write_line("boot: M3.11 complete — Linux GTIMER3 + SHELL OK");
            } else {
                serial::write_line("boot: M3.10 complete — Linux SHELL OK");
            }
        } else {
            serial::write_line("boot: M3.10 complete — proto path OK");
        }
        serial::qemu_exit_success();
    } else {
        serial::write_line("boot: boot gate failed");
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
        assert_eq!(M2_EPT_OK_MARKER, "RAYNU-V-M2-EPT-OK");
        assert_eq!(M2_GUEST_OK_MARKER, "RAYNU-V-M2-GUEST-OK");
        assert_eq!(M2_OWN_OK_MARKER, "RAYNU-V-M2-OWN-OK");
        assert_eq!(M2_ALLOC_OK_MARKER, "RAYNU-V-M2-ALLOC-OK");
        assert_eq!(M2_IRQ_OK_MARKER, "RAYNU-V-M2-IRQ-OK");
        assert_eq!(M2_TIMER_OK_MARKER, "RAYNU-V-M2-TIMER-OK");
        assert_eq!(M3_IO_OK_MARKER, "RAYNU-V-M3-IO-OK");
        assert_eq!(M3_CPUID_OK_MARKER, "RAYNU-V-M3-CPUID-OK");
        assert_eq!(M3_EARLY_OK_MARKER, "RAYNU-V-M3-EARLY-OK");
        assert_eq!(M3_GTIMER_OK_MARKER, "RAYNU-V-M3-GTIMER-OK");
        assert_eq!(M3_SHELL_OK_MARKER, "RAYNU-V-M3-SHELL-OK");
        assert_eq!(M3_LOOP_OK_MARKER, "RAYNU-V-M3-LOOP-OK");
        assert_eq!(LOOP_HLT_TARGET, 4);
        assert_eq!(EXIT_REASON_HLT, 12);
        assert_eq!(EXIT_REASON_EXTERNAL_INTERRUPT, 1);
        assert_eq!(EXIT_REASON_CPUID, 10);
        assert_eq!(EXIT_REASON_IO_INSTRUCTION, 30);
        assert_eq!(EXIT_REASON_XSETBV, 55);
        assert_eq!(PIN_BASED_EXTERNAL_INTERRUPT_EXITING, 1);
        assert_eq!(VM_EXIT_ACK_INTERRUPT_ON_EXIT, 1 << 15);
        assert_eq!(CPU_BASED_CPUID_EXITING, 1 << 21);
        assert_eq!(CPU_BASED_UNCONDITIONAL_IO, 1 << 24);
    }
}
