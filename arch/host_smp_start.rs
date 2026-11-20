//! Bring other host cores to VMX root, then sleep (ADR-024).
//!
//! Core 0 writes the BSP registers before the first SIPI. Each AP reads them
//! and writes only `AP_STATE[core]` and the TSC handshake words.

use core::sync::atomic::{AtomicU32, AtomicU64, AtomicU8, Ordering};

use super::{apic_id, ap_base, low_base, n_ap, stack_top, x2apic, BLOCK_BYTES, MAX_CORES};
use crate::arch::cpu::{self, DescriptorTablePtr};
use crate::arch::host_smp_tramp;
use crate::vmx::VmxLifecycle;

const AP_STARTED: u8 = 1;
const AP_ONLINE: u8 = 2;
const AP_BAD_APIC: u8 = 3;
const AP_BAD_TSC: u8 = 4;
const AP_NO_VMX: u8 = 5;

static AP_STATE: [AtomicU8; MAX_CORES] = [const { AtomicU8::new(0) }; MAX_CORES];
static HAS_MWAIT: AtomicU8 = AtomicU8::new(0);

static BSP_CR0: AtomicU64 = AtomicU64::new(0);
static BSP_CR3: AtomicU64 = AtomicU64::new(0);
static BSP_CR4: AtomicU64 = AtomicU64::new(0);
static BSP_EFER: AtomicU64 = AtomicU64::new(0);
static BSP_PAT: AtomicU64 = AtomicU64::new(0);
static BSP_XCR0: AtomicU64 = AtomicU64::new(0);
static BSP_IDT_LIMIT: AtomicU64 = AtomicU64::new(0);
static BSP_IDT_BASE: AtomicU64 = AtomicU64::new(0);
static TSC_ROUND: AtomicU64 = AtomicU64::new(0);
static TSC_T0: AtomicU64 = AtomicU64::new(0);
static TSC_T1: AtomicU64 = AtomicU64::new(0);
/// 0 idle, 1 core 0 is sampling, 2 the sample is finished.
static TSC_GO: AtomicU8 = AtomicU8::new(0);
/// Added to this core's IA32_TSC_ADJUST when TSC_SYNC is 1. 2 means applied.
static TSC_DELTA: AtomicU64 = AtomicU64::new(0);
static TSC_SYNC: AtomicU8 = AtomicU8::new(0);
static APIC_SEEN: AtomicU32 = AtomicU32::new(0);

#[repr(C, align(64))]
struct Mail {
    word: AtomicU64,
}
static MAILBOX: [Mail; MAX_CORES] = [const { Mail { word: AtomicU64::new(0) } }; MAX_CORES];

/// Switch off the trampoline's temporary CR3 onto core 0's tables, then enter Rust.
#[unsafe(naked)]
unsafe extern "C" fn ap_entry() -> ! {
    core::arch::naked_asm!(
        "mov rax, qword ptr [rip + {cr3}]",
        "mov cr3, rax",
        "xor rbp, rbp",
        "sub rsp, 32",
        "call {main}",
        "2:",
        "jmp 2b",
        cr3 = sym BSP_CR3_SLOT,
        main = sym ap_main,
    );
}

#[unsafe(no_mangle)]
static BSP_CR3_SLOT: AtomicU64 = AtomicU64::new(0);

