//! Virtual local APIC for M3.11/M3.12 (x2APIC MSRs + xAPIC MMIO via EPT hole).
//!
//! Pillar: [Z] · Proven Core: **outside** (ADR-002)
//!
//! M3.12:
//! - Real `CUR_COUNT` so `calibrate_APIC_clock` can succeed.
//! - IRR → ISR on deliver, EOI clears ISR (fixes Latitude
//!   `Fatal exception in interrupt` from bare VM-entry inject).
//! - Host LAPIC one-shot still wakes the hypervisor; guest sees LVT vector
//!   only through the IRR/ISR path (`RAYNU-V-M3-APIC-OK`).

use crate::guest::slot::{self, PerVcpu, PerVcpuCell};
use crate::{per_vcpu, per_vcpu_cell};
use core::sync::atomic::{AtomicBool, AtomicU16, Ordering};

use crate::arch::apic::DEFAULT_APIC_PHYS;
use crate::arch::cpu;

/// COM1 marker when the virtual APIC timer model has been exercised.
pub const M3_GTIMER3_OK_MARKER: &str = "RAYNU-V-M3-GTIMER3-OK";

/// COM1 marker when an LVT timer vector was delivered via IRR/ISR.
pub const M3_APIC_OK_MARKER: &str = "RAYNU-V-M3-APIC-OK";

/// Guest APIC GPA (identity hole).
pub const APIC_GPA: u64 = DEFAULT_APIC_PHYS;

/// x2APIC MSR base (IA32_X2APIC_APICID = 0x802, …).
pub const X2APIC_MSR_BASE: u32 = 0x800;
pub const X2APIC_MSR_LAST: u32 = 0x83F;

// MMIO / x2APIC register offsets (same numbering).
const REG_ID: u32 = 0x20;
const REG_VERSION: u32 = 0x30;
const REG_TPR: u32 = 0x80;
const REG_PPR: u32 = 0xA0;
const REG_EOI: u32 = 0xB0;
const REG_SVR: u32 = 0xF0;
const REG_ISR_BASE: u32 = 0x100;
const REG_IRR_BASE: u32 = 0x200;
const REG_LVT_TIMER: u32 = 0x320;
const REG_INIT_COUNT: u32 = 0x380;
const REG_CUR_COUNT: u32 = 0x390;
const REG_DIVIDE: u32 = 0x3E0;
/// ICR low triggers a send. ICR high holds the destination (xAPIC bits 31:24).
const REG_ICR_LOW: u32 = 0x300;
const REG_ICR_HIGH: u32 = 0x310;
const REG_LDR: u32 = 0xD0;
const REG_DFR: u32 = 0xE0;

const LVT_MASKED: u32 = 1 << 16;
const LVT_PERIODIC: u32 = 1 << 17;
const SVR_ENABLED: u32 = 1 << 8;
/// Version 0x14, 6 LVT entries (Intel xAPIC). Sink zeros made this 0.
pub const XAPIC_VERSION: u32 = 0x0005_0014;

/// TSC ticks per undivided APIC-bus cycle (~100 MHz bus @ ~3.2 GHz TSC).
const TSC_PER_BUS_CYCLE: u64 = 32;

static APIC_ID: PerVcpuCell<u32> = per_vcpu_cell!(0);
static APIC_TPR: PerVcpuCell<u32> = per_vcpu_cell!(0);
static APIC_SVR: PerVcpuCell<u32> = per_vcpu_cell!(0xFF | SVR_ENABLED);
static APIC_LVT_TIMER: PerVcpuCell<u32> = per_vcpu_cell!(LVT_MASKED | 0xEF);
static APIC_DIVIDE: PerVcpuCell<u32> = per_vcpu_cell!(0b0011); // ÷16 (Linux calibrate default)
static APIC_INIT_COUNT: PerVcpuCell<u32> = per_vcpu_cell!(0);
/// In-service and request registers (8×32 = 256 vectors).
static APIC_ISR: PerVcpuCell<[u32; 8]> = per_vcpu_cell!([0; 8]);
static APIC_IRR: PerVcpuCell<[u32; 8]> = per_vcpu_cell!([0; 8]);
/// Host one-shot armed on behalf of guest (LVT unmasked + counting).
static HOST_TIMER_FOR_GUEST: PerVcpuCell<bool> = per_vcpu_cell!(false);
static GTIMER3_OK: PerVcpuCell<bool> = per_vcpu_cell!(false);
static GTIMER3_PRINT: PerVcpuCell<bool> = per_vcpu_cell!(false);
static APIC_OK: PerVcpuCell<bool> = per_vcpu_cell!(false);
static APIC_OK_PRINT: PerVcpuCell<bool> = per_vcpu_cell!(false);
/// Vector latched when the guest last armed delivery (for host-fire → IRR).
static PENDING_VECTOR: PerVcpuCell<Option<u8>> = per_vcpu_cell!(None);
/// TSC of the last stalled-timer relatch. 0 means not yet.
static TIMER_RELA_TSC: PerVcpuCell<u64> = per_vcpu_cell!(0);
/// TSC when INIT_COUNT was last written (countdown base).
static TIMER_START_TSC: PerVcpuCell<u64> = per_vcpu_cell!(0);
static TIMER_RUNNING: PerVcpuCell<bool> = per_vcpu_cell!(false);
/// Last ICR high (destination). A following ICR low sends the IPI.
static APIC_ICR_HIGH: PerVcpuCell<u32> = per_vcpu_cell!(0);
static APIC_LDR: PerVcpuCell<u32> = per_vcpu_cell!(0);
static APIC_DFR: PerVcpuCell<u32> = per_vcpu_cell!(0xFFFF_FFFF);
/// NMI accepted by this vCPU and not yet injected.
static NMI_PENDING: PerVcpu<AtomicBool> = per_vcpu!(AtomicBool::new(false));
/// INIT accepted for this vCPU (an AP). Drained once by the exit path.
static AP_INIT: PerVcpu<AtomicBool> = per_vcpu!(AtomicBool::new(false));
/// SIPI page for this vCPU, or `0xFFFF` when none is waiting.
static AP_SIPI: PerVcpu<AtomicU16> = per_vcpu!(AtomicU16::new(0xFFFF));
/// Vectors latched by a fixed self-IPI. Linux 7 `irq_work` starts SRCU
/// grace periods this way (`1b01c2d8`: fsnotify workers never woke).
static SELF_IPI_MASK: PerVcpuCell<[u32; 8]> = per_vcpu_cell!([0; 8]);

