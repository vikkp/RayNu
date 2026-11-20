//! Guest scheduler: N product guests on one physical CPU (ADR-022).
//!
//! Pillar: [Z] · Outside the Proven Core.
//!
//! Every guest is the same machine in its own slot ([`crate::guest::slot`]).
//! One guest is in VMX non-root at a time. At the end of each VM exit the
//! exit path asks [`resume`] which guest runs next; a switch saves the
//! leaving guest's CPU state that the VMCS does not hold and loads the next
//! guest's. The exit handlers, the devices and RayNu-F do not know there is
//! more than one guest.
//!
//! INVARIANTS:
//!   - A guest's registers, extended state, debug registers and passthrough
//!     MSRs are loaded from its own context before it enters, and from
//!     nowhere else. A guest never enters with another guest's values.
//!   - A slot is entered only while it is on, and its VMCS is the current
//!     VMCS when it enters.
//!   - The policy ([`pick`]) is pure. It never starves an on guest for more
//!     than [`STARVE_US`] plus one quantum.
//!   - With one guest on, [`resume`] is a plain `VMRESUME` of that guest.
//!
//! What is per guest and what is not: the VMCS holds RIP/RSP/RFLAGS,
//! segments, control registers, DR7 and the VMCS-backed MSRs, so those need
//! no work here. This module carries the rest: GPRs, XMM (saved by the exit
//! trampoline), the XSAVE image, XCR0, CR2, DR0–DR3, DR6, the syscall MSRs
//! the firewall passes to hardware, and the two shadowed MSRs.
//!
//! VERIFICATION: L1 (host tests on the policy and the arithmetic).

use crate::guest::slot::{MAX_GUESTS, MAX_UNITS};

/// Longest a busy guest runs before another runnable guest gets the CPU.
pub const QUANTUM_US: u64 = 2_000;
/// A halted guest with no known work still runs this often. It is the
/// backstop for an event the scheduler cannot see (a paced console byte,
/// a device timer inside the guest's own exit path).
pub const STARVE_US: u64 = 10_000;
/// A guest that leaves the CPU on its own is booted again this many times
/// for each operator Start.
pub const RESTARTS_PER_START: u32 = 1;
/// One XSAVE image per slot. AVX-512 needs about 2.7 KiB and AMX about
/// 11 KiB; a CPU that reports more than this runs one guest at a time.
pub const XSAVE_AREA_BYTES: usize = 16 * 1024;
/// Chunk size of the leftover-DRAM bump the extra slots are built from.
pub const ARENA_CHUNK_BYTES: u64 = 2 * 1024 * 1024;
/// Smallest report-RAM a second guest is built with (448 MiB + the slab).
pub const SLOT_MIN_REPORT_CHUNKS: u64 = 224;
/// Install disk a nested (no PERC) extra slot gets from the bump.
pub const NESTED_SLOT_DISK_BYTES: u64 = 256 * 1024 * 1024;
/// Period of the COM2 status line while guests share the CPU.
pub const BEAT_US: u64 = 30_000_000;

/// What the scheduler knows about a guest that is not on the CPU.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Other {
    /// Launched and not yet left.
    pub on: bool,
    /// It was idle (HLT with nothing to deliver) when it left the CPU.
    pub halted: bool,
    /// Something is waiting for it: an interrupt, a Stop, a device event.
    pub work: bool,
    /// TSC ticks since it left the CPU.
    pub waited_tsc: u64,
}

/// Outcome of [`pick`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pick {
    Stay,
    Switch(usize),
}

/// A guest off the CPU that should get it now.
pub fn runnable(o: Other, starve_tsc: u64) -> bool {
    o.on && (!o.halted || o.work || o.waited_tsc >= starve_tsc)
}

/// Which unit runs next. A unit is `(slot, vcpu)` (ADR-023).
///
/// A busy unit keeps the CPU for one quantum and `other` is not called.
/// After that, or when the unit is idle, the units are asked in
/// round-robin order starting after `cur` and the first runnable one wins.
/// A 4-vCPU guest has four units to a 1-vCPU guest's one.
///
/// INVARIANTS:
///   - Never returns `Switch(cur)`.
///   - Returns `Stay` when no other unit is runnable.
pub fn pick(
    cur: usize,
    cur_idle: bool,
    slice_used_tsc: u64,
    quantum_tsc: u64,
    starve_tsc: u64,
    mut other: impl FnMut(usize) -> Other,
) -> Pick {
    if !cur_idle && slice_used_tsc < quantum_tsc {
        return Pick::Stay;
    }
    for k in 1..MAX_UNITS {
        let u = (cur + k) % MAX_UNITS;
        if runnable(other(u), starve_tsc) {
            return Pick::Switch(u);
        }
    }
    Pick::Stay
}

/// [`pick`] limited to the units whose home is `core`. A unit homed
/// elsewhere is invisible, so a core does not run another core's vCPU.
pub fn pick_on_core(
    core: usize,
    cur: usize,
    cur_idle: bool,
    slice_used_tsc: u64,
    quantum_tsc: u64,
    starve_tsc: u64,
    mut other: impl FnMut(usize) -> Other,
    mut home: impl FnMut(usize) -> usize,
) -> Pick {
    pick(
        cur,
        cur_idle,
        slice_used_tsc,
        quantum_tsc,
        starve_tsc,
        |u| {
            if home(u) == core {
                other(u)
            } else {
                Other::default()
            }
        },
    )
}

/// TSC ticks in `us` microseconds. `tsc_hz == 0` uses `fallback_hz`.
pub fn us_to_tsc(us: u64, tsc_hz: u64, fallback_hz: u64) -> u64 {
    let hz = if tsc_hz == 0 { fallback_hz } else { tsc_hz };
    hz.saturating_mul(us) / 1_000_000
}

/// VMX-preemption-timer ticks in one quantum. Never zero.
pub fn quantum_ticks(tsc_hz: u64, fallback_hz: u64, shift: u32) -> u32 {
    let tsc = us_to_tsc(QUANTUM_US, tsc_hz, fallback_hz);
    (tsc >> shift.min(31)).clamp(1, u64::from(u32::MAX)) as u32
}

/// The guest is idle: it executed HLT and this resume delivers nothing.
/// Linux HLT exits are skipped, so an idle guest returns here at once;
/// that is the moment another guest can have the CPU.
pub fn guest_idle(exit_was_hlt: bool, entry_intr_info: u64) -> bool {
    exit_was_hlt && (entry_intr_info & (1 << 31)) == 0
}

