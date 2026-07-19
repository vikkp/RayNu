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

const LVT_MASKED: u32 = 1 << 16;
const LVT_PERIODIC: u32 = 1 << 17;
const SVR_ENABLED: u32 = 1 << 8;

/// TSC ticks per undivided APIC-bus cycle (~100 MHz bus @ ~3.2 GHz TSC).
const TSC_PER_BUS_CYCLE: u64 = 32;

static mut APIC_ID: u32 = 0;
static mut APIC_TPR: u32 = 0;
static mut APIC_SVR: u32 = 0xFF | SVR_ENABLED;
static mut APIC_LVT_TIMER: u32 = LVT_MASKED | 0xEF;
static mut APIC_DIVIDE: u32 = 0b0011; // ÷16 (Linux calibrate default)
static mut APIC_INIT_COUNT: u32 = 0;
/// In-service and request registers (8×32 = 256 vectors).
static mut APIC_ISR: [u32; 8] = [0; 8];
static mut APIC_IRR: [u32; 8] = [0; 8];
/// Host one-shot armed on behalf of guest (LVT unmasked + counting).
static mut HOST_TIMER_FOR_GUEST: bool = false;
static mut GTIMER3_OK: bool = false;
static mut GTIMER3_PRINT: bool = false;
static mut APIC_OK: bool = false;
static mut APIC_OK_PRINT: bool = false;
/// Vector latched when the guest last armed delivery (for host-fire → IRR).
static mut PENDING_VECTOR: Option<u8> = None;
/// TSC when INIT_COUNT was last written (countdown base).
static mut TIMER_START_TSC: u64 = 0;
static mut TIMER_RUNNING: bool = false;

pub fn is_x2apic_msr(index: u32) -> bool {
    (X2APIC_MSR_BASE..=X2APIC_MSR_LAST).contains(&index)
}

fn reg_from_msr(index: u32) -> u32 {
    (index - X2APIC_MSR_BASE) << 4
}

fn reg_from_gpa(gpa: u64) -> Option<u32> {
    if gpa < APIC_GPA || gpa >= APIC_GPA + 0x1000 {
        return None;
    }
    Some((gpa - APIC_GPA) as u32)
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
    unsafe { (APIC_LVT_TIMER & LVT_MASKED) == 0 && (APIC_SVR & SVR_ENABLED) != 0 }
}