pub fn is_x2apic_msr(index: u32) -> bool {
    (X2APIC_MSR_BASE..=X2APIC_MSR_LAST).contains(&index)
}

fn reg_from_msr(index: u32) -> u32 {
    (index - X2APIC_MSR_BASE) << 4
}

fn reg_from_gpa(gpa: u64) -> Option<u32> {
    if !is_xapic_mmio_gpa(gpa) {
        return None;
    }
    Some((gpa - APIC_GPA) as u32)
}

/// Guest-UEFI product ISO leaves this 4 KiB unmapped (trap-and-emulate).
pub fn is_xapic_mmio_gpa(gpa: u64) -> bool {
    gpa >= APIC_GPA && gpa < APIC_GPA + 0x1000
}

fn divide_value(dcr: u32) -> u32 {
    match dcr & 0b1011 {
        0b0000 => 2,
        0b0001 => 4,
        0b0010 => 8,
        0b0011 => 16,
        0b1000 => 1,
        0b1001 => 32,
        0b1010 => 64,
        0b1011 => 128,
        _ => 16,
    }
}

fn timer_should_deliver() -> bool {
    // SAFETY: VMEXIT path.
    unsafe { ((*APIC_LVT_TIMER.p()) & LVT_MASKED) == 0 && ((*APIC_SVR.p()) & SVR_ENABLED) != 0 }
}

fn lvt_timer_vector() -> u8 {
    // SAFETY: VMEXIT path.
    unsafe { ((*APIC_LVT_TIMER.p()) & 0xFF) as u8 }
}

fn bit_set(regs: *mut [u32; 8], vec: u8) {
    // SAFETY: caller passes exclusive APIC_IRR/ISR pointer on VMEXIT path.
    unsafe {
        let w = &mut (*regs)[(vec / 32) as usize];
        *w |= 1u32 << (vec % 32);
    }
}

fn bit_clear(regs: *mut [u32; 8], vec: u8) {
    // SAFETY: caller passes exclusive APIC_IRR/ISR pointer on VMEXIT path.
    unsafe {
        let w = &mut (*regs)[(vec / 32) as usize];
        *w &= !(1u32 << (vec % 32));
    }
}

fn highest_bit(regs: *const [u32; 8]) -> Option<u8> {
    // SAFETY: caller passes APIC_IRR/ISR pointer on VMEXIT path.
    unsafe {
        for i in (0..8).rev() {
            let w = (*regs)[i];
            if w != 0 {
                let bit = 31 - w.leading_zeros();
                return Some((i as u8) * 32 + bit as u8);
            }
        }
        None
    }
}

fn bit_is_set(regs: *const [u32; 8], vec: u8) -> bool {
    // SAFETY: caller passes APIC_IRR/ISR pointer on VMEXIT path.
    unsafe {
        let w = (*regs)[(vec / 32) as usize];
        (w & (1u32 << (vec % 32))) != 0
    }
}

fn set_irr(vec: u8) {
    bit_set(APIC_IRR.p(), vec)
}

/// Fixed ICR send aimed at this CPU. Linux 7 queues `process_srcu` from
/// `irq_work`, and x86 raises that with a local-APIC self-IPI. Dropping
/// the write left fsnotify in `synchronize_srcu` (`1b01c2d8`).
/// Not `ISO-INSTALL-OK`.
/// Delivery mode in ICR low bits 10:8.
pub fn icr_mode(low: u32) -> u32 {
    (low >> 8) & 0x7
}

/// Startup-IPI page (the vector field). Meaningful when [`icr_mode`] is 6.
pub fn icr_startup_page(low: u32) -> u8 {
    (low & 0xFF) as u8
}

