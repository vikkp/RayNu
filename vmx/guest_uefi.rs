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

use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicU8, Ordering};

use crate::arch::cpu::CPUID_ECX_X2APIC;
use crate::boot::ovmf_esp::{self, MIN_REAL_OVMF_BYTES};
use crate::memory::frame_allocator::FrameAllocator;
use crate::sched::msr_firewall::{self, CpuidRegs};
use crate::vmx::launch::{
    alias_ept_covers_reset, GuestUefiLaunchError, GUEST_UEFI_FIRMWARE_TOP_GPA,
    GUEST_UEFI_PRIVATE_VMCS_ID,
};

#[cfg(target_os = "uefi")]
use crate::arch::cpu::{
    self, adjust_vmx_controls, true_ctl_msrs_supported, IA32_EFER, IA32_FS_BASE,
    IA32_GS_BASE, IA32_SYSENTER_CS, IA32_SYSENTER_EIP, IA32_SYSENTER_ESP, IA32_VMX_CR0_FIXED0,
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
use crate::memory::ept_hw::GUEST_UEFI_LOW_RAM_BYTES;
#[cfg(target_os = "uefi")]
use crate::memory::ept_hw::{self, frames_required_firmware_alias};
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
    "residual: private guest-UEFI VMCS + EPT VMLAUNCH of retained ESP OVMF.fd; CR4.VMXE host-owned + CR4.OSXSAVE host-owned so OVMF SEC mov cr4,0x640 does not #GP and CpuDxe mov cr4,0x668 does not clear OSXSAVE; COM1/COM2 forwarded; past-SEC when linear leaves last 64KiB and PEI PCI or firmware serial or HLT; attach_cdrom_uefi after FirmwareArmed is GuestVisible (PCI IDE/ATAPI; IDE at 00:00.1); unarmed stays UnsupportedOnFirmware; CMOS/fw_cfg/i440fx platform; i440FX host at 00:08.0; PEI DID probe is virtio at 00:00.0; virtio Header Type is multifunction so a walk finds IDE fn1; PIIX 00:01.1 is the same CD; PIIX4 PM at 00:01.3; remap i440FX DID in guest-private OVMF copy (cmp bx, not LZMA 37 12); CF8|CFC byte offset matches QEMU pci_host_data_read; EPT sink-resume for high MMIO; 4MiB flash window (VARS gap at 0xFFC00000); empty VARS _FVH; live HPET; HPET 1s step; stop RIP insn dump; spin jmp skip; past-PEI/DXE or CD boot attempt; empty virtio-blk at 00:00.0; fw_cfg bootorder CD then disk (PIIX ide@1,1 then virtio-fn1 ide@0,1, master drive@0, not slave drive@1; scsi-first skipped IDE Start); ACPI PM timer (port 0 dword + PIIX 0x408) so AcpiTimerLib Delay can end when DID is 0x1042; post-DXE spends the 32768-exit cap until ATAPI sectors>0 (not virtio-alone; not both-enum-alone; 1b07692 n=1111 BOTH then stopped with sectors=0; 8e55abf n=2048 ata=0 unh=0 still PciBus cf8=0x80000838 ISA 00:01.0 offset 0x38; 5d9e346 n=8192 ataio=0 unh=3 port=0xcf8 empty-slot walk + KBC; 8192-exit cap ended on CF8; 2674629 n=32768 ataio=0 acpi=16612 port=0 in eax,dx); PIIX3 ISA PIRQ 0x60-0x63 default 0x80; HPET 1s on preemption/HLT not PCI I/O; 8042 KBC 0x60/0x64; ACPI PM 1s step; iron COM2 #UD RIP 0x109D pci_ide=0; iron 0ca02e6 skipped eb ec then #UD RIP 0x109D CR4=0x668 DebugLib dumped COM1 until cap; #UD intercept XSAVE retry/UD2 skip; iron d5f9431 #UD gone then n=1280..8192 reason=0x34 rip=0x6e81ca (pause CpuDeadLoop, no BOTH-OK); preempt pause/jcc skip; e2af81e missed GCC eb fc / 0F 84 rel32 (iron COM2 insn=ebec jmp -20); preempt eb/jcc32 skip; iron 891eb5b OSXSAVE CR4 intercept then skipped ebecc9c3 leave; ret then #UD 0x109D DAA PE header; do not skip jmp whose fallthrough is leave; ret; dump ASSERT retaddr; iron 17449e2 ASSERT noskip ret=0x6e8946 rip=0x6e81ca after host CPUID (Xeon topology+VMX); guest-UEFI CPUID uniprocessor hide VMX/x2APIC; FEATURE_CONTROL lock no VMX; QEMU CI 17449e2 stuck ebf3c9c3 (jmp -13 leave;ret) — keep that skip (nested BOTH-OK); noskip only iron eb ec; preempt noskip dump; guest-UEFI INVPCID/RDTSCP/XSAVES; XSETBV executes XCR0 (not skip_insn); fw_cfg etc/boot-menu-wait 0ms skip BdsWait; HLT skip so DXE can walk PCI; CR-access resume; firmware-simultaneous PCI enum; 8259 PIC RAZ/WI; fw_cfg etc/e820 32MiB; exception insn dump; ATAPI signature + PACKET interrupt-reason so firmware can READ(10); 8-byte IDE command BAR and BAR-relocated ATA; EXECUTE DEVICE DIAGNOSTIC 0x90 restores 0xEB14; BMIDE BAR4 RAZ/WI; first unhandled I/O traced; not firmware El Torito boot; not installer; not ISO-INSTALL-OK; no guest UEFI distro; VMLAUNCH insn issued only when presence is true";

/// QEMU / serial marker when OVMF ran past the first triple-fault.
pub const M7_E5_OVMF_ALIVE_OK_MARKER: &str = "RAYNU-V-M7-E5-OVMF-ALIVE-OK";

/// QEMU / serial marker when OVMF left the SEC tail (not full DXE / not installer).
pub const M7_E5_OVMF_PAST_SEC_OK_MARKER: &str = "RAYNU-V-M7-E5-OVMF-PAST-SEC-OK";

/// QEMU / serial marker when the guest-UEFI VMCS can see CD media.
pub const M7_E5_OVMF_CDROM_OK_MARKER: &str = "RAYNU-V-M7-E5-OVMF-CDROM-OK";

/// QEMU / serial marker when PEI/DXE progressed or the guest attempted CD boot.
pub const M7_E5_OVMF_DXE_OK_MARKER: &str = "RAYNU-V-M7-E5-OVMF-DXE-OK";

/// QEMU / serial marker when guest-UEFI sees empty virtio-blk + CD→disk order.
pub const M7_E5_OVMF_VIRTIO_OK_MARKER: &str = "RAYNU-V-M7-E5-OVMF-VIRTIO-OK";

/// QEMU / serial marker when firmware enumerated virtio `00:00.0` and IDE `00:00.1`
/// on the same boot. Not ATAPI sectors. Not installer.
pub const M7_E5_OVMF_BOTH_OK_MARKER: &str = "RAYNU-V-M7-E5-OVMF-BOTH-OK";

/// QEMU / serial marker when firmware issued ATAPI READ and `sectors>0`.
/// Not a completed El Torito CD boot. Not installer.
pub const M7_E5_OVMF_ATAPI_OK_MARKER: &str = "RAYNU-V-M7-E5-OVMF-ATAPI-OK";

/// Last 64 KiB of the 4 GiB space. OVMF 4M SEC / VTF lives here
/// (reset vector `0xFFFF_FFF0`; Stage 38 first exits at `0xFFFF_Fxxx`).
pub const GUEST_UEFI_SEC_TAIL_GPA: u64 = 0xFFFF_0000;

/// Resume cap after Stage 40's 256-exit window — PEI/DXE + PciBus + ATAPI.
/// Nested VT-x `5d9e346`: BOTH-OK then n=8192 `ataio=0` `unh=3`
/// `port=0xcf8` (empty-slot walk + KBC). Nested VT-x `2674629`:
/// 32768 still `ataio=0` `acpi=16612` `port=0` (BdsWait).
pub const GUEST_UEFI_RESUME_CAP: u32 = 32768;

/// After DXE evidence, spend the rest of [`GUEST_UEFI_RESUME_CAP`] unless firmware
/// actually read an ATAPI sector. Nested VT-x `1b07692`: BOTH-OK at n=1111
/// then the private VMCS stopped with `sectors=0` — PciBus never reached PACKET.
pub const GUEST_UEFI_POST_DXE_TAIL: u32 = GUEST_UEFI_RESUME_CAP;

/// Pin-based VMX-preemption timer (SDM 24.6.1 bit 6). Lets a HPET Delay
/// that never does I/O still VMEXIT so [`hpet_tick_sink`] can move time.
#[cfg(target_os = "uefi")]
const PIN_BASED_VMX_PREEMPTION_TIMER: u32 = 1 << 6;
#[cfg(target_os = "uefi")]
const VMX_PREEMPTION_TIMER_VALUE: u64 = 0x482E;
#[cfg(target_os = "uefi")]
const VMX_PREEMPTION_TIMER_TICKS: u64 = 0x0010_0000;
#[cfg(target_os = "uefi")]
const EXIT_REASON_PREEMPTION_TIMER: u32 = 52;

/// Guest-UEFI HLT must skip/resume. Stopping on HLT aborts the post-DXE
/// PciBus walk of IDE `00:00.1`. Not a timer inject. Not ATAPI.
pub fn hlt_should_resume() -> bool {
    true
}

/// DebugLib `CpuDeadLoop` is `jmp rel8` −13 (`eb f3`) or `jmp $` (`eb fe`).
/// Nested VT-x `707a849`: 1s HPET left `rip=0x6e812d insn=ebf3…` `pci_ide=0`.
/// Do **not** skip every backward `jmp rel8` on I/O exits — iron COM2
/// skipped a retry then #UD-dumped COM1 until the cap (`pci_ide=0`).
/// Delay `jcc` on I/O stays.
pub fn spin_short_jmp_should_skip(b0: u8, b1: u8) -> bool {
    b0 == 0xEB && (b1 == 0xF3 || b1 == 0xFE)
}

/// Preemption-only CpuDeadLoop (iron `d5f9431` n=1280..8192 `reason=0x34`
/// `rip=0x6e81ca`, one tick `0x6e81b8`). Ubuntu OVMF `CpuDeadLoop` is
/// `for (;;) CpuPause()`. GCC `-O2` is often `pause` + `jmp rel8` −4
/// (`eb fc`); MSVC/near is `0F 84` rel32. `e2af81e` skipped only
/// `pause` / `jcc rel8` / `eb f3`/`eb fe`, so `eb fc` stayed stuck.
/// Not used on I/O exits (Delay `jcc` stays; never skip every
/// backward `jmp rel8` on I/O — that #UD-dumped COM1).
/// Two-byte match does **not** see `leave; ret` fallthrough — use
/// [`preempt_deadloop_skip_len`].
pub fn preempt_deadloop_should_skip(b0: u8, b1: u8) -> bool {
    if b0 == 0xF3 && b1 == 0x90 {
        return true;
    }
    if b0 == 0xEB || (0x70..=0x7F).contains(&b0) {
        let off = b1 as i8;
        if off <= -2 && off >= -32 {
            return true;
        }
    }
    false
}

/// Skipping this insn would land on `leave; ret` (`c9 c3`).
pub fn insn_fallthrough_is_leave_ret(bytes: &[u8], insn_len: usize) -> bool {
    bytes.len() >= insn_len + 2 && bytes[insn_len] == 0xC9 && bytes[insn_len + 1] == 0xC3
}

/// Iron `891eb5b`/`17449e2`: `jmp rel8` −20 (`eb ec`) then `leave; ret`.
/// QEMU CI `17449e2` failed BOTH/ATAPI: Ubuntu OVMF is `jmp rel8` −13
/// (`eb f3`) then `leave; ret` (`insn=ebf3c9c3` `rip=0x6e812d`). Nested
/// `1b07692` BOTH-OK needed that `eb f3` skip so PciBus could walk fn1.
/// Only the iron −20 encoding is the PE-header `#UD` ASSERT; keep `eb f3`.
pub fn preempt_deadloop_is_assert_epilogue(bytes: &[u8]) -> bool {
    bytes.len() >= 4 && bytes[0] == 0xEB && bytes[1] == 0xEC && bytes[2] == 0xC9 && bytes[3] == 0xC3
}

/// Bytes to advance guest RIP on a preemption CpuDeadLoop match.
/// 2: `pause` / backward `jmp rel8` / backward `jcc rel8` (including QEMU
///    `eb f3` + `leave; ret`).
/// 6: near `jcc` (`0F 8x` rel32) with a small backward displacement.
/// 0: unknown, or iron `eb ec` + `leave; ret` (PE-header `#UD` at `0x109D`).
pub fn preempt_deadloop_skip_len(bytes: &[u8]) -> u8 {
    if bytes.len() < 2 {
        return 0;
    }
    if preempt_deadloop_is_assert_epilogue(bytes) {
        return 0;
    }
    if preempt_deadloop_should_skip(bytes[0], bytes[1]) {
        return 2;
    }
    if bytes.len() >= 6 && bytes[0] == 0x0F && (0x80..=0x8F).contains(&bytes[1]) {
        let disp = i32::from_le_bytes([bytes[2], bytes[3], bytes[4], bytes[5]]);
        if disp <= -2 && disp >= -64 {
            return 6;
        }
    }
    0
}

/// XSETBV only accepts XCR0. Other XCRs would #GP; we skip those.
pub fn xsetbv_accepts_xcr(xcr: u32) -> bool {
    xcr == 0
}

/// Mask guest XSETBV XCR0 to host CPUID.0D:0. Bit 0 (x87) stays set.
/// AVX (bit 2) requires SSE (bit 1). Same rule as E4 `handle_xsetbv_and_resume`.
pub fn xsetbv_masked_xcr0(value: u64, host_mask: u64) -> u64 {
    let mut v = (value & host_mask) | 1;
    if v & 0x6 == 0x4 {
        v |= 0x2;
    }
    v
}

/// IA32_FEATURE_CONTROL for guest-UEFI: locked, VMX off.
/// Raw `rdmsr` was 0 (unlocked); OVMF CpuDxe then ASSERTs. Nested QEMU
/// already presents a locked MSR. Do not `#GP` firmware.
pub const GUEST_UEFI_FEATURE_CONTROL_VALUE: u64 = 1;

/// CPUID.1:EDX bit 28 — HTT / multi-thread package.
pub const CPUID_EDX_HTT: u32 = 1 << 28;

/// Filter guest-UEFI CPUID to a uniprocessor without VMX/x2APIC.
///
/// Iron `17449e2`: DXE CPUID at `rip=0x1cf11b5` then ASSERT `CpuDeadLoop`
/// `ret=0x6e8946` `rip=0x6e81ca`. Host passthrough advertised Xeon Silver
/// 4110 topology + VMX (16 threads). Nested QEMU is `-smp 1` without VMX
/// for L2, so BOTH-OK never hit this ASSERT.
pub fn guest_uefi_filter_cpuid(leaf: u32, subleaf: u32) -> CpuidRegs {
    let mut r = msr_firewall::filter_cpuid(leaf, subleaf);
    r.ecx &= !CPUID_ECX_X2APIC;
    match leaf {
        1 => {
            r.ebx = (r.ebx & 0xFFFF) | (1 << 16);
            r.edx &= !CPUID_EDX_HTT;
        }
        4 => r.eax &= !(0x3F << 26),
        0xB | 0x1F => {
            r.eax = 0;
            r.ebx = 0;
            r.ecx = subleaf & 0xFF;
            r.edx = 0;
        }
        _ => {}
    }
    r
}

/// Leaf 1 reports one logical processor and APIC ID 0.
pub fn guest_uefi_cpuid_leaf1_is_uniprocessor(ebx: u32, edx: u32) -> bool {
    ((ebx >> 16) & 0xFF) == 1 && (ebx >> 24) == 0 && (edx & CPUID_EDX_HTT) == 0
}

/// Stop the private VMCS after DXE once firmware read an ATAPI sector, or the tail is spent.
///
/// INVARIANTS:
/// - `false` until DXE printed (PEI still needs the full resume cap)
/// - `true` as soon as DXE printed **and** `sectors > 0` (honest PACKET READ)
/// - `true` after `GUEST_UEFI_POST_DXE_TAIL` exits past the DXE print (the 32768 cap)
/// - both PCI enums alone do **not** stop (Stage 43 `1b07692` n=1111 BOTH then
///   stopped with `sectors=0`; firmware never issued PACKET)
///
/// Nested VT-x: PEI only `inw` DID of `00:00.0`. IDE is virtio fn1 `00:00.1`.
pub fn post_dxe_should_stop(dxe_printed: bool, exit_n: u32, dxe_at: u32, sectors: u32) -> bool {
    if !dxe_printed {
        return false;
    }
    atapi_read_evidence(sectors) || exit_n.saturating_sub(dxe_at) >= GUEST_UEFI_POST_DXE_TAIL
}

/// Honest ATAPI evidence: firmware (or a host PACKET path) read a CD sector.
/// Do not fake. PCI enum is not a sector read.
pub fn atapi_read_evidence(sectors: u32) -> bool {
    sectors > 0
}

/// Firmware-simultaneous CD + disk: both PCI functions enumerated on one boot.
/// Do not fake. GuestVisible is not `ide_enum`.
pub fn both_pci_evidence(virtio_enum: bool, ide_enum: bool) -> bool {
    virtio_enum && ide_enum
}

/// Bitmask slot for bus 0 `dev.fun` (devs 0–31). Used to log a CF8 select once.
pub fn pci_bdf_bit(dev: u8, fun: u8) -> Option<(usize, u64)> {
    if fun > 7 || dev > 31 {
        return None;
    }
    let idx = u32::from(dev) * 8 + u32::from(fun);
    Some(((idx / 64) as usize, 1u64 << (idx % 64)))
}

/// OVMF SEC on 4M CODE does `mov eax,0x640; mov cr4,eax` (clears VMXE).
/// Same #GP as Linux `startup_64` without the E4 CR4.VMXE mask.
pub const E5_OVMF_SEC_CR4_VALUE: u64 = 0x640;

/// CR4.VMXE — host-owned so SEC `mov cr4,0x640` does not #GP.
pub const GUEST_UEFI_CR4_VMXE: u64 = 1 << 13;
/// CR4.OSXSAVE — host-owned so CpuDxe `mov cr4,0x668` cannot clear it.
/// Iron `0ca02e6`: dump CR4=0x668 (no bit 18), then XSAVE `#UD` RIP `0x109D`.
pub const GUEST_UEFI_CR4_OSXSAVE: u64 = 1 << 18;
/// Guest-UEFI CR4 bits the host keeps set across MOV CR4.
pub const GUEST_UEFI_CR4_HOST_OWNED: u64 = GUEST_UEFI_CR4_VMXE | GUEST_UEFI_CR4_OSXSAVE;

/// Apply a guest MOV-to-CR4: keep VMXE+OSXSAVE set (SDM 28.2.1 mask).
pub fn apply_guest_cr4_write(requested: u64) -> u64 {
    (requested & !GUEST_UEFI_CR4_HOST_OWNED) | GUEST_UEFI_CR4_HOST_OWNED
}

/// CR4 read shadow: guest must not see VMXE; guest must see OSXSAVE.
pub fn guest_cr4_read_shadow(guest_cr4: u64) -> u64 {
    guest_cr4 & !GUEST_UEFI_CR4_VMXE
}

/// Strip 66/67/F0/F2/F3 and REX so host tests can match XSAVE/UD2 encodings.
pub fn skip_x86_legacy_prefixes(bytes: &[u8]) -> &[u8] {
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            0x66 | 0x67 | 0xF0 | 0xF2 | 0xF3 | 0x40..=0x4F => i += 1,
            _ => break,
        }
    }
    &bytes[i..]
}