fn lvt_timer_vector() -> u8 {
    // SAFETY: VMEXIT path.
    unsafe { (APIC_LVT_TIMER & 0xFF) as u8 }
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

fn set_irr(vec: u8) {
    bit_set(core::ptr::addr_of_mut!(APIC_IRR), vec)
}

fn processor_priority() -> u32 {
    // SAFETY: VMEXIT path.
    unsafe {
        let tpr = APIC_TPR & 0xFF;
        let isr = match highest_bit(core::ptr::addr_of!(APIC_ISR)) {
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
        if TIMER_RUNNING && timer_should_deliver() && current_count_raw() != 0 {
            HOST_TIMER_FOR_GUEST = true;
            PENDING_VECTOR = Some(lvt_timer_vector());
        }
    }
}

/// Elapsed APIC timer counts since INIT_COUNT write (may exceed INIT).
fn elapsed_counts() -> u64 {
    // SAFETY: VMEXIT path.
    unsafe {
        if !TIMER_RUNNING || APIC_INIT_COUNT == 0 {
            return 0;
        }
        let elapsed_tsc = cpu::rdtsc().wrapping_sub(TIMER_START_TSC);
        let bus = elapsed_tsc / TSC_PER_BUS_CYCLE;
        bus / divide_value(APIC_DIVIDE) as u64
    }
}

fn current_count_raw() -> u32 {
    // SAFETY: VMEXIT path.
    unsafe {
        if !TIMER_RUNNING || APIC_INIT_COUNT == 0 {
            return 0;
        }
        let init = APIC_INIT_COUNT as u64;
        let elapsed = elapsed_counts();
        if (APIC_LVT_TIMER & LVT_PERIODIC) != 0 {
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
    if let Some(v) = highest_bit(core::ptr::addr_of!(APIC_ISR)) {
        bit_clear(core::ptr::addr_of_mut!(APIC_ISR), v);
    }
}

fn read_reg(reg: u32) -> u32 {
    // SAFETY: single-threaded VMEXIT path.
    unsafe {
        if let Some(i) = isr_reg_index(reg) {
            return APIC_ISR[i];
        }
        if let Some(i) = irr_reg_index(reg) {
            return APIC_IRR[i];
        }
        match reg {
            REG_ID => APIC_ID,
            REG_VERSION => 0x50014, // version 0x14, 6 LVT entries
            REG_TPR => APIC_TPR,
            REG_PPR => processor_priority(),
            REG_EOI => 0,
            REG_SVR => APIC_SVR,
            REG_LVT_TIMER => APIC_LVT_TIMER,
            REG_INIT_COUNT => APIC_INIT_COUNT,
            REG_CUR_COUNT => current_count_raw(),
            REG_DIVIDE => APIC_DIVIDE,
            _ => 0,
        }
    }
}

/// Start / restart the virtual countdown from `val`.
fn start_countdown(val: u32) {
    // SAFETY: VMEXIT path.
    unsafe {
        APIC_INIT_COUNT = val;
        if val == 0 {
            TIMER_RUNNING = false;
            HOST_TIMER_FOR_GUEST = false;
            PENDING_VECTOR = None;
            return;
        }
        TIMER_START_TSC = cpu::rdtsc();
        TIMER_RUNNING = true;
        arm_guest_delivery_if_needed();
    }
}

fn write_reg(reg: u32, val: u32) {
    // SAFETY: single-threaded VMEXIT path.
    unsafe {
        match reg {
            REG_TPR => APIC_TPR = val,
            REG_EOI => handle_eoi(),
            REG_SVR => {
                APIC_SVR = val;
                arm_guest_delivery_if_needed();
            }
            REG_LVT_TIMER => {
                APIC_LVT_TIMER = val;
                arm_guest_delivery_if_needed();
            }
            REG_DIVIDE => APIC_DIVIDE = val & 0b1011,
            REG_INIT_COUNT => start_countdown(val),
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
    unsafe { HOST_TIMER_FOR_GUEST }
}

/// True when IRR holds a vector deliverable against the current PPR.
pub fn has_deliverable_irr() -> bool {
    let Some(vec) = highest_bit(core::ptr::addr_of!(APIC_IRR)) else {
        return false;
    };
    let ppr = processor_priority() & 0xF0;
    ((vec as u32) & 0xF0) > ppr
}

/// Host LAPIC one-shot expired for a guest-armed virtual timer.
/// Latches IRR (and GTIMER3); does **not** by itself program VM-entry inject.
pub fn on_host_timer_fire() -> bool {
    // SAFETY: VMEXIT path.
    unsafe {
        if !HOST_TIMER_FOR_GUEST {
            return false;
        }
        HOST_TIMER_FOR_GUEST = false;
        let v = PENDING_VECTOR.unwrap_or(lvt_timer_vector());
        PENDING_VECTOR = None;
        if timer_should_deliver() {
            set_irr(v);
        }
        if !GTIMER3_OK {
            GTIMER3_OK = true;
            GTIMER3_PRINT = true;
        }
        if (APIC_LVT_TIMER & LVT_PERIODIC) != 0 && APIC_INIT_COUNT != 0 && timer_should_deliver()
        {
            TIMER_START_TSC = cpu::rdtsc();
            TIMER_RUNNING = true;
            arm_guest_delivery_if_needed();
        } else {
            TIMER_RUNNING = false;
        }
        true
    }
}

/// Move the highest deliverable IRR bit into ISR and return its vector.
pub fn take_deliverable_vector() -> Option<u32> {
    // SAFETY: VMEXIT path.
    unsafe {
        let vec = highest_bit(core::ptr::addr_of!(APIC_IRR))?;
        let ppr = processor_priority() & 0xF0;
        if ((vec as u32) & 0xF0) <= ppr {
            return None;
        }
        bit_clear(core::ptr::addr_of_mut!(APIC_IRR), vec);
        bit_set(core::ptr::addr_of_mut!(APIC_ISR), vec);
        // Timer LVT vector (typically 0xEF) → M3.12 APIC-OK.
        if vec == lvt_timer_vector() || vec == 0xEF {
            if !APIC_OK {
                APIC_OK = true;
                APIC_OK_PRINT = true;
            }
        }
        Some(vec as u32)
    }
}

/// True once after GTIMER3 latches; caller should print the COM1 marker.
pub fn take_gtimer3_latch() -> bool {
    // SAFETY: VMEXIT path.
    unsafe {
        if GTIMER3_PRINT {
            GTIMER3_PRINT = false;
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
        if APIC_OK_PRINT {
            APIC_OK_PRINT = false;
            true
        } else {
            false
        }
    }
}

pub fn gtimer3_ok() -> bool {
    // SAFETY: written on VMEXIT; read for gate.
    unsafe { GTIMER3_OK }
}

pub fn apic_ok() -> bool {
    // SAFETY: written on VMEXIT; read for gate / IRQ0 drop.
    unsafe { APIC_OK }
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
    Some(unsafe { HOST_TIMER_FOR_GUEST })
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