/// One LAPIC accepts `dest`. Physical: its ID or 0xFF. Logical flat: LDR
/// bit test. Logical cluster (`DFR[31:28] == 0`): same cluster and a set bit.
pub fn dest_hit(dest: u8, logical: bool, apic_id: u8, ldr: u32, dfr: u32) -> bool {
    if !logical {
        return dest == 0xFF || dest == apic_id;
    }
    let ldr_log = (ldr >> 24) as u8;
    if (dfr >> 28) == 0 {
        let same_cluster = (dest & 0xF0) == (ldr_log & 0xF0);
        let bit = (dest & 0x0F) & (ldr_log & 0x0F) != 0;
        return same_cluster && bit;
    }
    dest & ldr_log != 0
}

/// Bit v set means vCPU v of the current slot accepts `dest`. Only `v < vcpus()`.
///
/// The architectural APIC ID of vCPU v is v (ADR-023 D7). The ID register is
/// written when that vCPU is reset; a SIPI arrives before the AP has run, so
/// the match uses the index.
pub fn dest_mask(dest: u8, logical: bool) -> u8 {
    let n = slot::vcpus();
    let s = slot::current();
    let mut mask = 0u8;
    for v in 0..n {
        // SAFETY: VMEXIT or host-test path; read of this slot's LAPIC bank.
        // KANI-TARGET: virtual LAPIC destination read (outside Proven Core).
        let (ldr, dfr) = unsafe { (*APIC_LDR.p_at(s, v), *APIC_DFR.p_at(s, v)) };
        if dest_hit(dest, logical, v as u8, ldr, dfr) {
            mask |= 1u8 << v;
        }
    }
    mask
}

/// The one vCPU a device interrupt goes to: lowest set bit of
/// `dest_mask & up_mask`. 0 when nothing matches.
pub fn pick_target(dest: u8, logical: bool) -> usize {
    let m = dest_mask(dest, logical) & slot::up_mask(slot::current());
    if m == 0 {
        0
    } else {
        m.trailing_zeros() as usize
    }
}

/// Post `vector` to vCPU `v` of slot `s`, which is not on the CPU.
///
/// One host core today, so these are plain stores (the PerVcpuCell argument).
/// B21: an atomic store, then a host IPI to the core running (s, v).
fn post_remote(s: usize, v: usize, vector: u8, ipi: bool) {
    // SAFETY: VMEXIT path on the one CPU that runs guests. `s`/`v` name a
    // bank entry; the target vCPU is not executing.
    // KANI-TARGET: remote LAPIC post (outside Proven Core).
    unsafe {
        bit_set(APIC_IRR.p_at(s, v), vector);
        if ipi {
            bit_set(SELF_IPI_MASK.p_at(s, v), vector);
        }
    }
    slot::wake_vcpu(s, v);
}

fn accept_icr_low(val: u32) {
    if slot::vcpus() == 1 {
        accept_icr_low_uniprocessor(val);
        return;
    }
    let me = slot::current_vcpu();
    let s = slot::current();
    // SAFETY: VMEXIT path; ICR high was stored by the preceding write.
    // KANI-TARGET: virtual LAPIC ICR high read (outside Proven Core).
    let high = unsafe { *APIC_ICR_HIGH.p() };
    let all = ((1u16 << slot::vcpus()) - 1) as u8;
    let mut mask = match (val >> 18) & 3 {
        1 => 1u8 << me,
        2 => all,
        3 => all & !(1u8 << me),
        _ => dest_mask((high >> 24) as u8, val & (1 << 11) != 0),
    };
    let vector = icr_startup_page(val);
    match icr_mode(val) {
        0 | 1 => {
            if vector < 32 {
                return;
            }
            if icr_mode(val) == 1 {
                mask &= mask.wrapping_neg();
            }
            for v in 0..slot::vcpus() {
                if mask & (1u8 << v) == 0 {
                    continue;
                }
                if v == me {
                    set_irr(vector);
                    bit_set(SELF_IPI_MASK.p(), vector);
                } else if slot::vcpu_up(s, v)
                    && crate::sched::ipi::validate_ipi_target(s as u64, s as u64, v as u32).is_ok()
                {
                    post_remote(s, v, vector, true);
                }
            }
        }
        4 => {
            for v in 0..slot::vcpus() {
                if mask & (1u8 << v) == 0 || !slot::vcpu_up(s, v) {
                    continue;
                }
                NMI_PENDING.at(s, v).store(true, Ordering::Release);
                slot::wake_vcpu(s, v);
            }
        }
        5 => {
            if val & (1 << 15) != 0 && val & (1 << 14) == 0 {
                return;
            }
            for v in 0..slot::vcpus() {
                if mask & (1u8 << v) == 0 || v == me || v == 0 {
                    continue;
                }
                AP_INIT.at(s, v).store(true, Ordering::Release);
            }
        }
        6 => {
            for v in 0..slot::vcpus() {
                if mask & (1u8 << v) == 0 || v == me || v == 0 {
                    continue;
                }
                AP_SIPI.at(s, v).store(u16::from(vector), Ordering::Release);
            }
        }
        _ => {}
    }
}