/// `#UD` that is XSAVE/XRSTOR/XSAVEOPT/XSAVEC/XSAVES/XRSTORS (needs OSXSAVE).
/// Not FXSAVE (`0F AE /0`) — that uses OSFXSR.
pub fn ud_xsave_family(bytes: &[u8]) -> bool {
    let b = skip_x86_legacy_prefixes(bytes);
    if b.len() < 3 || b[0] != 0x0F {
        return false;
    }
    let reg = (b[2] >> 3) & 7;
    match b[1] {
        0xAE => reg == 4 || reg == 5 || reg == 6,
        0xC7 => reg == 3 || reg == 4 || reg == 5,
        _ => false,
    }
}

/// ASSERT / DebugLib `ud2` (`0F 0B`).
pub fn ud_is_ud2(bytes: &[u8]) -> bool {
    let b = skip_x86_legacy_prefixes(bytes);
    b.len() >= 2 && b[0] == 0x0F && b[1] == 0x0B
}

/// I/O exit qualification → port (SDM 28.2.1 bits 31:16).
pub fn io_port_from_qual(qual: u64) -> u16 {
    ((qual >> 16) & 0xffff) as u16
}

/// Linear RIP has left the last 64 KiB (typical OVMF 4M SEC/VTF window).
pub fn linear_left_sec_tail(linear: u64) -> bool {
    linear < GUEST_UEFI_SEC_TAIL_GPA
}

pub fn is_pci_config_port(port: u16) -> bool {
    port == 0xCF8 || port == 0xCFC
}

pub fn is_com_uart_port(port: u16) -> bool {
    (0x03F8..=0x03FF).contains(&port) || (0x02F8..=0x02FF).contains(&port)
}

/// Honest past-SEC: left the SEC tail and saw PEI PCI, firmware COM, or HLT.
pub fn past_sec_evidence(
    left_sec: bool,
    pci_config: bool,
    com_bytes: u32,
    guest_hlt: bool,
) -> bool {
    left_sec && (pci_config || com_bytes > 0 || guest_hlt)
}

/// Linear RIP is executing from guest-UEFI low RAM (PEI relocated / DXE).
pub fn exec_from_low_ram(linear: u64) -> bool {
    linear < GUEST_UEFI_LOW_RAM_BYTES
}

/// Honest past-PEI/DXE or a guest CD boot attempt (ATAPI sector read).
///
/// Platform CMOS/fw_cfg alone is not enough. Need a CD READ or firmware
/// executing from the low-RAM window after past-SEC.
pub fn dxe_or_cd_boot_evidence(
    past_sec: bool,
    sectors: u32,
    platform_mem: bool,
    exec_ram: bool,
) -> bool {
    past_sec && (sectors > 0 || (platform_mem && exec_ram))
}

/// Copy up to `out.len()` bytes of guest-UEFI low RAM at identity `linear`.
///
/// Used to dump the `#GP` instruction (nested VT-x `5b2739a` `rip=0x80201a`)
/// and the stop RIP (nested VT-x `105ffbe` `rip=0x6e812d` HPET poll).
/// PEI runs with paging off / identity, so GPA = linear.
pub fn copy_low_ram_at(ram: &[u8], linear: u64, out: &mut [u8]) -> usize {
    let start = linear as usize;
    if out.is_empty() || start >= ram.len() {
        return 0;
    }
    let n = out.len().min(ram.len() - start);
    out[..n].copy_from_slice(&ram[start..start + n]);
    n
}

static LAUNCH_ENTERED: AtomicBool = AtomicBool::new(false);
static MARKER_PRINTED: AtomicBool = AtomicBool::new(false);
static LAST_EXIT_REASON: AtomicU32 = AtomicU32::new(0);
static LAST_GUEST_RIP: AtomicU64 = AtomicU64::new(0);
static LAST_LINEAR: AtomicU64 = AtomicU64::new(0);
static LAST_GUEST_PHYS: AtomicU64 = AtomicU64::new(0);
static LAST_INSN_ERROR: AtomicU32 = AtomicU32::new(0);
static EXIT_COUNT: AtomicU32 = AtomicU32::new(0);
static NON_TF_EXITS: AtomicU32 = AtomicU32::new(0);
static ALIVE_PRINTED: AtomicBool = AtomicBool::new(false);
static PAST_SEC_PRINTED: AtomicBool = AtomicBool::new(false);
static LEFT_SEC: AtomicBool = AtomicBool::new(false);
static PCI_CONFIG_SEEN: AtomicBool = AtomicBool::new(false);
static COM_BYTES: AtomicU32 = AtomicU32::new(0);
static COM_BANNER: AtomicBool = AtomicBool::new(false);
static UART_LCR_COM1: AtomicU8 = AtomicU8::new(0);
static UART_LCR_COM2: AtomicU8 = AtomicU8::new(0);
static CONTINUE_GUEST: AtomicBool = AtomicBool::new(false);
static DXE_PRINTED: AtomicBool = AtomicBool::new(false);
static BOTH_PRINTED: AtomicBool = AtomicBool::new(false);
static ATAPI_PRINTED: AtomicBool = AtomicBool::new(false);
static DXE_AT_N: AtomicU32 = AtomicU32::new(0);
static EPT_PML4: AtomicU64 = AtomicU64::new(0);
static SINK_HPA: AtomicU64 = AtomicU64::new(0);
static SINK_MAPS: AtomicU32 = AtomicU32::new(0);
static PCI_DID_TRACE: AtomicU32 = AtomicU32::new(0);
static PCI_HT_TRACE: AtomicU32 = AtomicU32::new(0);
static PCI_BAR_TRACE: AtomicU32 = AtomicU32::new(0);
static HLT_SKIPS: AtomicU32 = AtomicU32::new(0);
static SPIN_JMP_SKIPS: AtomicU32 = AtomicU32::new(0);
static LAST_PREEMPT_RIP: AtomicU64 = AtomicU64::new(u64::MAX);
static PREEMPT_SAME_RIP: AtomicU32 = AtomicU32::new(0);
static CR_ACCESSES: AtomicU32 = AtomicU32::new(0);
static PCI_BDF_SEEN0: AtomicU64 = AtomicU64::new(0);
static PCI_BDF_SEEN1: AtomicU64 = AtomicU64::new(0);
static PCI_BDF_SEEN2: AtomicU64 = AtomicU64::new(0);
static PCI_BDF_SEEN3: AtomicU64 = AtomicU64::new(0);
static LAST_IO_PORT: AtomicU32 = AtomicU32::new(0);
static LAST_CF8: AtomicU32 = AtomicU32::new(0);
static RAM_HPA: AtomicU64 = AtomicU64::new(0);
static RAM_REMAP_N: AtomicU32 = AtomicU32::new(0);
static RAM_REMAP_TRIES: AtomicU32 = AtomicU32::new(0);
static HPET_TICKS: AtomicU32 = AtomicU32::new(0);
static PREEMPT_RELOAD: AtomicU32 = AtomicU32::new(0);
static IO_UNHANDLED_N: AtomicU32 = AtomicU32::new(0);
#[cfg(target_os = "uefi")]
static XSETBV_N: AtomicU32 = AtomicU32::new(0);
#[cfg(target_os = "uefi")]
static UD_XSAVE_RETRY: AtomicU32 = AtomicU32::new(0);
#[cfg(target_os = "uefi")]
static UD2_SKIPS: AtomicU32 = AtomicU32::new(0);
#[cfg(target_os = "uefi")]
static ASSERT_DEADLOOP_DUMP: AtomicU32 = AtomicU32::new(0);
#[cfg(target_os = "uefi")]
static MSR_RD_TRACE: AtomicU32 = AtomicU32::new(0);

