//! Local APIC helpers for M2.5 one-shot timer bring-up.
//!
//! Pillar: [D] [V]
//! Proven Core: **outside** (hardware glue); inject path stays in `sched/`.
//!
//! Programs the host LAPIC timer while VMX root stays CLI. Guest never
//! touches the APIC — the HV EOIs and re-injects via VM-entry info.

use crate::arch::cpu;

/// IA32_APIC_BASE MSR.
pub const IA32_APIC_BASE: u32 = 0x1B;
pub const APIC_BASE_ENABLE: u64 = 1 << 11;
pub const APIC_BASE_X2APIC: u64 = 1 << 10;
pub const APIC_BASE_ADDR_MASK: u64 = !0xFFF;

/// Default xAPIC MMIO base (identity-mapped under UEFI + 4 GiB EPT).
pub const DEFAULT_APIC_PHYS: u64 = 0xFEE0_0000;

// xAPIC MMIO offsets
const OFF_EOI: u64 = 0xB0;
const OFF_SVR: u64 = 0xF0;
const OFF_LVT_TIMER: u64 = 0x320;
const OFF_INIT_COUNT: u64 = 0x380;
const OFF_DIVIDE: u64 = 0x3E0;

// x2APIC MSRs (when EXTD=1)
const MSR_EOI: u32 = 0x80B;
const MSR_SVR: u32 = 0x80F;
const MSR_LVT_TIMER: u32 = 0x832;
const MSR_INIT_COUNT: u32 = 0x838;
const MSR_DIVIDE: u32 = 0x83E;

/// SVR: APIC software enable + spurious vector 0xFF.
const SVR_ENABLE: u32 = (1 << 8) | 0xFF;

/// LVT timer: one-shot (bits 18:17 = 0), unmasked, vector in low 8 bits.
const LVT_MASKED: u32 = 1 << 16;

/// Divide config: bits encode ÷1 (SDM: 0b1011).
const DIVIDE_BY_1: u32 = 0b1011;

/// One-shot initial count — large enough for nested VMWRITE/serial before resume.
pub const ONESHOT_COUNT: u32 = 0x0100_0000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApicError {
    Disabled,
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ApicMode {
    XApic { base: u64 },
    X2Apic,
}

fn probe() -> Result<ApicMode, ApicError> {
    // SAFETY: IA32_APIC_BASE is architectural on modern x86_64.
    let v = unsafe { cpu::rdmsr(IA32_APIC_BASE) };
    if (v & APIC_BASE_ENABLE) == 0 {
        return Err(ApicError::Disabled);
    }
    if (v & APIC_BASE_X2APIC) != 0 {
        Ok(ApicMode::X2Apic)
    } else {
        let base = v & APIC_BASE_ADDR_MASK;
        Ok(ApicMode::XApic {
            base: if base == 0 { DEFAULT_APIC_PHYS } else { base },
        })
    }
}

unsafe fn mmio_write(base: u64, off: u64, val: u32) {
    core::ptr::write_volatile((base + off) as *mut u32, val);
}

unsafe fn mmio_read(base: u64, off: u64) -> u32 {
    core::ptr::read_volatile((base + off) as *const u32)
}

unsafe fn outb(port: u16, val: u8) {
    core::arch::asm!("out dx, al", in("dx") port, in("al") val, options(nostack, preserves_flags));
}

/// Mask both 8259A PICs so only the LAPIC timer can raise IRQs.
///
/// SAFETY: I/O ports 0x21 / 0xA1 are the PIC IMRs.
pub unsafe fn mask_pic() {
    outb(0x21, 0xFF);
    outb(0xA1, 0xFF);
}

/// Write APIC EOI (xAPIC MMIO or x2APIC MSR).
///
/// SAFETY: APIC enabled; called from VMX root after external-interrupt exit.
pub unsafe fn eoi() -> Result<(), ApicError> {
    match probe()? {
        ApicMode::XApic { base } => {
            mmio_write(base, OFF_EOI, 0);
            Ok(())
        }
        ApicMode::X2Apic => {
            cpu::wrmsr(MSR_EOI, 0);
            Ok(())
        }
    }
}

/// Arm a one-shot LAPIC timer for `vector` with `count` bus ticks.
///
/// Host must remain CLI. Masks PIC first. Does not STI.
///
/// SAFETY: identity-mapped APIC page (or x2APIC MSRs); VMX root / early boot.
pub unsafe fn arm_oneshot_timer(vector: u8, count: u32) -> Result<(), ApicError> {
    mask_pic();
    match probe()? {
        ApicMode::XApic { base } => {
            // Soft-enable APIC.
            let svr = mmio_read(base, OFF_SVR);
            mmio_write(base, OFF_SVR, (svr & !0xFF) | SVR_ENABLE);
            // Mask timer while programming.
            mmio_write(base, OFF_LVT_TIMER, LVT_MASKED | (vector as u32));
            mmio_write(base, OFF_DIVIDE, DIVIDE_BY_1);
            mmio_write(base, OFF_INIT_COUNT, count);
            // Unmask, one-shot.
            mmio_write(base, OFF_LVT_TIMER, vector as u32);
            let _ = mmio_read(base, OFF_LVT_TIMER);
            Ok(())
        }
        ApicMode::X2Apic => {
            let svr = cpu::rdmsr(MSR_SVR) as u32;
            cpu::wrmsr(MSR_SVR, ((svr & !0xFF) | SVR_ENABLE) as u64);
            cpu::wrmsr(MSR_LVT_TIMER, (LVT_MASKED | (vector as u32)) as u64);
            cpu::wrmsr(MSR_DIVIDE, DIVIDE_BY_1 as u64);
            cpu::wrmsr(MSR_INIT_COUNT, count as u64);
            cpu::wrmsr(MSR_LVT_TIMER, vector as u64);
            Ok(())
        }
    }
}

/// Convenience: arm with [`ONESHOT_COUNT`] and the bring-up IRQ vector.
pub unsafe fn arm_bringup_timer(vector: u8) -> Result<(), ApicError> {
    arm_oneshot_timer(vector, ONESHOT_COUNT)
}

#[cfg(test)]
mod apic_test {
    use super::*;

    #[test]
    fn constants_stable() {
        assert_eq!(IA32_APIC_BASE, 0x1B);
        assert_eq!(DEFAULT_APIC_PHYS, 0xFEE0_0000);
        assert_eq!(ONESHOT_COUNT, 0x0100_0000);
        assert_eq!(DIVIDE_BY_1, 0b1011);
        assert_eq!(LVT_MASKED, 1 << 16);
    }
}