/// N=1 ICR: the path `main` had, without a recorded remote INIT/SIPI.
fn accept_icr_low_uniprocessor(val: u32) {
    let shorthand = (val >> 18) & 0x3;
    let mode = icr_mode(val);
    let vector = icr_startup_page(val);
    // SAFETY: VMEXIT path; ICR high was stored by the preceding write.
    // KANI-TARGET: virtual LAPIC uniprocessor ICR (outside Proven Core).
    let to_self = unsafe {
        let dest = (*APIC_ICR_HIGH.p()) >> 24;
        let apic_id = (*APIC_ID.p()) >> 24;
        shorthand == 0b01 || shorthand == 0b10 || (shorthand == 0 && dest == apic_id)
    };
    if !to_self && (mode == 5 || mode == 6) {
        return;
    }
    if mode != 0 || vector < 32 || !to_self {
        return;
    }
    // SAFETY: VMEXIT path.
    // KANI-TARGET: virtual LAPIC self-IPI latch (outside Proven Core).
    unsafe {
        set_irr(vector);
        bit_set(SELF_IPI_MASK.p(), vector);
    }
}

/// INIT latched for vCPU `v` of the current slot, once.
pub fn take_ap_init(v: usize) -> bool {
    if v >= slot::MAX_VCPUS {
        return false;
    }
    AP_INIT.at(slot::current(), v).swap(false, Ordering::AcqRel)
}

/// SIPI page latched for vCPU `v` of the current slot, once.
pub fn take_ap_sipi(v: usize) -> Option<u8> {
    if v >= slot::MAX_VCPUS {
        return None;
    }
    let page = AP_SIPI
        .at(slot::current(), v)
        .swap(0xFFFF, Ordering::AcqRel);
    if page == 0xFFFF {
        None
    } else {
        Some(page as u8)
    }
}

/// NMI waiting on the current vCPU, once.
pub fn take_nmi() -> bool {
    NMI_PENDING.swap(false, Ordering::AcqRel)
}

/// NMI waiting on the current vCPU.
pub fn nmi_pending() -> bool {
    NMI_PENDING.load(Ordering::Acquire)
}

fn highest_self_ipi() -> Option<u8> {
    // SAFETY: VMEXIT path.
    unsafe {
        for i in (0..8).rev() {
            let w = (*SELF_IPI_MASK.p())[i] & (*APIC_IRR.p())[i];
            if w != 0 {
                let bit = 31 - w.leading_zeros();
                return Some((i as u8) * 32 + bit as u8);
            }
        }
        None
    }
}

/// Move one pending self-IPI from IRR into ISR. PIC timer stays pending.
/// Not `ISO-INSTALL-OK`.
pub fn take_self_ipi() -> Option<u32> {
    let vec = highest_self_ipi()?;
    // SAFETY: VMEXIT path.
    let blocked = unsafe {
        let ppr = processor_priority() & 0xF0;
        ((vec as u32) & 0xF0) <= ppr
    };
    if blocked {
        return None;
    }
    // SAFETY: VMEXIT path.
    unsafe {
        bit_clear(SELF_IPI_MASK.p(), vec);
    }
    take_irr_vec(vec, false)
}

/// APIC TPR (8-bit). CR8 is bits 7:4.
pub fn tpr() -> u32 {
    // SAFETY: VMEXIT / host-test path.
    unsafe { (*APIC_TPR.p()) & 0xFF }
}

pub fn set_tpr(val: u32) {
    // SAFETY: VMEXIT / host-test path.
    unsafe {
        (*APIC_TPR.p()) = val & 0xFF;
    }
}

/// Guest CR8 (TPR class 0..15). Linux `write_cr8` / `read_cr8`.
pub fn cr8() -> u64 {
    u64::from((tpr() >> 4) & 0xF)
}

pub fn set_cr8(val: u64) {
    set_tpr(((val as u32) & 0xF) << 4);
}

/// Latch a device IRQ (IOAPIC vector) into IRR. Stage 46 product ISO uses
/// this so Linux `ack_APIC_irq` EOI matches; bare VM-entry inject of the
/// same vector is the M3.12 `Fatal exception in interrupt` path.
pub fn latch_irr(vec: u8) {
    if vec >= 16 {
        set_irr(vec);
    }
}

/// MSI-X message. With one vCPU this is [`latch_irr`]. With more, the
/// address names the vCPU (destination in bits 19:12, logical when bit 2
/// is set).
pub fn latch_irr_msi(addr: u64, vec: u8) {
    if slot::vcpus() == 1 {
        latch_irr(vec);
        return;
    }
    let v = pick_target(((addr >> 12) & 0xFF) as u8, addr & 4 != 0);
    if v == slot::current_vcpu() {
        latch_irr(vec);
    } else if vec >= 16 {
        post_remote(slot::current(), v, vec, false);
    }
}

