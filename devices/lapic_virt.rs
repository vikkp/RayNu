//! Virtual local APIC for M3.11 (x2APIC MSRs + xAPIC MMIO via EPT hole).
//!
//! Pillar: [Z] · Proven Core: **outside** (ADR-002)
//! Timer fire still uses the host LAPIC one-shot; inject uses `sched/interrupt`.

use crate::arch::apic::DEFAULT_APIC_PHYS;

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
const SVR_ENABLED: u32 = 1 << 8;

static mut APIC_ID: u32 = 0;
static mut APIC_TPR: u32 = 0;
static mut APIC_SVR: u32 = 0xFF | SVR_ENABLED;
static mut APIC_LVT_TIMER: u32 = LVT_MASKED | 0xEF;
static mut APIC_DIVIDE: u32 = 0b1011;
static mut APIC_INIT_COUNT: u32 = 0;
static mut APIC_CUR_COUNT: u32 = 0;
/// Host one-shot armed on behalf of guest INIT_COUNT write.
static mut HOST_TIMER_FOR_GUEST: bool = false;
static mut GTIMER3_OK: bool = false;
/// Pending guest vector to inject after host timer VMEXIT.
static mut PENDING_VECTOR: Option<u8> = None;

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
            REG_CUR_COUNT => APIC_CUR_COUNT,
            REG_DIVIDE => APIC_DIVIDE,
            _ => 0,
        }
    }
}

/// Returns `Some(host_vector_hint)` when the host one-shot should be armed.
fn write_reg(reg: u32, val: u32) -> Option<()> {
    // SAFETY: single-threaded VMEXIT path.
    unsafe {
        match reg {
            REG_TPR => APIC_TPR = val,
            REG_EOI => {}
            REG_SVR => APIC_SVR = val,
            REG_LVT_TIMER => APIC_LVT_TIMER = val,
            REG_DIVIDE => APIC_DIVIDE = val & 0b1011,
            REG_INIT_COUNT => {
                APIC_INIT_COUNT = val;
                APIC_CUR_COUNT = val;
                if val != 0 && (APIC_LVT_TIMER & LVT_MASKED) == 0 && (APIC_SVR & SVR_ENABLED) != 0
                {
                    HOST_TIMER_FOR_GUEST = true;
                    PENDING_VECTOR = Some((APIC_LVT_TIMER & 0xFF) as u8);
                }
            }
            REG_ID | REG_VERSION | REG_CUR_COUNT => {}
            _ => {}
        }
    }
    Some(())
}

pub fn host_timer_armed_for_guest() -> bool {
    // SAFETY: VMEXIT path.
    unsafe { HOST_TIMER_FOR_GUEST }
}

/// Called on host external-interrupt VMEXIT. If this was our guest timer,
/// returns the guest vector to inject and clears the arm flag.
pub fn take_guest_timer_inject() -> Option<u32> {
    // SAFETY: VMEXIT path.
    unsafe {
        if !HOST_TIMER_FOR_GUEST {
            return None;
        }
        HOST_TIMER_FOR_GUEST = false;
        APIC_CUR_COUNT = 0;
        let v = match PENDING_VECTOR {
            Some(x) => {
                PENDING_VECTOR = None;
                x as u32
            }
            None => 0xEF,
        };
        if !GTIMER3_OK {
            GTIMER3_OK = true;
        }
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
