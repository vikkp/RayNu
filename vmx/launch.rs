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
//! M4.0: after G0 SHELL+APIC, VMLAUNCH G1 under private EPT → `RAYNU-V-M4-2VM-OK`.
//! M4.1: credit scheduler time-slices G0↔G1 → `RAYNU-V-M4-SCHED-OK`.
//! M4.2: scale to G0+G1+G2+G3 (≥4) → `RAYNU-V-M4-NVM-OK`.
//! M4.3: after NVM-OK, virtio-blk MMIO probe → `RAYNU-V-M4-BLK-OK`.
//! M4.4: after BLK-OK, virtio-net dual-port vSwitch → `RAYNU-V-M4-NET-OK`.
//! M4.5: after NET-OK, dual-vCPU BSP+AP shared-EPT probe → `RAYNU-V-M4-SMP-OK`.

use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

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
use crate::devices::virtio_blk::{
    self, M4_BLK_OK_MARKER, M7_ISO_BOOTED_FROM_DISK_MARKER, M7_ISO_DISK_WRITTEN_MARKER,
    M7_ISO_INSTALL_LAB_OK_MARKER, M7_ISO_REBOOT_PENDING_MARKER,
};
use crate::devices::virtio_net::{self, M4_NET_OK_MARKER};
use crate::mgmt::iso_install;
use crate::mgmt::spa_launch::{self, M7_E4_SPA_LAUNCH_OK_MARKER};

/// Finish marker when IRQ4 inject is gone and IRQ0 stops at SHELL (M3.19).
pub const M3_NOIRQ_OK_MARKER: &str = "RAYNU-V-M3-NOIRQ-OK";
use crate::memory::ept::{
    self, M2_BRINGUP_GUEST_ID, M2_OWN_OK_MARKER, M4_2VM_OK_MARKER, M4_GUEST1_ID, M4_SHELL_G1_MARKER,
};
use crate::memory::ept_hw::{self, GUEST_ISR_OFF, M2_EPT_OK_MARKER, M2_GUEST_OK_MARKER};
use crate::memory::frame_allocator::{self, M2_ALLOC_OK_MARKER};
use crate::sched::interrupt::{
    self, M2_IRQ_OK_MARKER, M2_IRQ_VECTOR, M2_TIMER_OK_MARKER, M3_GTIMER2_OK_MARKER,
    M3_GTIMER_OK_MARKER,
};
use crate::sched::msr_firewall::{self, MsrAccess, MsrAction, M3_CPUID_OK_MARKER};
use crate::sched::scheduler::{
    CreditScheduler, DEFAULT_CREDIT, M4_NVM_OK_MARKER, M4_SCHED_OK_MARKER, M4_SLICE_G0_MARKER,
    M4_SLICE_G1_MARKER, M4_SLICE_G2_MARKER, M4_SLICE_G3_MARKER,
};
use crate::sched::smp_probe::{self, M4_SMP_OK_MARKER};
use crate::vmx::fields::*;
use crate::vmx::hardware;
use crate::vmx::ops::{self, VmFailKind, VmcsOpError};
use crate::vmx::{guest_pt, mmio_decode};

/// M7.5 / HDA E2 runtime marker — printed by firmware after a successful
/// Linux SHELL bring-up (and any armed M4 probes). Closing the *gate* in
/// `docs/evidence/r640/` still requires real PowerEdge serial; host smoke
/// never prints this string.
pub const M7_R640_BOOT_OK_MARKER: &str = "RAYNU-V-R640-BOOT-OK";

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
    VmwriteFailed {
        field: u64,
    },
    LaunchFailed {
        instruction_error: u32,
    },
}

impl From<VmcsOpError> for LaunchError {
    fn from(_: VmcsOpError) -> Self {
        // Prefer the typed `vw()` path which records the field encoding.
        Self::VmwriteFailed { field: 0xffff_ffff }
    }
}

/// ADR-003 split-mode path for a live EDK2 `OVMF.fd`. Not embedded.
pub const GUEST_UEFI_OVMF_ESP_PATH: &str = "\\EFI\\RayNu\\OVMF.fd";
/// Minimum live-sized ESP map. 1 MiB EDK2 fixture stays below this.
pub const MIN_LIVE_ESP_OVMF_BYTES: usize = 2 * 1024 * 1024;
/// Minimum firmware-alias image. Live-sized map stays below this.
pub const MIN_FIRMWARE_ALIAS_BYTES: usize = 4 * 1024 * 1024;
/// x86 reset-vector stub length at the end of a firmware image (SDM 9.1.4).
pub const GUEST_UEFI_RESET_VECTOR_LEN: usize = 16;
/// JMP FAR opcode that qualifies a reset-vector stub. Not EDK2 SEC.
pub const GUEST_UEFI_RESET_VECTOR_OPCODE: u8 = 0xEA;
/// Real-mode CS for the reset-vector VMCS contract.
pub const GUEST_UEFI_RESET_CS: u16 = 0xF000;
/// RIP at CS.base + 0xFFF0 → GPA `0xFFFF_FFF0`.
pub const GUEST_UEFI_RESET_RIP: u64 = 0xFFF0;
/// GPA of the x86 reset vector (firmware alias at the top of 4 GiB).
pub const GUEST_UEFI_RESET_VECTOR_GPA: u64 = 0xFFFF_FFF0;
/// Top of the 4 GiB firmware-alias window (exclusive).
pub const GUEST_UEFI_FIRMWARE_TOP_GPA: u64 = 0x1_0000_0000;
/// Unrestricted-guest secondary bit. Contract only — not ORed into E4 SHELL VMCS.
pub const GUEST_UEFI_UNRESTRICTED_GUEST: u32 =
    crate::vmx::fields::SECONDARY_ENABLE_UNRESTRICTED_GUEST;

const _: () = assert!(MIN_LIVE_ESP_OVMF_BYTES > 1024 * 1024);
const _: () = assert!(MIN_LIVE_ESP_OVMF_BYTES < MIN_FIRMWARE_ALIAS_BYTES);
const _: () = assert!(GUEST_UEFI_UNRESTRICTED_GUEST == 1 << 7);

/// Documented reset-vector VMCS entry. Not a live VMWRITE / not VMLAUNCH.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GuestUefiResetVmcs {
    pub cs_selector: u16,
    pub rip: u64,
    pub reset_gpa: u64,
}

/// Documented 4 GiB firmware-alias EPT window. Not a live EPT write / not VMLAUNCH.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GuestUefiAliasEpt {
    pub gpa: u64,
    pub bytes_len: u64,
}

/// Intel SDM Vol 3A 9.1.4 first-instruction contract for guest UEFI.
pub const GUEST_UEFI_RESET_VMCS: GuestUefiResetVmcs = GuestUefiResetVmcs {
    cs_selector: GUEST_UEFI_RESET_CS,
    rip: GUEST_UEFI_RESET_RIP,
    reset_gpa: GUEST_UEFI_RESET_VECTOR_GPA,
};

/// 4 MiB alias window under 4 GiB. Reset vector `0xFFFF_FFF0` sits inside.
pub const GUEST_UEFI_ALIAS_EPT: GuestUefiAliasEpt = GuestUefiAliasEpt {
    gpa: GUEST_UEFI_FIRMWARE_TOP_GPA - MIN_FIRMWARE_ALIAS_BYTES as u64,
    bytes_len: MIN_FIRMWARE_ALIAS_BYTES as u64,
};

const _: () = assert!(GUEST_UEFI_ALIAS_EPT.gpa == 0xFFC0_0000);
const _: () = assert!(GUEST_UEFI_ALIAS_EPT.gpa <= GUEST_UEFI_RESET_VECTOR_GPA);
const _: () = assert!(
    GUEST_UEFI_RESET_VECTOR_GPA < GUEST_UEFI_ALIAS_EPT.gpa + GUEST_UEFI_ALIAS_EPT.bytes_len
);

/// Guest UEFI VMLAUNCH error. Not the E4 SHELL path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuestUefiLaunchError {
    /// No live ESP `\\EFI\\RayNu\\OVMF.fd` mapping. Mock / floor / 1 MiB fixture refused.
    MissingEspFirmware,
    /// Live-sized map is recorded; VMLAUNCH instruction is not issued this slice.
    LiveMappedNotLaunched,
    /// Live map is present but the last 16 bytes are not a JMP FAR reset stub.
    NoResetVector,
    /// Reset-vector VMCS contract is recorded; VMLAUNCH instruction is not issued.
    ResetVectorNotLaunched,
    /// Firmware-alias contract is recorded; VMLAUNCH instruction is not issued.
    FirmwareAliasNotLaunched,
    /// Alias-EPT program contract is recorded; VMLAUNCH instruction is not issued.
    AliasEptNotLaunched,
}

/// JUSTIFICATION (global state): live-map bookkeeping is process-local.
/// Host tests reset via [`reset_live_esp_ovmf_mapping`]. Not a VMCS write.
static LIVE_ESP_OVMF_MAPPED: AtomicBool = AtomicBool::new(false);
static LIVE_ESP_OVMF_BYTES: AtomicU64 = AtomicU64::new(0);
static GUEST_UEFI_RESET_ARMED: AtomicBool = AtomicBool::new(false);
static GUEST_UEFI_FIRMWARE_ALIAS_ARMED: AtomicBool = AtomicBool::new(false);
static GUEST_UEFI_FIRMWARE_ALIAS_BYTES: AtomicU64 = AtomicU64::new(0);
static GUEST_UEFI_ALIAS_EPT_PROGRAMMED: AtomicBool = AtomicBool::new(false);
static GUEST_UEFI_ALIAS_EPT_GPA: AtomicU64 = AtomicU64::new(0);

/// Record a live-sized ESP map. Rejects the 1 MiB fixture. Not VMLAUNCH.
pub fn arm_live_esp_ovmf_mapping(bytes_len: u64) -> Result<(), GuestUefiLaunchError> {
    if bytes_len < MIN_LIVE_ESP_OVMF_BYTES as u64 {
        return Err(GuestUefiLaunchError::MissingEspFirmware);
    }
    LIVE_ESP_OVMF_BYTES.store(bytes_len, Ordering::Release);
    LIVE_ESP_OVMF_MAPPED.store(true, Ordering::Release);
    Ok(())
}

/// True after a successful [`arm_live_esp_ovmf_mapping`].
pub fn live_esp_ovmf_is_mapped() -> bool {
    LIVE_ESP_OVMF_MAPPED.load(Ordering::Acquire)
}

/// Recorded live-map length. Zero when unmapped.
pub fn live_esp_ovmf_bytes_len() -> u64 {
    LIVE_ESP_OVMF_BYTES.load(Ordering::Acquire)
}

/// Clear the process-local live-map, reset-vector, and firmware-alias flags.
pub fn reset_live_esp_ovmf_mapping() {
    LIVE_ESP_OVMF_MAPPED.store(false, Ordering::Release);
    LIVE_ESP_OVMF_BYTES.store(0, Ordering::Release);
    GUEST_UEFI_RESET_ARMED.store(false, Ordering::Release);
    GUEST_UEFI_FIRMWARE_ALIAS_ARMED.store(false, Ordering::Release);
    GUEST_UEFI_FIRMWARE_ALIAS_BYTES.store(0, Ordering::Release);
    GUEST_UEFI_ALIAS_EPT_PROGRAMMED.store(false, Ordering::Release);
    GUEST_UEFI_ALIAS_EPT_GPA.store(0, Ordering::Release);
}

/// True after a successful [`arm_guest_uefi_reset_vector`].
pub fn guest_uefi_reset_vector_is_armed() -> bool {
    GUEST_UEFI_RESET_ARMED.load(Ordering::Acquire)
}

/// Clear only the reset-vector flag.
pub fn reset_guest_uefi_reset_vector() {
    GUEST_UEFI_RESET_ARMED.store(false, Ordering::Release);
}

/// GPA of a firmware-alias mapping under the 4 GiB top. None if undersized.
pub fn firmware_alias_gpa(bytes_len: u64) -> Option<u64> {
    if bytes_len < MIN_FIRMWARE_ALIAS_BYTES as u64 || bytes_len > MIN_FIRMWARE_ALIAS_BYTES as u64 {
        return None;
    }
    Some(GUEST_UEFI_FIRMWARE_TOP_GPA - bytes_len)
}

/// True after a successful [`arm_guest_uefi_firmware_alias`].
pub fn guest_uefi_firmware_alias_is_armed() -> bool {
    GUEST_UEFI_FIRMWARE_ALIAS_ARMED.load(Ordering::Acquire)
}

/// Record the unrestricted-guest + 4 GiB firmware-alias contract (ADR-014 Stage 15).
///
/// INVARIANTS:
/// - Requires a prior reset-vector arm
/// - Requires `bytes_len >= MIN_FIRMWARE_ALIAS_BYTES`
/// - Does not VMWRITE and does not issue VMLAUNCH
/// - Does not OR unrestricted guest into the E4 SHELL VMCS
pub fn arm_guest_uefi_firmware_alias(bytes_len: u64) -> Result<(), GuestUefiLaunchError> {
    if !guest_uefi_reset_vector_is_armed() {
        return Err(GuestUefiLaunchError::ResetVectorNotLaunched);
    }
    if firmware_alias_gpa(bytes_len).is_none() {
        return Err(GuestUefiLaunchError::MissingEspFirmware);
    }
    GUEST_UEFI_FIRMWARE_ALIAS_BYTES.store(bytes_len, Ordering::Release);
    GUEST_UEFI_FIRMWARE_ALIAS_ARMED.store(true, Ordering::Release);
    Ok(())
}

/// True when reset GPA sits inside `[gpa, gpa + bytes_len)`.
pub fn alias_ept_covers_reset(gpa: u64, bytes_len: u64) -> bool {
    match gpa.checked_add(bytes_len) {
        Some(end) => gpa <= GUEST_UEFI_RESET_VECTOR_GPA && GUEST_UEFI_RESET_VECTOR_GPA < end,
        None => false,
    }
}

/// True after a successful [`program_guest_uefi_alias_ept`].
pub fn guest_uefi_alias_ept_is_programmed() -> bool {
    GUEST_UEFI_ALIAS_EPT_PROGRAMMED.load(Ordering::Acquire)
}

/// Record the alias-EPT program contract after firmware-alias (ADR-014 Stage 16).
///
/// INVARIANTS:
/// - Requires a prior firmware-alias arm
/// - Requires `firmware_alias_gpa(bytes_len)` and reset-vector coverage
/// - Does not write live EPT, does not VMWRITE, and does not issue VMLAUNCH
pub fn program_guest_uefi_alias_ept(bytes_len: u64) -> Result<(), GuestUefiLaunchError> {
    if !guest_uefi_firmware_alias_is_armed() {
        return Err(GuestUefiLaunchError::FirmwareAliasNotLaunched);
    }
    let Some(gpa) = firmware_alias_gpa(bytes_len) else {
        return Err(GuestUefiLaunchError::MissingEspFirmware);
    };
    if !alias_ept_covers_reset(gpa, bytes_len) {
        return Err(GuestUefiLaunchError::MissingEspFirmware);
    }
    GUEST_UEFI_ALIAS_EPT_GPA.store(gpa, Ordering::Release);
    GUEST_UEFI_ALIAS_EPT_PROGRAMMED.store(true, Ordering::Release);
    Ok(())
}