fn processor_priority() -> u32 {
    // SAFETY: VMEXIT path.
    unsafe {
        let tpr = (*APIC_TPR.p()) & 0xFF;
        let isr = match highest_bit(APIC_ISR.p().cast_const()) {
            Some(v) => (v as u32) & 0xF0,
            None => 0,
        };
        let tpr_class = tpr & 0xF0;
        if isr > tpr_class {
            isr
        } else {
            tpr
        }
    }
}

fn isr_reg_index(reg: u32) -> Option<usize> {
    if (REG_ISR_BASE..REG_ISR_BASE + 0x80).contains(&reg) && (reg & 0xF) == 0 {
        Some(((reg - REG_ISR_BASE) / 0x10) as usize)
    } else {
        None
    }
}

fn irr_reg_index(reg: u32) -> Option<usize> {
    if (REG_IRR_BASE..REG_IRR_BASE + 0x80).contains(&reg) && (reg & 0xF) == 0 {
        Some(((reg - REG_IRR_BASE) / 0x10) as usize)
    } else {
        None
    }
}

fn arm_guest_delivery_if_needed() {
    // SAFETY: VMEXIT path.
    unsafe {
        if (*TIMER_RUNNING.p()) && timer_should_deliver() && current_count_raw() != 0 {
            (*HOST_TIMER_FOR_GUEST.p()) = true;
            (*PENDING_VECTOR.p()) = Some(lvt_timer_vector());
        }
    }
}

/// Elapsed APIC timer counts since INIT_COUNT write (may exceed INIT).
fn elapsed_counts() -> u64 {
    // SAFETY: VMEXIT path.
    unsafe {
        if !(*TIMER_RUNNING.p()) || (*APIC_INIT_COUNT.p()) == 0 {
            return 0;
        }
        let elapsed_tsc = cpu::rdtsc().wrapping_sub((*TIMER_START_TSC.p()));
        let bus = elapsed_tsc / TSC_PER_BUS_CYCLE;
        bus / divide_value((*APIC_DIVIDE.p())) as u64
    }
}

fn current_count_raw() -> u32 {
    // SAFETY: VMEXIT path.
    unsafe {
        if !(*TIMER_RUNNING.p()) || (*APIC_INIT_COUNT.p()) == 0 {
            return 0;
        }
        let init = (*APIC_INIT_COUNT.p()) as u64;
        let elapsed = elapsed_counts();
        if ((*APIC_LVT_TIMER.p()) & LVT_PERIODIC) != 0 {
            if init == 0 {
                return 0;
            }
            let phase = elapsed % init;
            (init - phase) as u32
        } else if elapsed >= init {
            0
        } else {
            (init - elapsed) as u32
        }
    }
}

fn handle_eoi() {
    if let Some(v) = highest_bit(APIC_ISR.p().cast_const()) {
        bit_clear(APIC_ISR.p(), v);
        crate::devices::guest_irq::ioapic_eoi(v);
    }
}

fn read_reg(reg: u32) -> u32 {
    // SAFETY: single-threaded VMEXIT path.
    unsafe {
        if let Some(i) = isr_reg_index(reg) {
            return (*APIC_ISR.p())[i];
        }
        if let Some(i) = irr_reg_index(reg) {
            return (*APIC_IRR.p())[i];
        }
        match reg {
            REG_ID => (*APIC_ID.p()),
            REG_VERSION => XAPIC_VERSION,
            REG_TPR => (*APIC_TPR.p()),
            REG_PPR => processor_priority(),
            REG_EOI => 0,
            REG_SVR => (*APIC_SVR.p()),
            REG_LVT_TIMER => (*APIC_LVT_TIMER.p()),
            REG_INIT_COUNT => (*APIC_INIT_COUNT.p()),
            REG_CUR_COUNT => current_count_raw(),
            REG_DIVIDE => (*APIC_DIVIDE.p()),
            REG_LDR => {
                if slot::vcpus() > 1 {
                    *APIC_LDR.p()
                } else {
                    0
                }
            }
            REG_DFR => {
                if slot::vcpus() > 1 {
                    *APIC_DFR.p()
                } else {
                    0
                }
            }
            _ => 0,
        }
    }
}

/// Start / restart the virtual countdown from `val`.
fn start_countdown(val: u32) {
    // SAFETY: VMEXIT path.
    unsafe {
        (*APIC_INIT_COUNT.p()) = val;
        if val == 0 {
            (*TIMER_RUNNING.p()) = false;
            (*HOST_TIMER_FOR_GUEST.p()) = false;
            (*PENDING_VECTOR.p()) = None;
            return;
        }
        (*TIMER_START_TSC.p()) = cpu::rdtsc();
        (*TIMER_RUNNING.p()) = true;
        arm_guest_delivery_if_needed();
    }
}