#[cfg(target_os = "uefi")]
static mut SAVED_RAX: u64 = 0;
#[cfg(target_os = "uefi")]
static mut SAVED_RBX: u64 = 0;
#[cfg(target_os = "uefi")]
static mut SAVED_RCX: u64 = 0;
#[cfg(target_os = "uefi")]
static mut SAVED_RDX: u64 = 0;
#[cfg(target_os = "uefi")]
static mut SAVED_RSI: u64 = 0;
#[cfg(target_os = "uefi")]
static mut SAVED_RDI: u64 = 0;
#[cfg(target_os = "uefi")]
static mut SAVED_RBP: u64 = 0;
#[cfg(target_os = "uefi")]
static mut SAVED_R8: u64 = 0;
#[cfg(target_os = "uefi")]
static mut SAVED_R9: u64 = 0;
#[cfg(target_os = "uefi")]
static mut SAVED_R10: u64 = 0;
#[cfg(target_os = "uefi")]
static mut SAVED_R11: u64 = 0;
#[cfg(target_os = "uefi")]
static mut SAVED_R12: u64 = 0;
#[cfg(target_os = "uefi")]
static mut SAVED_R13: u64 = 0;
#[cfg(target_os = "uefi")]
static mut SAVED_R14: u64 = 0;
#[cfg(target_os = "uefi")]
static mut SAVED_R15: u64 = 0;

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

/// QEMU/OVMF 4 MiB pflash base (`OVMF.fd` VARS at `0xFFC00000`, CODE on top).
pub const GUEST_UEFI_FLASH_BASE: u64 = 0xFFC0_0000;
/// Full flash window. Nested VT-x `1991a27` EPT-faulted at `gpa=0xffc00000`
/// because a CODE-only image was top-aligned at `0xFFC84000` (VARS gap).
pub const GUEST_UEFI_FLASH_WINDOW: u64 = 4 * 1024 * 1024;

/// Map any 1–4 MiB retained image into a 4 MiB window at [`GUEST_UEFI_FLASH_BASE`].
/// Pad is leading erased flash (`0xFF`); [`stamp_empty_ovmf_vars`] writes a
/// VARS `_FVH` when the pad is Debian 4M sized. Reset vector stays at `0xFFFF_FFF0`.
pub fn flash_window_gpa_and_pad(image_len: u64) -> Option<(u64, u64)> {
    const MIN: u64 = MIN_REAL_OVMF_BYTES as u64;
    if image_len < MIN || image_len > GUEST_UEFI_FLASH_WINDOW {
        return None;
    }
    Some((GUEST_UEFI_FLASH_BASE, GUEST_UEFI_FLASH_WINDOW - image_len))
}

/// Debian/QEMU 4M VARS firmware-volume size (`OVMF_VARS_4M.fd`).
pub const OVMF_VARS_FV_BYTES: usize = 0x84000;

/// Empty NV storage prefix: `EFI_FIRMWARE_VOLUME_HEADER` + authenticated
/// `VARIABLE_STORE_HEADER`. Byte-identical to the first 0x64 bytes of
/// Debian `OVMF_VARS_4M.fd`. Remainder of the FV is erased NOR (`0xFF`).
///
/// FileSystemGuid is `EFI_SYSTEM_NV_DATA_FV_GUID`. Store signature is
/// `gEfiAuthenticatedVariableGuid`. Format `0x5A` / State `0xFE` (healthy).
pub const OVMF_VARS_EMPTY_PREFIX: [u8; 0x64] = [
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x8d, 0x2b, 0xf1, 0xff, 0x96, 0x76, 0x8b, 0x4c, 0xa9, 0x85, 0x27, 0x47, 0x07, 0x5b, 0x4f, 0x50,
    0x00, 0x40, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x5f, 0x46, 0x56, 0x48, 0xff, 0xfe, 0x04, 0x00,
    0x48, 0x00, 0xaf, 0xb8, 0x00, 0x00, 0x00, 0x02, 0x84, 0x00, 0x00, 0x00, 0x00, 0x10, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x78, 0x2c, 0xf3, 0xaa, 0x7b, 0x94, 0x9a, 0x43,
    0xa1, 0x80, 0x2e, 0x14, 0x4e, 0xc3, 0x77, 0x92, 0xb8, 0xff, 0x03, 0x00, 0x5a, 0xfe, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00,
];

/// Stamp an empty VARS firmware volume into a CODE-only 4 MiB pad.
///
/// INVARIANTS:
/// - Writes only when `pad.len() == OVMF_VARS_FV_BYTES` (Debian 4M CODE-only)
/// - Prefix is `_FVH` + empty authenticated variable store
/// - Does not touch CODE (caller copies CODE after this)
/// - Does not fake PCI enum
///
/// VERIFICATION: L1 (host tests)
pub fn stamp_empty_ovmf_vars(pad: &mut [u8]) -> bool {
    if pad.len() != OVMF_VARS_FV_BYTES {
        return false;
    }
    let n = OVMF_VARS_EMPTY_PREFIX.len();
    pad[..n].copy_from_slice(&OVMF_VARS_EMPTY_PREFIX);
    true
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
    LAST_LINEAR.store(0, Ordering::Release);
    LAST_GUEST_PHYS.store(0, Ordering::Release);
    LAST_INSN_ERROR.store(0, Ordering::Release);
    EXIT_COUNT.store(0, Ordering::Release);
    NON_TF_EXITS.store(0, Ordering::Release);
    ALIVE_PRINTED.store(false, Ordering::Release);
    PAST_SEC_PRINTED.store(false, Ordering::Release);
    LEFT_SEC.store(false, Ordering::Release);
    PCI_CONFIG_SEEN.store(false, Ordering::Release);
    COM_BYTES.store(0, Ordering::Release);
    COM_BANNER.store(false, Ordering::Release);
    UART_LCR_COM1.store(0, Ordering::Release);
    UART_LCR_COM2.store(0, Ordering::Release);
    CONTINUE_GUEST.store(false, Ordering::Release);
    DXE_PRINTED.store(false, Ordering::Release);
    BOTH_PRINTED.store(false, Ordering::Release);
    ATAPI_PRINTED.store(false, Ordering::Release);
    DXE_AT_N.store(0, Ordering::Release);
    EPT_PML4.store(0, Ordering::Release);
    SINK_HPA.store(0, Ordering::Release);
    SINK_MAPS.store(0, Ordering::Release);
    PCI_DID_TRACE.store(0, Ordering::Release);
    PCI_HT_TRACE.store(0, Ordering::Release);
    PCI_BAR_TRACE.store(0, Ordering::Release);
    HLT_SKIPS.store(0, Ordering::Release);
    SPIN_JMP_SKIPS.store(0, Ordering::Release);
    LAST_PREEMPT_RIP.store(u64::MAX, Ordering::Release);
    PREEMPT_SAME_RIP.store(0, Ordering::Release);
    CR_ACCESSES.store(0, Ordering::Release);
    PCI_BDF_SEEN0.store(0, Ordering::Release);
    PCI_BDF_SEEN1.store(0, Ordering::Release);
    PCI_BDF_SEEN2.store(0, Ordering::Release);
    PCI_BDF_SEEN3.store(0, Ordering::Release);
    LAST_IO_PORT.store(0, Ordering::Release);
    LAST_CF8.store(0, Ordering::Release);
    RAM_HPA.store(0, Ordering::Release);
    RAM_REMAP_N.store(0, Ordering::Release);
    RAM_REMAP_TRIES.store(0, Ordering::Release);
    HPET_TICKS.store(0, Ordering::Release);
    PREEMPT_RELOAD.store(0, Ordering::Release);
    IO_UNHANDLED_N.store(0, Ordering::Release);
    crate::devices::guest_platform::reset();
    crate::devices::guest_virtio_blk::reset();
}

/// Exits after a successful entry that were not triple-fault / VM-entry fail.
pub fn guest_uefi_non_tf_exits() -> u32 {
    NON_TF_EXITS.load(Ordering::Acquire)
}

pub fn guest_uefi_alive() -> bool {
    ALIVE_PRINTED.load(Ordering::Acquire)
}

pub fn guest_uefi_past_sec() -> bool {
    PAST_SEC_PRINTED.load(Ordering::Acquire)
}

pub fn guest_uefi_com_bytes() -> u32 {
    COM_BYTES.load(Ordering::Acquire)
}

pub fn guest_uefi_dxe() -> bool {
    DXE_PRINTED.load(Ordering::Acquire)
}

pub fn guest_uefi_both() -> bool {
    BOTH_PRINTED.load(Ordering::Acquire)
}

