//! Virtual local APIC for M3.11 (x2APIC MSRs + xAPIC MMIO via EPT hole).
//!
//! Pillar: [Z] · Proven Core: **outside** (ADR-002)
//! Timer fire still uses the host LAPIC one-shot; inject uses `sched/interrupt`.
//!
//! CUR_COUNT decreases based on host TSC even while LVT is masked — Linux
//! `calibrate_APIC_clock` polls TMCCT with a masked LVTT and disables the
//! timer if the delta is ~0 ("APIC frequency too slow").

use crate::arch::apic::DEFAULT_APIC_PHYS;
use crate::arch::cpu;

/// COM1 marker when emulated guest APIC timer has fired + inject once.
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
const LVT_PERIODIC: u32 = 1 << 17;
const SVR_ENABLED: u32 = 1 << 8;

/// TSC ticks per undivided APIC-bus cycle (~100 MHz bus @ ~3.2 GHz TSC).
/// Nested TSC rates vary; this stays fast enough that a ~100 ms calibrate
/// window sees a large TMCCT delta, and slow enough not to hit 0 early on
/// Linux's 0x0fffffff INIT_COUNT.
const TSC_PER_BUS_CYCLE: u64 = 32;

static mut APIC_ID: u32 = 0;
static mut APIC_TPR: u32 = 0;
static mut APIC_SVR: u32 = 0xFF | SVR_ENABLED;
static mut APIC_LVT_TIMER: u32 = LVT_MASKED | 0xEF;
static mut APIC_DIVIDE: u32 = 0b0011; // ÷16 (Linux calibrate default)
static mut APIC_INIT_COUNT: u32 = 0;
/// Host one-shot armed on behalf of guest (LVT unmasked + counting).
static mut HOST_TIMER_FOR_GUEST: bool = false;
static mut GTIMER3_OK: bool = false;
/// Pending guest vector to inject after host timer VMEXIT.
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

/// Decode APIC divide-configuration register → divisor.
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
    unsafe {
        (APIC_LVT_TIMER & LVT_MASKED) == 0 && (APIC_SVR & SVR_ENABLED) != 0
    }
}

fn arm_guest_delivery_if_needed() {
    // SAFETY: VMEXIT path.
    unsafe {
        if TIMER_RUNNING && timer_should_deliver() && current_count_raw() != 0 {
            HOST_TIMER_FOR_GUEST = true;
            PENDING_VECTOR = Some((APIC_LVT_TIMER & 0xFF) as u8);
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
            let phase = elapsed % init;
            (init - phase) as u32
        } else if elapsed >= init {
            0
        } else {
            (init - elapsed) as u32
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
            REG_EOI => {}
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
            REG_ID | REG_VERSION | REG_CUR_COUNT => {}
            _ => {}
        }
    }
}

pub fn host_timer_armed_for_guest() -> bool {
    // SAFETY: VMEXIT path.
    unsafe { HOST_TIMER_FOR_GUEST }
}

/// Called on host external-interrupt VMEXIT when the guest virtual timer
/// asked for a host one-shot. Marks GTIMER3 and clears the arm flag.
///
/// Returns `true` if this consumed a guest-armed host timer. Does **not**
/// supply a guest vector — Latitude showed that VM-entry inject of the LVT
/// vector (even with IF=1) panics Linux (`Fatal exception in interrupt`)
/// without IRR/ISR fidelity. M3.11 gate = arm + expire; inject is M3.12.
pub fn acknowledge_guest_timer_fire() -> bool {
    // SAFETY: VMEXIT path.
    unsafe {
        if !HOST_TIMER_FOR_GUEST {
            return false;
        }
        HOST_TIMER_FOR_GUEST = false;
        PENDING_VECTOR = None;
        if !GTIMER3_OK {
            GTIMER3_OK = true;
        }
        // Keep countdown model for CUR_COUNT reads; do not re-request inject.
        if (APIC_LVT_TIMER & LVT_PERIODIC) != 0 && APIC_INIT_COUNT != 0 {
            TIMER_START_TSC = cpu::rdtsc();
            TIMER_RUNNING = true;
        } else {
            TIMER_RUNNING = false;
        }
        true
    }
}

/// Legacy helper for unit tests: acknowledge + return the would-be vector.
pub fn take_guest_timer_inject() -> Option<u32> {
    // SAFETY: test / VMEXIT path.
    unsafe {
        if !HOST_TIMER_FOR_GUEST {
            return None;
        }
        let v = PENDING_VECTOR.unwrap_or(0xEF) as u32;
        let _ = acknowledge_guest_timer_fire();
        Some(v)
    }
}

pub fn gtimer3_ok() -> bool {
    // SAFETY: written on VMEXIT; read for gate.
    unsafe { GTIMER3_OK }
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

/// Handle APIC MMIO access at `gpa`. `write_val` is the store datum when `is_write`.
/// Returns `Some(read_val)` for loads, `None` for stores.
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
    // IA32_APIC_BASE (0x1B) read+write — guest shadow, not host.
    set_msr_bit(base, 0x1B, true);
    set_msr_bit(base, 0x1B, false);
}

unsafe fn set_msr_bit(bitmap: *mut u8, msr: u32, read: bool) {
    // Low MSR bitmaps: reads at +0, writes at +1024.
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