extern "C" fn ap_main(core: usize) -> ! {
    // UEFI x64 passes the first argument in RCX. The trampoline sets RCX
    // and RDI to the core index.
    if core >= MAX_CORES {
        loop {
            core::hint::spin_loop();
        }
    }
    // SAFETY: core 0 published these registers before SIPI. This core has
    // not enabled interrupts. KANI-TARGET: host_smp ap_main regs
    unsafe {
        let idtr = DescriptorTablePtr {
            limit: BSP_IDT_LIMIT.load(Ordering::Acquire) as u16,
            base: BSP_IDT_BASE.load(Ordering::Acquire),
        };
        core::arch::asm!("lidt [{}]", in(reg) &idtr, options(readonly, nostack));
        cpu::write_cr0(BSP_CR0.load(Ordering::Acquire));
        let cr4 = BSP_CR4.load(Ordering::Acquire) & !cpu::CR4_VMXE;
        cpu::write_cr4(cr4);
        // Bit 10 (LMA) is read-only. The BSP value has it set once paging is
        // on, and WRMSR of that bit #GPs. The firmware IDT then swallows the
        // AP, so core 0 only sees "no answer".
        let efer = BSP_EFER.load(Ordering::Acquire) & !(1 << 10);
        cpu::wrmsr(0xC000_0080, efer);
        cpu::wrmsr(0x277, BSP_PAT.load(Ordering::Acquire));
        // UEFI often leaves OSXSAVE clear (CR4=0x2668). XSETBV #UDs then.
        let xcr0 = BSP_XCR0.load(Ordering::Acquire);
        if cr4 & cpu::CR4_OSXSAVE != 0 && xcr0 != 0 {
            cpu::xsetbv(0, xcr0);
        }
        core::arch::asm!("fninit", options(nostack, preserves_flags));
    }
    if let Err(code) = ap_lapic(core) {
        AP_STATE[core].store(code, Ordering::Release);
        loop {
            core::hint::spin_loop();
        }
    }
    AP_STATE[core].store(AP_STARTED, Ordering::Release);
    if !tsc_handshake_ap() {
        AP_STATE[core].store(AP_BAD_TSC, Ordering::Release);
        loop {
            core::hint::spin_loop();
        }
    }
    let vmxon = ap_base() + ((core - 1) as u64) * BLOCK_BYTES;
    let mut life = VmxLifecycle::new();
    if life.enable(vmxon).is_err() {
        AP_STATE[core].store(AP_NO_VMX, Ordering::Release);
        loop {
            core::hint::spin_loop();
        }
    }
    // The lifecycle stays on this stack for the life of the core.
    core::mem::forget(life);
    AP_STATE[core].store(AP_ONLINE, Ordering::Release);
    unsafe { ap_idle(core) }
}

fn ap_lapic(core: usize) -> Result<(), u8> {
    // SAFETY: this core's local APIC. No other core programs it.
    // KANI-TARGET: host_smp ap lapic
    unsafe {
        if x2apic() {
            let base = cpu::rdmsr(crate::arch::apic::IA32_APIC_BASE);
            if base & crate::arch::apic::APIC_BASE_X2APIC == 0 {
                cpu::wrmsr(
                    crate::arch::apic::IA32_APIC_BASE,
                    base | crate::arch::apic::APIC_BASE_ENABLE | crate::arch::apic::APIC_BASE_X2APIC,
                );
            }
            cpu::wrmsr(0x80F, u64::from((1u32 << 8) | 0xFF));
            cpu::wrmsr(0x808, 0);
            // x2APIC MSR 0x802 holds the id in bits 31:0. The xAPIC
            // shift (>> 24) reads 0 for every id on this R640 and the
            // core is refused before the TSC sample (iron 70afcdf9).
            let id = super::x2apic_id(cpu::rdmsr(0x802));
            if id != apic_id(core) {
                APIC_SEEN.store(id, Ordering::Release);
                return Err(AP_BAD_APIC);
            }
        } else {
            let base = cpu::rdmsr(crate::arch::apic::IA32_APIC_BASE)
                & crate::arch::apic::APIC_BASE_ADDR_MASK;
            let base = if base == 0 {
                crate::arch::apic::DEFAULT_APIC_PHYS
            } else {
                base
            };
            let svr = core::ptr::read_volatile((base + 0xF0) as *const u32);
            core::ptr::write_volatile((base + 0xF0) as *mut u32, svr | (1 << 8) | 0xFF);
            core::ptr::write_volatile((base + 0x80) as *mut u32, 0);
            let id = super::xapic_id(core::ptr::read_volatile((base + 0x20) as *const u32));
            if id != apic_id(core) {
                APIC_SEEN.store(id, Ordering::Release);
                return Err(AP_BAD_APIC);
            }
        }
    }
    Ok(())
}