pub fn guest_uefi_atapi() -> bool {
    ATAPI_PRINTED.load(Ordering::Acquire)
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
    let Some((gpa, _pad)) = flash_window_gpa_and_pad(bytes.len() as u64) else {
        return Err(GuestUefiLaunchError::LaunchSetupFailed);
    };
    if !alias_ept_covers_reset(gpa, GUEST_UEFI_FLASH_WINDOW) {
        return Err(GuestUefiLaunchError::LaunchSetupFailed);
    }
    let map_len = GUEST_UEFI_FLASH_WINDOW;

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
    let pad = (fw_len as usize).saturating_sub(bytes.len());
    // SAFETY: exclusive 4 MiB guest-private flash copy; pad is in-range.
    // KANI-TARGET: pad CODE-only OVMF into 4MiB window (outside Proven Core).
    unsafe {
        core::ptr::write_bytes(fw_hpa as *mut u8, 0xFF, (pages * 4096) as usize);
    }
    // Empty VARS `_FVH` at 0xFFC00000 so PEI does not parse erased NOR.
    // Matches Debian OVMF_VARS_4M.fd prefix; remainder stays 0xFF.
    let vars_n = if pad > 0 {
        // SAFETY: pad bytes are the leading range of the exclusive 4 MiB copy.
        // KANI-TARGET: stamp empty VARS into CODE-only pad (outside Proven Core).
        let pad_slice = unsafe { core::slice::from_raw_parts_mut(fw_hpa as *mut u8, pad) };
        u32::from(stamp_empty_ovmf_vars(pad_slice))
    } else {
        0
    };
    // SAFETY: CODE image fits after `pad` in the exclusive 4 MiB copy.
    // KANI-TARGET: copy CODE-only OVMF after VARS pad (outside Proven Core).
    unsafe {
        core::ptr::copy_nonoverlapping(bytes.as_ptr(), (fw_hpa as *mut u8).add(pad), bytes.len());
    }
    // SAFETY: exclusive guest-private firmware copy; the retain buffer is
    // a different range. Remap only this image so OVMF's host-bridge
    // switch matches virtio DID `0x1042` as i440FX-class.
    // KANI-TARGET: remap guest-private OVMF copy (outside Proven Core).
    let remap_n = crate::boot::ovmf_esp::remap_i440fx_did_imm(unsafe {
        core::slice::from_raw_parts_mut((fw_hpa as *mut u8).add(pad), bytes.len())
    });
    serial::write_str("boot: guest-UEFI 4MiB flash pad=0x");
    write_hex(pad as u64);
    serial::write_byte(b'\n');
    serial::write_str("boot: guest-UEFI empty VARS _FVH n=");
    write_dec(vars_n as u64);
    serial::write_byte(b'\n');
    serial::write_str("boot: guest-UEFI ovmf remap i440FX DID->virtio n=");
    write_dec(remap_n as u64);
    serial::write_byte(b'\n');

    let ram_pages = GUEST_UEFI_LOW_RAM_BYTES / 4096;
    let Some(ram_frame) = alloc.allocate_contiguous_aligned(ram_pages, 512) else {
        serial::write_line("boot: guest-UEFI no 32MiB RAM slab");
        return Err(GuestUefiLaunchError::LaunchSetupFailed);
    };
    let ram_hpa = ram_frame.to_phys();
    core::ptr::write_bytes(ram_hpa as *mut u8, 0, GUEST_UEFI_LOW_RAM_BYTES as usize);
    RAM_HPA.store(ram_hpa, Ordering::Release);

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
        || !ept_hw::gpa_is_mapped(pml4, GUEST_UEFI_LOW_RAM_BYTES - 4096)
    {
        serial::write_line("boot: guest-UEFI alias EPT walk failed");
        return Err(GuestUefiLaunchError::LaunchSetupFailed);
    }
    crate::devices::guest_platform::reset();
    EPT_PML4.store(pml4, Ordering::Release);
    if let Some(sink_frame) = alloc.allocate_contiguous_aligned(512, 512) {
        let sink_hpa = sink_frame.to_phys();
        core::ptr::write_bytes(sink_hpa as *mut u8, 0, 2 * 1024 * 1024);
        // SAFETY: exclusive 2 MiB sink; HPET sits at 0xFED00000 in this page.
        // KANI-TARGET: live HPET in guest-UEFI sink (outside Proven Core).
        let hpet_ok = crate::devices::guest_platform::hpet_init_sink(unsafe {
            core::slice::from_raw_parts_mut(sink_hpa as *mut u8, 2 * 1024 * 1024)
        });
        SINK_HPA.store(sink_hpa, Ordering::Release);
        for &mm in &[0xFCE0_0000u64, 0xFEC0_0000, 0xFED0_0000, 0xFEE0_0000] {
            if ept_map_2m_sink(mm) {
                SINK_MAPS.fetch_add(1, Ordering::AcqRel);
            }
        }
        serial::write_str("boot: guest-UEFI platform sink_hpa=0x");
        write_hex(sink_hpa);
        serial::write_str(" maps=");
        write_dec(SINK_MAPS.load(Ordering::Acquire) as u64);
        serial::write_str(" live HPET=");
        write_dec(hpet_ok as u64);
        serial::write_byte(b'\n');
    } else {
        serial::write_line("boot: guest-UEFI no 2MiB platform sink — CMOS/fw_cfg still live");
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

    if crate::devices::ide_cdrom::present_placeholder_if_idle() {
        serial::write_str("boot: guest-UEFI CD GuestVisible iso=");
        write_dec(crate::devices::ide_cdrom::retained_iso_id());
        serial::write_str(" bytes=");
        write_dec(crate::devices::ide_cdrom::retained_len() as u64);
        serial::write_byte(b'\n');
    }
    if crate::devices::guest_virtio_blk::present() {
        serial::write_line("boot: guest-UEFI virtio-blk empty CD→disk order");
    }

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

    let pin = adjust_vmx_controls(
        PIN_BASED_EXTERNAL_INTERRUPT_EXITING | PIN_BASED_VMX_PREEMPTION_TIMER,
        pin_msr,
    );
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
    // Iron COM2: host CPUID advertises INVPCID/XSAVES/RDTSCP (Xeon Silver
    // 4110); OVMF then executes them. Without these bits the insn #UD at
    // RIP 0x109D, DebugLib dumps COM1, CpuDeadLoop, pci_ide=0. Nested
    // QEMU CPUID often lacks them, so BOTH-OK never saw this. Same bits
    // as the E4 SHELL VMCS (launch.rs).
    let secondary = adjust_vmx_controls(
        SECONDARY_ENABLE_EPT
            | GUEST_UEFI_UNRESTRICTED_GUEST
            | SECONDARY_ENABLE_RDTSCP
            | SECONDARY_ENABLE_INVPCID
            | SECONDARY_ENABLE_XSAVES,
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
    if secondary & SECONDARY_ENABLE_RDTSCP == 0 {
        serial::write_line("boot: guest-UEFI WARN — RDTSCP not allowed; OVMF may #UD");
    }
    if secondary & SECONDARY_ENABLE_INVPCID == 0 {
        serial::write_line("boot: guest-UEFI WARN — INVPCID not allowed; OVMF may #UD");
    }
    if secondary & SECONDARY_ENABLE_XSAVES == 0 {
        serial::write_line("boot: guest-UEFI WARN — XSAVES not allowed; OVMF may #UD");
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
    let guest_cr4 = apply_guest_cr4_write(guest_cr4_real());

    prepare_vmcs_region(vmcs)?;
    ops::vmclear(vmcs).map_err(|_| LaunchError::ClearFailed)?;
    prepare_vmcs_region(vmcs)?;

    let host_rip = guest_uefi_vmexit_landing as *const () as u64;

    match ops::vmptrld_and_vmwrite(vmcs, VMCS_LINK_POINTER, !0u64) {
        Ok(()) => {}
        Err(_) => {
            return Err(LaunchError::VmwriteFailed {
                field: VMCS_LINK_POINTER,
            })
        }
    }

    vw(PIN_BASED_VM_EXEC_CONTROL, pin as u64)?;
    if pin & PIN_BASED_VMX_PREEMPTION_TIMER != 0 {
        vw(VMX_PREEMPTION_TIMER_VALUE, VMX_PREEMPTION_TIMER_TICKS)?;
        PREEMPT_RELOAD.store(VMX_PREEMPTION_TIMER_TICKS as u32, Ordering::Release);
        serial::write_line("boot: guest-UEFI VMX preemption timer for live HPET");
    }
    vw(PRIMARY_PROC_BASED_VM_EXEC_CONTROL, primary as u64)?;
    vw(VM_EXIT_CONTROLS, exit_ctls as u64)?;
    vw(VM_ENTRY_CONTROLS, entry_ctls as u64)?;
    // Catch #UD/#DF/#GP/#PF. #UD: iron 0ca02e6 XSAVE without OSXSAVE
    // dumped COM1 in the guest; intercept so we can set OSXSAVE and retry.
    const UEFI_EXC_BITMAP: u32 = (1 << 6) | (1 << 8) | (1 << 13) | (1 << 14);
    vw(EXCEPTION_BITMAP, UEFI_EXC_BITMAP as u64)?;
    vw(PAGE_FAULT_ERROR_CODE_MASK, 0)?;
    vw(PAGE_FAULT_ERROR_CODE_MATCH, 0)?;
    vw(CR3_TARGET_COUNT, 0)?;
    vw(VM_EXIT_MSR_STORE_COUNT, 0)?;
    vw(VM_EXIT_MSR_LOAD_COUNT, 0)?;
    vw(VM_ENTRY_MSR_LOAD_COUNT, 0)?;
    vw(VM_ENTRY_INTERRUPTION_INFO, 0)?;
    vw(CR0_GUEST_HOST_MASK, 0)?;
    // Host-own CR4.VMXE (E4 Linux path) and CR4.OSXSAVE (iron 0ca02e6).
    // OVMF SEC `mov cr4, 0x640` clears VMXE; CpuDxe `mov cr4, 0x668` clears OSXSAVE.
    vw(CR4_GUEST_HOST_MASK, GUEST_UEFI_CR4_HOST_OWNED)?;
    vw(CR0_READ_SHADOW, 0)?;
    vw(CR4_READ_SHADOW, guest_cr4_read_shadow(guest_cr4))?;
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

/// HOST_RIP trampoline — save guest GPRs before Rust clobbers them.
#[cfg(target_os = "uefi")]
#[unsafe(naked)]
pub unsafe extern "C" fn guest_uefi_vmexit_landing() -> ! {
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
        slot_rax = sym SAVED_RAX,
        slot_rbx = sym SAVED_RBX,
        slot_rcx = sym SAVED_RCX,
        slot_rdx = sym SAVED_RDX,
        slot_rsi = sym SAVED_RSI,
        slot_rdi = sym SAVED_RDI,
        slot_rbp = sym SAVED_RBP,
        slot_r8 = sym SAVED_R8,
        slot_r9 = sym SAVED_R9,
        slot_r10 = sym SAVED_R10,
        slot_r11 = sym SAVED_R11,
        slot_r12 = sym SAVED_R12,
        slot_r13 = sym SAVED_R13,
        slot_r14 = sym SAVED_R14,
        slot_r15 = sym SAVED_R15,
        cont = sym guest_uefi_vmexit,
    );
}

#[cfg(target_os = "uefi")]
#[unsafe(naked)]
unsafe extern "C" fn guest_uefi_vmresume() -> ! {
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
        slot_rax = sym SAVED_RAX,
        slot_rbx = sym SAVED_RBX,
        slot_rcx = sym SAVED_RCX,
        slot_rdx = sym SAVED_RDX,
        slot_rsi = sym SAVED_RSI,
        slot_rdi = sym SAVED_RDI,
        slot_rbp = sym SAVED_RBP,
        slot_r8 = sym SAVED_R8,
        slot_r9 = sym SAVED_R9,
        slot_r10 = sym SAVED_R10,
        slot_r11 = sym SAVED_R11,
        slot_r12 = sym SAVED_R12,
        slot_r13 = sym SAVED_R13,
        slot_r14 = sym SAVED_R14,
        slot_r15 = sym SAVED_R15,
        fail = sym guest_uefi_resume_failed,
    );
}

#[cfg(target_os = "uefi")]
unsafe extern "C" fn guest_uefi_resume_failed() -> ! {
    serial::write_line("boot: guest-UEFI VMRESUME failed — continuing E4 SHELL");
    leave_to_e4();
}

#[cfg(target_os = "uefi")]
fn tick_hpet_on_exit(basic: u32, gpa: u64, qual: u64) {
    let sink = SINK_HPA.load(Ordering::Acquire);
    if sink == 0 {
        return;
    }
    let acpi_io = basic == EXIT_REASON_IO_INSTRUCTION && {
        let port = io_port_from_qual(qual);
        let size = ((qual & 7) + 1) as u8;
        crate::devices::guest_platform::is_acpi_pm_timer_io(port, size)
    };
    let step = if basic == EXIT_REASON_PREEMPTION_TIMER
        || basic == EXIT_REASON_HLT
        || acpi_io
        || (basic == EXIT_REASON_EPT_VIOLATION && crate::devices::guest_platform::is_hpet_gpa(gpa))
    {
        crate::devices::guest_platform::HPET_MAIN_STEP
    } else {
        0
    };
    // SAFETY: 2 MiB exclusive sink allocated at launch.
    // KANI-TARGET: HPET tick in guest-UEFI sink (outside Proven Core).
    let v = crate::devices::guest_platform::hpet_tick_sink_by(
        unsafe { core::slice::from_raw_parts_mut(sink as *mut u8, 2 * 1024 * 1024) },
        step,
    );
    if step != 0 && v != 0 {
        HPET_TICKS.fetch_add(1, Ordering::AcqRel);
    }
}

/// HOST_RIP continuation for the private guest-UEFI VMCS. Not the E4 SHELL landing.
#[cfg(target_os = "uefi")]
pub unsafe extern "C" fn guest_uefi_vmexit() -> ! {
    LAUNCH_ENTERED.store(true, Ordering::Release);
    let reason = ops::vmread(EXIT_REASON).unwrap_or(0xFFFF) as u32;
    let qual = ops::vmread(EXIT_QUALIFICATION).unwrap_or(0);
    let rip = ops::vmread(GUEST_RIP).unwrap_or(0);
    let cs_base = ops::vmread(GUEST_CS_BASE).unwrap_or(0);
    let gpa = ops::vmread(GUEST_PHYSICAL_ADDRESS).unwrap_or(0);
    let intr = ops::vmread(VM_EXIT_INTR_INFO).unwrap_or(0);
    tick_hpet_on_exit(reason & 0xFFFF, gpa, qual);
    LAST_EXIT_REASON.store(reason, Ordering::Release);
    LAST_GUEST_RIP.store(rip, Ordering::Release);
    LAST_GUEST_PHYS.store(gpa, Ordering::Release);

    let n = EXIT_COUNT.fetch_add(1, Ordering::AcqRel) + 1;
    let basic = reason & 0xFFFF;
    let entry_fail = (reason & 0x8000_0000) != 0
        || basic == EXIT_REASON_VMENTRY_GUEST_STATE
        || basic == EXIT_REASON_VMENTRY_MSR_LOAD;
    let tf = basic == EXIT_REASON_TRIPLE_FAULT;
    let fetch_fail = basic == EXIT_REASON_EPT_VIOLATION && gpa == GUEST_UEFI_RESET_VECTOR_GPA;
    let linear = cs_base.wrapping_add(rip);
    LAST_LINEAR.store(linear, Ordering::Release);
    if linear_left_sec_tail(linear) {
        LEFT_SEC.store(true, Ordering::Release);
    }
    if basic == EXIT_REASON_PREEMPTION_TIMER {
        let prev = LAST_PREEMPT_RIP.swap(rip, Ordering::AcqRel);
        if prev == rip {
            PREEMPT_SAME_RIP.fetch_add(1, Ordering::AcqRel);
        } else {
            PREEMPT_SAME_RIP.store(1, Ordering::Release);
        }
    }

    if n <= 16 {
        serial::write_str("boot: guest-UEFI VMEXIT n=");
        write_dec(n as u64);
        serial::write_str(" reason=0x");
        write_hex_u32(reason);
        serial::write_str(" rip=0x");
        write_hex(rip);
        serial::write_str(" cs_base=0x");
        write_hex(cs_base);
        serial::write_str(" qual=0x");
        write_hex(qual);
        serial::write_str(" gpa=0x");
        write_hex(gpa);
        if basic == EXIT_REASON_EXCEPTION_NMI {
            serial::write_str(" intr=0x");
            write_hex(intr);
        }
        serial::write_byte(b'\n');
    } else if n % 256 == 0 {
        serial::write_str("boot: guest-UEFI tick n=");
        write_dec(n as u64);
        serial::write_str(" reason=0x");
        write_hex_u32(reason);
        serial::write_str(" rip=0x");
        write_hex(rip);
        serial::write_str(" hpet=");
        write_dec(HPET_TICKS.load(Ordering::Acquire) as u64);
        serial::write_str(" pci_ide=");
        write_dec(crate::devices::ide_cdrom::pci_enumerated() as u64);
        serial::write_str(" ataio=");
        write_dec(crate::devices::ide_cdrom::ata_io_accesses() as u64);
        serial::write_str(" spin=");
        write_dec(SPIN_JMP_SKIPS.load(Ordering::Acquire) as u64);
        serial::write_str(" same=");
        write_dec(PREEMPT_SAME_RIP.load(Ordering::Acquire) as u64);
        serial::write_str(" insn=");
        dump_low_ram_insn(linear);
        serial::write_byte(b'\n');
    }

    if !entry_fail && !fetch_fail {
        if !MARKER_PRINTED.swap(true, Ordering::AcqRel) {
            serial::write_line(M7_E5_OVMF_VMLAUNCH_OK_MARKER);
            serial::write_str("boot: guest-UEFI linear=0x");
            write_hex(linear);
            serial::write_byte(b'\n');
        }
        if n == 1 {
            audit_log!(AuditEvent::OvmfGuestUefiVmlaunched {
                exit_reason: reason as u64,
                guest_rip: rip,
            });
        }
        if !tf {
            let nt = NON_TF_EXITS.fetch_add(1, Ordering::AcqRel) + 1;
            if nt >= 2 || basic == EXIT_REASON_HLT {
                maybe_print_alive(basic);
            }
            maybe_print_past_sec(basic == EXIT_REASON_HLT);
        }
    } else {
        serial::write_line("boot: guest-UEFI VM-entry/fetch failed — marker not claimed");
    }

    let mut resume = false;
    if !entry_fail && !tf && !fetch_fail && n < GUEST_UEFI_RESUME_CAP {
        resume = match basic {
            EXIT_REASON_IO_INSTRUCTION => handle_io(qual),
            EXIT_REASON_CPUID => handle_cpuid(),
            EXIT_REASON_MSR_READ => handle_rdmsr(),
            EXIT_REASON_MSR_WRITE => handle_wrmsr(),
            EXIT_REASON_HLT => {
                maybe_print_alive(basic);
                maybe_print_past_sec(true);
                // Skip, do not stop: a firmware HLT would otherwise cut the
                // post-DXE tail before PciBus walks IDE `00:00.1`.
                if hlt_should_resume() {
                    let k = HLT_SKIPS.fetch_add(1, Ordering::AcqRel);
                    if k < 4 {
                        serial::write_line("boot: guest-UEFI HLT skip");
                    }
                    skip_insn()
                } else {
                    false
                }
            }
            EXIT_REASON_EPT_VIOLATION => handle_ept(gpa),
            EXIT_REASON_CR_ACCESS => handle_cr(qual),
            EXIT_REASON_EXCEPTION_NMI => handle_exception_nmi(intr, rip, linear),
            EXIT_REASON_EXTERNAL_INTERRUPT => true,
            EXIT_REASON_PREEMPTION_TIMER => true,
            EXIT_REASON_XSETBV => handle_xsetbv(),
            // INVD / INVLPG / RDTSC / PAUSE / WBINVD — skip, keep PEI moving.
            13 | 14 | 16 | 40 | 54 => skip_insn(),
            _ => false,
        };
        if resume {
            // Preemption (and any other resume) is not always an instruction
            // exit. CpuDeadLoop `pause` / `jmp $` never does I/O; skip those
            // so firmware can fall through. Delay `jcc` on I/O stays.
            // Do **not** skip `eb ec` + `leave; ret` (iron 891eb5b escaped
            // ASSERT then #UD at PE header 0x109D).
            // Iron d5f9431: pause+jcc deadloop at 0x6e81ca only on
            // preemption (HPET +256 per 256 exits, no PCI/ATA).
            let skipped = if basic == EXIT_REASON_EXCEPTION_NMI {
                false
            } else if basic == EXIT_REASON_PREEMPTION_TIMER {
                skip_preempt_deadloop(linear, rip)
            } else {
                skip_spin_short_jmp(linear, rip)
            };
            if skipped {
                let k = SPIN_JMP_SKIPS.fetch_add(1, Ordering::AcqRel);
                if k < 8 {
                    serial::write_str("boot: guest-UEFI spin jmp skip insn=");
                    dump_low_ram_insn(linear);
                    serial::write_byte(b'\n');
                }
            } else if basic == EXIT_REASON_PREEMPTION_TIMER {
                let same = PREEMPT_SAME_RIP.load(Ordering::Acquire);
                if ASSERT_DEADLOOP_DUMP.load(Ordering::Acquire) == 0 && (same == 8 || same == 64)
                {
                    serial::write_str("boot: guest-UEFI preempt noskip same=");
                    write_dec(same as u64);
                    serial::write_str(" insn=");
                    dump_low_ram_insn(linear);
                    serial::write_byte(b'\n');
                }
            }
        }
        if resume
            && post_dxe_should_stop(
                DXE_PRINTED.load(Ordering::Acquire),
                n,
                DXE_AT_N.load(Ordering::Acquire),
                crate::devices::ide_cdrom::sectors_read(),
            )
        {
            resume = false;
        }
    }

    if resume {
        let reload = PREEMPT_RELOAD.load(Ordering::Acquire);
        if reload != 0 {
            let _ = ops::vmwrite(VMX_PREEMPTION_TIMER_VALUE, u64::from(reload));
        }
        CONTINUE_GUEST.store(true, Ordering::Release);
        guest_uefi_vmresume();
    }
    serial::write_str("boot: guest-UEFI stop n=");
    write_dec(n as u64);
    serial::write_str(" reason=0x");
    write_hex_u32(reason);
    serial::write_str(" rip=0x");
    write_hex(rip);
    serial::write_str(" left_sec=");
    write_dec(LEFT_SEC.load(Ordering::Acquire) as u64);
    serial::write_str(" pci=");
    write_dec(PCI_CONFIG_SEEN.load(Ordering::Acquire) as u64);
    serial::write_str(" com=");
    write_dec(COM_BYTES.load(Ordering::Acquire) as u64);
    serial::write_str(" past_sec=");
    write_dec(PAST_SEC_PRINTED.load(Ordering::Acquire) as u64);
    serial::write_str(" cd=");
    write_dec(crate::devices::ide_cdrom::is_visible() as u64);
    serial::write_str(" pci_ide=");
    write_dec(crate::devices::ide_cdrom::pci_enumerated() as u64);
    serial::write_str(" virtio=");
    write_dec(crate::devices::guest_virtio_blk::pci_enumerated() as u64);
    serial::write_str(" sectors=");
    write_dec(crate::devices::ide_cdrom::sectors_read() as u64);
    serial::write_str(" packet=");
    write_dec(crate::devices::ide_cdrom::packet_commands() as u64);
    serial::write_str(" scsi=0x");
    write_hex_u32(u32::from(crate::devices::ide_cdrom::last_scsi()));
    serial::write_str(" ata=0x");
    write_hex_u32(u32::from(crate::devices::ide_cdrom::last_ata_cmd()));
    serial::write_str(" ataio=");
    write_dec(crate::devices::ide_cdrom::ata_io_accesses() as u64);
    serial::write_str(" unh=");
    write_dec(IO_UNHANDLED_N.load(Ordering::Acquire) as u64);
    serial::write_str(" plat=");
    write_dec(crate::devices::guest_platform::platform_memory_served() as u64);
    serial::write_str(" dxe=");
    write_dec(DXE_PRINTED.load(Ordering::Acquire) as u64);
    serial::write_str(" cf8=0x");
    write_hex_u32(LAST_CF8.load(Ordering::Acquire));
    serial::write_str(" port=0x");
    write_hex_u32(LAST_IO_PORT.load(Ordering::Acquire));
    serial::write_str(" bdfs=");
    write_dec(
        u64::from(PCI_BDF_SEEN0.load(Ordering::Acquire).count_ones())
            + u64::from(PCI_BDF_SEEN1.load(Ordering::Acquire).count_ones())
            + u64::from(PCI_BDF_SEEN2.load(Ordering::Acquire).count_ones())
            + u64::from(PCI_BDF_SEEN3.load(Ordering::Acquire).count_ones()),
    );
    serial::write_str(" hlt=");
    write_dec(HLT_SKIPS.load(Ordering::Acquire) as u64);
    serial::write_str(" spin=");
    write_dec(SPIN_JMP_SKIPS.load(Ordering::Acquire) as u64);
    serial::write_str(" cr=");
    write_dec(CR_ACCESSES.load(Ordering::Acquire) as u64);
    serial::write_str(" acpi=");
    write_dec(crate::devices::guest_platform::acpi_pm_timer_reads() as u64);
    serial::write_str(" ramr=");
    write_dec(RAM_REMAP_N.load(Ordering::Acquire) as u64);
    serial::write_str(" cmos=0x");
    write_hex_u32(u32::from(crate::devices::guest_platform::last_cmos_index()));
    serial::write_str(" hpet=");
    write_dec(HPET_TICKS.load(Ordering::Acquire) as u64);
    serial::write_str(" pre=");
    dump_low_ram_insn(linear.saturating_sub(16));
    serial::write_str(" insn=");
    dump_low_ram_insn(linear);
    serial::write_byte(b'\n');
    leave_to_e4();
}

#[cfg(target_os = "uefi")]
fn maybe_print_alive(basic: u32) {
    if MARKER_PRINTED.load(Ordering::Acquire)
        && !ALIVE_PRINTED.swap(true, Ordering::AcqRel)
        && (NON_TF_EXITS.load(Ordering::Acquire) >= 2 || basic == EXIT_REASON_HLT)
    {
        serial::write_line(M7_E5_OVMF_ALIVE_OK_MARKER);
        serial::write_str("boot: guest-UEFI non-tf exits=");
        write_dec(NON_TF_EXITS.load(Ordering::Acquire) as u64);
        serial::write_byte(b'\n');
        audit_log!(AuditEvent::OvmfGuestUefiAlive {
            exits: NON_TF_EXITS.load(Ordering::Acquire) as u64,
            last_reason: LAST_EXIT_REASON.load(Ordering::Acquire) as u64,
        });
    }
}

#[cfg(target_os = "uefi")]
fn maybe_print_past_sec(guest_hlt: bool) {
    if !MARKER_PRINTED.load(Ordering::Acquire) || !ALIVE_PRINTED.load(Ordering::Acquire) {
        return;
    }
    if !past_sec_evidence(
        LEFT_SEC.load(Ordering::Acquire),
        PCI_CONFIG_SEEN.load(Ordering::Acquire),
        COM_BYTES.load(Ordering::Acquire),
        guest_hlt,
    ) {
        return;
    }
    if PAST_SEC_PRINTED.swap(true, Ordering::AcqRel) {
        return;
    }
    serial::write_line(M7_E5_OVMF_PAST_SEC_OK_MARKER);
    serial::write_str("boot: guest-UEFI past-SEC linear left 0xFFFF_0000 pci=");
    write_dec(PCI_CONFIG_SEEN.load(Ordering::Acquire) as u64);
    serial::write_str(" com=");
    write_dec(COM_BYTES.load(Ordering::Acquire) as u64);
    serial::write_byte(b'\n');
    audit_log!(AuditEvent::OvmfGuestUefiPastSec {
        exits: NON_TF_EXITS.load(Ordering::Acquire) as u64,
        linear: LAST_GUEST_RIP.load(Ordering::Acquire),
        com_bytes: COM_BYTES.load(Ordering::Acquire) as u64,
    });
    maybe_print_cdrom();
    maybe_print_virtio();
    maybe_print_both();
    maybe_print_atapi();
    maybe_print_dxe();
}

#[cfg(target_os = "uefi")]
fn maybe_print_cdrom() {
    if !PAST_SEC_PRINTED.load(Ordering::Acquire) {
        return;
    }
    if crate::devices::ide_cdrom::take_marker() {
        serial::write_line(M7_E5_OVMF_CDROM_OK_MARKER);
        serial::write_str("boot: guest-UEFI CD visible pci_ide=");
        write_dec(crate::devices::ide_cdrom::pci_enumerated() as u64);
        serial::write_str(" sectors=");
        write_dec(crate::devices::ide_cdrom::sectors_read() as u64);
        serial::write_byte(b'\n');
        audit_log!(AuditEvent::OvmfGuestUefiCdrom {
            exits: NON_TF_EXITS.load(Ordering::Acquire) as u64,
            pci_enum: crate::devices::ide_cdrom::pci_enumerated() as u64,
            sectors: crate::devices::ide_cdrom::sectors_read() as u64,
        });
    }
    maybe_print_virtio();
    maybe_print_both();
    maybe_print_atapi();
    maybe_print_dxe();
}

#[cfg(target_os = "uefi")]
fn maybe_print_virtio() {
    if !PAST_SEC_PRINTED.load(Ordering::Acquire) {
        return;
    }
    if crate::devices::guest_virtio_blk::take_marker() {
        serial::write_line(M7_E5_OVMF_VIRTIO_OK_MARKER);
        serial::write_str("boot: guest-UEFI virtio-blk pci=");
        write_dec(crate::devices::guest_virtio_blk::pci_enumerated() as u64);
        serial::write_str(" boot=CD,disk");
        serial::write_byte(b'\n');
        audit_log!(AuditEvent::OvmfGuestUefiVirtio {
            exits: NON_TF_EXITS.load(Ordering::Acquire) as u64,
            pci_enum: crate::devices::guest_virtio_blk::pci_enumerated() as u64,
        });
    }
    maybe_print_both();
    maybe_print_atapi();
    maybe_print_dxe();
}

#[cfg(target_os = "uefi")]
fn maybe_print_both() {
    if !PAST_SEC_PRINTED.load(Ordering::Acquire) {
        return;
    }
    if !both_pci_evidence(
        crate::devices::guest_virtio_blk::pci_enumerated(),
        crate::devices::ide_cdrom::pci_enumerated(),
    ) {
        return;
    }
    if BOTH_PRINTED.swap(true, Ordering::AcqRel) {
        return;
    }
    serial::write_line(M7_E5_OVMF_BOTH_OK_MARKER);
    serial::write_str("boot: guest-UEFI both pci virtio=1 ide=1");
    serial::write_byte(b'\n');
    audit_log!(AuditEvent::OvmfGuestUefiBoth {
        exits: NON_TF_EXITS.load(Ordering::Acquire) as u64,
        virtio: 1,
        ide: 1,
    });
    maybe_print_atapi();
    maybe_print_dxe();
}

#[cfg(target_os = "uefi")]
fn maybe_print_atapi() {
    if !PAST_SEC_PRINTED.load(Ordering::Acquire) {
        return;
    }
    let sectors = crate::devices::ide_cdrom::sectors_read();
    if !atapi_read_evidence(sectors) {
        return;
    }
    if ATAPI_PRINTED.swap(true, Ordering::AcqRel) {
        return;
    }
    serial::write_line(M7_E5_OVMF_ATAPI_OK_MARKER);
    serial::write_str("boot: guest-UEFI atapi sectors=");
    write_dec(sectors as u64);
    serial::write_str(" packet=");
    write_dec(crate::devices::ide_cdrom::packet_commands() as u64);
    serial::write_str(" scsi=0x");
    write_hex_u32(u32::from(crate::devices::ide_cdrom::last_scsi()));
    serial::write_byte(b'\n');
    audit_log!(AuditEvent::OvmfGuestUefiAtapi {
        exits: NON_TF_EXITS.load(Ordering::Acquire) as u64,
        sectors: sectors as u64,
    });
    maybe_print_dxe();
}

#[cfg(target_os = "uefi")]
fn maybe_print_dxe() {
    if !PAST_SEC_PRINTED.load(Ordering::Acquire) {
        return;
    }
    let linear = LAST_LINEAR.load(Ordering::Acquire);
    let cs_ok = exec_from_low_ram(linear);
    if !dxe_or_cd_boot_evidence(
        true,
        crate::devices::ide_cdrom::sectors_read(),
        crate::devices::guest_platform::platform_memory_served(),
        cs_ok,
    ) {
        return;
    }
    if DXE_PRINTED.swap(true, Ordering::AcqRel) {
        return;
    }
    DXE_AT_N.store(EXIT_COUNT.load(Ordering::Acquire), Ordering::Release);
    serial::write_line(M7_E5_OVMF_DXE_OK_MARKER);
    serial::write_str("boot: guest-UEFI past-PEI/DXE or CD boot attempt sectors=");
    write_dec(crate::devices::ide_cdrom::sectors_read() as u64);
    serial::write_str(" plat=");
    write_dec(crate::devices::guest_platform::platform_memory_served() as u64);
    serial::write_str(" ram_rip=");
    write_dec(cs_ok as u64);
    serial::write_byte(b'\n');
    audit_log!(AuditEvent::OvmfGuestUefiDxe {
        exits: NON_TF_EXITS.load(Ordering::Acquire) as u64,
        sectors: crate::devices::ide_cdrom::sectors_read() as u64,
        platform: crate::devices::guest_platform::platform_memory_served() as u64,
    });
}

#[cfg(target_os = "uefi")]
fn note_pci_cf8(addr: u32) {
    if (addr & 0x8000_0000) == 0 {
        return;
    }
    let (bus, dev, fun, _) = crate::devices::guest_platform::pci_bdf(addr);
    if bus != 0 {
        return;
    }
    let Some((word, bit)) = pci_bdf_bit(dev, fun) else {
        return;
    };
    let slot = match word {
        0 => &PCI_BDF_SEEN0,
        1 => &PCI_BDF_SEEN1,
        2 => &PCI_BDF_SEEN2,
        _ => &PCI_BDF_SEEN3,
    };
    let prev = slot.fetch_or(bit, Ordering::AcqRel);
    if prev & bit != 0 {
        return;
    }
    serial::write_str("boot: guest-UEFI pci select 00:");
    write_hex_u8(dev);
    serial::write_byte(b'.');
    write_hex_u8(fun);
    serial::write_byte(b'\n');
}

/// First DID / Header Type / class+BAR config cycles (Stage 44: PciBus vs ATA).
#[cfg(target_os = "uefi")]
fn trace_pci_cfg(cfg: u32, val: u32, size: u8, write: bool) {
    let aligned = (cfg as u8) & 0xFC;
    let (ctr, cap) = if aligned == 0 {
        (&PCI_DID_TRACE, 16u32)
    } else if aligned == 0x0C {
        (&PCI_HT_TRACE, 8u32)
    } else if aligned == 0x08 || (0x10..=0x20).contains(&aligned) {
        (&PCI_BAR_TRACE, 24u32)
    } else {
        return;
    };
    let n = ctr.fetch_add(1, Ordering::AcqRel);
    if n >= cap {
        return;
    }
    serial::write_str(if write {
        "boot: guest-UEFI pci wr=0x"
    } else {
        "boot: guest-UEFI pci cfg=0x"
    });
    write_hex_u32(cfg);
    serial::write_str(" val=0x");
    write_hex_u32(val);
    serial::write_str(" size=");
    write_dec(u64::from(size));
    serial::write_byte(b'\n');
}

#[cfg(target_os = "uefi")]
fn write_hex_u8(v: u8) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    serial::write_byte(HEX[(v >> 4) as usize]);
    serial::write_byte(HEX[(v & 0xf) as usize]);
}

/// Keep guest+host CR4.OSXSAVE set (XSAVE / XSETBV). Host-owned bit 18.
#[cfg(target_os = "uefi")]
unsafe fn force_guest_cr4_osxsave() {
    let cur = ops::vmread(GUEST_CR4).unwrap_or(0);
    let val = apply_guest_cr4_write(cur);
    let _ = ops::vmwrite(GUEST_CR4, val);
    let _ = ops::vmwrite(CR4_READ_SHADOW, guest_cr4_read_shadow(val));
    // SAFETY: VMX root. OSXSAVE is not CR4-fixed0-forbidden on this CPU.
    // KANI-TARGET: guest-UEFI host CR4.OSXSAVE (outside Proven Core).
    let host = cpu::read_cr4();
    if host & cpu::CR4_OSXSAVE == 0 {
        cpu::write_cr4(host | cpu::CR4_OSXSAVE);
    }
}

#[cfg(target_os = "uefi")]
unsafe fn read_low_ram_insn(linear: u64, buf: &mut [u8; 16]) -> usize {
    let hpa = RAM_HPA.load(Ordering::Acquire);
    if hpa == 0 || linear >= GUEST_UEFI_LOW_RAM_BYTES {
        return 0;
    }
    // SAFETY: exclusive guest-UEFI 32 MiB RAM slab; firmware is halted in VMX.
    // KANI-TARGET: #UD insn fetch from guest RAM (outside Proven Core).
    let ram = core::slice::from_raw_parts(hpa as *const u8, GUEST_UEFI_LOW_RAM_BYTES as usize);
    copy_low_ram_at(ram, linear, buf)
}

/// Iron `0ca02e6`: `#UD` RIP `0x109D` CR4=0x668 (no OSXSAVE). Guest DebugLib
/// dumped COM1; preempt skip of `eb ec` re-entered the dump until n=32768.
/// Iron `891eb5b`: OSXSAVE host-own intercepted SEC CR4; `#UD` intercept
/// stopped at n=1439 (no dump loop) after skip of `ebecc9c3` executed
/// `leave; ret` into PE-header DAA at `0x109D`. Retry XSAVE after setting
/// OSXSAVE. Skip `ud2` so ASSERT does not dump-loop. Unknown `#UD` stops.
#[cfg(target_os = "uefi")]
unsafe fn handle_ud(rip: u64, linear: u64) -> bool {
    let mut buf = [0u8; 16];
    let n = read_low_ram_insn(linear, &mut buf);
    force_guest_cr4_osxsave();
    if ud_xsave_family(&buf[..n]) {
        let k = UD_XSAVE_RETRY.fetch_add(1, Ordering::AcqRel);
        if k < 4 {
            serial::write_str("boot: guest-UEFI #UD XSAVE retry OSXSAVE insn=");
            dump_low_ram_insn(linear);
            serial::write_byte(b'\n');
        }
        return true;
    }
    if ud_is_ud2(&buf[..n]) {
        let k = UD2_SKIPS.fetch_add(1, Ordering::AcqRel);
        if k < 4 {
            serial::write_str("boot: guest-UEFI #UD2 skip insn=");
            dump_low_ram_insn(linear);
            serial::write_byte(b'\n');
        }
        if k >= 32 {
            return false;
        }
        return ops::vmwrite(GUEST_RIP, rip.wrapping_add(2)).is_ok();
    }
    serial::write_str("boot: guest-UEFI #UD unknown linear=0x");
    write_hex(linear);
    serial::write_str(" insn=");
    dump_low_ram_insn(linear);
    serial::write_byte(b'\n');
    false
}

#[cfg(target_os = "uefi")]
unsafe fn handle_exception_nmi(intr: u64, rip: u64, linear: u64) -> bool {
    let valid = (intr & (1u64 << 31)) != 0;
    let vec = (intr & 0xff) as u8;
    if valid && vec == 6 {
        return handle_ud(rip, linear);
    }
    let err = ops::vmread(VM_EXIT_INTR_ERROR_CODE).unwrap_or(0);
    let cs = ops::vmread(GUEST_CS_SELECTOR).unwrap_or(0);
    let cr0 = ops::vmread(GUEST_CR0).unwrap_or(0);
    serial::write_str("boot: guest-UEFI exception intr=0x");
    write_hex(intr);
    serial::write_str(" err=0x");
    write_hex(err);
    serial::write_str(" cs=0x");
    write_hex(cs);
    serial::write_str(" cr0=0x");
    write_hex(cr0);
    serial::write_str(" linear=0x");
    write_hex(linear);
    serial::write_str(" insn=");
    dump_low_ram_insn(linear);
    serial::write_byte(b'\n');
    false
}

/// SDM 28.2.1 CR-access: MOV to/from CR0/CR3/CR4, keep VMXE+OSXSAVE host-owned.
#[cfg(target_os = "uefi")]
unsafe fn handle_cr(qual: u64) -> bool {
    let n = CR_ACCESSES.fetch_add(1, Ordering::AcqRel);
    if n < 4 {
        serial::write_str("boot: guest-UEFI CR access cr=");
        write_dec(qual & 0xf);
        serial::write_str(" type=");
        write_dec((qual >> 4) & 3);
        serial::write_byte(b'\n');
    }
    let cr = (qual & 0xf) as u8;
    let typ = ((qual >> 4) & 3) as u8;
    let gpr = ((qual >> 8) & 0xf) as u8;
    match (cr, typ) {
        (0, 0) => {
            let _ = ops::vmwrite(GUEST_CR0, cr_gpr(gpr));
        }
        (3, 0) => {
            let _ = ops::vmwrite(GUEST_CR3, cr_gpr(gpr));
        }
        (4, 0) => {
            let val = apply_guest_cr4_write(cr_gpr(gpr));
            let _ = ops::vmwrite(GUEST_CR4, val);
            let _ = ops::vmwrite(CR4_READ_SHADOW, guest_cr4_read_shadow(val));
        }
        (0, 1) => set_cr_gpr(gpr, ops::vmread(GUEST_CR0).unwrap_or(0)),
        (3, 1) => set_cr_gpr(gpr, ops::vmread(GUEST_CR3).unwrap_or(0)),
        (4, 1) => set_cr_gpr(gpr, ops::vmread(CR4_READ_SHADOW).unwrap_or(0)),
        _ => {}
    }
    skip_insn()
}

#[cfg(target_os = "uefi")]
unsafe fn cr_gpr(idx: u8) -> u64 {
    match idx {
        0 => SAVED_RAX,
        1 => SAVED_RCX,
        2 => SAVED_RDX,
        3 => SAVED_RBX,
        4 => ops::vmread(GUEST_RSP).unwrap_or(0),
        5 => SAVED_RBP,
        6 => SAVED_RSI,
        7 => SAVED_RDI,
        8 => SAVED_R8,
        9 => SAVED_R9,
        10 => SAVED_R10,
        11 => SAVED_R11,
        12 => SAVED_R12,
        13 => SAVED_R13,
        14 => SAVED_R14,
        15 => SAVED_R15,
        _ => 0,
    }
}

#[cfg(target_os = "uefi")]
unsafe fn set_cr_gpr(idx: u8, val: u64) {
    match idx {
        0 => SAVED_RAX = val,
        1 => SAVED_RCX = val,
        2 => SAVED_RDX = val,
        3 => SAVED_RBX = val,
        4 => {
            let _ = ops::vmwrite(GUEST_RSP, val);
        }
        5 => SAVED_RBP = val,
        6 => SAVED_RSI = val,
        7 => SAVED_RDI = val,
        8 => SAVED_R8 = val,
        9 => SAVED_R9 = val,
        10 => SAVED_R10 = val,
        11 => SAVED_R11 = val,
        12 => SAVED_R12 = val,
        13 => SAVED_R13 = val,
        14 => SAVED_R14 = val,
        15 => SAVED_R15 = val,
        _ => {}
    }
}

#[cfg(target_os = "uefi")]
unsafe fn skip_rel8_if(linear: u64, rip: u64, pred: fn(u8, u8) -> bool) -> bool {
    let hpa = RAM_HPA.load(Ordering::Acquire);
    if hpa == 0 || linear >= GUEST_UEFI_LOW_RAM_BYTES {
        return false;
    }
    let mut buf = [0u8; 2];
    // SAFETY: exclusive guest-UEFI 32 MiB RAM slab; firmware is in VMX.
    // KANI-TARGET: CpuDeadLoop jmp skip from guest RAM (outside Proven Core).
    let ram = core::slice::from_raw_parts(hpa as *const u8, GUEST_UEFI_LOW_RAM_BYTES as usize);
    if copy_low_ram_at(ram, linear, &mut buf) < 2 {
        return false;
    }
    if !pred(buf[0], buf[1]) {
        return false;
    }
    ops::vmwrite(GUEST_RIP, rip.wrapping_add(2)).is_ok()
}

#[cfg(target_os = "uefi")]
unsafe fn skip_spin_short_jmp(linear: u64, rip: u64) -> bool {
    skip_rel8_if(linear, rip, spin_short_jmp_should_skip)
}

/// Frame return-address GPA: `[RBP+8]` in long mode, `[EBP+4]` in 32-bit.
pub fn assert_deadloop_return_gpa(rbp: u64, long_mode: bool) -> u64 {
    if long_mode {
        rbp.wrapping_add(8)
    } else {
        u64::from((rbp as u32).wrapping_add(4))
    }
}

#[cfg(target_os = "uefi")]
unsafe fn peek_low_u64(linear: u64) -> u64 {
    let mut slot = [0u8; 16];
    let n = read_low_ram_insn(linear, &mut slot);
    if n < 8 {
        return 0;
    }
    let mut le = [0u8; 8];
    le.copy_from_slice(&slot[..8]);
    u64::from_le_bytes(le)
}

#[cfg(target_os = "uefi")]
unsafe fn dump_assert_deadloop_once(linear: u64) {
    if ASSERT_DEADLOOP_DUMP.fetch_add(1, Ordering::AcqRel) != 0 {
        return;
    }
    let ar = ops::vmread(GUEST_CS_ACCESS_RIGHTS).unwrap_or(0);
    let long = (ar & (1 << 13)) != 0;
    let rbp = SAVED_RBP;
    let rsp = ops::vmread(GUEST_RSP).unwrap_or(0);
    let ret_at = assert_deadloop_return_gpa(rbp, long);
    let mut slot = [0u8; 16];
    let n = read_low_ram_insn(ret_at, &mut slot);
    let ret = if long && n >= 8 {
        let mut le = [0u8; 8];
        le.copy_from_slice(&slot[..8]);
        u64::from_le_bytes(le)
    } else if !long && n >= 4 {
        u64::from(u32::from_le_bytes([slot[0], slot[1], slot[2], slot[3]]))
    } else {
        0
    };
    serial::write_str("boot: guest-UEFI ASSERT CpuDeadLoop noskip rbp=0x");
    write_hex(rbp);
    serial::write_str(" rsp=0x");
    write_hex(rsp);
    serial::write_str(" ret=0x");
    write_hex(ret);
    let prev_rbp = peek_low_u64(rbp);
    let site = assert_deadloop_return_gpa(prev_rbp, long);
    serial::write_str(" site=0x");
    write_hex(site);
    serial::write_str(" insn=");
    dump_low_ram_insn(linear);
    serial::write_str(" caller=");
    dump_low_ram_insn(ret);
    serial::write_str(" siteinsn=");
    dump_low_ram_insn(site);
    serial::write_byte(b'\n');
}

#[cfg(target_os = "uefi")]
unsafe fn skip_preempt_deadloop(linear: u64, rip: u64) -> bool {
    let hpa = RAM_HPA.load(Ordering::Acquire);
    if hpa == 0 || linear >= GUEST_UEFI_LOW_RAM_BYTES {
        return false;
    }
    let mut buf = [0u8; 8];
    // SAFETY: exclusive guest-UEFI 32 MiB RAM slab; firmware is in VMX.
    // KANI-TARGET: CpuDeadLoop skip from guest RAM (outside Proven Core).
    let ram = core::slice::from_raw_parts(hpa as *const u8, GUEST_UEFI_LOW_RAM_BYTES as usize);
    let n = copy_low_ram_at(ram, linear, &mut buf);
    if preempt_deadloop_is_assert_epilogue(&buf[..n]) {
        dump_assert_deadloop_once(linear);
        return false;
    }
    let len = u64::from(preempt_deadloop_skip_len(&buf[..n]));
    if len == 0 {
        return false;
    }
    ops::vmwrite(GUEST_RIP, rip.wrapping_add(len)).is_ok()
}

#[cfg(target_os = "uefi")]
unsafe fn dump_low_ram_insn(linear: u64) {
    let hpa = RAM_HPA.load(Ordering::Acquire);
    if hpa == 0 || linear >= GUEST_UEFI_LOW_RAM_BYTES {
        return;
    }
    let mut buf = [0u8; 16];
    // SAFETY: exclusive guest-UEFI 32 MiB RAM slab; firmware is halted in VMX.
    // KANI-TARGET: #GP insn dump from guest RAM (outside Proven Core).
    let ram = core::slice::from_raw_parts(hpa as *const u8, GUEST_UEFI_LOW_RAM_BYTES as usize);
    let n = copy_low_ram_at(ram, linear, &mut buf);
    for i in 0..n {
        write_hex2(buf[i]);
    }
}

#[cfg(target_os = "uefi")]
fn write_hex2(b: u8) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    serial::write_byte(HEX[(b >> 4) as usize]);
    serial::write_byte(HEX[(b & 0xf) as usize]);
}