fn write_reg(reg: u32, val: u32) {
    // SAFETY: single-threaded VMEXIT path.
    unsafe {
        match reg {
            REG_TPR => (*APIC_TPR.p()) = val,
            REG_EOI => handle_eoi(),
            REG_SVR => {
                (*APIC_SVR.p()) = val;
                arm_guest_delivery_if_needed();
            }
            REG_LVT_TIMER => {
                (*APIC_LVT_TIMER.p()) = val;
                arm_guest_delivery_if_needed();
            }
            REG_DIVIDE => (*APIC_DIVIDE.p()) = val & 0b1011,
            REG_INIT_COUNT => start_countdown(val),
            REG_ICR_HIGH => (*APIC_ICR_HIGH.p()) = val,
            REG_ICR_LOW => accept_icr_low(val),
            REG_LDR => (*APIC_LDR.p()) = val & 0xFF00_0000,
            REG_DFR => (*APIC_DFR.p()) = val | 0x0FFF_FFFF,
            REG_ID | REG_VERSION | REG_CUR_COUNT | REG_PPR => {}
            _ => {
                // ISR/IRR are read-only; ignore guest stores.
            }
        }
    }
}

/// True when the hypervisor should arm a host one-shot for the guest timer.
pub fn host_timer_armed_for_guest() -> bool {
    // SAFETY: VMEXIT path.
    unsafe { (*HOST_TIMER_FOR_GUEST.p()) }
}

/// Guest-UEFI has no host LAPIC one-shot. Poll CUR_COUNT on VMX preempt/HLT
/// and latch IRR when the virtual countdown reaches zero. Does not inject.
pub fn poll_timer_expiry() -> bool {
    // SAFETY: VMEXIT path.
    unsafe {
        if !(*HOST_TIMER_FOR_GUEST.p()) || !(*TIMER_RUNNING.p()) || (*APIC_INIT_COUNT.p()) == 0 {
            return false;
        }
        if elapsed_counts() < (*APIC_INIT_COUNT.p()) as u64 {
            return false;
        }
    }
    on_host_timer_fire()
}

/// HLT-exiting spends no guest time, so `poll_timer_expiry` never sees
/// CUR_COUNT hit 0 while BDS CpuSleeps. Force LVT expiry into IRR.
/// If LVT is still masked (reset `0xEF`), use vec 0x20 (iron `eac424b`
/// IDT[0x20] writes CR8). If OVMF already unmasked, keep that vector.
/// firmware LAPIC timer expiry. Not `ISO-INSTALL-OK`.
pub fn force_firmware_lapic_timer_expiry() -> bool {
    // SAFETY: VMEXIT / host-test path.
    unsafe {
        let masked = ((*APIC_LVT_TIMER.p()) & LVT_MASKED) != 0;
        if masked {
            (*APIC_LVT_TIMER.p()) =
                ((*APIC_LVT_TIMER.p()) & !0xFF & !LVT_MASKED) | 0x20 | LVT_PERIODIC;
        } else {
            (*APIC_LVT_TIMER.p()) = ((*APIC_LVT_TIMER.p()) & !LVT_MASKED) | LVT_PERIODIC;
        }
        (*APIC_SVR.p()) |= SVR_ENABLED;
        if (*APIC_INIT_COUNT.p()) == 0 {
            (*APIC_INIT_COUNT.p()) = 1;
        }
        (*TIMER_START_TSC.p()) = 0;
        (*TIMER_RUNNING.p()) = true;
        (*HOST_TIMER_FOR_GUEST.p()) = true;
        (*PENDING_VECTOR.p()) = Some(lvt_timer_vector());
    }
    on_host_timer_fire()
}

/// True when IRR holds a vector deliverable against the current PPR.
pub fn has_deliverable_irr() -> bool {
    let Some(vec) = highest_bit(APIC_IRR.p().cast_const()) else {
        return false;
    };
    let ppr = processor_priority() & 0xF0;
    ((vec as u32) & 0xF0) > ppr
}

/// True when IRR has any latched vector, ignoring TPR/PPR.
/// firmware HLT ignores TPR. Not `ISO-INSTALL-OK`.
pub fn has_pending_irr() -> bool {
    highest_bit(APIC_IRR.p().cast_const()).is_some()
}

/// True when IRR holds `vec` (does not consult TPR/PPR).
/// firmware prefer ATA IRR. Not `ISO-INSTALL-OK`.
pub fn has_irr_vec(vec: u8) -> bool {
    if vec < 16 {
        return false;
    }
    bit_is_set(APIC_IRR.p().cast_const(), vec)
}

/// LVT timer vector is sitting in IRR. Does not consume it.
pub fn timer_vector_pending() -> bool {
    has_irr_vec(lvt_timer_vector())
}

/// One-shot mode latches IRR once. If that vector is never taken, the
/// guest does not write `INIT_COUNT` again and the clock stops.
/// A write of 0 is a cancel and stays idle. Iron `4ed6522c`: `sleep 1`
/// returned, then `timeout` did not.
pub fn relatch_if_idle(now: u64) -> bool {
    const MIN_TSC: u64 = 2_000_000;
    // SAFETY: VMEXIT / host-test path. LVT and IRR are the virtual APIC.
    // KANI-TARGET: stalled one-shot relatch (outside Proven Core).
    unsafe {
        if !timer_should_deliver() || timer_vector_pending() {
            return false;
        }
        if (*APIC_INIT_COUNT.p()) == 0 || ((*TIMER_RUNNING.p()) && (*HOST_TIMER_FOR_GUEST.p())) {
            return false;
        }
        if (*TIMER_RELA_TSC.p()) != 0 && now.wrapping_sub((*TIMER_RELA_TSC.p())) < MIN_TSC {
            return false;
        }
        (*TIMER_RELA_TSC.p()) = now;
        set_irr(lvt_timer_vector());
        true
    }
}