/// Preemption-timer value for a resume while more than one guest is on.
///
/// Returns `(value, paced, due)`. `paced` exits are scheduling (or UART
/// pace) ticks only: they step no HPET and raise no PIT. `due` is the TSC
/// at which the guest's nominal HPET/PIT tick is owed.
///
/// A guest that was off the CPU when its nominal tick came due gets it at
/// once (`value == 1`, not paced) instead of losing it: with one guest the
/// tick is lost only when the host itself ran past it, with two it would
/// be lost every time the deadline fell in the other guest's quantum.
///
/// INVARIANTS:
///   - `value >= 1` and `value <= reload`.
///   - `value <= quantum` whenever `paced`.
pub fn shared_preempt_arm(
    reload: u32,
    prev_due: u64,
    now: u64,
    shift: u32,
    real_tick: bool,
    uart_pace: Option<u32>,
    quantum: u32,
) -> (u32, bool, u64) {
    let reload = reload.max(1);
    if prev_due != 0 && now >= prev_due && !real_tick {
        return (1, false, prev_due);
    }
    let period = u64::from(reload) << shift.min(31);
    let due = if prev_due == 0 || now >= prev_due {
        now.wrapping_add(period)
    } else {
        prev_due
    };
    let pace = uart_pace.map_or(quantum, |u| u.min(quantum)).max(1);
    let remaining = if due > now {
        ((due - now) >> shift.min(31)).min(u64::from(reload)) as u32
    } else {
        0
    };
    if remaining > pace {
        (pace, true, due)
    } else {
        (remaining.max(1), false, due)
    }
}

/// What happens to a guest that left the CPU while another is still on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Leave {
    /// The operator stopped it (or it powered itself off). It stays off.
    StayOff,
    /// It left on its own. Boot it again.
    Restart,
    /// It left on its own again. It is marked off; Start boots it.
    Off,
}

pub fn leave_plan(operator_stopped: bool, restarts: u32) -> Leave {
    if operator_stopped {
        Leave::StayOff
    } else if restarts < RESTARTS_PER_START {
        Leave::Restart
    } else {
        Leave::Off
    }
}

/// The CPU's largest XSAVE image fits a slot's area.
pub fn xsave_fits(cpuid_d0_ecx: u32) -> bool {
    cpuid_d0_ecx as usize <= XSAVE_AREA_BYTES && cpuid_d0_ecx >= 576
}

/// 2 MiB chunks an extra slot needs besides its report-RAM and its disk:
/// the low-RAM slab, a private flash image, the HPET sink, the hole-zero
/// page, the MMIO scratch pool, and one chunk of 4 KiB table pages.
pub fn slot_overhead_chunks(low_ram_bytes: u64, flash_bytes: u64, scratch_slots: u64) -> u64 {
    let c = ARENA_CHUNK_BYTES;
    low_ram_bytes.div_ceil(c) + flash_bytes.div_ceil(c) + 1 + 1 + scratch_slots + 1
}

/// Report-RAM chunks an extra slot is built with, or `None` when the bump
/// cannot hold the smallest guest.
pub fn slot_report_chunks(
    arena_chunks: u64,
    overhead_chunks: u64,
    disk_chunks: u64,
    want_chunks: u64,
) -> Option<u64> {
    let left = arena_chunks.checked_sub(overhead_chunks + disk_chunks)?;
    if left < SLOT_MIN_REPORT_CHUNKS {
        return None;
    }
    Some(left.min(want_chunks))
}

/// Why a slot must not be launched.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LaunchBlock {
    /// vCPU index still marked up.
    Up(usize),
    /// vCPU index assigned to a core other than 0.
    Assigned(usize, u8),
    /// vCPU index whose VMCS is active on `core`.
    Homed(usize, u8),
}

/// A slot may be launched only when every unit is down, not assigned away
/// from core 0, and not homed on any core. `home[v] == 0xFF` means not homed.
pub fn launch_block(up_mask: u8, assigned: &[u8], home: &[u8]) -> Option<LaunchBlock> {
    let n = assigned.len().min(home.len()).min(4);
    for v in 0..n {
        if up_mask & (1u8 << v) != 0 {
            return Some(LaunchBlock::Up(v));
        }
        if assigned[v] != 0 {
            return Some(LaunchBlock::Assigned(v, assigned[v]));
        }
        if home[v] != 0xFF {
            return Some(LaunchBlock::Homed(v, home[v]));
        }
    }
    None
}

/// Scheduler booted the guest again after the guest left on its own.
pub const RESTART_SCHEDULER: u8 = 1;
/// Guest asked to reboot (CF9 / keyboard / ACPI reset).
pub const RESTART_REBOOT: u8 = 2;

/// `VmStarted` is the operator Start and autostart. A restart the
/// hypervisor performs on its own is `GuestRestarted`.
pub fn start_audit_is_vm_started(operator_or_autostart: bool) -> bool {
    operator_or_autostart
}

/// vCPU of a slot that core 0 may `VMPTRLD`.
///
/// `assigned[v]` is the core that holds vCPU `v` (0 is core 0). vCPU 0 stays
/// on core 0 while its up-bit is set (plan 5g, I4). A vCPU another core left
/// in `CUR_VCPU` is not a VMCS this core can load.
pub fn home_vcpu(up_mask: u8, assigned: &[u8]) -> Option<usize> {
    let n = assigned.len().min(4);
    if up_mask & 1 != 0 && assigned.first().copied().unwrap_or(1) == 0 {
        return Some(0);
    }
    for v in 1..n {
        if up_mask & (1u8 << v) != 0 && assigned[v] == 0 {
            return Some(v);
        }
    }
    None
}

#[cfg(target_os = "uefi")]
pub(super) use hw::*;