#[cfg(target_os = "uefi")]
unsafe fn skip_insn() -> bool {
    let rip = ops::vmread(GUEST_RIP).unwrap_or(0);
    let len = ops::vmread(VM_EXIT_INSTRUCTION_LEN).unwrap_or(0);
    if len == 0 || len > 15 {
        return false;
    }
    ops::vmwrite(GUEST_RIP, rip.wrapping_add(len)).is_ok()
}

#[cfg(target_os = "uefi")]
unsafe fn handle_io(qual: u64) -> bool {
    let size = (qual & 7) + 1;
    let is_in = (qual & (1 << 3)) != 0;
    let port = io_port_from_qual(qual);
    LAST_IO_PORT.store(u32::from(port), Ordering::Release);
    if is_pci_config_port(port) || crate::devices::ide_cdrom::is_pci_data_port(port) {
        PCI_CONFIG_SEEN.store(true, Ordering::Release);
        maybe_print_past_sec(false);
        handle_pci(port, is_in, size as u8);
        maybe_print_cdrom();
        maybe_print_virtio();
        maybe_print_both();
        maybe_print_atapi();
        return skip_insn();
    }
    if crate::devices::ide_cdrom::is_ata_primary_port(port) {
        let before = crate::devices::ide_cdrom::ata_commands();
        SAVED_RAX = crate::devices::ide_cdrom::ata_io(port, is_in, size as u8, SAVED_RAX);
        let n = crate::devices::ide_cdrom::ata_commands();
        if n > before && n <= 8 {
            serial::write_str("boot: guest-UEFI ata cmd=0x");
            write_hex_u32(u32::from(crate::devices::ide_cdrom::last_ata_cmd()));
            serial::write_str(" n=");
            write_dec(n as u64);
            serial::write_byte(b'\n');
        }
        maybe_print_cdrom();
        maybe_print_atapi();
        return skip_insn();
    }
    if crate::devices::ide_cdrom::is_bmide_port(port) {
        SAVED_RAX = crate::devices::ide_cdrom::bmide_io(port, is_in, size as u8, SAVED_RAX);
        return skip_insn();
    }
    if crate::devices::guest_platform::is_platform_io_port(port)
        || crate::devices::guest_platform::is_acpi_pm_timer_io(port, size as u8)
        || crate::devices::guest_platform::is_piix_pm_io(port)
    {
        SAVED_RAX = crate::devices::guest_platform::io(port, is_in, size as u8, SAVED_RAX);
        maybe_print_dxe();
        return skip_insn();
    }
    if is_com_uart_port(port) {
        return handle_uart(port, is_in, size);
    }
    let k = IO_UNHANDLED_N.fetch_add(1, Ordering::AcqRel);
    if k < 12 {
        serial::write_str("boot: guest-UEFI io unhandled port=0x");
        write_hex_u32(u32::from(port));
        serial::write_str(" in=");
        write_dec(is_in as u64);
        serial::write_str(" size=");
        write_dec(size);
        serial::write_byte(b'\n');
    }
    if is_in {
        let mask = if size == 1 {
            0xffu64
        } else if size == 2 {
            0xffff
        } else {
            0xffff_ffff
        };
        SAVED_RAX = (SAVED_RAX & !mask) | mask;
    }
    skip_insn()
}