fn tsc_handshake_ap() -> bool {
    // Iron a9301533682b: every AP's IA32_TSC_ADJUST differed from core 0, so
    // requiring equal offsets left online=1 of 16. The value that matters is
    // rdtsc. A constant offset is applied once, below, when core 0 asks.
    // SAFETY: CPUID leaf 7 is architectural. This core, before VMXON.
    // KANI-TARGET: host_smp tsc leaf
    let has_adj = unsafe { cpu::cpuid(7, 0).ebx & (1 << 1) != 0 };
    for _ in 0..8 {
        if TSC_GO.load(Ordering::Acquire) == 2 {
            return true;
        }
        let round = TSC_ROUND.load(Ordering::Acquire);
        while TSC_ROUND.load(Ordering::Acquire) == round && TSC_GO.load(Ordering::Acquire) != 2 {
            core::hint::spin_loop();
        }
        if TSC_GO.load(Ordering::Acquire) == 2 {
            return true;
        }
        TSC_T1.store(cpu::rdtsc().max(1), Ordering::Release);
        while TSC_ROUND.load(Ordering::Acquire) == round.wrapping_add(1)
            && TSC_GO.load(Ordering::Acquire) != 2
        {
            core::hint::spin_loop();
        }
        if TSC_GO.load(Ordering::Acquire) == 2 {
            return true;
        }
        if has_adj && TSC_SYNC.load(Ordering::Acquire) == 1 {
            let delta = TSC_DELTA.load(Ordering::Acquire);
            // SAFETY: this core's IA32_TSC_ADJUST, before VMXON. Core 0
            // published `delta` under TSC_SYNC. KANI-TARGET: host_smp tsc adjust
            unsafe {
                let adj = cpu::rdmsr(0x3B);
                cpu::wrmsr(0x3B, adj.wrapping_add(delta));
            }
            TSC_SYNC.store(2, Ordering::Release);
        }
    }
    true
}

pub(super) fn wake_core(core: usize) {
    if core == 0 || core >= MAX_CORES {
        return;
    }
    MAILBOX[core].word.store(1, Ordering::Release);
}

/// Sleep until [`wake_core`] writes this core's mailbox, then clear it.
///
/// # Safety
/// `core` is the caller. MONITOR/MWAIT address this core's mailbox line.
pub(super) unsafe fn wait_mailbox(core: usize) {
    if core >= MAX_CORES {
        return;
    }
    let mwait = HAS_MWAIT.load(Ordering::Acquire) != 0;
    while MAILBOX[core].word.load(Ordering::Acquire) == 0 {
        if mwait {
            let addr = &MAILBOX[core].word as *const AtomicU64 as u64;
            // SAFETY: MONITOR/MWAIT on this core's own mailbox line.
            // KANI-TARGET: host_smp mwait
            core::arch::asm!(
                "monitor",
                in("rax") addr,
                in("rcx") 0u64,
                in("rdx") 0u64,
                options(nostack, preserves_flags)
            );
            if MAILBOX[core].word.load(Ordering::Acquire) == 0 {
                core::arch::asm!(
                    "mwait",
                    in("rax") 0u64,
                    in("rcx") 0u64,
                    options(nostack, preserves_flags)
                );
            }
        } else {
            core::hint::spin_loop();
        }
    }
    MAILBOX[core].word.store(0, Ordering::Release);
}

/// The highest online core stays in C0 and writes the pulse every two
/// seconds. It does not take the monitor. The mailbox word is cleared
/// only after a wake stores into it.
fn watchdog_wait(core: usize) {
    let hz = crate::boot::raynu_f_flag::tsc_hz().max(1_000_000);
    let gap = hz.saturating_mul(2);
    let mut next = cpu::rdtsc();
    while MAILBOX[core].word.load(Ordering::Acquire) == 0 {
        let now = cpu::rdtsc();
        if now.wrapping_sub(next) >= gap {
            super::emit_core0_pulse(now);
            next = now;
        }
        core::hint::spin_loop();
    }
    MAILBOX[core].word.store(0, Ordering::Release);
}

unsafe fn ap_idle(core: usize) -> ! {
    // `cores_online` is published after the last AP has entered here.
    // Spin briefly so that AP is still here when the count lands.
    let hz = crate::boot::raynu_f_flag::tsc_hz().max(1_000_000);
    let start = cpu::rdtsc();
    while super::cores_online() <= 1 && cpu::rdtsc().wrapping_sub(start) < hz / 20 {
        core::hint::spin_loop();
    }
    loop {
        if super::is_watchdog_core(core) {
            watchdog_wait(core);
        } else {
            wait_mailbox(core);
        }
        crate::vmx::guest_uefi::on_ap_wake(core);
    }
}