#[cfg(target_os = "uefi")]
mod hw {
    use super::*;
    use crate::arch::cpu;
    use crate::audit::AuditEvent;
    use crate::audit_log;
    use crate::boot::serial;
    use crate::guest::slot::{self, PerGuest, PerVcpu, PerVcpuCell};
    use crate::sched::msr_firewall;
    use crate::vmx::ops;
    use crate::{per_guest, per_vcpu, per_vcpu_cell};
    use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicU8, AtomicUsize, Ordering};

    const MSR_STAR: u32 = 0xC000_0081;
    const MSR_LSTAR: u32 = 0xC000_0082;
    const MSR_CSTAR: u32 = 0xC000_0083;
    const MSR_SFMASK: u32 = 0xC000_0084;
    const MSR_KERNEL_GS_BASE: u32 = 0xC000_0102;
    const MSR_TSC_AUX: u32 = 0xC000_0103;
    const MSR_SPEC_CTRL: u32 = 0x48;
    const MSR_APIC_BASE: u32 = 0x1B;
    const MSR_PRED_CMD: u32 = 0x49;
    /// Syscall-path MSRs the firewall passes to hardware (`HostPassthrough`).
    const PASS_MSRS: [u32; 5] = [
        MSR_STAR,
        MSR_LSTAR,
        MSR_CSTAR,
        MSR_SFMASK,
        MSR_KERNEL_GS_BASE,
    ];
    const APIC_BASE_RESET: u64 = 0xFEE0_0000 | (1 << 11);
    const DR6_RESET: u64 = 0xFFFF_0FF0;
    const MXCSR_RESET: u32 = 0x1F80;

    #[repr(C, align(64))]
    struct XsaveArea([u8; XSAVE_AREA_BYTES]);

    /// CPU state of a guest that is off the CPU and that its VMCS does not
    /// hold.
    struct VcpuCtx {
        gpr: [u64; 15],
        xmm: [u8; 256],
        cr2: u64,
        dr: [u64; 4],
        dr6: u64,
        msr: [u64; 5],
        tsc_aux: u64,
        spec_ctrl: u64,
        apic_base: u64,
        xcr0: u64,
    }

    static CTX: PerVcpuCell<VcpuCtx> = per_vcpu_cell!(VcpuCtx {
        gpr: [0; 15],
        xmm: [0; 256],
        cr2: 0,
        dr: [0; 4],
        dr6: DR6_RESET,
        msr: [0; 5],
        tsc_aux: 0,
        spec_ctrl: 0,
        apic_base: APIC_BASE_RESET,
        xcr0: 1,
    });
    static XSAVE: PerVcpuCell<XsaveArea> = per_vcpu_cell!(XsaveArea([0; XSAVE_AREA_BYTES]));

    /// The vCPU was idle when it left the CPU.
    static HALTED: PerVcpu<AtomicBool> = per_vcpu!(AtomicBool::new(false));
    /// TSC when the vCPU left the CPU.
    static OFF_TSC: PerVcpu<AtomicU64> = per_vcpu!(AtomicU64::new(0));
    /// TSC when this guest last left the CPU for another guest.
    static SLOT_OFF_TSC: PerGuest<AtomicU64> = per_guest!(AtomicU64::new(0));
    /// TSC ticks the guest has spent off the CPU while another guest ran.
    static AWAY_TSC: PerGuest<AtomicU64> = per_guest!(AtomicU64::new(0));
    /// Times the guest was booted again since the operator's Start.
    static RESTARTS: PerGuest<AtomicU32> = per_guest!(AtomicU32::new(0));
    /// The pending start is the scheduler's own restart, not the operator.
    static AUTO_RESTART: PerGuest<AtomicBool> = per_guest!(AtomicBool::new(false));
    static RESTART_REASON: PerGuest<AtomicU8> = per_guest!(AtomicU8::new(0));
    /// The guest takes COM2 when it launches (an operator Start).
    static TAKE_CONSOLE: PerGuest<AtomicBool> = per_guest!(AtomicBool::new(false));
    /// TSC when the guest on this core got the CPU. `load` and `store` keep
    /// the call text; the word is chosen by [`crate::arch::host_smp::this_core`].
    struct SliceClock;
    impl SliceClock {
        fn load(&self, _order: Ordering) -> u64 {
            let i = crate::arch::host_smp::this_core()
                .min(crate::arch::host_smp::MAX_CORES - 1);
            SLICE_CLOCKS[i].load(Ordering::Acquire)
        }
        fn store(&self, val: u64, _order: Ordering) {
            let i = crate::arch::host_smp::this_core()
                .min(crate::arch::host_smp::MAX_CORES - 1);
            SLICE_CLOCKS[i].store(val, Ordering::Release);
        }
    }
    /// TSC when the guest on this core got the CPU. One word per host core.
    static SLICE_START: SliceClock = SliceClock;
    static SLICE_CLOCKS: [AtomicU64; crate::arch::host_smp::MAX_CORES] =
        [const { AtomicU64::new(0) }; crate::arch::host_smp::MAX_CORES];
    /// Lab only (`twoguest.txt`): slot + 1 that boots the retained product
    /// ISO beside slot 0. Zero when nothing is asked. Nested QEMU has no
    /// PERC catalog to start a second guest from.
    static LAB_START: AtomicUsize = AtomicUsize::new(0);
    /// Lab only: slot + 1 that is started once more after it powers off,
    /// so nested QEMU runs "one guest leaves, the other keeps running"
    /// and "Start of a slot that already has a machine".
    static LAB_AGAIN: AtomicUsize = AtomicUsize::new(0);
    static SWITCHES: AtomicU64 = AtomicU64::new(0);
    static SWITCH_LOGGED: AtomicBool = AtomicBool::new(false);
    /// Printed once when a switch stays inside one slot (vCPUs share the core).
    static VCPU_SHARE_LOGGED: AtomicBool = AtomicBool::new(false);
    /// The scheduler is booting this slot and it has not entered yet. A
    /// leave in that window is a launch that failed.
    static LAUNCHING: PerGuest<AtomicBool> = per_guest!(AtomicBool::new(false));
    /// TSC ticks the vCPU has held the CPU while it shared it.
    static RUN_TSC: PerVcpu<AtomicU64> = per_vcpu!(AtomicU64::new(0));
    /// Times the vCPU was given the CPU by a switch.
    static RUNS: PerVcpu<AtomicU64> = per_vcpu!(AtomicU64::new(0));
    /// TSC of the last shared-CPU status line.
    static BEAT_TSC: AtomicU64 = AtomicU64::new(0);

    /// Ask for the lab second guest. Host tests and iron never call this.
    pub fn request_lab_start(slot: usize) {
        if slot != 0 && slot < MAX_GUESTS {
            LAB_START.store(slot + 1, Ordering::Release);
            LAB_AGAIN.store(slot + 1, Ordering::Release);
            slot::with_slot(
                slot,
                crate::devices::guest_serial_answer::lab_power_off_after_prove,
            );
        }
    }

    /// Guest switches since boot.
    pub fn switches() -> u64 {
        SWITCHES.load(Ordering::Acquire)
    }

    /// The TSC with the current guest's time off the CPU taken out.
    ///
    /// A watchdog that asks "has this guest been quiet for N ticks" must
    /// not count the ticks another guest ran. With one guest this is `now`.
    pub fn guest_tsc(now: u64) -> u64 {
        now.wrapping_sub(AWAY_TSC.load(Ordering::Acquire))
    }

    /// A slot has a Start waiting. The idle loop boots it.
    pub fn start_waiting() -> bool {
        pending_start(MAX_GUESTS).is_some()
    }

    fn tsc_hz() -> u64 {
        crate::boot::raynu_f_flag::tsc_hz()
    }

    fn host_xcr0_mask() -> u64 {
        // SAFETY: CPUID leaf 0xD is architectural on a CPU that has XSAVE.
        // KANI-TARGET: guest scheduler CPUID.0xD read (outside Proven Core).
        let r = unsafe { cpu::cpuid(0xD, 0) };
        (((r.edx as u64) << 32) | (r.eax as u64)) | 1
    }

    /// More than one guest can be on: the CPU's XSAVE image fits a slot.
    pub fn can_share() -> bool {
        // SAFETY: as above.
        // KANI-TARGET: guest scheduler CPUID.0xD size read (outside Proven Core).
        let r = unsafe { cpu::cpuid(0xD, 0) };
        xsave_fits(r.ecx)
    }

    /// Scheduling clamp for the preemption timer, when more than one guest
    /// is on.
    pub fn clamp_ticks(shift: u32) -> Option<u32> {
        if slot::cpu_shared() {
            Some(quantum_ticks(
                tsc_hz(),
                super::super::UART_PACE_TSC_HZ_FALLBACK,
                shift,
            ))
        } else {
            None
        }
    }

    unsafe fn read_dr(n: u8) -> u64 {
        let v: u64;
        // SAFETY: CPL0; MOV from a debug register has no side effect.
        // KANI-TARGET: guest scheduler DR read (outside Proven Core).
        match n {
            0 => {
                core::arch::asm!("mov {}, dr0", out(reg) v, options(nomem, nostack, preserves_flags))
            }
            1 => {
                core::arch::asm!("mov {}, dr1", out(reg) v, options(nomem, nostack, preserves_flags))
            }
            2 => {
                core::arch::asm!("mov {}, dr2", out(reg) v, options(nomem, nostack, preserves_flags))
            }
            3 => {
                core::arch::asm!("mov {}, dr3", out(reg) v, options(nomem, nostack, preserves_flags))
            }
            _ => {
                core::arch::asm!("mov {}, dr6", out(reg) v, options(nomem, nostack, preserves_flags))
            }
        }
        v
    }

    unsafe fn write_dr(n: u8, v: u64) {
        // SAFETY: CPL0. The host arms no breakpoint (DR7 is loaded from the
        // VMCS on entry and reset to 0x400 on exit), so DR0–DR3/DR6 are
        // guest values only. DR6 reserved bits are forced to their fixed
        // values so the write cannot #GP.
        // KANI-TARGET: guest scheduler DR write (outside Proven Core).
        match n {
            0 => {
                core::arch::asm!("mov dr0, {}", in(reg) v, options(nomem, nostack, preserves_flags))
            }
            1 => {
                core::arch::asm!("mov dr1, {}", in(reg) v, options(nomem, nostack, preserves_flags))
            }
            2 => {
                core::arch::asm!("mov dr2, {}", in(reg) v, options(nomem, nostack, preserves_flags))
            }
            3 => {
                core::arch::asm!("mov dr3, {}", in(reg) v, options(nomem, nostack, preserves_flags))
            }
            _ => {
                let v = (v & 0xFFFF_FFFF) | DR6_RESET;
                core::arch::asm!("mov dr6, {}", in(reg) v, options(nomem, nostack, preserves_flags))
            }
        }
    }

    fn has_rdtscp() -> bool {
        // SAFETY: extended CPUID leaf; architectural.
        // KANI-TARGET: guest scheduler CPUID RDTSCP read (outside Proven Core).
        let r = unsafe { cpu::cpuid(0x8000_0001, 0) };
        r.edx & (1 << 27) != 0
    }

    fn has_ibpb() -> bool {
        // SAFETY: CPUID leaf 7; architectural.
        // KANI-TARGET: guest scheduler CPUID IBPB read (outside Proven Core).
        let r = unsafe { cpu::cpuid(7, 0) };
        r.edx & (1 << 26) != 0
    }

    unsafe fn ensure_osxsave() {
        let cr4 = cpu::read_cr4();
        if cr4 & cpu::CR4_OSXSAVE == 0 {
            // SAFETY: OSXSAVE is not fixed-0 under VMX; XSETBV/XSAVE need it.
            // KANI-TARGET: guest scheduler CR4.OSXSAVE (outside Proven Core).
            cpu::write_cr4(cr4 | cpu::CR4_OSXSAVE);
        }
    }

    unsafe fn xsave_all(area: *mut XsaveArea, mask: u64) {
        // SAFETY: `area` is a 64-byte-aligned image at least as large as
        // CPUID.0xD.0:ECX (checked by `can_share`); XCR0 == `mask`.
        // KANI-TARGET: guest scheduler XSAVE (outside Proven Core).
        core::arch::asm!(
            "xsave64 [{p}]",
            p = in(reg) area,
            in("eax") mask as u32,
            in("edx") (mask >> 32) as u32,
            options(nostack),
        );
    }

    unsafe fn xrstor_all(area: *const XsaveArea, mask: u64) {
        // SAFETY: `area` holds an image this CPU wrote with XSAVE under the
        // same mask, or a zero image with a valid MXCSR (standard form,
        // XCOMP_BV zero); XCR0 == `mask`.
        // KANI-TARGET: guest scheduler XRSTOR (outside Proven Core).
        core::arch::asm!(
            "xrstor64 [{p}]",
            p = in(reg) area,
            in("eax") mask as u32,
            in("edx") (mask >> 32) as u32,
            // The host is built soft-float and keeps nothing in vector
            // registers; the clobbers say so to the compiler as well.
            out("xmm0") _, out("xmm1") _, out("xmm2") _, out("xmm3") _,
            out("xmm4") _, out("xmm5") _, out("xmm6") _, out("xmm7") _,
            out("xmm8") _, out("xmm9") _, out("xmm10") _, out("xmm11") _,
            out("xmm12") _, out("xmm13") _, out("xmm14") _, out("xmm15") _,
            options(nostack),
        );
    }

    fn cur_unit() -> usize {
        slot::unit(slot::current(), slot::current_vcpu())
    }

    /// Name a vCPU of `slot` whose VMCS is on core 0, and store it in
    /// `CUR_VCPU[slot]`. vCPU 0 when it is up. Falls back to 0 when every
    /// started vCPU sits on another core; the caller then reports the miss.
    /// Bring every unit of `slot` back, then clear up/assigned/home.
    /// Core 0: recall this slot's remote units, then clear them.
    pub unsafe fn recall_slot(slot_n: usize) {
        recall_and_release(slot_n);
    }

    fn recall_and_release(slot_n: usize) {
        let mut homes = [0usize; slot::MAX_VCPUS];
        let mut n = 0usize;
        for v in 0..slot::MAX_VCPUS {
            let u = slot::unit(slot_n, v);
            let c = crate::arch::host_smp::assigned(u);
            let homed = crate::arch::host_smp::home_of(u) != crate::arch::host_smp::NO_CORE as usize;
            if c != 0 && (slot::vcpu_up(slot_n, v) || homed) && !homes[..n].contains(&c) {
                homes[n] = c;
                n += 1;
            }
        }
        for &core in &homes[..n] {
            crate::arch::host_smp::request_recall(core);
            // SAFETY: caller holds the monitor on the exit path.
            unsafe {
                super::super::yield_monitor_until(Some(core), || {
                    crate::arch::host_smp::recall_finished(core)
                });
            }
        }
        crate::arch::host_smp::release_slot_units(slot_n);
    }

    fn select_home_vcpu(slot_n: usize) -> usize {
        let mut assigned = [0u8; slot::MAX_VCPUS];
        for v in 0..slot::MAX_VCPUS {
            assigned[v] = crate::arch::host_smp::assigned(slot::unit(slot_n, v)) as u8;
        }
        let v = super::home_vcpu(slot::up_mask(slot_n), &assigned).unwrap_or(0);
        slot::set_slot_vcpu(slot_n, v);
        v
    }

    /// Save the vCPU on the CPU. Its VMCS stays loaded and launched.
    /// `cur` is a unit, `(slot, vcpu)`.
    unsafe fn switch_out(cur: usize, idle: bool, now: u64) {
        let s = slot::unit_slot(cur);
        let v = slot::unit_vcpu(cur);
        serial::side_break_stale(now);
        let c = &mut *CTX.p_at(s, v);
        c.gpr = super::super::saved_gprs();
        c.xmm = super::super::saved_xmm();
        c.cr2 = cpu::read_cr2();
        for (i, d) in c.dr.iter_mut().enumerate() {
            *d = read_dr(i as u8);
        }
        c.dr6 = read_dr(6);
        for (i, m) in PASS_MSRS.iter().enumerate() {
            c.msr[i] = cpu::rdmsr(*m);
        }
        if has_rdtscp() {
            c.tsc_aux = cpu::rdmsr(MSR_TSC_AUX);
        }
        c.spec_ctrl = msr_firewall::shadow_read(MSR_SPEC_CTRL);
        c.apic_base = msr_firewall::shadow_read(MSR_APIC_BASE);
        ensure_osxsave();
        let mask = host_xcr0_mask();
        c.xcr0 = cpu::xgetbv(0);
        // Every component the CPU has, not only the ones this guest
        // enabled: the next guest must not find this one's values in a
        // component it enables later.
        cpu::xsetbv(0, mask);
        xsave_all(XSAVE.p_at(s, v), mask);
        HALTED.at(s, v).store(idle, Ordering::Release);
        OFF_TSC.at(s, v).store(now, Ordering::Release);
        SLOT_OFF_TSC.of(s).store(now, Ordering::Release);
        RUN_TSC.at(s, v).fetch_add(
            now.wrapping_sub(SLICE_START.load(Ordering::Acquire)),
            Ordering::AcqRel,
        );
    }

    /// Load unit `next` and make it current. Its VMCS becomes current.
    /// Returns false when the VMCS cannot be loaded.
    unsafe fn switch_in(next: usize, now: u64) -> bool {
        let s = slot::unit_slot(next);
        let v = slot::unit_vcpu(next);
        let prev_slot = slot::current();
        let prev_vcpu = slot::slot_vcpu(prev_slot);
        slot::set_current(s);
        slot::set_current_vcpu(v);
        let vmcs = super::super::slot_vmcs();
        if vmcs == 0 || ops::vmptrld(vmcs).is_err() {
            // The switch did not happen. Put the names back so a later
            // leave does not treat `s` as the guest that was running.
            slot::set_slot_vcpu(prev_slot, prev_vcpu);
            slot::set_current(prev_slot);
            return false;
        }
        let core = crate::arch::host_smp::this_core();
        if core != 0 {
            if !super::super::bind_host_rsp() {
                return false;
            }
            crate::arch::host_smp::note_loaded(core);
        }
        let c = &*CTX.p_at(s, v);
        ensure_osxsave();
        let mask = host_xcr0_mask();
        cpu::xsetbv(0, mask);
        xrstor_all(XSAVE.p_at(s, v), mask);
        cpu::xsetbv(0, (c.xcr0 & mask) | 1);
        super::super::load_saved_gprs(&c.gpr);
        super::super::load_saved_xmm(&c.xmm);
        for (i, d) in c.dr.iter().enumerate() {
            write_dr(i as u8, *d);
        }
        write_dr(6, c.dr6);
        for (i, m) in PASS_MSRS.iter().enumerate() {
            cpu::wrmsr(*m, c.msr[i]);
        }
        if has_rdtscp() {
            cpu::wrmsr(MSR_TSC_AUX, c.tsc_aux & 0xFFFF_FFFF);
        }
        msr_firewall::shadow_write(MSR_SPEC_CTRL, c.spec_ctrl);
        msr_firewall::shadow_write(MSR_APIC_BASE, c.apic_base);
        cpu::write_cr2(c.cr2);
        if has_ibpb() {
            // The next guest must not steer on this one's branch history.
            cpu::wrmsr(MSR_PRED_CMD, 1);
        }
        if prev_slot != s {
            AWAY_TSC.of(s).fetch_add(
                now.wrapping_sub(SLOT_OFF_TSC.of(s).load(Ordering::Acquire)),
                Ordering::AcqRel,
            );
        }
        SLICE_START.store(now, Ordering::Release);
        SWITCHES.fetch_add(1, Ordering::AcqRel);
        RUNS.at(s, v).fetch_add(1, Ordering::AcqRel);
        crate::arch::host_smp::set_home(next, core as u8);
        super::super::rearm_preempt_on_switch_in();
        true
    }

    /// Memory image of a vCPU that has not run. Does not touch live
    /// registers: the caller may still be on another vCPU.
    unsafe fn fill_fresh_ctx(s: usize, v: usize) {
        let c = &mut *CTX.p_at(s, v);
        c.gpr = [0; 15];
        c.xmm = [0; 256];
        c.cr2 = 0;
        c.dr = [0; 4];
        c.dr6 = DR6_RESET;
        c.msr = [0; 5];
        c.tsc_aux = 0;
        c.spec_ctrl = 0;
        c.apic_base = APIC_BASE_RESET;
        c.xcr0 = 1;
        let area = XSAVE.p_at(s, v);
        core::ptr::write_bytes(area as *mut u8, 0, XSAVE_AREA_BYTES);
        // Legacy region offset 24: MXCSR. XRSTOR loads it whenever SSE or
        // AVX is in the mask, and a reserved bit is #GP.
        core::ptr::write_unaligned((area as *mut u8).add(24) as *mut u32, MXCSR_RESET);
    }

    /// Reset state for a guest that is about to boot: no register, debug
    /// register, MSR or extended-state value of the guest that ran before
    /// it survives. The launch path loads RIP/RCX/RDX itself. vCPU 0.
    unsafe fn load_fresh_cpu_state(slot_n: usize) {
        fill_fresh_ctx(slot_n, 0);
        ensure_osxsave();
        let mask = host_xcr0_mask();
        if can_share() {
            cpu::xsetbv(0, mask);
            xrstor_all(XSAVE.p_at(slot_n, 0), mask);
        }
        for i in 0..4u8 {
            write_dr(i, 0);
        }
        write_dr(6, DR6_RESET);
        for m in PASS_MSRS.iter() {
            cpu::wrmsr(*m, 0);
        }
        if has_rdtscp() {
            cpu::wrmsr(MSR_TSC_AUX, 0);
        }
        msr_firewall::shadow_write(MSR_SPEC_CTRL, 0);
        msr_firewall::shadow_write(MSR_APIC_BASE, APIC_BASE_RESET);
        cpu::write_cr2(0);
        if has_ibpb() {
            cpu::wrmsr(MSR_PRED_CMD, 1);
        }
    }

    /// A vCPU that is about to be started by INIT/SIPI. Fills its saved
    /// context and marks it not halted. Does not touch live registers: the
    /// sender is still on the CPU.
    pub unsafe fn ap_unit_ready(s: usize, v: usize, now: u64) {
        fill_fresh_ctx(s, v);
        HALTED.at(s, v).store(false, Ordering::Release);
        OFF_TSC.at(s, v).store(now, Ordering::Release);
    }

    /// A slot other than `not` is waiting to boot.
    fn pending_start(not: usize) -> Option<usize> {
        let lab = LAB_START.load(Ordering::Acquire);
        (0..MAX_GUESTS).find(|&s| {
            s != not
                && !slot::is_on(s)
                && (lab == s + 1
                    || slot::with_slot(s, crate::mgmt::perc_boot_choice::relaunch_pending))
        })
    }

    fn write_dec(mut n: u64) {
        let mut buf = [0u8; 20];
        let mut i = 20;
        if n == 0 {
            serial::write_byte(b'0');
            return;
        }
        while n > 0 && i > 0 {
            i -= 1;
            buf[i] = b'0' + (n % 10) as u8;
            n /= 10;
        }
        for &b in &buf[i..] {
            serial::write_byte(b);
        }
    }

    /// What [`prepare_start`] decided about the slot it readied.
    struct Start {
        /// The scheduler's own restart, not an operator Start.
        auto: bool,
        /// [`super::RESTART_SCHEDULER`] or [`super::RESTART_REBOOT`].
        reason: u8,
    }

    /// Ready the current slot to boot: consume the request, build the
    /// machine if the slot has none, and load reset CPU state. `None` when
    /// the slot cannot boot; it is then marked off so Start can try again.
    unsafe fn prepare_start() -> Option<Start> {
        if !crate::arch::host_smp::assert_core0("prepare_start") {
            return None;
        }
        let s = slot::current();
        let lab = LAB_START.load(Ordering::Acquire) == s + 1;
        if lab {
            LAB_START.store(0, Ordering::Release);
        }
        let _ = crate::mgmt::perc_boot_choice::take_perc_relaunch();
        let auto = AUTO_RESTART.swap(false, Ordering::AcqRel);
        let reason = RESTART_REASON.swap(0, Ordering::AcqRel);
        if !auto {
            RESTARTS.store(0, Ordering::Release);
            super::super::clear_guest_reset_count();
            TAKE_CONSOLE.store(true, Ordering::Release);
        }
        if slot::on_count() != 0 && !can_share() {
            serial::write_line(
                "boot: guest slot refused: the CPU's XSAVE image is larger than a slot holds (ADR-022)",
            );
            crate::mgmt::perc_boot_choice::note_guest_off();
            return None;
        }
        if let Some(block) = crate::arch::host_smp::slot_launch_block(s) {
            crate::arch::host_smp::report_launch_refused(s, block);
            crate::mgmt::perc_boot_choice::note_guest_off();
            return None;
        }
        if !super::super::slot_built() {
            if let Err(why) = super::super::build_slot_machine(lab) {
                serial::write_str("boot: guest slot ");
                write_dec(s as u64);
                serial::write_str(" not built: ");
                serial::write_line(why);
                crate::mgmt::perc_boot_choice::note_guest_off();
                return None;
            }
        }
        load_fresh_cpu_state(s);
        Some(Start { auto, reason })
    }

    /// Boot the slot [`prepare_start`] readied. Ends in VM entry.
    unsafe fn launch(start: Start) -> ! {
        if !crate::arch::host_smp::assert_core0("launch") {
            loop {
                core::hint::spin_loop();
            }
        }
        let s = slot::current();
        LAUNCHING.store(true, Ordering::Release);
        if start.auto {
            audit_log!(AuditEvent::GuestRestarted {
                guest_id: s as u64,
                reason: start.reason,
            });
        } else {
            audit_log!(AuditEvent::VmStarted { guest_id: s as u64 });
        }
        serial::write_str("boot: guest slot ");
        write_dec(s as u64);
        serial::write_str(if start.auto { " boots again" } else { " start" });
        serial::write_str(" guests_on=");
        write_dec(slot::on_count() as u64);
        serial::write_line(" (one guest model, ADR-022; not ISO-INSTALL-OK)");
        super::super::launch_slot_guest(!start.auto)
    }

    /// The idle loop (no guest on the CPU) boots the first slot with a
    /// Start waiting. Returns when nothing is waiting, or when that slot
    /// could not boot.
    pub unsafe fn start_pending_from_idle() {
        let prev = slot::current();
        let Some(s) = pending_start(MAX_GUESTS) else {
            return;
        };
        slot::set_current(s);
        match prepare_start() {
            Some(start) => launch(start),
            None => {
                slot::set_current(prev);
            }
        }
    }

    /// A non-zero core runs only the units homed on it. It does not boot a
    /// slot, tick the page, or leave to the idle loop.
    unsafe fn resume_on_ap(idle: bool) -> ! {
        let core = crate::arch::host_smp::this_core();
        if crate::arch::host_smp::recall_waiting(core) {
            super::super::remote_recall_and_park();
        }
        let cu = cur_unit();
        let now = cpu::rdtsc();
        let hz = tsc_hz();
        let fb = super::super::UART_PACE_TSC_HZ_FALLBACK;
        let used = now.wrapping_sub(SLICE_START.load(Ordering::Acquire));
        let quantum = us_to_tsc(QUANTUM_US, hz, fb);
        let starve = us_to_tsc(STARVE_US, hz, fb);
        let home = |u: usize| crate::arch::host_smp::assigned(u);
        match pick_on_core(core, cu, idle, used, quantum, starve, |u| other(u, now), home) {
            Pick::Stay => {
                if idle {
                    switch_out(cu, true, now);
                    super::super::ap_halt_pause(core, cu);
                }
                super::super::clamp_firmware_preempt();
                super::super::enter_guest()
            }
            Pick::Switch(next) => {
                switch_out(cu, idle, now);
                if !switch_in(next, now) {
                    super::super::remote_recall_and_park();
                }
                super::super::enter_guest()
            }
        }
    }

    /// Load `unit` and enter it. The caller holds the monitor.
    pub unsafe fn launch_on_core(core: usize) -> bool {
        if crate::arch::host_smp::recall_waiting(core) {
            super::super::remote_recall_and_park();
        }
        let Some(unit) = (0..MAX_UNITS).find(|&u| {
            crate::arch::host_smp::assigned(u) == core
                && slot::is_on(slot::unit_slot(u))
                && slot::vcpu_up(slot::unit_slot(u), slot::unit_vcpu(u))
        }) else {
            return false;
        };
        if !switch_in(unit, cpu::rdtsc()) {
            return false;
        }
        resume(false)
    }

    /// After a halt pause, load `unit` again and enter it.
    pub unsafe fn reenter(unit: usize) -> ! {
        if !switch_in(unit, cpu::rdtsc()) {
            super::super::remote_recall_and_park();
        }
        super::super::enter_guest()
    }

    /// End of a VM exit: enter the guest that should run now.
    ///
    /// `idle` is [`guest_idle`] for the guest that took the exit. With one
    /// guest on and no Start waiting this is `VMRESUME`.
    pub unsafe fn resume(idle: bool) -> ! {
        let core_now = crate::arch::host_smp::this_core();
        if crate::arch::host_smp::running_remote(core_now) {
            resume_on_ap(idle);
        }
        let cu = cur_unit();
        if let Some(s) = pending_start(slot::current()) {
            let now = cpu::rdtsc();
            switch_out(cu, idle, now);
            // The new guest's first lines are written straight to COM2.
            // Whatever this guest still has queued goes out first.
            let _ = serial::flush_guest_tx();
            slot::set_current(s);
            if let Some(start) = prepare_start() {
                launch(start)
            }
            // The new slot could not boot. Give the CPU back.
            if switch_in(cu, cpu::rdtsc()) {
                super::super::enter_guest()
            }
            super::super::leave_to_e4()
        }
        if !slot::cpu_shared() {
            super::super::enter_guest()
        }
        let now = cpu::rdtsc();
        let hz = tsc_hz();
        let fb = super::super::UART_PACE_TSC_HZ_FALLBACK;
        let used = now.wrapping_sub(SLICE_START.load(Ordering::Acquire));
        let quantum = us_to_tsc(QUANTUM_US, hz, fb);
        let starve = us_to_tsc(STARVE_US, hz, fb);
        beat(slot::current(), now, us_to_tsc(BEAT_US, hz, fb));
        crate::arch::host_smp::maybe_core_line(now, us_to_tsc(BEAT_US, hz, fb));
        let home = |u: usize| crate::arch::host_smp::assigned(u);
        match pick_on_core(0, cu, idle, used, quantum, starve, |u| other(u, now), home) {
            Pick::Stay => {
                super::super::clamp_firmware_preempt();
                super::super::enter_guest()
            }
            Pick::Switch(next) => {
                switch_out(cu, idle, now);
                if !switch_in(next, now) {
                    serial::write_line_nowait(
                        "boot: guest switch failed: VMPTRLD refused the next guest's VMCS",
                    );
                    super::super::leave_to_e4()
                }
                if slot::unit_slot(next) != slot::unit_slot(cu) {
                    if !SWITCH_LOGGED.swap(true, Ordering::AcqRel) {
                        serial::write_line_nowait(
                            "boot: guest scheduler: two guests share the CPU (2 ms quantum; ADR-022; not ISO-INSTALL-OK)",
                        );
                    }
                } else if !VCPU_SHARE_LOGGED.swap(true, Ordering::AcqRel) {
                    serial::write_line_nowait(
                        "boot: guest scheduler: vCPUs share the CPU (2 ms quantum; ADR-023; not ISO-INSTALL-OK)",
                    );
                }
                super::super::enter_guest()
            }
        }
    }

    /// One COM2 line every [`BEAT_US`] while guests share the CPU: how
    /// often each one ran and for how long. Written only while the console
    /// owner is on the CPU so it is not printed as another guest's line.
    fn beat(cur: usize, now: u64, period: u64) {
        if cur != slot::console_owner() {
            return;
        }
        let last = BEAT_TSC.load(Ordering::Acquire);
        if last == 0 {
            BEAT_TSC.store(now, Ordering::Release);
            return;
        }
        if now.wrapping_sub(last) < period {
            return;
        }
        BEAT_TSC.store(now, Ordering::Release);
        let hz = match tsc_hz() {
            0 => super::super::UART_PACE_TSC_HZ_FALLBACK,
            h => h,
        };
        serial::write_str_nowait("boot: guests on=");
        write_dec_nowait(slot::on_count() as u64);
        serial::write_str_nowait(" switches=");
        write_dec_nowait(switches());
        for s in 0..MAX_GUESTS {
            if !slot::is_on(s) {
                continue;
            }
            serial::write_str_nowait(" g");
            write_dec_nowait(s as u64);
            let mut runs = 0u64;
            let mut run_tsc = 0u64;
            for v in 0..slot::MAX_VCPUS {
                runs = runs.wrapping_add(RUNS.at(s, v).load(Ordering::Acquire));
                run_tsc = run_tsc.wrapping_add(RUN_TSC.at(s, v).load(Ordering::Acquire));
            }
            serial::write_str_nowait(" runs=");
            write_dec_nowait(runs);
            serial::write_str_nowait(" cpu_ms=");
            write_dec_nowait(run_tsc / (hz / 1000).max(1));
        }
        serial::write_line_nowait(" (ADR-022; not ISO-INSTALL-OK)");
    }

    fn write_dec_nowait(mut n: u64) {
        let mut buf = [0u8; 20];
        let mut i = 20;
        if n == 0 {
            serial::write_byte_nowait(b'0');
            return;
        }
        while n > 0 && i > 0 {
            i -= 1;
            buf[i] = b'0' + (n % 10) as u8;
            n /= 10;
        }
        for &b in &buf[i..] {
            serial::write_byte_nowait(b);
        }
    }

    unsafe fn other(u: usize, now: u64) -> Other {
        let s = slot::unit_slot(u);
        let v = slot::unit_vcpu(u);
        if crate::arch::host_smp::home_req_pending(s) {
            return Other::default();
        }
        if !slot::is_on(s) || !slot::vcpu_up(s, v) {
            return Other::default();
        }
        let halted = HALTED.at(s, v).load(Ordering::Acquire);
        let waited_tsc = now.wrapping_sub(OFF_TSC.at(s, v).load(Ordering::Acquire));
        let work = halted
            && (slot::take_wake_vcpu(s, v)
                || slot::with_vcpu(s, v, || {
                    (v == 0 && crate::mgmt::perc_boot_choice::stop_pending())
                        || super::super::guest_irq_waiting(now)
                }));
        Other {
            on: true,
            halted,
            work,
            waited_tsc,
        }
    }

    /// The launch path is about to enter the current slot's guest.
    pub fn mark_running() {
        let cur = slot::current();
        LAUNCHING.store(false, Ordering::Release);
        slot::set_on(cur, true);
        let owner = slot::console_owner();
        if TAKE_CONSOLE.swap(false, Ordering::AcqRel) || owner == cur || !slot::is_on(owner) {
            slot::set_console_owner(cur);
        }
        HALTED.store(false, Ordering::Release);
        SLICE_START.store(cpu::rdtsc(), Ordering::Release);
    }

    /// The current slot's guest is leaving the CPU for good (Stop, power
    /// off, a fault, a launch that failed).
    ///
    /// A product guest that the operator did not stop is booted again once
    /// for each Start ([`leave_plan`]); after that it is marked off and
    /// Start boots it. When another guest is still on, the CPU goes to it
    /// and this never returns. When it was the last guest this returns and
    /// the caller leaves to the idle loop, which boots a waiting slot.
    ///
    /// The lab guest (no product media) and a slot the scheduler never
    /// launched are not touched: the caller continues as it always did.
    ///
    /// Returns whether the caller should idle. `false` means another guest
    /// is still on and the switch onto it failed: that guest stays on.
    pub unsafe fn on_guest_left() -> bool {
        if !crate::arch::host_smp::assert_core0("on_guest_left") {
            return true;
        }
        let cur = slot::current();
        serial::side_break();
        let was_on = slot::is_on(cur);
        let failed_launch = LAUNCHING.swap(false, Ordering::AcqRel);
        // Recall every core that still holds this slot, then drop the
        // unit words. set_on(false) is the on-bit only.
        recall_and_release(cur);
        slot::set_on(cur, false);
        if !was_on && !failed_launch {
            return true;
        }
        let next = slot::first_on_except(cur);
        if next.is_none() && !crate::devices::ide_cdrom::product_iso_window_armed() {
            return true;
        }
        let plan = if was_on {
            leave_plan(
                crate::mgmt::perc_boot_choice::guest_stopped(),
                RESTARTS.load(Ordering::Acquire),
            )
        } else {
            // It never entered. Do not loop on it.
            Leave::Off
        };
        match plan {
            Leave::StayOff => {}
            Leave::Restart => {
                RESTARTS.fetch_add(1, Ordering::AcqRel);
                RESTART_REASON.store(super::RESTART_SCHEDULER, Ordering::Release);
                AUTO_RESTART.store(true, Ordering::Release);
                crate::mgmt::perc_boot_choice::note_perc_relaunch();
            }
            Leave::Off => crate::mgmt::perc_boot_choice::note_guest_off(),
        }
        audit_log!(AuditEvent::VmStopped {
            guest_id: cur as u64
        });
        if was_on
            && LAB_AGAIN
                .compare_exchange(cur + 1, 0, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
        {
            // What the Guests page does on Start of a stopped row.
            let _ = crate::mgmt::perc_boot_choice::rearm_guest_after_stop();
            LAB_START.store(cur + 1, Ordering::Release);
            serial::write_line(
                "boot: lab second guest powered off and is started again (twoguest.txt; ADR-022; not ISO-INSTALL-OK)",
            );
        }
        let Some(next) = next else {
            serial::write_str("boot: guest slot ");
            serial::write_byte(b'0' + cur as u8);
            serial::write_line(match plan {
                Leave::StayOff => " is off (ADR-022; not ISO-INSTALL-OK)",
                Leave::Restart => " left the CPU and boots again (ADR-022; not ISO-INSTALL-OK)",
                Leave::Off => {
                    " left the CPU and is off; Start boots it (ADR-022; not ISO-INSTALL-OK)"
                }
            });
            return true;
        };
        let vmcs = super::super::slot_vmcs();
        if vmcs != 0 {
            let _ = ops::vmclear(vmcs);
        }
        serial::set_linux_earlycon_share(false);
        serial::set_linux_high_half(false);
        if slot::console_owner() == cur {
            slot::set_console_owner(next);
        }
        let now = cpu::rdtsc();
        let home_v = select_home_vcpu(next);
        if !switch_in(slot::unit(next, home_v), now) {
            // The remaining guest keeps its units and its on-bit. Idling
            // here is what restarted slot 0 after Stop of slot 1.
            crate::arch::host_smp::report_switch_kept(next, home_v);
            slot::set_current(next);
            return false;
        }
        serial::write_str_nowait("boot: guest slot ");
        serial::write_byte_nowait(b'0' + cur as u8);
        serial::write_str_nowait(match plan {
            Leave::StayOff => " is off; ",
            Leave::Restart => " left the CPU and boots again; ",
            Leave::Off => " left the CPU and is off; ",
        });
        serial::write_byte_nowait(b'0' + slot::on_count() as u8);
        serial::write_line_nowait(" guest(s) still on (ADR-022; not ISO-INSTALL-OK)");
        super::super::enter_guest()
    }
}

#[cfg(test)]
#[path = "guest_sched_test.rs"]
mod guest_sched_test;