/// After PEIMs LZMA-decompress into low RAM, patch `cmp bx, 0x1237` there too.
/// Flash remap cannot see compressed copies. Retry a few DID probes until a
/// hit (first `inw` of `00:00.0` can race decompress). Not two-phase DID.
#[cfg(target_os = "uefi")]
unsafe fn maybe_remap_guest_ram() {
    if RAM_REMAP_N.load(Ordering::Acquire) > 0 {
        return;
    }
    let tries = RAM_REMAP_TRIES.fetch_add(1, Ordering::AcqRel);
    if tries >= 8 {
        return;
    }
    let hpa = RAM_HPA.load(Ordering::Acquire);
    if hpa == 0 {
        return;
    }
    // SAFETY: exclusive guest-UEFI 32 MiB RAM slab; firmware is halted in VMX.
    // KANI-TARGET: remap decompressed OVMF in guest RAM (outside Proven Core).
    let n = crate::boot::ovmf_esp::remap_i440fx_did_imm(core::slice::from_raw_parts_mut(
        hpa as *mut u8,
        GUEST_UEFI_LOW_RAM_BYTES as usize,
    ));
    if n > 0 {
        RAM_REMAP_N.store(n, Ordering::Release);
        serial::write_str("boot: guest-UEFI ram remap i440FX DID->virtio n=");
        write_dec(n as u64);
        serial::write_byte(b'\n');
    } else if tries == 0 {
        serial::write_line("boot: guest-UEFI ram remap i440FX DID->virtio n=0");
    }
}