/// Probe a JMP FAR reset-vector stub at the end of a live-sized image.
///
/// INVARIANTS:
/// - Requires `bytes.len() >= MIN_LIVE_ESP_OVMF_BYTES`
/// - Last [`GUEST_UEFI_RESET_VECTOR_LEN`] starts with JMP FAR (`0xEA`)
/// - A synthetic stub is not a shipped EDK2 `OVMF.fd`
pub fn probe_guest_uefi_reset_vector(bytes: &[u8]) -> Result<(), GuestUefiLaunchError> {
    if bytes.len() < MIN_LIVE_ESP_OVMF_BYTES {
        return Err(GuestUefiLaunchError::MissingEspFirmware);
    }
    let off = bytes.len() - GUEST_UEFI_RESET_VECTOR_LEN;
    if bytes[off] != GUEST_UEFI_RESET_VECTOR_OPCODE {
        return Err(GuestUefiLaunchError::NoResetVector);
    }
    Ok(())
}

/// Record the reset-vector VMCS contract after a live-sized map (ADR-014 Stage 14).
///
/// INVARIANTS:
/// - Requires a prior live-sized map
/// - Requires a JMP FAR stub at the image end
/// - Does not VMWRITE and does not issue VMLAUNCH
pub fn arm_guest_uefi_reset_vector(bytes: &[u8]) -> Result<(), GuestUefiLaunchError> {
    if !live_esp_ovmf_is_mapped() {
        return Err(GuestUefiLaunchError::MissingEspFirmware);
    }
    probe_guest_uefi_reset_vector(bytes)?;
    GUEST_UEFI_RESET_ARMED.store(true, Ordering::Release);
    Ok(())
}

/// Guest UEFI VMLAUNCH from ESP `\\EFI\\RayNu\\OVMF.fd` (ADR-014 Stage 16).
///
/// INVARIANTS:
/// - Does not VMLAUNCH the 80-byte mock, 4 KiB floor, 1 MiB fixture,
///   2 MiB map, synthetic `0xEA` stub, or 4 MiB alias fixture
/// - Unmapped → [`GuestUefiLaunchError::MissingEspFirmware`]
/// - Mapped, no reset-vector → [`GuestUefiLaunchError::LiveMappedNotLaunched`]
/// - Reset-vector armed, no alias → [`GuestUefiLaunchError::ResetVectorNotLaunched`]
/// - Alias armed, no EPT program → [`GuestUefiLaunchError::FirmwareAliasNotLaunched`]
/// - Alias-EPT programmed → [`GuestUefiLaunchError::AliasEptNotLaunched`]
///   (contracts [`GUEST_UEFI_RESET_VMCS`] + [`GUEST_UEFI_ALIAS_EPT`];
///   no live EPT write / no VMWRITE / no insn)
/// - Does not change `iso=0` E4 SHELL
///
/// VERIFICATION: L0 (documented). Outside the firmware-blob Proven Core set.
pub fn try_vmlaunch_guest_uefi_ovmf() -> Result<(), GuestUefiLaunchError> {
    if !live_esp_ovmf_is_mapped() {
        return Err(GuestUefiLaunchError::MissingEspFirmware);
    }
    if !guest_uefi_reset_vector_is_armed() {
        return Err(GuestUefiLaunchError::LiveMappedNotLaunched);
    }
    if !guest_uefi_firmware_alias_is_armed() {
        let _ = GUEST_UEFI_RESET_VMCS;
        return Err(GuestUefiLaunchError::ResetVectorNotLaunched);
    }
    if !guest_uefi_alias_ept_is_programmed() {
        let _ = (
            GUEST_UEFI_RESET_VMCS,
            GUEST_UEFI_UNRESTRICTED_GUEST,
            firmware_alias_gpa(GUEST_UEFI_FIRMWARE_ALIAS_BYTES.load(Ordering::Acquire)),
        );
        return Err(GuestUefiLaunchError::FirmwareAliasNotLaunched);
    }
    let _ = (
        GUEST_UEFI_RESET_VMCS,
        GUEST_UEFI_UNRESTRICTED_GUEST,
        GUEST_UEFI_ALIAS_EPT,
        GUEST_UEFI_ALIAS_EPT_GPA.load(Ordering::Acquire),
    );
    Err(GuestUefiLaunchError::AliasEptNotLaunched)
}

/// Physical frames needed for the M1.2/M2.x HLT + IRQ guest under EPT.
#[derive(Clone, Copy)]
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
    /// Guest CR3 override (`None` = share host CR3 — G0 bring-up).
    /// G1 must supply slab-local tables (private EPT cannot walk host PTs).
    pub guest_cr3_phys: Option<u64>,
    /// Optional MSR bitmap (only if primary controls force USE_MSR_BITMAPS).
    pub msr_bitmap_phys: Option<u64>,
    pub io_bitmap_a_phys: Option<u64>,
    pub io_bitmap_b_phys: Option<u64>,
}

/// M4.2: G0 + up to three SHELL guests (G1–G3).
pub const M4_NVM_GUEST_SLOTS: usize = 4;

/// Prepared guest frames: slot 0 = G0 (Linux), slots 1..3 = SHELL CPUID guests.
static mut GUEST_FRAMES: [Option<LaunchFrames>; M4_NVM_GUEST_SLOTS] = [None; M4_NVM_GUEST_SLOTS];
/// Highest prepared shell slot index (1..=3); 0 means none.
static mut SHELL_SLOT_MAX: usize = 0;
/// True when at least one shell guest is registered.
static mut HAS_SECOND_GUEST: bool = false;
/// True after G0 SHELL+APIC and we have started launching shell guests.
static mut SECOND_GUEST_STARTED: bool = false;
/// Active guest id for SHELL routing (`M2_BRINGUP_GUEST_ID` or M4_GUEST*).
static mut ACTIVE_GUEST_ID: u64 = 1; // M2_BRINGUP_GUEST_ID

/// M4.1/M4.2: credit scheduler active after all shell guests have latched SHELL.
static mut SCHED_MODE: bool = false;
static mut SCHED: CreditScheduler = CreditScheduler::new();
static mut SCHED_SLOT_CUR: usize = 0;
static mut SCHED_SLICE: [bool; M4_NVM_GUEST_SLOTS] = [false; M4_NVM_GUEST_SLOTS];
static mut SCHED_OK_LATCHED: bool = false;
static mut NVM_OK_LATCHED: bool = false;
/// True after M4.3–M4.5 probes finished; scheduler may resume for Phase F.
static mut M4_LADDER_DONE: bool = false;
static mut TWO_VM_LATCHED: bool = false;
/// E4: SPA start VMLAUNCHed G1 from the unmapped 2 MiB slab (not G0 identity VMCS).
static mut SPA_LAUNCHED: bool = false;
static mut SPA_RUNNABLE: bool = false;
/// G0 VMCS cloned (VMREAD/VMWRITE) to a host-only punched slab.
static mut G0_VMCS_RELOCATED: bool = false;
/// After `VMCLEAR` of G0, first re-entry must be `VMLAUNCH` (clear state).
static mut G0_NEEDS_VMLAUNCH: bool = false;
/// Sticky: first `VMPTRLD` of slot 0 failed after E4 — do not pick slot 0 again.
static mut G0_VMPTRLD_FAILED: bool = false;
/// After `VMCLEAR` of slot 1, first re-entry must be `VMLAUNCH` (clear state).
static mut SPA_NEEDS_VMLAUNCH: bool = false;
/// Sticky: first `VMPTRLD` of slot 1 failed after E4 — park SPA (no retry flood).
static mut SPA_VMPTRLD_FAILED: bool = false;
/// Software shadow of [`VMCS_CLONE_FIELDS`] for slots 0 and 1.
/// Iron 2026-08-21 (no-incoming-rewrite): first SPA `VMLAUNCH` OK, then
/// `VMCLEAR` + later `VMLAUNCH` saw pin/primary/exit/entry/EPTP/RIP all 0
/// (error 7). SDM: do not assume `VMCLEAR` leaves VMCS data unmodified.
/// Restore from this shadow before any clear-state `VMLAUNCH`.
const VMCS_SHADOW_MAX: usize = 128;
static mut VMCS_SHADOW_VAL: [[u64; VMCS_SHADOW_MAX]; 2] = [[0; VMCS_SHADOW_MAX]; 2];
static mut VMCS_SHADOW_PRESENT: [[bool; VMCS_SHADOW_MAX]; 2] = [[false; VMCS_SHADOW_MAX]; 2];
static mut VMCS_SHADOW_N: [u32; 2] = [0, 0];
/// Latch ERROR/WARN once so COM2 is not flooded every scheduler tick.
static mut SCHED_VMPTRLD_FAIL_LOGGED: bool = false;
/// First E4 G0/SPA clear-state re-entry logs; later quanta stay quiet.
static mut E4_G0_REENTRY_LOGGED: bool = false;
static mut E4_SPA_REENTRY_LOGGED: bool = false;
static mut E4_RESTORE_LOGGED: [bool; 2] = [false; 2];
static mut E4_SWITCH_QUIET_HINT: bool = false;

/// M4.3: virtio-blk probe guest frames (launched after NVM-OK).
static mut BLK_PROBE_FRAMES: Option<LaunchFrames> = None;
static mut HAS_BLK_PROBE: bool = false;
static mut BLK_PROBE_MODE: bool = false;
/// Guest id for the blk probe VMCS (not part of the G0–G3 scheduler set).
const M4_BLK_PROBE_GUEST_ID: u64 = 5;

/// M4.4: virtio-net probe guest frames (launched after BLK-OK).
static mut NET_PROBE_FRAMES: Option<LaunchFrames> = None;
static mut HAS_NET_PROBE: bool = false;
static mut NET_PROBE_MODE: bool = false;
const M4_NET_PROBE_GUEST_ID: u64 = 6;

/// M4.5: dual-vCPU SMP probe (BSP + AP), same guest id, shared EPT.
static mut SMP_BSP_FRAMES: Option<LaunchFrames> = None;
static mut SMP_AP_FRAMES: Option<LaunchFrames> = None;
static mut HAS_SMP_PROBE: bool = false;
static mut SMP_PROBE_MODE: bool = false;
static mut SMP_AP_LAUNCHED: bool = false;
const M4_SMP_GUEST_ID: u64 = 7;

/// Per-guest GPR banks (SAVED_* is the live working set for the active guest).
#[derive(Clone, Copy)]
struct GuestGprBank {
    rax: u64,
    rbx: u64,
    rcx: u64,
    rdx: u64,
    rsi: u64,
    rdi: u64,
    rbp: u64,
    r8: u64,
    r9: u64,
    r10: u64,
    r11: u64,
    r12: u64,
    r13: u64,
    r14: u64,
    r15: u64,
}

impl GuestGprBank {
    const ZERO: Self = Self {
        rax: 0,
        rbx: 0,
        rcx: 0,
        rdx: 0,
        rsi: 0,
        rdi: 0,
        rbp: 0,
        r8: 0,
        r9: 0,
        r10: 0,
        r11: 0,
        r12: 0,
        r13: 0,
        r14: 0,
        r15: 0,
    };
}

static mut GUEST_GPRS: [GuestGprBank; M4_NVM_GUEST_SLOTS] =
    [GuestGprBank::ZERO; M4_NVM_GUEST_SLOTS];

/// Host one-shot ticks for an M4.1/M4.2 time-slice (same scale as Linux quiet ticks).
const SCHED_SLICE_COUNT: u32 = 0x0010_0000;

fn guest_id_for_slot(slot: usize) -> u64 {
    if slot == 0 {
        M2_BRINGUP_GUEST_ID
    } else {
        M4_GUEST1_ID + (slot as u64 - 1)
    }
}

fn slot_for_guest_id(gid: u64) -> Option<usize> {
    if gid == M2_BRINGUP_GUEST_ID {
        Some(0)
    } else if gid >= M4_GUEST1_ID && gid < M4_GUEST1_ID + 3 {
        Some((gid - M4_GUEST1_ID + 1) as usize)
    } else {
        None
    }
}

/// Register a SHELL guest at `slot` (1..=3) for launch after G0 SHELL+APIC.
pub fn set_shell_guest(slot: usize, frames: LaunchFrames) {
    if slot == 0 || slot >= M4_NVM_GUEST_SLOTS {
        return;
    }
    // SAFETY: single-threaded boot before first VMLAUNCH.
    unsafe {
        GUEST_FRAMES[slot] = Some(frames);
        HAS_SECOND_GUEST = true;
        if slot > SHELL_SLOT_MAX {
            SHELL_SLOT_MAX = slot;
        }
    }
}

/// Register G1 (M4.0 compatibility wrapper).
pub fn set_second_guest(frames: LaunchFrames) {
    set_shell_guest(1, frames);
}

/// Register the M4.3 virtio-blk probe guest (launched after NVM-OK).
pub fn set_blk_probe(frames: LaunchFrames) {
    // SAFETY: single-threaded boot before first VMLAUNCH.
    unsafe {
        BLK_PROBE_FRAMES = Some(frames);
        HAS_BLK_PROBE = true;
    }
}

/// Register the M4.4 virtio-net probe guest (launched after BLK-OK).
pub fn set_net_probe(frames: LaunchFrames) {
    // SAFETY: single-threaded boot before first VMLAUNCH.
    unsafe {
        NET_PROBE_FRAMES = Some(frames);
        HAS_NET_PROBE = true;
    }
}

/// Register M4.5 BSP + AP probe frames (shared EPT; launched after NET-OK).
pub fn set_smp_probe(bsp: LaunchFrames, ap: LaunchFrames) {
    // SAFETY: single-threaded boot before first VMLAUNCH.
    unsafe {
        SMP_BSP_FRAMES = Some(bsp);
        SMP_AP_FRAMES = Some(ap);
        HAS_SMP_PROBE = true;
    }
}

/// Minimal IA-32e TSS size (SDM Vol. 3A §7.7) — enough for LTR / host TR.
const TSS_BYTES: usize = 0x68;

/// SDM: IA32_VMX_BASIC[30:0] is the VMCS revision. Bit 31 of the first dword
/// is the shadow-VMCS indicator and must be 0 for a non-shadow `VMPTRLD`.
/// App. C error 11 = VMPTRLD with incorrect VMCS revision identifier.
pub const fn vmcs_revision_from_basic(basic: u64) -> u32 {
    (basic as u32) & 0x7fff_ffff
}

/// Prepare VMCS revision ID in the first dword (same as VMXON region).
///
/// SAFETY: `region_phys` is a writable identity-mapped 4K frame.
pub unsafe fn prepare_vmcs_region(region_phys: u64) -> Result<(), LaunchError> {
    debug_assert_eq!(region_phys & 0xfff, 0);
    let basic = cpu::rdmsr(IA32_VMX_BASIC);
    let revision = vmcs_revision_from_basic(basic);
    let ptr = region_phys as *mut u8;
    core::ptr::write_bytes(ptr, 0, 4096);
    core::ptr::write_volatile(ptr.cast::<u32>(), revision);
    Ok(())
}