/// INIT, SIPI, SIPI each core that the probe reserved. One at a time.
///
/// # Safety
/// Core 0, after its own VMXON, identity map still the firmware map.
pub unsafe fn start_cores() {
    if n_ap() == 0 || low_base() == 0 || ap_base() == 0 {
        write_online(1, 1);
        return;
    }
    let leaf1 = cpu::cpuid(1, 0);
    HAS_MWAIT.store(if leaf1.ecx & (1 << 3) != 0 { 1 } else { 0 }, Ordering::Release);
    let cr0 = cpu::read_cr0();
    let cr3 = cpu::read_cr3();
    let cr4 = cpu::read_cr4();
    let efer = cpu::rdmsr(0xC000_0080);
    let pat = cpu::rdmsr(0x277);
    // UEFI leaves CR4.OSXSAVE clear. XGETBV #UDs the BSP before any SIPI
    // (nested d7291acc, CR4=0x2668, RIP on xgetbv). Same rule as the AP.
    let xcr0 = if cr4 & cpu::CR4_OSXSAVE != 0 {
        cpu::xgetbv(0)
    } else {
        0
    };
    let idtr = cpu::sidt();
    BSP_CR0.store(cr0, Ordering::Release);
    BSP_CR3_SLOT.store(cr3, Ordering::Release);
    BSP_CR3.store(cr3, Ordering::Release);
    BSP_CR4.store(cr4, Ordering::Release);
    BSP_EFER.store(efer, Ordering::Release);
    BSP_PAT.store(pat, Ordering::Release);
    BSP_XCR0.store(xcr0, Ordering::Release);
    BSP_IDT_LIMIT.store(u64::from(idtr.limit), Ordering::Release);
    BSP_IDT_BASE.store(idtr.base, Ordering::Release);

    let pdpt = low_base() + 0x2000;
    let pd = low_base() + 0x3000;
    let Some((pml4, pdpt_e, pd_e)) = super::build_boot_tables(cr3, pdpt, pd, read_table) else {
        crate::boot::serial::write_line("boot: B21 host cores stay 1: page tables (ADR-024)");
        write_online(1, n_ap() + 1);
        return;
    };
    write_table(low_base() + 0x1000, &pml4);
    write_table(pdpt, &pdpt_e);
    write_table(pd, &pd_e);
    let efer_bits = if efer & (1 << 11) != 0 { 0x901 } else { 0x100 };
    host_smp_tramp::install(
        low_base() as *mut u8,
        low_base(),
        (low_base() + 0x1000) as u32,
        efer_bits as u32,
    );

    let hz = crate::boot::raynu_f_flag::tsc_hz().max(1);
    let mut online = 1usize;
    for core in 1..=n_ap() {
        host_smp_tramp::patch_core(
            low_base() as *mut u8,
            stack_top(core, ap_base(), 0),
            core as u64,
            ap_entry as *const () as usize as u64,
        );
        let id = apic_id(core);
        TSC_GO.store(0, Ordering::Release);
        send_init_sipi(id, low_base());
        let started = wait_state(core, AP_STARTED, hz, 100_000);
        let early = AP_STATE[core].load(Ordering::Acquire);
        // BAD_APIC is numerically above STARTED. Treating that as "started"
        // ran the TSC wait and printed `not used: tsc` for an id mismatch.
        let done = if early == AP_BAD_APIC || early == AP_BAD_TSC || early == AP_NO_VMX {
            early
        } else if early == AP_STARTED {
            let tsc_ok = tsc_rounds_ok(core);
            let s = wait_state_done(core, hz, 200_000);
            if tsc_ok { s } else { AP_BAD_TSC }
        } else {
            early
        };
        if done == AP_ONLINE {
            online += 1;
            crate::boot::serial::write_str("boot: B21 host core ");
            write_dec(core as u64);
            crate::boot::serial::write_str(" apic=0x");
            write_hex(u64::from(id));
            crate::boot::serial::write_line(" online (VMXON ok)");
            crate::audit::integrity::record_event(crate::audit::AuditEvent::VmxEnabled {
                vcpu_id: core as u32,
            });
        } else {
            let why = if !started {
                "no answer"
            } else if done == AP_BAD_APIC {
                "apic id"
            } else if done == AP_BAD_TSC {
                "tsc"
            } else {
                "vmxon"
            };
            crate::boot::serial::write_str("boot: B21 host core ");
            write_dec(core as u64);
            crate::boot::serial::write_str(" apic=0x");
            write_hex(u64::from(id));
            crate::boot::serial::write_str(" not used: ");
            crate::boot::serial::write_str(why);
            if done == AP_BAD_APIC {
                crate::boot::serial::write_str(" saw=0x");
                write_hex(u64::from(APIC_SEEN.load(Ordering::Acquire)));
            }
            crate::boot::serial::write_line("");
        }
    }
    write_online(online, n_ap() + 1);
}