#[cfg(target_os = "uefi")]
unsafe fn handle_pci(port: u16, is_in: bool, size: u8) {
    if port == 0xCF8 {
        if is_in {
            let mask = if size == 1 {
                0xffu64
            } else if size == 2 {
                0xffff
            } else {
                0xffff_ffff
            };
            let v = u64::from(crate::devices::ide_cdrom::pci_read_addr());
            SAVED_RAX = (SAVED_RAX & !mask) | (v & mask);
        } else {
            let addr = SAVED_RAX as u32;
            crate::devices::ide_cdrom::pci_write_addr(addr);
            crate::devices::guest_platform::pci_write_addr(addr);
            crate::devices::guest_virtio_blk::pci_write_addr(addr);
            LAST_CF8.store(addr, Ordering::Release);
            note_pci_cf8(addr);
        }
        return;
    }
    if crate::devices::ide_cdrom::is_pci_data_port(port) {
        if is_in {
            let mask = if size == 1 {
                0xffu64
            } else if size == 2 {
                0xffff
            } else {
                0xffff_ffff
            };
            let v = if let Some(p) = crate::devices::guest_platform::pci_read_data(port, size) {
                u64::from(p)
            } else if crate::devices::guest_virtio_blk::pci_addr_selects_virtio(
                crate::devices::guest_virtio_blk::pci_read_addr(),
            ) {
                u64::from(crate::devices::guest_virtio_blk::pci_read_data(port, size))
            } else {
                u64::from(crate::devices::ide_cdrom::pci_read_data(port, size))
            };
            SAVED_RAX = (SAVED_RAX & !mask) | (v & mask);
            let cfg = crate::devices::ide_cdrom::pci_read_addr()
                | u32::from(port.wrapping_sub(0xCFC) & 3);
            let off = (cfg & 0xff) as u8;
            let aligned = off & 0xFC;
            if aligned == 0
                && crate::devices::guest_virtio_blk::pci_addr_selects_virtio(
                    crate::devices::guest_virtio_blk::pci_read_addr(),
                )
            {
                maybe_remap_guest_ram();
            }
            trace_pci_cfg(cfg, v as u32, size, false);
        } else {
            crate::devices::guest_platform::pci_write_data(port, size, SAVED_RAX as u32);
            crate::devices::ide_cdrom::pci_write_data(port, size, SAVED_RAX as u32);
            crate::devices::guest_virtio_blk::pci_write_data(port, size, SAVED_RAX as u32);
            let cfg = crate::devices::ide_cdrom::pci_read_addr()
                | u32::from(port.wrapping_sub(0xCFC) & 3);
            trace_pci_cfg(cfg, SAVED_RAX as u32, size, true);
        }
    }
}