/// Rewrite only the VMCS revision identifier (first dword).
///
/// Does **not** zero the 4K region (unlike [`prepare_vmcs_region`]). Bit 31
/// (shadow-VMCS) is cleared. Nested VT-x / an implicit current-VMCS flush
/// can leave a revision this CPU will not `VMPTRLD` (iron error 11 on slot 1).
///
/// INVARIANTS:
/// - First dword == `IA32_VMX_BASIC[30:0]` with bit 31 clear
/// - Remaining 4K bytes unchanged
///
/// VERIFICATION: L1 (serial `rev=` on `VMPTRLD` fail)
/// SAFETY: `region_phys` is a writable identity-mapped 4K-aligned VMCS frame.
/// KANI-TARGET
pub unsafe fn rewrite_vmcs_revision(region_phys: u64) {
    debug_assert_eq!(region_phys & 0xfff, 0);
    let basic = cpu::rdmsr(IA32_VMX_BASIC);
    core::ptr::write_volatile(region_phys as *mut u32, vmcs_revision_from_basic(basic));
}

unsafe fn read_vmcs_revision(region_phys: u64) -> u32 {
    core::ptr::read_volatile(region_phys as *const u32)
}

fn ar_busy_tr(mut ar: u32) -> u32 {
    // Available 32/64-bit TSS (9) must be busy (B) for VMCS host/guest TR.
    if (ar & 0xF) == 0x9 {
        ar = (ar & !0xF) | 0xB;
    }
    ar
}

/// Remember host GDT/TSS from the first [`install_host_tss`] — VM-exit forces
/// `GDTR.limit = 0xFFFF` (SDM), so a second copy-from-sgdt would look 64 KiB wide.
///
/// Names must not collide with VMCS field encodings in `vmx::fields` (e.g.
/// `HOST_TR_BASE`), or `vw(HOST_TR_BASE, …)` silently uses the phys addr as the
/// field encoding (insn error 12).
static mut INSTALLED_HOST_TSS: bool = false;
static mut INSTALLED_GDT_BASE: u64 = 0;
static mut INSTALLED_GDT_LIMIT: u16 = 0;
static mut INSTALLED_TR_SEL: u16 = 0;
static mut INSTALLED_TR_BASE: u64 = 0;