fn write_online(online: usize, use_n: usize) {
    super::set_cores_online(online);
    crate::boot::serial::write_str("boot: B21 host cores online=");
    write_dec(online as u64);
    crate::boot::serial::write_str(" of ");
    write_dec(use_n as u64);
    crate::boot::serial::write_line(" (ADR-024; not ISO-INSTALL-OK)");
}

fn read_table(pa: u64) -> [u64; 512] {
    let mut out = [0u64; 512];
    unsafe {
        core::ptr::copy_nonoverlapping(pa as *const u64, out.as_mut_ptr(), 512);
    }
    out
}

fn write_table(pa: u64, entries: &[u64; 512]) {
    unsafe {
        core::ptr::copy_nonoverlapping(entries.as_ptr(), pa as *mut u64, 512);
    }
}

fn send_init_sipi(apic_id: u32, low: u64) {
    let page = (low >> 12) as u32;
    let hz = crate::boot::raynu_f_flag::tsc_hz().max(1);
    unsafe {
        icr(apic_id, 0x4500);
    }
    wait_us(hz, 10_000);
    unsafe {
        icr(apic_id, 0x4600 | page);
    }
    wait_us(hz, 200);
    unsafe {
        icr(apic_id, 0x4600 | page);
    }
    wait_us(hz, 200);
}

unsafe fn icr(apic_id: u32, cmd: u32) {
    if x2apic() {
        cpu::wrmsr(0x830, (u64::from(apic_id) << 32) | u64::from(cmd));
        return;
    }
    let base = cpu::rdmsr(crate::arch::apic::IA32_APIC_BASE) & crate::arch::apic::APIC_BASE_ADDR_MASK;
    let base = if base == 0 {
        crate::arch::apic::DEFAULT_APIC_PHYS
    } else {
        base
    };
    core::ptr::write_volatile((base + 0x310) as *mut u32, apic_id << 24);
    core::ptr::write_volatile((base + 0x300) as *mut u32, cmd);
    for _ in 0..100_000 {
        let v = core::ptr::read_volatile((base + 0x300) as *const u32);
        if v & (1 << 12) == 0 {
            break;
        }
        core::hint::spin_loop();
    }
}

fn wait_us(hz: u64, us: u64) {
    let ticks = hz.saturating_mul(us) / 1_000_000;
    let t0 = cpu::rdtsc();
    while cpu::rdtsc().wrapping_sub(t0) < ticks {
        core::hint::spin_loop();
    }
}

fn wait_state(core: usize, want: u8, hz: u64, us: u64) -> bool {
    let ticks = hz.saturating_mul(us) / 1_000_000;
    let t0 = cpu::rdtsc();
    while cpu::rdtsc().wrapping_sub(t0) < ticks {
        if AP_STATE[core].load(Ordering::Acquire) >= want {
            return true;
        }
        core::hint::spin_loop();
    }
    false
}

fn wait_state_done(core: usize, hz: u64, us: u64) -> u8 {
    let ticks = hz.saturating_mul(us) / 1_000_000;
    let t0 = cpu::rdtsc();
    loop {
        let s = AP_STATE[core].load(Ordering::Acquire);
        if s >= AP_ONLINE {
            return s;
        }
        if cpu::rdtsc().wrapping_sub(t0) >= ticks {
            return s;
        }
        core::hint::spin_loop();
    }
}

