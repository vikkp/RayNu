//! Virtual local APIC for M3.11 (x2APIC MSRs + xAPIC MMIO via EPT hole).
//!
//! Pillar: [Z] · Proven Core: **outside** (ADR-002)
//!
//! M3.11 strategy (Latitude):
//! - Run an internal TSC countdown when the guest writes INIT_COUNT.
//! - Latch `GTIMER3` once that model shows a real drop (proves virtual timer).
//! - Expose a *stuck* CUR_COUNT (= INIT) to the guest so `calibrate_APIC_clock`
//!   gets delta≈0, prints "APIC frequency too slow", and disables the lapic
//!   clockevent — then the existing IRQ0 crutch can reach `/init` SHELL.
//! - Do **not** VM-entry-inject the LVT vector (panics without IRR/ISR; M3.12).

use crate::arch::apic::DEFAULT_APIC_PHYS;
use crate::arch::cpu;

/// COM1 marker when the virtual APIC timer model has been exercised.
pub const M3_GTIMER3_OK_MARKER: &str = "RAYNU-V-M3-GTIMER3-OK";

/// Guest APIC GPA (identity hole).
pub const APIC_GPA: u64 = DEFAULT_APIC_PHYS;

/// x2APIC MSR base (IA32_X2APIC_APICID = 0x802, …).
pub const X2APIC_MSR_BASE: u32 = 0x800;
pub const X2APIC_MSR_LAST: u32 = 0x83F;

// MMIO / x2APIC register offsets (same numbering).
const REG_ID: u32 = 0x20;
const REG_VERSION: u32 = 0x30;
const REG_TPR: u32 = 0x80;
const REG_EOI: u32 = 0xB0;
const REG_SVR: u32 = 0xF0;
const REG_LVT_TIMER: u32 = 0x320;
const REG_INIT_COUNT: u32 = 0x380;
const REG_CUR_COUNT: u32 = 0x390;
const REG_DIVIDE: u32 = 0x3E0;

const LVT_MASKED: u32 = 1 << 16;
const SVR_ENABLED: u32 = 1 << 8;

/// TSC ticks per undivided APIC-bus cycle (~100 MHz bus @ ~3.2 GHz TSC).
const TSC_PER_BUS_CYCLE: u64 = 32;
/// Internal drop required before latching GTIMER3.
const GTIMER3_DROP_THRESH: u32 = 500;

static mut APIC_ID: u32 = 0;
static mut APIC_TPR: u32 = 0;
static mut APIC_SVR: u32 = 0xFF | SVR_ENABLED;
static mut APIC_LVT_TIMER: u32 = LVT_MASKED | 0xEF;
static mut APIC_DIVIDE: u32 = 0b0011; // ÷16 (Linux calibrate default)
static mut APIC_INIT_COUNT: u32 = 0;
static mut GTIMER3_OK: bool = false;
/// Set when GTIMER3 latches; cleared by [`take_gtimer3_latch`].
static mut GTIMER3_PRINT: bool = false;
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

/// True countdown (HV-private).
fn current_count_real() -> u32 {
    // SAFETY: VMEXIT path.
    unsafe {
        if !TIMER_RUNNING || APIC_INIT_COUNT == 0 {
            return 0;
        }
        let init = APIC_INIT_COUNT as u64;
        let elapsed = elapsed_counts();
        if elapsed >= init {
            0
        } else {
            (init - elapsed) as u32
        }
    }
}

fn maybe_latch_gtimer3() {
    // SAFETY: VMEXIT path.
    unsafe {
        if GTIMER3_OK || !TIMER_RUNNING || APIC_INIT_COUNT == 0 {
            return;
        }
        let real = current_count_real();
        let dropped = APIC_INIT_COUNT.saturating_sub(real);
        if dropped >= GTIMER3_DROP_THRESH {
            GTIMER3_OK = true;
            GTIMER3_PRINT = true;
        }
    }
}

fn read_reg(reg: u32) -> u32 {
    // SAFETY: single-threaded VMEXIT path.
    unsafe {
        match reg {
            REG_ID => APIC_ID,
            REG_VERSION => 0x50014, // version 0x14, 6 LVT entries
            REG_TPR => APIC_TPR,
            REG_EOI => 0,
            REG_SVR => APIC_SVR,
            REG_LVT_TIMER => APIC_LVT_TIMER,
            REG_INIT_COUNT => APIC_INIT_COUNT,
            REG_CUR_COUNT => {
                maybe_latch_gtimer3();
                // Stuck at INIT → calibrate delta≈0 → Linux disables lapic timer.
                APIC_INIT_COUNT
            }
            REG_DIVIDE => APIC_DIVIDE,
            _ => 0,
        }
    }
}

fn start_countdown(val: u32) {
    // SAFETY: VMEXIT path.
    unsafe {
        APIC_INIT_COUNT = val;
        if val == 0 {
            TIMER_RUNNING = false;
            return;
        }
        TIMER_START_TSC = cpu::rdtsc();
        TIMER_RUNNING = true;
    }
}

fn write_reg(reg: u32, val: u32) {
    // SAFETY: single-threaded VMEXIT path.
    unsafe {
        match reg {
            REG_TPR => APIC_TPR = val,
            REG_EOI => {}
            REG_SVR => APIC_SVR = val,
            REG_LVT_TIMER => APIC_LVT_TIMER = val,
            REG_DIVIDE => APIC_DIVIDE = val & 0b1011,
            REG_INIT_COUNT => start_countdown(val),
            REG_ID | REG_VERSION | REG_CUR_COUNT => {}
            _ => {}
        }
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

pub fn gtimer3_ok() -> bool {
    // SAFETY: written on VMEXIT; read for gate.
    unsafe { GTIMER3_OK }
}

/// Host one-shot arming is unused in M3.11 (no LVT inject).
pub fn host_timer_armed_for_guest() -> bool {
    false
}

pub fn rdmsr(index: u32) -> Option<u64> {
    if !is_x2apic_msr(index) {
        return None;
    }
    Some(read_reg(reg_from_msr(index)) as u64)
}

/// Handle x2APIC WRMSR. Always reports "do not arm host timer" in M3.11.
pub fn wrmsr(index: u32, value: u64) -> Option<bool> {
    if !is_x2apic_msr(index) {
        return None;
    }
    write_reg(reg_from_msr(index), value as u32);
    Some(false)
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