/// Move the LVT timer vector from IRR into ISR.
/// A higher device bit stays pending. Not `ISO-INSTALL-OK`.
pub fn take_timer_vector() -> Option<u32> {
    let vec = lvt_timer_vector();
    take_irr_vec(vec, false)
}

/// Host LAPIC one-shot expired for a guest-armed virtual timer.
/// Latches IRR (and GTIMER3); does **not** by itself program VM-entry inject.
pub fn on_host_timer_fire() -> bool {
    // SAFETY: VMEXIT path.
    unsafe {
        if !(*HOST_TIMER_FOR_GUEST.p()) {
            return false;
        }
        (*HOST_TIMER_FOR_GUEST.p()) = false;
        let v = (*PENDING_VECTOR.p()).unwrap_or(lvt_timer_vector());
        (*PENDING_VECTOR.p()) = None;
        if timer_should_deliver() {
            set_irr(v);
        }
        if !(*GTIMER3_OK.p()) {
            (*GTIMER3_OK.p()) = true;
            (*GTIMER3_PRINT.p()) = true;
        }
        if ((*APIC_LVT_TIMER.p()) & LVT_PERIODIC) != 0
            && (*APIC_INIT_COUNT.p()) != 0
            && timer_should_deliver()
        {
            (*TIMER_START_TSC.p()) = cpu::rdtsc();
            (*TIMER_RUNNING.p()) = true;
            arm_guest_delivery_if_needed();
        } else {
            (*TIMER_RUNNING.p()) = false;
        }
        true
    }
}

fn take_irr_vector(ignore_tpr: bool) -> Option<u32> {
    // SAFETY: VMEXIT path.
    unsafe {
        let vec = highest_bit(APIC_IRR.p().cast_const())?;
        if !ignore_tpr {
            let ppr = processor_priority() & 0xF0;
            if ((vec as u32) & 0xF0) <= ppr {
                return None;
            }
        }
        bit_clear(APIC_IRR.p(), vec);
        bit_set(APIC_ISR.p(), vec);
        // Timer LVT vector (typically 0xEF) → M3.12 APIC-OK.
        if vec == lvt_timer_vector() || vec == 0xEF {
            if !(*APIC_OK.p()) {
                (*APIC_OK.p()) = true;
                (*APIC_OK_PRINT.p()) = true;
            }
        }
        Some(vec as u32)
    }
}

/// Move the highest deliverable IRR bit into ISR and return its vector.
pub fn take_deliverable_vector() -> Option<u32> {
    take_irr_vector(false)
}

/// IRR→ISR ignoring TPR/PPR. Firmware BDS CpuSleep after BOTH-OK.
/// firmware HLT ignores TPR. Not `ISO-INSTALL-OK`.
pub fn take_highest_irr() -> Option<u32> {
    take_irr_vector(true)
}

/// Move one chosen IRR bit into ISR. Firmware PACKET prefers ATA `0x2E`
/// over LVT `0xEF` so ignore-TPR does not livelock the timer ISR
/// (iron `ea30da1`). After ataio, `has_deliverable_irr` drops class `0x20`
/// while CR8>=2 (iron `084430f`). firmware prefer ATA IRR.
/// Not `ISO-INSTALL-OK`.
pub fn take_irr_vec(vec: u8, ignore_tpr: bool) -> Option<u32> {
    if vec < 16 {
        return None;
    }
    // SAFETY: VMEXIT / host-test path.
    unsafe {
        if !bit_is_set(APIC_IRR.p().cast_const(), vec) {
            return None;
        }
        if !ignore_tpr {
            let ppr = processor_priority() & 0xF0;
            if ((vec as u32) & 0xF0) <= ppr {
                return None;
            }
        }
        bit_clear(APIC_IRR.p(), vec);
        bit_set(APIC_ISR.p(), vec);
        if vec == lvt_timer_vector() || vec == 0xEF {
            if !(*APIC_OK.p()) {
                (*APIC_OK.p()) = true;
                (*APIC_OK_PRINT.p()) = true;
            }
        }
        Some(vec as u32)
    }
}

/// True once after GTIMER3 latches; caller should print the COM1 marker.
pub fn take_gtimer3_latch() -> bool {
    // SAFETY: VMEXIT path.
    unsafe {
        if (*GTIMER3_PRINT.p()) {
            (*GTIMER3_PRINT.p()) = false;
            true
        } else {
            false
        }
    }
}

/// True once after the first IRR/ISR LVT deliver; caller prints APIC-OK.
pub fn take_apic_ok_latch() -> bool {
    // SAFETY: VMEXIT path.
    unsafe {
        if (*APIC_OK_PRINT.p()) {
            (*APIC_OK_PRINT.p()) = false;
            true
        } else {
            false
        }
    }
}