/// Core 0 side of the TSC check. Three samples within 1ms of core 0.
/// A missed sample is retried. One sample outside that window asks the
/// other core to add the offset to `IA32_TSC_ADJUST`; three fresh samples
/// then have to land inside the window. A clock that is still outside is
/// not used.
fn tsc_rounds_ok(core: usize) -> bool {
    let hz = crate::boot::raynu_f_flag::tsc_hz().max(1);
    let slack = (hz / 1000).max(1);
    // SAFETY: CPUID leaf 7 is architectural. Core 0.
    // KANI-TARGET: host_smp tsc leaf
    let has_adj = unsafe { cpu::cpuid(7, 0).ebx & (1 << 1) != 0 };
    TSC_GO.store(1, Ordering::Release);
    TSC_SYNC.store(0, Ordering::Release);
    let mut good = 0u32;
    let mut synced = false;
    let mut passed = false;
    for _ in 0..8 {
        TSC_T1.store(0, Ordering::Release);
        let t0 = cpu::rdtsc().max(1);
        TSC_T0.store(t0, Ordering::Release);
        TSC_ROUND.fetch_add(1, Ordering::Release);
        let ticks = hz.saturating_mul(250_000) / 1_000_000;
        let start = cpu::rdtsc();
        while TSC_T1.load(Ordering::Acquire) == 0 {
            if cpu::rdtsc().wrapping_sub(start) > ticks {
                break;
            }
            core::hint::spin_loop();
        }
        let t1 = TSC_T1.load(Ordering::Acquire);
        let t2 = cpu::rdtsc();
        let off = t1 != 0
            && (t1.saturating_add(slack) < t0 || t1 > t2.saturating_add(slack));
        if off && !synced && has_adj {
            let delta = super::tsc_sync_delta(t0, t1);
            TSC_DELTA.store(delta, Ordering::Release);
            TSC_SYNC.store(1, Ordering::Release);
            TSC_ROUND.fetch_add(1, Ordering::Release);
            let wait = cpu::rdtsc();
            while TSC_SYNC.load(Ordering::Acquire) != 2 {
                if cpu::rdtsc().wrapping_sub(wait) > ticks {
                    break;
                }
                core::hint::spin_loop();
            }
            if TSC_SYNC.load(Ordering::Acquire) != 2 {
                break;
            }
            TSC_SYNC.store(0, Ordering::Release);
            synced = true;
            good = 0;
            crate::boot::serial::write_str("boot: B21 host core ");
            write_dec(core as u64);
            crate::boot::serial::write_str(" tsc sync delta=0x");
            write_hex(delta);
            crate::boot::serial::write_line("");
            continue;
        }
        TSC_ROUND.fetch_add(1, Ordering::Release);
        if t1 == 0 {
            continue;
        }
        if off {
            break;
        }
        good += 1;
        if good == 3 {
            passed = true;
            break;
        }
    }
    TSC_GO.store(2, Ordering::Release);
    passed
}

fn write_dec(mut v: u64) {
    let mut tmp = [0u8; 20];
    let mut k = tmp.len();
    if v == 0 {
        k -= 1;
        tmp[k] = b'0';
    }
    while v > 0 {
        k -= 1;
        tmp[k] = b'0' + (v % 10) as u8;
        v /= 10;
    }
    let s = core::str::from_utf8(&tmp[k..]).unwrap_or("0");
    crate::boot::serial::write_str(s);
}

fn write_hex(mut v: u64) {
    let mut tmp = [0u8; 16];
    let mut k = tmp.len();
    if v == 0 {
        k -= 1;
        tmp[k] = b'0';
    }
    while v > 0 {
        k -= 1;
        let d = (v & 0xF) as u8;
        tmp[k] = if d < 10 { b'0' + d } else { b'a' + (d - 10) };
        v >>= 4;
    }
    let s = core::str::from_utf8(&tmp[k..]).unwrap_or("0");
    crate::boot::serial::write_str(s);
}
