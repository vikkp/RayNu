//! Bare-metal dual-vCPU SMP probe (M4.5).
//!
//! Pillar: [V]
//! Proven Core: **outside** (ADR-002) — bring-up probe, not Proven Core
//!
//! Accepted MV: BSP + AP under **two VMCS / one guest id / shared EPT**.
//! AP wake is **host-documented**: after BSP stores its ready flag and HLTs,
//! the hypervisor VMLAUNCHes the AP VMCS (INIT-SIPI equivalent for this gate).
//! Both vCPUs write distinct bytes on a host-owned shared flag page; when both
//! are seen the host latches [`M4_SMP_OK_MARKER`]. Full Linux `CONFIG_SMP` /
//! ICR Wait-for-SIPI is deferred.

/// COM1 / gate marker when BSP + AP both latched.
pub const M4_SMP_OK_MARKER: &str = "RAYNU-V-M4-SMP-OK";

/// Offsets in the shared flag page.
pub const OFF_BSP_READY: usize = 0;
pub const OFF_AP_READY: usize = 1;
pub const READY_MAGIC: u8 = 0xA5;

static mut FLAG_PHYS: u64 = 0;
static mut BSP_SEEN: bool = false;
static mut AP_SEEN: bool = false;
static mut SMP_OK: bool = false;
static mut SMP_MARKED: bool = false;

/// Install the shared flag page (host-owned allocator frame).
///
/// SAFETY: `flag_phys` is a writable identity-mapped 4 KiB frame.
pub unsafe fn init(flag_phys: u64) {
    FLAG_PHYS = flag_phys;
    BSP_SEEN = false;
    AP_SEEN = false;
    SMP_OK = false;
    SMP_MARKED = false;
    core::ptr::write_bytes(flag_phys as *mut u8, 0, 4096);
}

pub fn flag_phys() -> u64 {
    unsafe { FLAG_PHYS }
}

pub fn smp_ok() -> bool {
    unsafe { SMP_OK }
}

pub fn take_smp_ok_latch() -> bool {
    unsafe {
        if SMP_OK && !SMP_MARKED {
            SMP_MARKED = true;
            true
        } else {
            false
        }
    }
}

/// Observe BSP HLT: require `flag[OFF_BSP_READY] == READY_MAGIC`.
pub fn note_bsp_ready() -> bool {
    unsafe {
        if FLAG_PHYS == 0 {
            return false;
        }
        let v = core::ptr::read_volatile((FLAG_PHYS as *const u8).add(OFF_BSP_READY));
        if v == READY_MAGIC {
            BSP_SEEN = true;
            maybe_latch();
            true
        } else {
            false
        }
    }
}

/// Observe AP HLT: require `flag[OFF_AP_READY] == READY_MAGIC`.
pub fn note_ap_ready() -> bool {
    unsafe {
        if FLAG_PHYS == 0 {
            return false;
        }
        let v = core::ptr::read_volatile((FLAG_PHYS as *const u8).add(OFF_AP_READY));
        if v == READY_MAGIC {
            AP_SEEN = true;
            maybe_latch();
            true
        } else {
            false
        }
    }
}

unsafe fn maybe_latch() {
    if BSP_SEEN && AP_SEEN {
        SMP_OK = true;
    }
}

#[cfg(test)]
#[path = "smp_probe_test.rs"]
mod smp_probe_test;