pub fn gtimer3_ok() -> bool {
    // SAFETY: written on VMEXIT; read for gate.
    unsafe { (*GTIMER3_OK.p()) }
}

pub fn apic_ok() -> bool {
    // SAFETY: written on VMEXIT; read for gate / IRQ0 drop.
    unsafe { (*APIC_OK.p()) }
}

pub fn rdmsr(index: u32) -> Option<u64> {
    if !is_x2apic_msr(index) {
        return None;
    }
    Some(read_reg(reg_from_msr(index)) as u64)
}

/// Handle x2APIC WRMSR. Returns whether the host one-shot should be armed.
pub fn wrmsr(index: u32, value: u64) -> Option<bool> {
    if !is_x2apic_msr(index) {
        return None;
    }
    write_reg(reg_from_msr(index), value as u32);
    Some(unsafe { (*HOST_TIMER_FOR_GUEST.p()) })
}

/// Stamp a 4 KiB xAPIC MMIO image (version ≠ 0, LVTs masked, flat DFR).
///
/// Guest-UEFI maps this at GPA `0xFEE00000` so OVMF `MmioRead32` of
/// the version register is not the 2 MiB platform-sink zero page.
pub fn fill_xapic_page(page: &mut [u8]) {
    if page.len() < 0x400 {
        return;
    }
    page[..0x400].fill(0);
    fn poke(page: &mut [u8], off: usize, v: u32) {
        page[off..off + 4].copy_from_slice(&v.to_le_bytes());
    }
    poke(page, REG_VERSION as usize, XAPIC_VERSION);
    poke(page, 0xE0, 0xFFFF_FFFF); // DFR flat
    poke(page, REG_SVR as usize, 0xFF | SVR_ENABLED);
    for off in [0x320usize, 0x330, 0x340, 0x350, 0x360, 0x370] {
        poke(page, off, LVT_MASKED);
    }
}

/// Handle APIC MMIO access at `gpa`.
pub fn mmio_access(gpa: u64, is_write: bool, write_val: u32) -> Option<Option<u32>> {
    let reg = reg_from_gpa(gpa)?;
    if is_write {
        write_reg(reg, write_val);
        Some(None)
    } else {
        Some(Some(read_reg(reg)))
    }
}

/// F7: restore power-on LAPIC statics for a RayNu-F relaunch. Does not
/// touch EPT or the xAPIC MMIO GPA. Not `ISO-INSTALL-OK`.
pub fn reset() {
    // SAFETY: BSP-only guest-UEFI path; the guest is halted for relaunch.
    // KANI-TARGET: guest-UEFI LAPIC reset (outside Proven Core).
    unsafe {
        (*APIC_ID.p()) = (slot::current_vcpu() as u32) << 24;
        (*APIC_LDR.p()) = 0;
        (*APIC_DFR.p()) = 0xFFFF_FFFF;
        (*APIC_TPR.p()) = 0;
        (*APIC_SVR.p()) = 0xFF | SVR_ENABLED;
        (*APIC_LVT_TIMER.p()) = LVT_MASKED | 0xEF;
        (*APIC_DIVIDE.p()) = 0b0011;
        (*APIC_INIT_COUNT.p()) = 0;
        (*APIC_ISR.p()) = [0; 8];
        (*APIC_IRR.p()) = [0; 8];
        (*APIC_ICR_HIGH.p()) = 0;
        (*SELF_IPI_MASK.p()) = [0; 8];
        (*HOST_TIMER_FOR_GUEST.p()) = false;
        (*GTIMER3_OK.p()) = false;
        (*GTIMER3_PRINT.p()) = false;
        (*APIC_OK.p()) = false;
        (*APIC_OK_PRINT.p()) = false;
        (*PENDING_VECTOR.p()) = None;
        (*TIMER_START_TSC.p()) = 0;
        (*TIMER_RELA_TSC.p()) = 0;
        (*TIMER_RUNNING.p()) = false;
    }
    NMI_PENDING.store(false, Ordering::Release);
    AP_INIT.store(false, Ordering::Release);
    AP_SIPI.store(0xFFFF, Ordering::Release);
}

/// Trap x2APIC MSRs in a zeroed MSR bitmap page (read + write maps).
///
/// SAFETY: `bitmap` is a writable zeroed 4K frame.
pub unsafe fn trap_x2apic_msrs(bitmap: u64) {
    let base = bitmap as *mut u8;
    for msr in X2APIC_MSR_BASE..=X2APIC_MSR_LAST {
        set_msr_bit(base, msr, true);
        set_msr_bit(base, msr, false);
    }
    set_msr_bit(base, 0x1B, true);
    set_msr_bit(base, 0x1B, false);
}

unsafe fn set_msr_bit(bitmap: *mut u8, msr: u32, read: bool) {
    let region = if read { 0usize } else { 1024 };
    let idx = region + (msr as usize) / 8;
    let bit = (msr % 8) as u8;
    let p = bitmap.add(idx);
    let cur = core::ptr::read_volatile(p);
    core::ptr::write_volatile(p, cur | (1 << bit));
}

#[cfg(test)]
#[path = "lapic_virt_test.rs"]
mod lapic_virt_test;