#[cfg(target_os = "uefi")]
unsafe fn handle_ept(gpa: u64) -> bool {
    if crate::devices::guest_platform::is_platform_sink_gpa(gpa) && ept_map_2m_sink(gpa) {
        SINK_MAPS.fetch_add(1, Ordering::AcqRel);
        if SINK_MAPS.load(Ordering::Acquire) <= 8 {
            serial::write_str("boot: guest-UEFI EPT sink gpa=0x");
            write_hex(gpa);
            serial::write_byte(b'\n');
        }
        maybe_print_dxe();
        return true;
    }
    serial::write_str("boot: guest-UEFI EPT violation gpa=0x");
    write_hex(gpa);
    serial::write_byte(b'\n');
    false
}

/// Map a 2 MiB sink leaf for `gpa` in the private guest-UEFI EPT (outside Proven Core).
#[cfg(target_os = "uefi")]
unsafe fn ept_map_2m_sink(gpa: u64) -> bool {
    let pml4 = EPT_PML4.load(Ordering::Acquire);
    let sink = SINK_HPA.load(Ordering::Acquire);
    if pml4 == 0 || sink == 0 || (sink & ((1 << 21) - 1)) != 0 {
        return false;
    }
    let pml4_i = ((gpa >> 39) & 0x1ff) as usize;
    let e0 = core::ptr::read_volatile((pml4 as *const u64).add(pml4_i));
    if e0 & 0b111 == 0 {
        return false;
    }
    let pdpt = e0 & !0xfff;
    let pdpt_i = ((gpa >> 30) & 0x1ff) as usize;
    let e1 = core::ptr::read_volatile((pdpt as *const u64).add(pdpt_i));
    if e1 & 0b111 == 0 || (e1 & (1 << 7)) != 0 {
        return false;
    }
    let pd = e1 & !0xfff;
    let pd_i = ((gpa >> 21) & 0x1ff) as usize;
    let e2 = core::ptr::read_volatile((pd as *const u64).add(pd_i));
    if e2 & 0b111 != 0 {
        return true;
    }
    let leaf = crate::memory::ept_hw::ept_leaf_large(sink, 0);
    core::ptr::write_volatile((pd as *mut u64).add(pd_i), leaf);
    crate::memory::ept_hw::invept_global();
    true
}

/// 16550-compatible COM1/COM2. THR bytes go to host serial (firmware evidence).
#[cfg(target_os = "uefi")]
unsafe fn handle_uart(port: u16, is_in: bool, size: u64) -> bool {
    let off = port & 7;
    let com1 = (0x03F8..=0x03FF).contains(&port);
    let lcr_slot: &AtomicU8 = if com1 { &UART_LCR_COM1 } else { &UART_LCR_COM2 };
    let mask = if size == 1 {
        0xffu64
    } else if size == 2 {
        0xffff
    } else {
        0xffff_ffff
    };
    if is_in {
        let val = match off {
            2 => 0x01u64, // IIR: no interrupt
            5 => 0x60,    // LSR: THRE | TEMT
            _ => 0,
        };
        SAVED_RAX = (SAVED_RAX & !mask) | (val & mask);
    } else if off == 3 {
        lcr_slot.store(SAVED_RAX as u8, Ordering::Release);
    } else if off == 0 && (lcr_slot.load(Ordering::Acquire) & 0x80) == 0 {
        let b = SAVED_RAX as u8;
        if !COM_BANNER.swap(true, Ordering::AcqRel) {
            serial::write_line("boot: guest-UEFI firmware-serial begin");
        }
        serial::write_byte(b);
        COM_BYTES.fetch_add(1, Ordering::AcqRel);
        maybe_print_past_sec(false);
    }
    skip_insn()
}

#[cfg(target_os = "uefi")]
unsafe fn handle_cpuid() -> bool {
    let leaf = SAVED_RAX as u32;
    let sub = SAVED_RCX as u32;
    let r = guest_uefi_filter_cpuid(leaf, sub);
    SAVED_RAX = r.eax as u64;
    SAVED_RBX = r.ebx as u64;
    SAVED_RCX = r.ecx as u64;
    SAVED_RDX = r.edx as u64;
    skip_insn()
}

/// XSETBV always VM-exits. Skipping without writing XCR0 leaves XCR0=0,
/// then XSAVES #UD/#GP after INVPCID bits (iron COM2 next after RIP 0x109D).
#[cfg(target_os = "uefi")]
unsafe fn handle_xsetbv() -> bool {
    let xcr = SAVED_RCX as u32;
    if !xsetbv_accepts_xcr(xcr) {
        return skip_insn();
    }
    let value = (SAVED_RAX & 0xffff_ffff) | ((SAVED_RDX & 0xffff_ffff) << 32);
    let host_mask = {
        let r = cpu::cpuid(0xD, 0);
        ((r.edx as u64) << 32) | (r.eax as u64)
    };
    let v = xsetbv_masked_xcr0(value, host_mask);
    force_guest_cr4_osxsave();
    // SAFETY: CR4.OSXSAVE is set; v is masked to CPUID.0D:0 with x87 set.
    // KANI-TARGET: guest-UEFI XSETBV XCR0 mask (outside Proven Core).
    cpu::xsetbv(0, v);
    let n = XSETBV_N.fetch_add(1, Ordering::AcqRel);
    if n < 2 {
        serial::write_str("boot: guest-UEFI XSETBV xcr0=0x");
        write_hex(v);
        serial::write_byte(b'\n');
    }
    skip_insn()
}

#[cfg(target_os = "uefi")]
unsafe fn handle_rdmsr() -> bool {
    let msr = SAVED_RCX as u32;
    if msr != 0x10 {
        let n = MSR_RD_TRACE.fetch_add(1, Ordering::AcqRel);
        if n < 8 {
            serial::write_str("boot: guest-UEFI RDMSR index=0x");
            write_hex(u64::from(msr));
            serial::write_byte(b'\n');
        }
    }
    let v = guest_uefi_rdmsr(msr);
    SAVED_RAX = v as u32 as u64;
    SAVED_RDX = (v >> 32) as u32 as u64;
    skip_insn()
}

#[cfg(target_os = "uefi")]
unsafe fn guest_uefi_rdmsr(msr: u32) -> u64 {
    if msr == crate::arch::cpu::IA32_FEATURE_CONTROL {
        return GUEST_UEFI_FEATURE_CONTROL_VALUE;
    }
    match msr_firewall::classify_msr(msr, msr_firewall::MsrAccess::Read) {
        msr_firewall::MsrAction::HostPassthrough => cpu::rdmsr(msr),
        msr_firewall::MsrAction::VmcsEfer => ops::vmread(GUEST_IA32_EFER).unwrap_or(0),
        msr_firewall::MsrAction::VmcsPat => ops::vmread(GUEST_IA32_PAT).unwrap_or(0),
        msr_firewall::MsrAction::VmcsSysenterCs => ops::vmread(GUEST_IA32_SYSENTER_CS).unwrap_or(0),
        msr_firewall::MsrAction::VmcsSysenterEsp => ops::vmread(GUEST_IA32_SYSENTER_ESP).unwrap_or(0),
        msr_firewall::MsrAction::VmcsSysenterEip => ops::vmread(GUEST_IA32_SYSENTER_EIP).unwrap_or(0),
        msr_firewall::MsrAction::VmcsFsBase => ops::vmread(GUEST_FS_BASE).unwrap_or(0),
        msr_firewall::MsrAction::VmcsGsBase => ops::vmread(GUEST_GS_BASE).unwrap_or(0),
        msr_firewall::MsrAction::Shadow => {
            if msr == 0x1B {
                0xFEE0_0000 | (1 << 8) | (1 << 11)
            } else {
                msr_firewall::shadow_read(msr)
            }
        }
        msr_firewall::MsrAction::InjectGp
        | msr_firewall::MsrAction::ReadZero
        | msr_firewall::MsrAction::IgnoreWrite => 0,
    }
}

#[cfg(target_os = "uefi")]
unsafe fn handle_wrmsr() -> bool {
    let msr = SAVED_RCX as u32;
    let v = (SAVED_RAX & 0xffff_ffff) | ((SAVED_RDX & 0xffff_ffff) << 32);
    match msr_firewall::classify_msr(msr, msr_firewall::MsrAccess::Write) {
        msr_firewall::MsrAction::VmcsEfer => {
            let _ = ops::vmwrite(GUEST_IA32_EFER, v);
        }
        msr_firewall::MsrAction::VmcsPat => {
            let _ = ops::vmwrite(GUEST_IA32_PAT, v);
        }
        msr_firewall::MsrAction::VmcsSysenterCs => {
            let _ = ops::vmwrite(GUEST_IA32_SYSENTER_CS, v);
        }
        msr_firewall::MsrAction::VmcsSysenterEsp => {
            let _ = ops::vmwrite(GUEST_IA32_SYSENTER_ESP, v);
        }
        msr_firewall::MsrAction::VmcsSysenterEip => {
            let _ = ops::vmwrite(GUEST_IA32_SYSENTER_EIP, v);
        }
        msr_firewall::MsrAction::VmcsFsBase => {
            let _ = ops::vmwrite(GUEST_FS_BASE, v);
        }
        msr_firewall::MsrAction::VmcsGsBase => {
            let _ = ops::vmwrite(GUEST_GS_BASE, v);
        }
        msr_firewall::MsrAction::Shadow => msr_firewall::shadow_write(msr, v),
        _ => {}
    }
    skip_insn()
}

#[cfg(target_os = "uefi")]
unsafe fn leave_to_e4() -> ! {
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