/// Build a host TSS + GDT and load them (LGDT/LTR).
///
/// UEFI/OVMF commonly leaves TR=0. Host-state checks then fail VMLAUNCH with
/// insn error 8 (invalid host-state). We always install our own TSS once;
/// later guests reuse it (M4.0 G1 `setup_vmcs`).
///
/// Returns `(new_gdtr_base, new_gdtr_limit, tr_selector, tr_base)`.
///
/// SAFETY: `gdt_phys`/`tss_phys` are owned zeroable frames; interrupts off.
unsafe fn install_host_tss(
    gdt_phys: u64,
    tss_phys: u64,
) -> Result<(u64, u16, u16, u64), LaunchError> {
    if INSTALLED_HOST_TSS {
        // Restore architecturally correct limit (VM-exit set limit=FFFF).
        let gdtr = cpu::DescriptorTablePtr {
            limit: INSTALLED_GDT_LIMIT,
            base: INSTALLED_GDT_BASE,
        };
        cpu::lgdt(&gdtr);
        // TR still points at our TSS; LTR again only if selector cleared.
        if cpu::read_tr() & 0xfffc == 0 {
            cpu::load_tr(INSTALLED_TR_SEL);
        }
        serial::write_line("boot: host TSS reused (post-VMX GDTR.limit fixup)");
        return Ok((
            INSTALLED_GDT_BASE,
            INSTALLED_GDT_LIMIT,
            INSTALLED_TR_SEL,
            INSTALLED_TR_BASE,
        ));
    }

    let old = cpu::sgdt();
    let old_limit = old.limit;
    let old_base = old.base;
    let old_size = (old_limit as usize) + 1;
    // Need room for a 16-byte system descriptor after the existing table.
    // Cap: VMX may have already forced limit=FFFF before first install (unusual).
    if old_size < 8 {
        return Err(LaunchError::PrepareFailed);
    }
    let copy_size = core::cmp::min(old_size, 4096 - 16);
    if copy_size < 8 {
        return Err(LaunchError::PrepareFailed);
    }

    core::ptr::write_bytes(gdt_phys as *mut u8, 0, 4096);
    core::ptr::write_bytes(tss_phys as *mut u8, 0, 4096);
    core::ptr::copy_nonoverlapping(old_base as *const u8, gdt_phys as *mut u8, copy_size);

    // Append available 64-bit TSS descriptor at the next 8-byte aligned slot.
    let tss_index = (copy_size + 7) / 8; // qword index; may skip a pad entry
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

    INSTALLED_HOST_TSS = true;
    INSTALLED_GDT_BASE = gdt_phys;
    INSTALLED_GDT_LIMIT = new_limit;
    INSTALLED_TR_SEL = tr_sel;
    INSTALLED_TR_BASE = tss_phys;

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
    // M4.1/M4.2: keep G0 frames so the scheduler can switch back after shell guests.
    GUEST_FRAMES[0] = Some(*frames);
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
    let (gdt_base, gdt_limit, tr, tr_base) = install_host_tss(frames.gdt_phys, frames.tss_phys)?;

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

    // Ext-IRQ (M2.5) + I/O (M3.0). CPUID always exits (no primary control).
    // Prefer I/O bitmaps (COM1 only): unconditional I/O makes every Linux
    // `io_delay` (port 0x80) a VMEXIT and stalls mid mem-init.
    // Do not set bit 21 (Use TPR shadow) — it needs a Virtual-APIC page.
    let pin = adjust_vmx_controls(PIN_BASED_EXTERNAL_INTERRUPT_EXITING, pin_msr);
    let primary = adjust_vmx_controls(
        CPU_BASED_HLT_EXITING
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
    if primary & CPU_BASED_USE_TPR_SHADOW != 0 {
        serial::write_line("boot: WARN — Use TPR shadow forced on by TRUE ctls");
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
    // INVPCID: without secondary bit 12, guest `invpcid` #UD (Linux PCID; iron panic).
    // XSAVES: without secondary bit 20, guest `xsaves`/`xrstors` #UD (compacted XSAVE
    // on Xeon; iron: TASK stack guard recursion when irq/FPU path hit userspace).
    let secondary = adjust_vmx_controls(
        SECONDARY_ENABLE_EPT
            | SECONDARY_ENABLE_RDTSCP
            | SECONDARY_ENABLE_INVPCID
            | SECONDARY_ENABLE_XSAVES,
        IA32_VMX_PROCBASED_CTLS2,
    );
    if secondary & SECONDARY_ENABLE_EPT == 0 {
        serial::write_line("boot: ERROR — enable-EPT not allowed by PROCBASED_CTLS2");
        return Err(LaunchError::EptUnsupported);
    }
    if secondary & SECONDARY_ENABLE_RDTSCP == 0 {
        serial::write_line("boot: WARN — enable-RDTSCP not allowed; Linux may #UD on rdtscp");
    }
    if secondary & SECONDARY_ENABLE_INVPCID == 0 {
        serial::write_line("boot: WARN — enable-INVPCID not allowed; Linux may #UD on invpcid");
    }
    if secondary & SECONDARY_ENABLE_XSAVES == 0 {
        serial::write_line("boot: WARN — enable-XSAVES not allowed; Linux may #UD on xsaves");
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
    // Guest may use a precise-window CR3 (iron); host must keep the real UEFI
    // CR3. Using the guest CR3 for HOST_CR3 caused silent death on VMEXIT when
    // the EFI image / IDT lived outside [0,512MiB).
    let host_cr3 = cpu::read_cr3();
    let guest_cr3 = frames.guest_cr3_phys.unwrap_or(host_cr3);
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
            report_vmwrite_fail(
                "VMPTRLD+VMWRITE(link)",
                VMCS_LINK_POINTER,
                kind,
                frames.vmcs_phys,
            );
            return Err(LaunchError::VmwriteFailed {
                field: VMCS_LINK_POINTER,
            });
        }
    }

    match ops::vmwrite_detailed(PIN_BASED_VM_EXEC_CONTROL, pin as u64) {
        Ok(()) => {}
        Err(kind) => {
            report_vmwrite_fail(
                "VMWRITE(pin)",
                PIN_BASED_VM_EXEC_CONTROL,
                kind,
                frames.vmcs_phys,
            );
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
    vw(GUEST_CR3, guest_cr3)?;
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
    vw(HOST_CR3, host_cr3)?;
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
///
/// Must use RIP-relative stores: `mov [{sym}], reg` lowers to 32-bit absolute
/// (`[disp32]` / SIB `0x25`) which zero-extends to `0x00000000_4xxxxxxx` while
/// the UEFI image lives at `0x140000000`. On iron that made I/O/CPUID handlers
/// read empty statics (leaf=0 / AL=0) while M2 HLT checks (memory verify) still
/// passed.
#[unsafe(naked)]
pub unsafe extern "C" fn vmexit_landing() -> ! {
    core::arch::naked_asm!(
        "mov [rip + {slot_rax}], rax",
        "mov [rip + {slot_rbx}], rbx",
        "mov [rip + {slot_rcx}], rcx",
        "mov [rip + {slot_rdx}], rdx",
        "mov [rip + {slot_rsi}], rsi",
        "mov [rip + {slot_rdi}], rdi",
        "mov [rip + {slot_rbp}], rbp",
        "mov [rip + {slot_r8}], r8",
        "mov [rip + {slot_r9}], r9",
        "mov [rip + {slot_r10}], r10",
        "mov [rip + {slot_r11}], r11",
        "mov [rip + {slot_r12}], r12",
        "mov [rip + {slot_r13}], r13",
        "mov [rip + {slot_r14}], r14",
        "mov [rip + {slot_r15}], r15",
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
        "mov rax, [rip + {slot_rax}]",
        "mov rbx, [rip + {slot_rbx}]",
        "mov rcx, [rip + {slot_rcx}]",
        "mov rdx, [rip + {slot_rdx}]",
        "mov rsi, [rip + {slot_rsi}]",
        "mov rdi, [rip + {slot_rdi}]",
        "mov rbp, [rip + {slot_rbp}]",
        "mov r8, [rip + {slot_r8}]",
        "mov r9, [rip + {slot_r9}]",
        "mov r10, [rip + {slot_r10}]",
        "mov r11, [rip + {slot_r11}]",
        "mov r12, [rip + {slot_r12}]",
        "mov r13, [rip + {slot_r13}]",
        "mov r14, [rip + {slot_r14}]",
        "mov r15, [rip + {slot_r15}]",
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

/// Restore saved GPRs and VMLAUNCH (G0 after E4 VMCS relocate / VMCLEAR).
#[unsafe(naked)]
unsafe extern "C" fn vmlaunch_with_gprs() -> ! {
    core::arch::naked_asm!(
        "mov rax, [rip + {slot_rax}]",
        "mov rbx, [rip + {slot_rbx}]",
        "mov rcx, [rip + {slot_rcx}]",
        "mov rdx, [rip + {slot_rdx}]",
        "mov rsi, [rip + {slot_rsi}]",
        "mov rdi, [rip + {slot_rdi}]",
        "mov rbp, [rip + {slot_rbp}]",
        "mov r8, [rip + {slot_r8}]",
        "mov r9, [rip + {slot_r9}]",
        "mov r10, [rip + {slot_r10}]",
        "mov r11, [rip + {slot_r11}]",
        "mov r12, [rip + {slot_r12}]",
        "mov r13, [rip + {slot_r13}]",
        "mov r14, [rip + {slot_r14}]",
        "mov r15, [rip + {slot_r15}]",
        "vmlaunch",
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
        fail = sym vmlaunch_gprs_failed,
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

    // M4.5: dual-vCPU SMP probe runs after NET-OK.
    if SMP_PROBE_MODE {
        handle_smp_probe_vmexit(basic, qual, guest_rax, guest_rip);
    }

    // M4.4: virtio-net probe runs after BLK-OK.
    if NET_PROBE_MODE {
        handle_net_probe_vmexit(basic, qual, guest_rax, guest_rip);
    }

    // M4.3: virtio-blk probe runs after NVM-OK (scheduler paused).
    if BLK_PROBE_MODE {
        handle_blk_probe_vmexit(basic, qual, guest_rax, guest_rip);
    }

    // M4.1: credit scheduler owns both guests after 2VM-OK.
    if SCHED_MODE {
        handle_sched_vmexit(basic, qual, guest_rax, guest_rip);
    }

    // M4.0–M4.2: shell guests use a dedicated exit path until SCHED_MODE.
    if ACTIVE_GUEST_ID != M2_BRINGUP_GUEST_ID {
        handle_shell_vmexit(basic, qual, guest_rax, guest_rip);
    }

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
    if REAL_LINUX_GUEST && (basic == EXIT_REASON_MSR_READ || basic == EXIT_REASON_MSR_WRITE) {
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

/// Emulate APIC MMIO (GPA 0xFEE00000) or virtio-blk BAR (M4.3).
unsafe fn handle_ept_violation_and_resume(qual: u64, guest_rip: u64) -> ! {
    let gpa = ops::vmread(GUEST_PHYSICAL_ADDRESS).unwrap_or(0);
    let is_apic = (lapic_virt::APIC_GPA..lapic_virt::APIC_GPA + 0x1000).contains(&gpa);
    let is_virtio = virtio_blk::bar_contains(gpa) || virtio_net::bar_contains(gpa);
    if !is_apic && !is_virtio {
        serial::write_str("boot: ERROR — EPT violation GPA=0x");
        write_hex_u64(gpa);
        serial::write_str(" guest_id=");
        write_hex_u64(ACTIVE_GUEST_ID);
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
        serial::write_line("boot: ERROR — MMIO insn fetch (guest PT walk)");
        serial::write_str("boot: guest cr3=0x");
        write_hex_u64(guest_cr3);
        serial::write_str(" rip=0x");
        write_hex_u64(guest_rip);
        serial::write_byte(b'\n');
        dump_linux_guest_state();
        finish_boot(false);
    }
    let Some(mov) = mmio_decode::decode_mov_mmio(&insn) else {
        serial::write_line("boot: ERROR — MMIO undecoded insn");
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
        serial::write_line("boot: WARN — MMIO mov direction ≠ EPT qual");
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
    if is_virtio {
        if mmio_decode::apply_virtio_mov(mov, gpa, &mut gprs).is_err() {
            serial::write_line("boot: ERROR — virtio-blk MMIO apply failed");
            finish_boot(false);
        }
    } else if mmio_decode::apply_apic_mov(mov, gpa, &mut gprs).is_err() {
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
    if is_virtio {
        if virtio_blk::take_blk_ok_latch() {
            serial::write_line(M4_BLK_OK_MARKER);
        }
        if virtio_blk::take_booted_from_disk_latch() {
            serial::write_line(M7_ISO_BOOTED_FROM_DISK_MARKER);
            let _ = iso_install::note_booted_from_disk_lab();
        }
        if virtio_blk::take_install_disk_written_latch() {
            serial::write_line(M7_ISO_DISK_WRITTEN_MARKER);
            // Lab close: sized install disk + probe OK + LBA1 write (not reboot-to-disk).
            serial::write_line(M7_ISO_INSTALL_LAB_OK_MARKER);
            let _ = iso_install::note_install_disk_written();
            if iso_install::note_reboot_pending_lab() {
                serial::write_line(M7_ISO_REBOOT_PENDING_MARKER);
                let _ = iso_install::take_reboot_pending_latch();
            }
        }
        if virtio_net::take_net_ok_latch() {
            serial::write_line(M4_NET_OK_MARKER);
        }
    } else {
        emit_lapic_markers();
        if lapic_virt::host_timer_armed_for_guest() {
            let _ = apic::arm_oneshot_timer(M2_IRQ_VECTOR as u8, LINUX_TICK_COUNT);
        }
    }
    let _ = ops::vmwrite(GUEST_RIP, guest_rip.wrapping_add(mov.len as u64));
    if !is_virtio {
        if try_inject_guest_apic_timer() {
            vmresume_with_gprs();
        }
        maybe_arm_interrupt_window_for_apic();
    }
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
/// M4.0–M4.2: when shell guests are prepared, launch them (G0 RIP already advanced).
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
        if HAS_SECOND_GUEST && !SECOND_GUEST_STARTED {
            save_live_gprs_to_slot(0);
            launch_shell_guest(1);
        }
        if !HAS_SECOND_GUEST {
            finish_boot(true);
        }
        // Multi-guest path finishes from the scheduler after NVM-OK.
    }
}

fn save_live_gprs_to_slot(slot: usize) {
    if slot >= M4_NVM_GUEST_SLOTS {
        return;
    }
    // SAFETY: single-threaded VMX root.
    unsafe {
        GUEST_GPRS[slot] = GuestGprBank {
            rax: SAVED_GUEST_RAX,
            rbx: SAVED_GUEST_RBX,
            rcx: SAVED_GUEST_RCX,
            rdx: SAVED_GUEST_RDX,
            rsi: SAVED_GUEST_RSI,
            rdi: SAVED_GUEST_RDI,
            rbp: SAVED_GUEST_RBP,
            r8: SAVED_GUEST_R8,
            r9: SAVED_GUEST_R9,
            r10: SAVED_GUEST_R10,
            r11: SAVED_GUEST_R11,
            r12: SAVED_GUEST_R12,
            r13: SAVED_GUEST_R13,
            r14: SAVED_GUEST_R14,
            r15: SAVED_GUEST_R15,
        };
    }
}

fn load_live_gprs_from_slot(slot: usize) {
    if slot >= M4_NVM_GUEST_SLOTS {
        return;
    }
    // SAFETY: single-threaded VMX root.
    unsafe {
        let b = GUEST_GPRS[slot];
        SAVED_GUEST_RAX = b.rax;
        SAVED_GUEST_RBX = b.rbx;
        SAVED_GUEST_RCX = b.rcx;
        SAVED_GUEST_RDX = b.rdx;
        SAVED_GUEST_RSI = b.rsi;
        SAVED_GUEST_RDI = b.rdi;
        SAVED_GUEST_RBP = b.rbp;
        SAVED_GUEST_R8 = b.r8;
        SAVED_GUEST_R9 = b.r9;
        SAVED_GUEST_R10 = b.r10;
        SAVED_GUEST_R11 = b.r11;
        SAVED_GUEST_R12 = b.r12;
        SAVED_GUEST_R13 = b.r13;
        SAVED_GUEST_R14 = b.r14;
        SAVED_GUEST_R15 = b.r15;
    }
}

fn frames_for_slot(slot: usize) -> Option<LaunchFrames> {
    if slot >= M4_NVM_GUEST_SLOTS {
        return None;
    }
    // SAFETY: boot single-threaded; frames set before VMLAUNCH.
    unsafe { core::ptr::addr_of!(GUEST_FRAMES[slot]).read() }
}

fn arm_sched_slice() {
    // SAFETY: VMX root; PIC already masked.
    unsafe {
        let _ = apic::arm_oneshot_timer(M2_IRQ_VECTOR as u8, SCHED_SLICE_COUNT);
    }
}

fn note_sched_slice(slot: usize) {
    // SAFETY: single-threaded.
    unsafe {
        if slot >= M4_NVM_GUEST_SLOTS {
            return;
        }
        if !SCHED_SLICE[slot] {
            SCHED_SLICE[slot] = true;
            match slot {
                0 => serial::write_line(M4_SLICE_G0_MARKER),
                1 => serial::write_line(M4_SLICE_G1_MARKER),
                2 => serial::write_line(M4_SLICE_G2_MARKER),
                3 => serial::write_line(M4_SLICE_G3_MARKER),
                _ => {}
            }
        }
        if SCHED_SLICE[0] && SCHED_SLICE[1] && !SCHED_OK_LATCHED {
            SCHED_OK_LATCHED = true;
            serial::write_line(M4_SCHED_OK_MARKER);
            serial::write_line("boot: M4.1 complete — credit scheduler time-sliced G0 + G1");
        }
        let need = SHELL_SLOT_MAX + 1;
        if need >= 4 {
            let mut ok = true;
            for i in 0..need {
                if !SCHED_SLICE[i] {
                    ok = false;
                    break;
                }
            }
            if ok {
                if !NVM_OK_LATCHED {
                    NVM_OK_LATCHED = true;
                    serial::write_line(M4_NVM_OK_MARKER);
                    serial::write_line(
                        "boot: M4.2 complete — ≥4 concurrent guests under credit scheduler",
                    );
                }
                if M4_LADDER_DONE {
                    return;
                }
                if HAS_BLK_PROBE {
                    try_launch_blk_probe();
                }
                finish_boot(true);
            }
        } else if SCHED_OK_LATCHED {
            // Fewer than 4 prepared — M4.1-only finish after SCHED-OK.
            finish_boot(true);
        }
    }
}

/// Latch the first scheduler `VMPTRLD` failure (COM2 must not flood).
unsafe fn log_sched_vmptrld_fail_once(slot: usize, phys: u64) {
    if SCHED_VMPTRLD_FAIL_LOGGED {
        return;
    }
    SCHED_VMPTRLD_FAIL_LOGGED = true;
    serial::write_str("boot: ERROR — sched VMPTRLD failed slot=");
    write_hex_u32(slot as u32);
    serial::write_str(" phys=0x");
    write_hex_u64(phys);
    serial::write_str(" rev=0x");
    write_hex_u32(read_vmcs_revision(phys));
    serial::write_byte(b'\n');
    if let Ok(ierr) = ops::vmread(VM_INSTRUCTION_ERROR) {
        serial::write_str("boot: VM_INSTRUCTION_ERROR=");
        write_dec_u32(ierr as u32);
        serial::write_byte(b'\n');
    }
    serial::write_str("boot: WARN — park VMPTRLD fail; resume slot=");
    write_hex_u32(SCHED_SLOT_CUR as u32);
    serial::write_line(" (VMX on; no VMXOFF)");
    if slot == 0 {
        serial::write_line("boot: HINT — G0 parked after VMPTRLD fail (no slot=0 retry)");
    } else if slot == 1 {
        serial::write_line("boot: HINT — SPA parked after VMPTRLD fail (no slot=1 retry)");
    }
}

/// VMPTRLD `slot` and VMRESUME (caller already saved live GPRs of the old guest).
unsafe fn switch_to_sched_slot(slot: usize) -> ! {
    let frames = match frames_for_slot(slot) {
        Some(f) => f,
        None => {
            serial::write_line("boot: ERROR — sched slot frames missing");
            failsoft_sched_or_finish();
        }
    };
    // Iron 2026-08-21 (`63cd694f`): leaving slot 1 current and `VMPTRLD` G0
    // flushed the SPA VMCS in an implementation-specific form; next
    // `VMPTRLD(0x10409000)` failed with error 11 (bad revision). VMCLEAR the
    // outgoing VMCS and rewrite only *that* region's first dword.
    // Iron `eb456eec`: rewriting the *incoming* VMCS (no VMCLEAR of it this
    // switch) left VMPTRLD OK but VMLAUNCH error 7 (invalid control field).
    // Do not rewrite the incoming VMCS.
    if M4_LADDER_DONE && SPA_LAUNCHED && SCHED_SLOT_CUR != slot {
        if let Some(cur_f) = frames_for_slot(SCHED_SLOT_CUR) {
            // Snapshot architectural fields while the outgoing VMCS is current.
            // Do not VMLAUNCH a VMCLEAR'd VMCS without restoring fields.
            capture_current_vmcs_shadow(SCHED_SLOT_CUR);
            let _ = ops::vmclear(cur_f.vmcs_phys);
            rewrite_vmcs_revision(cur_f.vmcs_phys);
            if SCHED_SLOT_CUR == 0 {
                G0_NEEDS_VMLAUNCH = true;
            } else if SCHED_SLOT_CUR == 1 {
                SPA_NEEDS_VMLAUNCH = true;
            }
        }
    }
    if ops::vmptrld(frames.vmcs_phys).is_err() {
        if slot == 0 {
            G0_VMPTRLD_FAILED = true;
        } else if slot == 1 {
            SPA_VMPTRLD_FAILED = true;
            SPA_RUNNABLE = false;
        }
        log_sched_vmptrld_fail_once(slot, frames.vmcs_phys);
        // Iron 2026-08-21: G0 identity-pool VMCS was scribbled; memcpy of a
        // VMCLEAR'd region is not VMPTRLD-safe. VMfailValid leaves the
        // current-VMCS pointer unchanged — resume the live slot. Never VMXOFF.
        failsoft_sched_or_finish();
    }
    SCHED_SLOT_CUR = slot;
    ACTIVE_GUEST_ID = guest_id_for_slot(slot);
    REAL_LINUX_GUEST = slot == 0;
    // Keep Linux quiet-path helpers armed for G0 post-SHELL resumes.
    if slot == 0 {
        LINUX_GTIMER2_DONE = true;
    }
    BRINGUP_GUEST_CODE_PHYS = frames.guest_code_phys;
    load_live_gprs_from_slot(slot);
    let _ = ops::vmwrite(VM_ENTRY_INTERRUPTION_INFO, 0);
    let _ = ops::vmwrite(GUEST_INTERRUPTIBILITY_STATE, 0);
    let _ = ops::vmwrite(GUEST_ACTIVITY_STATE, 0);
    arm_sched_slice();
    if !M4_LADDER_DONE {
        serial::write_str("boot: sched switch → slot=");
        write_hex_u32(slot as u32);
        serial::write_byte(b'\n');
    }
    if slot == 0 && G0_NEEDS_VMLAUNCH {
        G0_NEEDS_VMLAUNCH = false;
        if !E4_G0_REENTRY_LOGGED {
            serial::write_line("boot: E4 G0 VMLAUNCH (VMCS relocated; was VMCLEAR)");
            E4_G0_REENTRY_LOGGED = true;
        }
        if !restore_vmcs_shadow(0) {
            serial::write_line("boot: WARN — E4 G0 shadow restore failed");
            failsoft_sched_or_finish();
        }
        e4_quiet_com2_after_first_reentry();
        let _ = ops::vmwrite(VM_ENTRY_INTERRUPTION_INFO, 0);
        vmlaunch_with_gprs();
    }
    if slot == 1 && SPA_NEEDS_VMLAUNCH {
        SPA_NEEDS_VMLAUNCH = false;
        if !E4_SPA_REENTRY_LOGGED {
            serial::write_line("boot: E4 SPA VMLAUNCH (VMCS was VMCLEAR; clear-state re-entry)");
            E4_SPA_REENTRY_LOGGED = true;
        }
        if !restore_vmcs_shadow(1) {
            serial::write_line("boot: WARN — E4 SPA shadow restore failed");
            failsoft_sched_or_finish();
        }
        e4_quiet_com2_after_first_reentry();
        let _ = ops::vmwrite(VM_ENTRY_INTERRUPTION_INFO, 0);
        vmlaunch_with_gprs();
    }
    vmresume_with_gprs();
}

/// After a failed VMPTRLD: keep VMX on and resume the live slot, or idle coexist.
unsafe fn failsoft_sched_or_finish() -> ! {
    if M4_LADDER_DONE {
        let resume = SCHED_SLOT_CUR;
        if let Some(cur_f) = frames_for_slot(resume) {
            let _ = ops::vmptrld(cur_f.vmcs_phys);
            if !SCHED_VMPTRLD_FAIL_LOGGED {
                serial::write_str("boot: WARN — park VMPTRLD fail; resume slot=");
                write_hex_u32(resume as u32);
                serial::write_line(" (VMX on; no VMXOFF)");
            }
            load_live_gprs_from_slot(resume);
            ACTIVE_GUEST_ID = guest_id_for_slot(resume);
            REAL_LINUX_GUEST = resume == 0;
            BRINGUP_GUEST_CODE_PHYS = cur_f.guest_code_phys;
            let _ = ops::vmwrite(VM_ENTRY_INTERRUPTION_INFO, 0);
            arm_sched_slice();
            if resume == 0 && G0_NEEDS_VMLAUNCH {
                G0_NEEDS_VMLAUNCH = false;
                let _ = restore_vmcs_shadow(0);
                vmlaunch_with_gprs();
            }
            if resume == 1 && SPA_NEEDS_VMLAUNCH {
                SPA_NEEDS_VMLAUNCH = false;
                let _ = restore_vmcs_shadow(1);
                vmlaunch_with_gprs();
            }
            vmresume_with_gprs();
        }
        serial::write_line("boot: WARN — VMPTRLD fail-soft idle (VMX on; coexist)");
        coexist_failsoft_idle();
    }
    finish_boot(false);
}

/// After the last shell guest SHELL: register all vCPUs and time-slice.
unsafe fn enter_sched_mode_from_shell(from_slot: usize) -> ! {
    if SCHED_MODE {
        arm_sched_slice();
        vmresume_with_gprs();
    }
    SCHED_MODE = true;
    SCHED = CreditScheduler::new();
    let n = SHELL_SLOT_MAX + 1;
    for _ in 0..n {
        let _ = SCHED.register_vcpu(DEFAULT_CREDIT);
    }
    SCHED_SLOT_CUR = from_slot;
    save_live_gprs_to_slot(from_slot);
    SCHED.consume_quantum(from_slot);
    serial::write_str("boot: M4.2 — credit scheduler armed slots=0..");
    write_hex_u32(SHELL_SLOT_MAX as u32);
    serial::write_byte(b'\n');
    let next = match core::ptr::addr_of!(SCHED)
        .as_ref()
        .unwrap()
        .pick_next_fair(Some(from_slot))
    {
        Ok(n) => n,
        Err(_) => {
            serial::write_line("boot: ERROR — sched no runnable after shell cascade");
            finish_boot(false);
        }
    };
    switch_to_sched_slot(next);
}

/// After a shell guest SHELL CPUID: launch the next shell or enter the scheduler.
unsafe fn after_shell_cpuid(slot: usize) -> ! {
    save_live_gprs_to_slot(slot);
    let next = slot + 1;
    if next <= SHELL_SLOT_MAX && frames_for_slot(next).is_some() {
        launch_shell_guest(next);
    }
    enter_sched_mode_from_shell(slot);
}

/// Consume SPA start/stop flags. VMLAUNCH of G1 may not return.
///
/// INVARIANTS:
/// - Caller is [`schedule_preempt`] (VMX root), never the HTTP tick
/// - Flags stay queued until [`M4_LADDER_DONE`] (coexist path)
/// - Live GPRs of `SCHED_SLOT_CUR` are saved before VMLAUNCH
/// - Fail-soft resumes slot 0; never `finish_boot(false)`
///
/// VERIFICATION: L1 (serial + park). FALLBACK: runtime WARN.
unsafe fn try_spa_vmlaunch() {
    if spa_launch::take_spa_stop() {
        SPA_RUNNABLE = false;
        serial::write_line("boot: E4 SPA stop — park slot=1 (G0 stays scheduled)");
    }
    // PRE-EBS / M4 ladder may queue start; do not consume until coexist.
    if !M4_LADDER_DONE {
        return;
    }
    let Some(_gid) = spa_launch::take_spa_start() else {
        return;
    };
    if SPA_LAUNCHED {
        if SPA_VMPTRLD_FAILED {
            serial::write_line("boot: E4 SPA start — slot=1 parked (VMPTRLD fail; stay on G0)");
            return;
        }
        SPA_RUNNABLE = true;
        serial::write_line("boot: E4 SPA start — resume parked slot=1");
        return;
    }
    save_live_gprs_to_slot(SCHED_SLOT_CUR);
    launch_spa_private_ept();
}

/// Snapshot [`VMCS_CLONE_FIELDS`] from the current VMCS into a slot shadow.
///
/// INVARIANTS:
/// - Caller is VMX root with a current VMCS for `slot` (0 or 1)
/// - Called **before** `VMCLEAR` of that region
///
/// VERIFICATION: L1. FALLBACK: `n_ok == 0` makes restore refuse VMLAUNCH.
unsafe fn capture_current_vmcs_shadow(slot: usize) {
    if slot > 1 {
        return;
    }
    debug_assert!(VMCS_CLONE_FIELDS.len() <= VMCS_SHADOW_MAX);
    let n = VMCS_CLONE_FIELDS.len();
    let mut n_ok = 0u32;
    for i in 0..n {
        VMCS_SHADOW_PRESENT[slot][i] = false;
        if let Ok(v) = ops::vmread(VMCS_CLONE_FIELDS[i]) {
            VMCS_SHADOW_VAL[slot][i] = v;
            VMCS_SHADOW_PRESENT[slot][i] = true;
            n_ok += 1;
        }
    }
    VMCS_SHADOW_N[slot] = n_ok;
}

/// VMWRITE a slot's shadow into the **current** VMCS (already `VMPTRLD`).
///
/// INVARIANTS:
/// - Current VMCS is the region for `slot`
/// - At least 40 architectural fields were captured
///
/// Returns false if the shadow is too thin to launch (would be error 7 zeros).
unsafe fn restore_vmcs_shadow(slot: usize) -> bool {
    if slot > 1 {
        return false;
    }
    let n = VMCS_CLONE_FIELDS.len();
    let mut n_ok = 0u32;
    for i in 0..n {
        if VMCS_SHADOW_PRESENT[slot][i] {
            let _ = ops::vmwrite(VMCS_CLONE_FIELDS[i], VMCS_SHADOW_VAL[slot][i]);
            n_ok += 1;
        }
    }
    if !E4_RESTORE_LOGGED[slot] {
        serial::write_str("boot: E4 restore VMCS shadow slot=");
        write_hex_u32(slot as u32);
        serial::write_str(" fields=");
        write_dec_u32(n_ok);
        serial::write_byte(b'\n');
        E4_RESTORE_LOGGED[slot] = true;
    }
    if n_ok < 40 {
        return false;
    }
    true
}

/// After the first G0↔SPA re-entry pair, stop logging every quantum.
/// ADR-011 default is quiet COM2; `paperverbose.txt` is the verbose path.
/// WARN / HTTP / markers still print.
unsafe fn e4_quiet_com2_after_first_reentry() {
    if E4_SWITCH_QUIET_HINT {
        return;
    }
    if E4_G0_REENTRY_LOGGED && E4_SPA_REENTRY_LOGGED {
        serial::write_line(
            "boot: HINT — COM2 quiet after first E4 re-entry (HTTP/WARN only; switch loop continues)",
        );
        E4_SWITCH_QUIET_HINT = true;
    }
}

/// Clone the current VMCS to `dst` via VMREAD/VMWRITE (not memcpy).
///
/// INVARIANTS:
/// - Caller is VMX root with a current VMCS (G0)
/// - `dst` is a 4 KiB-aligned host-owned frame that is not the VMXON region
/// - On success, `dst` is in the **clear** state (first entry is VMLAUNCH)
/// - On failure, the source VMCS is left loadable (VMPTRLD src restored)
///
/// VERIFICATION: L1 (serial + GUEST_RIP round-trip). FALLBACK: park G0.
unsafe fn clone_current_vmcs_to(src: u64, dst: u64) -> Option<u64> {
    debug_assert_eq!(dst & 0xfff, 0);
    const MAX: usize = 128;
    debug_assert!(VMCS_CLONE_FIELDS.len() <= MAX);
    let mut vals = [0u64; MAX];
    let mut present = [false; MAX];
    let n = VMCS_CLONE_FIELDS.len();
    let mut n_ok = 0u32;
    for i in 0..n {
        if let Ok(v) = ops::vmread(VMCS_CLONE_FIELDS[i]) {
            vals[i] = v;
            present[i] = true;
            n_ok += 1;
        }
    }
    // Keep slot 0 shadow in sync with the clone snapshot (G0 relocate).
    if n_ok >= 40 {
        for i in 0..n {
            VMCS_SHADOW_VAL[0][i] = vals[i];
            VMCS_SHADOW_PRESENT[0][i] = present[i];
        }
        VMCS_SHADOW_N[0] = n_ok;
    }
    let Ok(src_rip) = ops::vmread(GUEST_RIP) else {
        serial::write_line("boot: WARN — E4 G0 VMREAD GUEST_RIP failed");
        return None;
    };
    if n_ok < 40 {
        serial::write_str("boot: WARN — E4 G0 VMREAD too few fields n=");
        write_dec_u32(n_ok);
        serial::write_byte(b'\n');
        return None;
    }
    if prepare_vmcs_region(dst).is_err()
        || ops::vmclear(dst).is_err()
        || prepare_vmcs_region(dst).is_err()
    {
        serial::write_line("boot: WARN — E4 dest VMCS prepare/VMCLEAR failed");
        let _ = ops::vmptrld(src);
        return None;
    }
    if ops::vmptrld(dst).is_err() {
        let rev = core::ptr::read_volatile(dst as *const u32);
        serial::write_str("boot: WARN — E4 dest VMPTRLD fail rev=0x");
        write_hex_u32(rev);
        serial::write_byte(b'\n');
        let _ = ops::vmptrld(src);
        return None;
    }
    for i in 0..n {
        if present[i] {
            let _ = ops::vmwrite(VMCS_CLONE_FIELDS[i], vals[i]);
        }
    }
    core::arch::asm!("mfence", options(nostack));
    let wrote_rip = ops::vmread(GUEST_RIP).unwrap_or(0);
    if wrote_rip != src_rip {
        serial::write_str("boot: WARN — E4 clone RIP mismatch src=0x");
        write_hex_u64(src_rip);
        serial::write_str(" dst=0x");
        write_hex_u64(wrote_rip);
        serial::write_byte(b'\n');
        let _ = ops::vmptrld(src);
        return None;
    }
    if ops::vmclear(dst).is_err() {
        serial::write_line("boot: WARN — E4 dest VMCLEAR after clone failed");
        let _ = ops::vmptrld(src);
        return None;
    }
    if ops::vmptrld(dst).is_err() {
        serial::write_line("boot: WARN — E4 dest VMPTRLD verify failed");
        if let Ok(ierr) = ops::vmread(VM_INSTRUCTION_ERROR) {
            serial::write_str("boot: VM_INSTRUCTION_ERROR=");
            write_dec_u32(ierr as u32);
            serial::write_byte(b'\n');
        }
        let _ = ops::vmptrld(src);
        return None;
    }
    let verify_rip = ops::vmread(GUEST_RIP).unwrap_or(0);
    if verify_rip != src_rip {
        serial::write_str("boot: WARN — E4 verify RIP mismatch got=0x");
        write_hex_u64(verify_rip);
        serial::write_byte(b'\n');
        let _ = ops::vmptrld(src);
        return None;
    }
    if ops::vmclear(dst).is_err() {
        serial::write_line("boot: WARN — E4 dest final VMCLEAR failed");
        let _ = ops::vmptrld(src);
        return None;
    }
    rewrite_vmcs_revision(dst);
    serial::write_str("boot: E4 G0 VMCS clone fields=");
    write_dec_u32(n_ok);
    serial::write_str(" rip=0x");
    write_hex_u64(src_rip);
    serial::write_line(" (VMREAD/VMWRITE; VMPTRLD verify ok)");
    Some(src_rip)
}

/// Flush G0's cached VMCS out of Linux-writable identity RAM.
///
/// INVARIANTS:
/// - Caller is [`launch_spa_private_ept`] while G0 is still current (or active)
/// - Clone via [`clone_current_vmcs_to`] (SDM: VMCS format is implementation-specific)
/// - After `VMCLEAR` the G0 VMCS is **clear**; first re-entry is `VMLAUNCH`
/// - On clone failure, leave `G0_VMCS_RELOCATED` false so the scheduler parks slot 0
///
/// VERIFICATION: L1 (serial + VMPTRLD verify). FALLBACK: WARN and park G0.
unsafe fn relocate_g0_vmcs_to_host_slab() -> bool {
    if G0_VMCS_RELOCATED {
        return true;
    }
    let Some(g0) = frames_for_slot(0) else {
        serial::write_line("boot: WARN — E4 G0 VMCS relocate skipped (no G0 frames)");
        return false;
    };
    if g0.vmcs_phys & 0xfff != 0 {
        serial::write_line("boot: WARN — E4 G0 VMCS not 4K-aligned");
        return false;
    }
    let mut shells = [0u64; M4_NVM_GUEST_SLOTS - 1];
    let mut n = 0usize;
    for s in 1..M4_NVM_GUEST_SLOTS {
        if let Some(f) = frames_for_slot(s) {
            shells[n] = f.guest_code_phys;
            n += 1;
        }
    }
    let Some(slab) = ept_hw::host_only_slab_after_shells(&shells[..n]) else {
        serial::write_line("boot: WARN — E4 no host-only slab for G0 VMCS");
        return false;
    };
    let pml4 = ept_hw::pml4_from_eptp(g0.eptp);
    // SAFETY: `pml4` is G0's precise-identity EPT root; `slab` is 2 MiB-aligned
    // inside PRECISE_BYTES and not a G1–G3 shell (picker sits after those HPAs).
    // KANI-TARGET
    if ept_hw::clear_2m_identity_leaf(pml4, slab).is_err() {
        serial::write_line("boot: WARN — E4 could not punch G0-VMCS slab from G0 EPT");
        return false;
    }
    ept_hw::invept_global();
    // SAFETY: slab is the same class of UEFI-identity RAM as G1–G3 shells.
    // KANI-TARGET
    core::ptr::write_bytes(slab as *mut u8, 0, ept_hw::TWO_MIB as usize);
    match ops::vmptrst() {
        Ok(cur) if cur == g0.vmcs_phys => {}
        _ => {
            if ops::vmptrld(g0.vmcs_phys).is_err() {
                serial::write_line("boot: WARN — E4 cannot VMPTRLD G0 for clone");
                return false;
            }
        }
    }
    let dst = slab + ept_hw::G0_HOST_SLAB_OFF_VMCS;
    let Some(_rip) = clone_current_vmcs_to(g0.vmcs_phys, dst) else {
        serial::write_line("boot: WARN — E4 G0 VMCS clone failed; park slot=0");
        let _ = ops::vmptrld(g0.vmcs_phys);
        return false;
    };
    // SAFETY: clone verified VMPTRLD of dest; abandon the identity-pool page.
    // KANI-TARGET
    if ops::vmclear(g0.vmcs_phys).is_err() {
        serial::write_line("boot: WARN — E4 VMCLEAR G0 source failed; dest still verified");
    }
    rewrite_vmcs_revision(dst);
    let mut nf = g0;
    nf.vmcs_phys = dst;
    GUEST_FRAMES[0] = Some(nf);
    G0_VMCS_RELOCATED = true;
    G0_NEEDS_VMLAUNCH = true;
    serial::write_str("boot: E4 G0 VMCS relocated HPA=0x");
    write_hex_u64(dst);
    serial::write_line(" (host slab; VMREAD/VMWRITE clone; punched from G0 identity)");
    true
}

/// VMLAUNCH G1 from the 2 MiB shell slab (VMCS + 2M EPT not in G0 identity).
///
/// INVARIANTS:
/// - EPT is a single 2 MiB identity leaf (not G0's 512 MiB precise map)
/// - VMCS / EPT tables / host stack live in the slab punched out of G0 EPT
/// - Guest is SHELL CPUID, not a Linux distro installer
///
/// VERIFICATION: L1. Iron marker: [`M7_E4_SPA_LAUNCH_OK_MARKER`].
unsafe fn launch_spa_private_ept() -> ! {
    let _ = relocate_g0_vmcs_to_host_slab();
    let Some(old) = frames_for_slot(1) else {
        serial::write_line("boot: WARN — E4 SPA start skipped (no G1 slab)");
        switch_to_sched_slot(0);
    };
    let slab = old.guest_code_phys & !(ept_hw::TWO_MIB - 1);
    let mut ept_frames = [
        slab + ept_hw::G1_SLAB_OFF_EPT_PML4,
        slab + ept_hw::G1_SLAB_OFF_EPT_PDPT,
        slab + ept_hw::G1_SLAB_OFF_EPT_PD,
    ];
    let eptp = match ept_hw::build_single_2m_identity(slab, &mut ept_frames) {
        Ok(v) => v,
        Err(_) => {
            serial::write_line("boot: WARN — E4 SPA 2M EPT build failed");
            switch_to_sched_slot(0);
        }
    };
    let _ = ept_hw::write_guest_identity_2m_tables(slab);
    ept_hw::write_guest_shell_cpuid_page(slab + ept_hw::G1_SLAB_OFF_CODE);
    let frames = LaunchFrames {
        vmcs_phys: slab + ept_hw::G1_SLAB_OFF_VMCS,
        guest_stack_phys: slab + ept_hw::G1_SLAB_OFF_STACK,
        host_stack_phys: slab + ept_hw::G1_SLAB_OFF_HOST_STACK,
        tss_phys: slab + ept_hw::G1_SLAB_OFF_TSS,
        gdt_phys: slab + ept_hw::G1_SLAB_OFF_GDT,
        eptp,
        guest_code_phys: slab + ept_hw::G1_SLAB_OFF_CODE,
        guest_idt_phys: slab + ept_hw::G1_SLAB_OFF_IDT,
        guest_cr3_phys: Some(slab + ept_hw::G1_SLAB_OFF_PML4),
        msr_bitmap_phys: Some(slab + ept_hw::G1_SLAB_OFF_MSR_BITMAP),
        io_bitmap_a_phys: Some(slab + ept_hw::G1_SLAB_OFF_IO_A),
        io_bitmap_b_phys: Some(slab + ept_hw::G1_SLAB_OFF_IO_B),
    };
    GUEST_FRAMES[1] = Some(frames);
    // G0 precise identity still maps the G1 2 MiB slab. Punch it so Linux
    // cannot scribble the SPA VMCS while G0 runs between VMCLEAR and re-entry.
    if let Some(g0) = frames_for_slot(0) {
        let pml4 = ept_hw::pml4_from_eptp(g0.eptp);
        // SAFETY: `slab` is G1's 2 MiB shell; G0 e820 ends at 256 MiB.
        // KANI-TARGET
        if ept_hw::clear_2m_identity_leaf(pml4, slab).is_err() {
            serial::write_line("boot: WARN — E4 could not punch SPA slab from G0 EPT");
        } else {
            ept_hw::invept_global();
        }
    }
    if core::ptr::addr_of!(SCHED).as_ref().unwrap().vcpu_count() < 2 {
        let _ = SCHED.register_vcpu(DEFAULT_CREDIT);
    }
    SPA_LAUNCHED = true;
    SPA_RUNNABLE = true;
    crate::audit_log!(crate::audit::AuditEvent::VmcsCreated {
        vcpu_id: 1,
        vmcs_id: frames.vmcs_phys,
    });
    crate::audit_log!(crate::audit::AuditEvent::EptMapped {
        guest_id: guest_id_for_slot(1),
        gpa: slab,
        hpa: slab,
    });
    ACTIVE_GUEST_ID = guest_id_for_slot(1);
    REAL_LINUX_GUEST = false;
    LINUX_GTIMER2_DONE = false;
    LINUX_GTIMER2_ARMED = false;
    BRINGUP_GUEST_CODE_PHYS = frames.guest_code_phys;
    reset_saved_gprs(0);
    GUEST_GPRS[1] = GuestGprBank::ZERO;
    SCHED_SLOT_CUR = 1;

    serial::write_line(
        "boot: E4 SPA VMLAUNCH slot=1 private 2M EPT (VMCS in slab; not G0 identity)",
    );

    apic::mask_pic();
    let _ = apic::mask_timer();
    let _ = apic::eoi();
    ept_hw::invept_global();

    if prepare_vmcs_region(frames.vmcs_phys).is_err()
        || ops::vmclear(frames.vmcs_phys).is_err()
        || prepare_vmcs_region(frames.vmcs_phys).is_err()
    {
        serial::write_line("boot: WARN — E4 SPA VMCS prepare failed; resume G0");
        SPA_LAUNCHED = false;
        SPA_RUNNABLE = false;
        switch_to_sched_slot(0);
    }
    if let Err(e) = setup_vmcs(&frames) {
        serial::write_str("boot: WARN — E4 SPA setup_vmcs failed: ");
        serial::write_line(launch_err_name(e));
        SPA_LAUNCHED = false;
        SPA_RUNNABLE = false;
        switch_to_sched_slot(0);
    }
    capture_current_vmcs_shadow(1);

    let _ = apic::mask_timer();
    let _ = apic::eoi();
    let _ = ops::vmwrite(VM_ENTRY_INTERRUPTION_INFO, 0);
    arm_sched_slice();

    serial::write_line("boot: VMLAUNCH → E4 SPA SHELL CPUID");
    match ops::vmlaunch() {
        Ok(()) => {
            serial::write_line("boot: WARN — E4 SPA VMLAUNCH returned Ok; resume G0");
            SPA_LAUNCHED = false;
            SPA_RUNNABLE = false;
            switch_to_sched_slot(0);
        }
        Err(_) => {
            let ierr = ops::vmread(VM_INSTRUCTION_ERROR).unwrap_or(0xFFFF) as u32;
            serial::write_str("boot: WARN — E4 SPA VMLAUNCH failed insn_error=0x");
            write_hex_u32(ierr);
            serial::write_line("; resume G0");
            SPA_LAUNCHED = false;
            SPA_RUNNABLE = false;
            switch_to_sched_slot(0);
        }
    }
}

/// Host LAPIC tick while SCHED_MODE: consume quantum and switch.
unsafe fn schedule_preempt() -> ! {
    let _ = apic::eoi();
    // ADR-013 Phase F: one bounded NIC poll per scheduler quantum (VMX on).
    crate::mgmt::tick_native_coexist();
    try_spa_vmlaunch();
    let cur = SCHED_SLOT_CUR;
    // Guest `cur` just ran a full host slice — latch progress before switch.
    note_sched_slice(cur);
    save_live_gprs_to_slot(cur);
    SCHED.consume_quantum(cur);
    let mut next = match core::ptr::addr_of!(SCHED)
        .as_ref()
        .unwrap()
        .pick_next_fair(Some(cur))
    {
        Ok(n) => n,
        Err(_) => {
            serial::write_line("boot: ERROR — sched no runnable on preempt");
            finish_boot(false);
        }
    };
    // After Phase F, G2/G3 identity-pool stubs stay parked (G0 + SPA only).
    if M4_LADDER_DONE && next >= 2 {
        next = 0;
    }
    // After Phase F, G1's identity-pool VMCS is parked. Remap to G0 until
    // SPA start relocates slot 1. During the M4.2 ladder G1 must still run
    // (iron hung at SLICE-G0 when this ran before M4_LADDER_DONE).
    if M4_LADDER_DONE && next == 1 && (!SPA_RUNNABLE || SPA_VMPTRLD_FAILED) {
        next = 0;
    }
    // Iron 2026-08-21: do not VMPTRLD G0 until the VMREAD/VMWRITE clone
    // verified, and never retry slot 0 after the first VMPTRLD failure.
    if M4_LADDER_DONE
        && next == 0
        && SPA_LAUNCHED
        && (!G0_VMCS_RELOCATED || G0_VMPTRLD_FAILED)
        && SPA_RUNNABLE
        && !SPA_VMPTRLD_FAILED
    {
        next = 1;
    }
    if next == cur {
        arm_sched_slice();
        let _ = ops::vmwrite(VM_ENTRY_INTERRUPTION_INFO, 0);
        vmresume_with_gprs();
    }
    switch_to_sched_slot(next);
}

/// M4.1/M4.2 multi-guest exit path under the credit scheduler.
unsafe fn handle_sched_vmexit(basic: u32, qual: u64, guest_rax: u64, guest_rip: u64) -> ! {
    match basic {
        EXIT_REASON_EXTERNAL_INTERRUPT => schedule_preempt(),
        EXIT_REASON_IO_INSTRUCTION => handle_io_and_resume(qual, guest_rax, guest_rip),
        EXIT_REASON_EPT_VIOLATION => handle_ept_violation_and_resume(qual, guest_rip),
        EXIT_REASON_CPUID => handle_cpuid_and_resume(guest_rip),
        EXIT_REASON_MSR_READ | EXIT_REASON_MSR_WRITE => {
            if REAL_LINUX_GUEST {
                handle_msr_and_resume(basic);
            }
            serial::write_line("boot: ERROR — unexpected MSR exit on shell guest in sched");
            finish_boot(false);
        }
        EXIT_REASON_XSETBV => {
            if REAL_LINUX_GUEST {
                handle_xsetbv_and_resume(guest_rip);
            }
            serial::write_line("boot: ERROR — unexpected XSETBV on shell guest in sched");
            finish_boot(false);
        }
        EXIT_REASON_HLT => {
            let _ = ops::vmwrite(VM_ENTRY_INTERRUPTION_INFO, 0);
            vmresume_with_gprs();
        }
        EXIT_REASON_INTERRUPT_WINDOW => {
            let _ = set_interrupt_window_exiting(false);
            let _ = ops::vmwrite(VM_ENTRY_INTERRUPTION_INFO, 0);
            vmresume_with_gprs();
        }
        _ => {
            serial::write_str("boot: ERROR — sched unhandled exit reason=0x");
            write_hex_u32(basic);
            serial::write_str(" guest=");
            write_hex_u64(ACTIVE_GUEST_ID);
            serial::write_byte(b'\n');
            finish_boot(false);
        }
    }
}

/// Pre-sched shell-guest exit handler (G1–G3): drain IRQs until SHELL CPUID.
unsafe fn handle_shell_vmexit(basic: u32, qual: u64, guest_rax: u64, guest_rip: u64) -> ! {
    serial::write_str("boot: shell VMEXIT guest=");
    write_hex_u64(ACTIVE_GUEST_ID);
    serial::write_str(" reason=0x");
    write_hex_u32(basic);
    serial::write_str(" rip=0x");
    write_hex_u64(guest_rip);
    serial::write_byte(b'\n');

    match basic {
        EXIT_REASON_EXTERNAL_INTERRUPT => {
            let _ = apic::eoi();
            let _ = apic::mask_timer();
            let _ = ops::vmwrite(VM_ENTRY_INTERRUPTION_INFO, 0);
            let _ = ops::vmwrite(GUEST_INTERRUPTIBILITY_STATE, 0);
            let _ = ops::vmwrite(GUEST_ACTIVITY_STATE, 0);
            vmresume_with_gprs();
        }
        EXIT_REASON_IO_INSTRUCTION => handle_io_and_resume(qual, guest_rax, guest_rip),
        EXIT_REASON_EPT_VIOLATION => handle_ept_violation_and_resume(qual, guest_rip),
        EXIT_REASON_CPUID => handle_cpuid_and_resume(guest_rip),
        EXIT_REASON_HLT => {
            serial::write_line("boot: ERROR — shell HLT without SHELL CPUID");
            finish_boot(false);
        }
        _ => {
            serial::write_str("boot: ERROR — unexpected shell exit reason=0x");
            write_hex_u32(basic);
            serial::write_byte(b'\n');
            finish_boot(false);
        }
    }
}

/// VMLAUNCH (or first entry) for shell guest `slot` (1..=3). Prior guests stay launched.
unsafe fn launch_shell_guest(slot: usize) -> ! {
    let frames = match frames_for_slot(slot) {
        Some(f) => f,
        None => {
            serial::write_line("boot: ERROR — shell guest frames missing");
            finish_boot(false);
        }
    };
    if frames_for_slot(0).is_none() {
        serial::write_line("boot: ERROR — G0 frames missing before shell launch");
        finish_boot(false);
    }
    SECOND_GUEST_STARTED = true;
    ACTIVE_GUEST_ID = guest_id_for_slot(slot);
    REAL_LINUX_GUEST = false;
    LINUX_GTIMER2_DONE = false;
    LINUX_GTIMER2_ARMED = false;
    BRINGUP_GUEST_CODE_PHYS = frames.guest_code_phys;
    reset_saved_gprs(0);
    GUEST_GPRS[slot] = GuestGprBank::ZERO;

    serial::write_str("boot: M4.2 — launching shell guest slot=");
    write_hex_u32(slot as u32);
    serial::write_str(" id=0x");
    write_hex_u64(ACTIVE_GUEST_ID);
    serial::write_byte(b'\n');

    apic::mask_pic();
    let _ = apic::mask_timer();
    let _ = apic::eoi();
    ept_hw::invept_global();

    if prepare_vmcs_region(frames.vmcs_phys).is_err() {
        serial::write_line("boot: ERROR — shell VMCS prepare failed");
        finish_boot(false);
    }
    if ops::vmclear(frames.vmcs_phys).is_err() {
        serial::write_line("boot: ERROR — shell VMCLEAR failed");
        finish_boot(false);
    }
    if prepare_vmcs_region(frames.vmcs_phys).is_err() {
        serial::write_line("boot: ERROR — shell VMCS re-prepare failed");
        finish_boot(false);
    }
    if let Err(e) = setup_vmcs(&frames) {
        serial::write_str("boot: ERROR — shell setup_vmcs failed: ");
        serial::write_line(launch_err_name(e));
        finish_boot(false);
    }

    let _ = apic::mask_timer();
    let _ = apic::eoi();
    let _ = ops::vmwrite(VM_ENTRY_INTERRUPTION_INFO, 0);

    serial::write_line("boot: VMLAUNCH → shell SHELL CPUID");
    match ops::vmlaunch() {
        Ok(()) => {
            serial::write_line("boot: ERROR — shell VMLAUNCH returned Ok");
            finish_boot(false);
        }
        Err(_) => {
            let ierr = ops::vmread(VM_INSTRUCTION_ERROR).unwrap_or(0xFFFF) as u32;
            serial::write_str("boot: ERROR — shell VMLAUNCH failed insn_error=0x");
            write_hex_u32(ierr);
            serial::write_byte(b'\n');
            finish_boot(false);
        }
    }
}

fn launch_err_name(e: LaunchError) -> &'static str {
    match e {
        LaunchError::PrepareFailed => "PrepareFailed",
        LaunchError::ClearFailed => "ClearFailed",
        LaunchError::PtrldFailed => "PtrldFailed",
        LaunchError::EptUnsupported => "EptUnsupported",
        LaunchError::CpuidExitingUnsupported => "CpuidExitingUnsupported",
        LaunchError::VmwriteFailed { .. } => "VmwriteFailed",
        LaunchError::LaunchFailed { .. } => "LaunchFailed",
    }
}

/// M4.0 compatibility: launch G1.
unsafe fn try_launch_second_guest() -> ! {
    launch_shell_guest(1);
}

/// M4.3: after NVM-OK, VMLAUNCH the virtio-blk probe guest (G0 EPTP + host CR3).
unsafe fn try_launch_blk_probe() -> ! {
    let frames = match core::ptr::addr_of!(BLK_PROBE_FRAMES).read() {
        Some(f) => f,
        None => {
            serial::write_line("boot: ERROR — blk probe frames missing");
            finish_boot(false);
        }
    };
    SCHED_MODE = false;
    BLK_PROBE_MODE = true;
    ACTIVE_GUEST_ID = M4_BLK_PROBE_GUEST_ID;
    REAL_LINUX_GUEST = false;
    LINUX_GTIMER2_DONE = false;
    LINUX_GTIMER2_ARMED = false;
    BRINGUP_GUEST_CODE_PHYS = frames.guest_code_phys;
    reset_saved_gprs(0);

    serial::write_line("boot: M4.3 — launching virtio-blk probe guest");

    apic::mask_pic();
    let _ = apic::mask_timer();
    let _ = apic::eoi();
    ept_hw::invept_global();

    if prepare_vmcs_region(frames.vmcs_phys).is_err() {
        serial::write_line("boot: ERROR — blk probe VMCS prepare failed");
        finish_boot(false);
    }
    if ops::vmclear(frames.vmcs_phys).is_err() {
        serial::write_line("boot: ERROR — blk probe VMCLEAR failed");
        finish_boot(false);
    }
    if prepare_vmcs_region(frames.vmcs_phys).is_err() {
        serial::write_line("boot: ERROR — blk probe VMCS re-prepare failed");
        finish_boot(false);
    }
    if let Err(e) = setup_vmcs(&frames) {
        serial::write_str("boot: ERROR — blk probe setup_vmcs failed: ");
        serial::write_line(launch_err_name(e));
        finish_boot(false);
    }

    let _ = apic::mask_timer();
    let _ = apic::eoi();
    let _ = ops::vmwrite(VM_ENTRY_INTERRUPTION_INFO, 0);

    serial::write_line("boot: VMLAUNCH → virtio-blk status probe");
    match ops::vmlaunch() {
        Ok(()) => {
            serial::write_line("boot: ERROR — blk probe VMLAUNCH returned Ok");
            finish_boot(false);
        }
        Err(_) => {
            let ierr = ops::vmread(VM_INSTRUCTION_ERROR).unwrap_or(0xFFFF) as u32;
            serial::write_str("boot: ERROR — blk probe VMLAUNCH failed insn_error=0x");
            write_hex_u32(ierr);
            serial::write_byte(b'\n');
            finish_boot(false);
        }
    }
}

/// Exit path for the M4.3 virtio-blk probe guest.
unsafe fn handle_blk_probe_vmexit(basic: u32, qual: u64, guest_rax: u64, guest_rip: u64) -> ! {
    match basic {
        EXIT_REASON_EXTERNAL_INTERRUPT => {
            let _ = apic::eoi();
            let _ = apic::mask_timer();
            let _ = ops::vmwrite(VM_ENTRY_INTERRUPTION_INFO, 0);
            vmresume_with_gprs();
        }
        EXIT_REASON_EPT_VIOLATION => handle_ept_violation_and_resume(qual, guest_rip),
        EXIT_REASON_IO_INSTRUCTION => handle_io_and_resume(qual, guest_rax, guest_rip),
        EXIT_REASON_HLT => {
            if virtio_blk::blk_ok() {
                if virtio_blk::take_blk_ok_latch() {
                    serial::write_line(M4_BLK_OK_MARKER);
                }
                if virtio_blk::take_booted_from_disk_latch() {
                    serial::write_line(M7_ISO_BOOTED_FROM_DISK_MARKER);
                    let _ = iso_install::note_booted_from_disk_lab();
                }
                if virtio_blk::take_install_disk_written_latch() {
                    serial::write_line(M7_ISO_DISK_WRITTEN_MARKER);
                    serial::write_line(M7_ISO_INSTALL_LAB_OK_MARKER);
                    let _ = iso_install::note_install_disk_written();
                    if iso_install::note_reboot_pending_lab() {
                        serial::write_line(M7_ISO_REBOOT_PENDING_MARKER);
                        let _ = iso_install::take_reboot_pending_latch();
                    }
                }
                serial::write_line(
                    "boot: M4.3 complete — virtio-blk MMIO handshake + write/readback",
                );
                BLK_PROBE_MODE = false;
                if HAS_NET_PROBE {
                    try_launch_net_probe();
                }
                finish_boot(true);
            }
            serial::write_line("boot: ERROR — blk probe HLT without DRIVER_OK readback");
            finish_boot(false);
        }
        _ => {
            serial::write_str("boot: ERROR — blk probe unhandled exit reason=0x");
            write_hex_u32(basic);
            serial::write_byte(b'\n');
            finish_boot(false);
        }
    }
}

/// M4.4: after BLK-OK, VMLAUNCH the virtio-net dual-port probe guest.
unsafe fn try_launch_net_probe() -> ! {
    let frames = match core::ptr::addr_of!(NET_PROBE_FRAMES).read() {
        Some(f) => f,
        None => {
            serial::write_line("boot: ERROR — net probe frames missing");
            finish_boot(false);
        }
    };
    SCHED_MODE = false;
    BLK_PROBE_MODE = false;
    NET_PROBE_MODE = true;
    ACTIVE_GUEST_ID = M4_NET_PROBE_GUEST_ID;
    REAL_LINUX_GUEST = false;
    LINUX_GTIMER2_DONE = false;
    LINUX_GTIMER2_ARMED = false;
    BRINGUP_GUEST_CODE_PHYS = frames.guest_code_phys;
    reset_saved_gprs(0);

    serial::write_line("boot: M4.4 — launching virtio-net probe guest");

    apic::mask_pic();
    let _ = apic::mask_timer();
    let _ = apic::eoi();
    ept_hw::invept_global();

    if prepare_vmcs_region(frames.vmcs_phys).is_err() {
        serial::write_line("boot: ERROR — net probe VMCS prepare failed");
        finish_boot(false);
    }
    if ops::vmclear(frames.vmcs_phys).is_err() {
        serial::write_line("boot: ERROR — net probe VMCLEAR failed");
        finish_boot(false);
    }
    if prepare_vmcs_region(frames.vmcs_phys).is_err() {
        serial::write_line("boot: ERROR — net probe VMCS re-prepare failed");
        finish_boot(false);
    }
    if let Err(e) = setup_vmcs(&frames) {
        serial::write_str("boot: ERROR — net probe setup_vmcs failed: ");
        serial::write_line(launch_err_name(e));
        finish_boot(false);
    }

    let _ = apic::mask_timer();
    let _ = apic::eoi();
    let _ = ops::vmwrite(VM_ENTRY_INTERRUPTION_INFO, 0);

    serial::write_line("boot: VMLAUNCH → virtio-net dual-port probe");
    match ops::vmlaunch() {
        Ok(()) => {
            serial::write_line("boot: ERROR — net probe VMLAUNCH returned Ok");
            finish_boot(false);
        }
        Err(_) => {
            let ierr = ops::vmread(VM_INSTRUCTION_ERROR).unwrap_or(0xFFFF) as u32;
            serial::write_str("boot: ERROR — net probe VMLAUNCH failed insn_error=0x");
            write_hex_u32(ierr);
            serial::write_byte(b'\n');
            finish_boot(false);
        }
    }
}

/// Exit path for the M4.4 virtio-net probe guest.
unsafe fn handle_net_probe_vmexit(basic: u32, qual: u64, guest_rax: u64, guest_rip: u64) -> ! {
    match basic {
        EXIT_REASON_EXTERNAL_INTERRUPT => {
            let _ = apic::eoi();
            let _ = apic::mask_timer();
            let _ = ops::vmwrite(VM_ENTRY_INTERRUPTION_INFO, 0);
            vmresume_with_gprs();
        }
        EXIT_REASON_EPT_VIOLATION => handle_ept_violation_and_resume(qual, guest_rip),
        EXIT_REASON_IO_INSTRUCTION => handle_io_and_resume(qual, guest_rax, guest_rip),
        EXIT_REASON_HLT => {
            if virtio_net::net_ok() {
                if virtio_net::take_net_ok_latch() {
                    serial::write_line(M4_NET_OK_MARKER);
                }
                serial::write_line("boot: M4.4 complete — virtio-net dual-port vSwitch exchange");
                NET_PROBE_MODE = false;
                if HAS_SMP_PROBE {
                    try_launch_smp_probe();
                }
                finish_boot(true);
            }
            serial::write_line("boot: ERROR — net probe HLT without port exchange");
            finish_boot(false);
        }
        _ => {
            serial::write_str("boot: ERROR — net probe unhandled exit reason=0x");
            write_hex_u32(basic);
            serial::write_byte(b'\n');
            finish_boot(false);
        }
    }
}

/// M4.5: after NET-OK, VMLAUNCH the SMP BSP (AP follows on BSP HLT).
unsafe fn try_launch_smp_probe() -> ! {
    let frames = match core::ptr::addr_of!(SMP_BSP_FRAMES).read() {
        Some(f) => f,
        None => {
            serial::write_line("boot: ERROR — SMP BSP frames missing");
            finish_boot(false);
        }
    };
    SCHED_MODE = false;
    BLK_PROBE_MODE = false;
    NET_PROBE_MODE = false;
    SMP_PROBE_MODE = true;
    SMP_AP_LAUNCHED = false;
    ACTIVE_GUEST_ID = M4_SMP_GUEST_ID;
    REAL_LINUX_GUEST = false;
    LINUX_GTIMER2_DONE = false;
    LINUX_GTIMER2_ARMED = false;
    BRINGUP_GUEST_CODE_PHYS = frames.guest_code_phys;
    reset_saved_gprs(0);

    serial::write_line("boot: M4.5 — launching SMP BSP (shared EPT; AP wake = host VMLAUNCH)");

    apic::mask_pic();
    let _ = apic::mask_timer();
    let _ = apic::eoi();
    ept_hw::invept_global();

    if prepare_vmcs_region(frames.vmcs_phys).is_err() {
        serial::write_line("boot: ERROR — SMP BSP VMCS prepare failed");
        finish_boot(false);
    }
    if ops::vmclear(frames.vmcs_phys).is_err() {
        serial::write_line("boot: ERROR — SMP BSP VMCLEAR failed");
        finish_boot(false);
    }
    if prepare_vmcs_region(frames.vmcs_phys).is_err() {
        serial::write_line("boot: ERROR — SMP BSP VMCS re-prepare failed");
        finish_boot(false);
    }
    if let Err(e) = setup_vmcs(&frames) {
        serial::write_str("boot: ERROR — SMP BSP setup_vmcs failed: ");
        serial::write_line(launch_err_name(e));
        finish_boot(false);
    }

    let _ = apic::mask_timer();
    let _ = apic::eoi();
    let _ = ops::vmwrite(VM_ENTRY_INTERRUPTION_INFO, 0);

    serial::write_line("boot: VMLAUNCH → SMP BSP ready-flag store");
    match ops::vmlaunch() {
        Ok(()) => {
            serial::write_line("boot: ERROR — SMP BSP VMLAUNCH returned Ok");
            finish_boot(false);
        }
        Err(_) => {
            let ierr = ops::vmread(VM_INSTRUCTION_ERROR).unwrap_or(0xFFFF) as u32;
            serial::write_str("boot: ERROR — SMP BSP VMLAUNCH failed insn_error=0x");
            write_hex_u32(ierr);
            serial::write_byte(b'\n');
            finish_boot(false);
        }
    }
}

/// Documented AP wake: VMLAUNCH the AP VMCS after BSP ready (INIT-SIPI equivalent).
unsafe fn launch_smp_ap() -> ! {
    let frames = match core::ptr::addr_of!(SMP_AP_FRAMES).read() {
        Some(f) => f,
        None => {
            serial::write_line("boot: ERROR — SMP AP frames missing");
            finish_boot(false);
        }
    };
    SMP_AP_LAUNCHED = true;
    ACTIVE_GUEST_ID = M4_SMP_GUEST_ID;
    BRINGUP_GUEST_CODE_PHYS = frames.guest_code_phys;
    reset_saved_gprs(0);

    serial::write_line("boot: M4.5 — AP wake via host VMLAUNCH (documented INIT-SIPI equiv)");

    ept_hw::invept_global();

    if prepare_vmcs_region(frames.vmcs_phys).is_err() {
        serial::write_line("boot: ERROR — SMP AP VMCS prepare failed");
        finish_boot(false);
    }
    if ops::vmclear(frames.vmcs_phys).is_err() {
        serial::write_line("boot: ERROR — SMP AP VMCLEAR failed");
        finish_boot(false);
    }
    if prepare_vmcs_region(frames.vmcs_phys).is_err() {
        serial::write_line("boot: ERROR — SMP AP VMCS re-prepare failed");
        finish_boot(false);
    }
    if let Err(e) = setup_vmcs(&frames) {
        serial::write_str("boot: ERROR — SMP AP setup_vmcs failed: ");
        serial::write_line(launch_err_name(e));
        finish_boot(false);
    }

    let _ = apic::mask_timer();
    let _ = apic::eoi();
    let _ = ops::vmwrite(VM_ENTRY_INTERRUPTION_INFO, 0);

    serial::write_line("boot: VMLAUNCH → SMP AP ready-flag store");
    match ops::vmlaunch() {
        Ok(()) => {
            serial::write_line("boot: ERROR — SMP AP VMLAUNCH returned Ok");
            finish_boot(false);
        }
        Err(_) => {
            let ierr = ops::vmread(VM_INSTRUCTION_ERROR).unwrap_or(0xFFFF) as u32;
            serial::write_str("boot: ERROR — SMP AP VMLAUNCH failed insn_error=0x");
            write_hex_u32(ierr);
            serial::write_byte(b'\n');
            finish_boot(false);
        }
    }
}

/// Exit path for the M4.5 SMP probe (BSP then AP under shared guest id / EPT).
unsafe fn handle_smp_probe_vmexit(basic: u32, qual: u64, guest_rax: u64, guest_rip: u64) -> ! {
    match basic {
        EXIT_REASON_EXTERNAL_INTERRUPT => {
            let _ = apic::eoi();
            let _ = apic::mask_timer();
            let _ = ops::vmwrite(VM_ENTRY_INTERRUPTION_INFO, 0);
            vmresume_with_gprs();
        }
        EXIT_REASON_EPT_VIOLATION => handle_ept_violation_and_resume(qual, guest_rip),
        EXIT_REASON_IO_INSTRUCTION => handle_io_and_resume(qual, guest_rax, guest_rip),
        EXIT_REASON_HLT => {
            if !SMP_AP_LAUNCHED {
                if !smp_probe::note_bsp_ready() {
                    serial::write_line("boot: ERROR — SMP BSP HLT without ready flag");
                    finish_boot(false);
                }
                serial::write_line("boot: M4.5 BSP ready — waking AP");
                launch_smp_ap();
            }
            if !smp_probe::note_ap_ready() {
                serial::write_line("boot: ERROR — SMP AP HLT without ready flag");
                finish_boot(false);
            }
            if smp_probe::smp_ok() {
                if smp_probe::take_smp_ok_latch() {
                    serial::write_line(M4_SMP_OK_MARKER);
                }
                serial::write_line("boot: M4.5 complete — dual-vCPU BSP+AP under shared EPT");
                finish_boot(true);
            }
            serial::write_line("boot: ERROR — SMP AP HLT but smp_ok not latched");
            finish_boot(false);
        }
        _ => {
            serial::write_str("boot: ERROR — SMP probe unhandled exit reason=0x");
            write_hex_u32(basic);
            serial::write_byte(b'\n');
            finish_boot(false);
        }
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
    let value = (SAVED_GUEST_RAX & 0xffff_ffff) | ((SAVED_GUEST_RDX & 0xffff_ffff) << 32);
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
    let mut after_shell_slot: Option<usize> = None;

    // M3.10: real `/init` SHELL hypercall (before any UART TX that may stall).
    // M3.19: latch shell on CPUID — no IRQ4 COM1 TX inject required.
    // M3.12: do not close until APIC-OK as well (maybe_finish_m312).
    // M4.0–M4.2: shell guests use the same SHELL CPUID leaf.
    if leaf == SHELL_CPUID_LEAF && subleaf == SHELL_CPUID_SUBLEAF {
        if let Some(slot) = slot_for_guest_id(ACTIVE_GUEST_ID) {
            if slot >= 1 && !SCHED_MODE {
                if slot == 1 && !TWO_VM_LATCHED {
                    TWO_VM_LATCHED = true;
                    serial::write_byte(b'\n');
                    serial::write_line(M4_SHELL_G1_MARKER);
                    serial::write_line(M4_2VM_OK_MARKER);
                    serial::write_line(
                        "boot: M4.0 complete — G0 Linux SHELL + G1 private EPT SHELL (2VM)",
                    );
                } else if slot >= 2 {
                    serial::write_str("boot: shell guest SHELL latched slot=");
                    write_hex_u32(slot as u32);
                    serial::write_byte(b'\n');
                }
                after_shell_slot = Some(slot);
            } else if SPA_LAUNCHED && slot == 1 {
                static mut E4_SPA_SHELL: bool = false;
                if !E4_SPA_SHELL {
                    E4_SPA_SHELL = true;
                    serial::write_byte(b'\n');
                    serial::write_line(M7_E4_SPA_LAUNCH_OK_MARKER);
                }
            } else if slot == 0 && REAL_LINUX_GUEST && LINUX_GTIMER2_DONE {
                serial_pio::note_shell_cpuid();
                static mut SHELL_CPUID_MARKED: bool = false;
                if !SHELL_CPUID_MARKED {
                    SHELL_CPUID_MARKED = true;
                    serial::write_byte(b'\n');
                    serial::write_line(M3_SHELL_OK_MARKER);
                }
            }
        }
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

    // G0 SHELL: RIP is past the hypercall — safe to leave the VMCS and launch shells.
    if leaf == SHELL_CPUID_LEAF
        && subleaf == SHELL_CPUID_SUBLEAF
        && ACTIVE_GUEST_ID == M2_BRINGUP_GUEST_ID
        && REAL_LINUX_GUEST
        && LINUX_GTIMER2_DONE
    {
        maybe_finish_m312();
    }

    if let Some(slot) = after_shell_slot {
        after_shell_cpuid(slot);
    }

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
    coexist_failsoft_idle();
}

unsafe extern "C" fn vmlaunch_gprs_failed() -> ! {
    let ierr = ops::vmread(VM_INSTRUCTION_ERROR).unwrap_or(0xFFFF) as u32;
    serial::write_str("boot: ERROR — VMLAUNCH(gprs) failed insn_error=0x");
    write_hex_u32(ierr);
    serial::write_byte(b'\n');
    dump_vm_entry_fail_ctls();
    // Iron `eb456eec`: SPA re-entry VMLAUNCH error 7 after incoming rewrite.
    // Park slot 1 and VMLAUNCH G0 (clear); keep HTTP. Never VMXOFF.
    if M4_LADDER_DONE && SCHED_SLOT_CUR == 1 {
        serial::write_line("boot: HINT — SPA VMLAUNCH fail; park slot=1 resume G0");
        failsoft_resume_g0_after_spa_vmlaunch_fail();
    }
    coexist_failsoft_idle();
}

/// Dump VM-entry control words from the current VMCS (error 7 diagnosis).
unsafe fn dump_vm_entry_fail_ctls() {
    let pin = ops::vmread(PIN_BASED_VM_EXEC_CONTROL).unwrap_or(0);
    let prim = ops::vmread(PRIMARY_PROC_BASED_VM_EXEC_CONTROL).unwrap_or(0);
    let sec = ops::vmread(SECONDARY_VM_EXEC_CONTROL).unwrap_or(0);
    let exit = ops::vmread(VM_EXIT_CONTROLS).unwrap_or(0);
    let entry = ops::vmread(VM_ENTRY_CONTROLS).unwrap_or(0);
    let eptp = ops::vmread(EPT_POINTER).unwrap_or(0);
    let link = ops::vmread(VMCS_LINK_POINTER).unwrap_or(0);
    let rip = ops::vmread(GUEST_RIP).unwrap_or(0);
    serial::write_str("boot: VMCS ctls pin=0x");
    write_hex_u32(pin as u32);
    serial::write_str(" primary=0x");
    write_hex_u32(prim as u32);
    serial::write_str(" secondary=0x");
    write_hex_u32(sec as u32);
    serial::write_str(" exit=0x");
    write_hex_u32(exit as u32);
    serial::write_str(" entry=0x");
    write_hex_u32(entry as u32);
    serial::write_byte(b'\n');
    serial::write_str("boot: EPTP=0x");
    write_hex_u64(eptp);
    serial::write_str(" link=0x");
    write_hex_u64(link);
    serial::write_str(" rip=0x");
    write_hex_u64(rip);
    serial::write_byte(b'\n');
}

/// After SPA `VMLAUNCH` error 7: park slot 1, `VMLAUNCH` relocated G0.
unsafe fn failsoft_resume_g0_after_spa_vmlaunch_fail() -> ! {
    SPA_RUNNABLE = false;
    SPA_NEEDS_VMLAUNCH = false;
    SPA_VMPTRLD_FAILED = true;
    if let Some(s1) = frames_for_slot(1) {
        let _ = ops::vmclear(s1.vmcs_phys);
        rewrite_vmcs_revision(s1.vmcs_phys);
    }
    let Some(g0) = frames_for_slot(0) else {
        coexist_failsoft_idle();
    };
    if ops::vmptrld(g0.vmcs_phys).is_err() {
        G0_VMPTRLD_FAILED = true;
        log_sched_vmptrld_fail_once(0, g0.vmcs_phys);
        coexist_failsoft_idle();
    }
    SCHED_SLOT_CUR = 0;
    ACTIVE_GUEST_ID = guest_id_for_slot(0);
    REAL_LINUX_GUEST = true;
    LINUX_GTIMER2_DONE = true;
    BRINGUP_GUEST_CODE_PHYS = g0.guest_code_phys;
    load_live_gprs_from_slot(0);
    let _ = ops::vmwrite(VM_ENTRY_INTERRUPTION_INFO, 0);
    arm_sched_slice();
    G0_NEEDS_VMLAUNCH = false;
    serial::write_line("boot: E4 G0 VMLAUNCH after SPA entry fail (VMX on; no VMXOFF)");
    if !restore_vmcs_shadow(0) {
        serial::write_line("boot: WARN — E4 G0 shadow restore failed after SPA entry fail");
        coexist_failsoft_idle();
    }
    vmlaunch_with_gprs();
}

/// Last-resort: keep native HTTP alive. Do not VMXOFF / `boot gate failed`.
unsafe fn coexist_failsoft_idle() -> ! {
    if M4_LADDER_DONE {
        serial::write_line("boot: WARN — VM-entry fail-soft idle (VMX on; coexist)");
        loop {
            crate::mgmt::tick_native_coexist();
            core::hint::spin_loop();
        }
    }
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
            crate::audit_log!(crate::audit::AuditEvent::MsrBlocked {
                vcpu_id: 0,
                msr_index: index,
            });
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
    if ok {
        // Do not call serial::init() here — reprogramming UART mid-SOL can
        // drop the next line(s). Only revive COM*_LIVE after possible THR timeout.
        serial::revive_ports();

        // E2 / M7.5 gate marker — print immediately and repeatedly so a SOL
        // capture cannot miss it. Phase F keeps VMX on (no VMXOFF-then-listen).
        serial::write_line("boot: E2 marker build=r640-boot-ok-marker");
        serial::write_line(M7_R640_BOOT_OK_MARKER);
        serial::write_line(M7_R640_BOOT_OK_MARKER);

        // SAFETY: boot single-threaded; flags set before / during guest path.
        if unsafe { SMP_PROBE_MODE } {
            serial::write_line("boot: M4.5 complete — SMP dual-vCPU path OK");
        } else if unsafe { NET_PROBE_MODE } {
            serial::write_line("boot: M4.4 complete — virtio-net path OK");
        } else if unsafe { BLK_PROBE_MODE } {
            serial::write_line("boot: M4.3 complete — virtio-blk path OK");
        } else if unsafe { SCHED_MODE } {
            serial::write_line("boot: M4.2 complete — multi-guest sched path OK");
        } else if unsafe { SECOND_GUEST_STARTED } {
            serial::write_line("boot: M4.0 complete — dual VMCS path OK");
        } else if unsafe { REAL_LINUX_GUEST } {
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
        serial::revive_ports();
        serial::write_line(M7_R640_BOOT_OK_MARKER);
        for _ in 0..5_000_000 {
            core::hint::spin_loop();
        }
        serial::qemu_exit_success();
        // QEMU exits above; iron continues with VMX still on.
        crate::mgmt::run_post_ebs_http_snp_warn_only();
        if crate::mgmt::try_arm_native_coexist() {
            unsafe {
                enter_sched_coexist();
            }
        }
        serial::write_line("boot: WARN — HOST-NIC coexist skipped; Phase D idle after VMXOFF");
    }

    // SAFETY: still in VMX root after VMEXIT; tear down before QEMU fail/idle.
    match unsafe { hardware::vmxoff() } {
        Ok(()) => serial::write_line("boot: VMXOFF ok"),
        Err(_) => serial::write_line("boot: ERROR — VMXOFF failed"),
    }

    if ok {
        crate::mgmt::run_post_ebs_http_idle();
    } else {
        serial::write_line("boot: boot gate failed");
        serial::qemu_exit_failure();
    }

    loop {
        core::hint::spin_loop();
    }
}

/// Resume G0–G3 under the credit scheduler with native NIC ticks (Phase F).
unsafe fn enter_sched_coexist() -> ! {
    SMP_PROBE_MODE = false;
    NET_PROBE_MODE = false;
    BLK_PROBE_MODE = false;
    M4_LADDER_DONE = true;
    SCHED_MODE = true;
    serial::write_line("boot: HOST-NIC coexist — resume G0 (VMX on; G1–G3 parked)");
    if !SCHED_OK_LATCHED || frames_for_slot(0).is_none() {
        serial::write_line("boot: WARN — coexist has no scheduler/G0; falling back");
        finish_boot_idle_after_vmxoff();
    }
    // G1–G3 are M4.2 SHELL stubs. G0 precise EPT identity-maps their VMCS
    // HPA (iron: VMPTRLD fail after SPA HTTP). Keep only G0 runnable.
    SCHED = CreditScheduler::new();
    let _ = SCHED.register_vcpu(DEFAULT_CREDIT);
    SCHED_SLOT_CUR = 0;
    switch_to_sched_slot(0);
}

fn finish_boot_idle_after_vmxoff() -> ! {
    match unsafe { hardware::vmxoff() } {
        Ok(()) => serial::write_line("boot: VMXOFF ok"),
        Err(_) => serial::write_line("boot: ERROR — VMXOFF failed"),
    }
    crate::mgmt::run_post_ebs_http_idle();
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
        assert_eq!(M7_R640_BOOT_OK_MARKER, "RAYNU-V-R640-BOOT-OK");
        assert_eq!(LOOP_HLT_TARGET, 4);
        assert_eq!(EXIT_REASON_HLT, 12);
        assert_eq!(EXIT_REASON_EXTERNAL_INTERRUPT, 1);
        assert_eq!(EXIT_REASON_CPUID, 10);
        assert_eq!(EXIT_REASON_IO_INSTRUCTION, 30);
        assert_eq!(EXIT_REASON_XSETBV, 55);
        assert_eq!(PIN_BASED_EXTERNAL_INTERRUPT_EXITING, 1);
        assert_eq!(VM_EXIT_ACK_INTERRUPT_ON_EXIT, 1 << 15);
        assert_eq!(CPU_BASED_USE_TPR_SHADOW, 1 << 21);
        assert_eq!(CPU_BASED_UNCONDITIONAL_IO, 1 << 24);
        assert_eq!(GUEST_UEFI_OVMF_ESP_PATH, "\\EFI\\RayNu\\OVMF.fd");
        assert_eq!(MIN_LIVE_ESP_OVMF_BYTES, 2 * 1024 * 1024);
        assert_eq!(MIN_FIRMWARE_ALIAS_BYTES, 4 * 1024 * 1024);
        assert_eq!(GUEST_UEFI_FIRMWARE_TOP_GPA, 0x1_0000_0000);
        assert_eq!(GUEST_UEFI_UNRESTRICTED_GUEST, 1 << 7);
        assert_eq!(
            firmware_alias_gpa(MIN_FIRMWARE_ALIAS_BYTES as u64),
            Some(0xFFC0_0000)
        );
        reset_live_esp_ovmf_mapping();
        assert!(!live_esp_ovmf_is_mapped());
        assert_eq!(
            try_vmlaunch_guest_uefi_ovmf(),
            Err(GuestUefiLaunchError::MissingEspFirmware)
        );
        assert_eq!(
            arm_live_esp_ovmf_mapping(1024 * 1024),
            Err(GuestUefiLaunchError::MissingEspFirmware)
        );
        assert!(!live_esp_ovmf_is_mapped());
        assert_eq!(
            arm_live_esp_ovmf_mapping(MIN_LIVE_ESP_OVMF_BYTES as u64),
            Ok(())
        );
        assert!(live_esp_ovmf_is_mapped());
        assert_eq!(live_esp_ovmf_bytes_len(), MIN_LIVE_ESP_OVMF_BYTES as u64);
        assert_eq!(
            try_vmlaunch_guest_uefi_ovmf(),
            Err(GuestUefiLaunchError::LiveMappedNotLaunched)
        );
        let mut stub = vec![0u8; MIN_LIVE_ESP_OVMF_BYTES];
        assert_eq!(
            arm_guest_uefi_reset_vector(&stub),
            Err(GuestUefiLaunchError::NoResetVector)
        );
        assert!(!guest_uefi_reset_vector_is_armed());
        stub[MIN_LIVE_ESP_OVMF_BYTES - GUEST_UEFI_RESET_VECTOR_LEN] =
            GUEST_UEFI_RESET_VECTOR_OPCODE;
        assert_eq!(arm_guest_uefi_reset_vector(&stub), Ok(()));
        assert!(guest_uefi_reset_vector_is_armed());
        assert_eq!(GUEST_UEFI_RESET_VMCS.reset_gpa, GUEST_UEFI_RESET_VECTOR_GPA);
        assert_eq!(
            try_vmlaunch_guest_uefi_ovmf(),
            Err(GuestUefiLaunchError::ResetVectorNotLaunched)
        );
        assert_eq!(
            arm_guest_uefi_firmware_alias(MIN_LIVE_ESP_OVMF_BYTES as u64),
            Err(GuestUefiLaunchError::MissingEspFirmware)
        );
        assert!(!guest_uefi_firmware_alias_is_armed());
        assert_eq!(
            arm_guest_uefi_firmware_alias(MIN_FIRMWARE_ALIAS_BYTES as u64),
            Ok(())
        );
        assert!(guest_uefi_firmware_alias_is_armed());
        assert_eq!(
            try_vmlaunch_guest_uefi_ovmf(),
            Err(GuestUefiLaunchError::FirmwareAliasNotLaunched)
        );
        assert!(alias_ept_covers_reset(
            0xFFC0_0000,
            MIN_FIRMWARE_ALIAS_BYTES as u64
        ));
        assert_eq!(GUEST_UEFI_ALIAS_EPT.gpa, 0xFFC0_0000);
        assert_eq!(
            program_guest_uefi_alias_ept(MIN_LIVE_ESP_OVMF_BYTES as u64),
            Err(GuestUefiLaunchError::MissingEspFirmware)
        );
        assert!(!guest_uefi_alias_ept_is_programmed());
        assert_eq!(
            program_guest_uefi_alias_ept(MIN_FIRMWARE_ALIAS_BYTES as u64),
            Ok(())
        );
        assert!(guest_uefi_alias_ept_is_programmed());
        assert_eq!(
            try_vmlaunch_guest_uefi_ovmf(),
            Err(GuestUefiLaunchError::AliasEptNotLaunched)
        );
        reset_live_esp_ovmf_mapping();
        assert!(!live_esp_ovmf_is_mapped());
        assert!(!guest_uefi_reset_vector_is_armed());
        assert!(!guest_uefi_firmware_alias_is_armed());
        assert!(!guest_uefi_alias_ept_is_programmed());
        assert_eq!(live_esp_ovmf_bytes_len(), 0);
    }

    #[test]
    fn vmcs_revision_clears_shadow_bit() {
        assert_eq!(vmcs_revision_from_basic(0x8000_0013), 0x13);
        assert_eq!(vmcs_revision_from_basic(0x0000_0013), 0x13);
        assert_eq!(vmcs_revision_from_basic(u64::MAX), 0x7fff_ffff);
        assert_eq!(vmcs_revision_from_basic(0), 0);
    }
}
