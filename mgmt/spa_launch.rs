//! E4 — SPA create/start queues a real VMLAUNCH (private EPT, slab VMCS).
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** (ADR-002). The HTTP handler only sets a flag;
//! `schedule_preempt` performs VMLAUNCH in VMX root.
//!
//! G1–G3 M4 stubs keep VMCS in the G0 identity pool (Linux can scribble).
//! SPA start relocates G1 into its 2 MiB slab (already punched out of G0 EPT)
//! and builds a **single 2 MiB** private EPT. G0's VMCS is `VMCLEAR`'d and
//! copied to a host-only punched slab before leaving G0 (iron: identity-pool
//! `VMPTRLD` of slot 0 failed after SHELL). Distro installer / TLS remain later.

/// Iron marker when SPA start VMLAUNCHes the private-EPT SHELL guest.
pub const M7_E4_SPA_LAUNCH_OK_MARKER: &str = "RAYNU-V-M7-E4-SPA-LAUNCH-OK";

static mut PENDING_START: Option<u64> = None;
static mut PENDING_STOP: bool = false;

/// Queue a SPA `POST /vms/{id}/start` for the coexist scheduler.
pub fn note_spa_start(guest_id: u64) {
    // SAFETY: BSP-only HTTP tick / host tests.
    unsafe {
        PENDING_START = Some(guest_id);
    }
}

/// Queue a SPA stop (park the E4 slot; G0 keeps running).
pub fn note_spa_stop(_guest_id: u64) {
    unsafe {
        PENDING_STOP = true;
    }
}

/// Take a pending start (scheduler). `None` if idle.
pub fn take_spa_start() -> Option<u64> {
    unsafe { PENDING_START.take() }
}

/// Take a pending stop.
pub fn take_spa_stop() -> bool {
    unsafe {
        let v = PENDING_STOP;
        PENDING_STOP = false;
        v
    }
}

/// Host: start REST queues a launch token.
pub fn prop_spa_start_queues() -> bool {
    let _ = take_spa_start();
    let _ = take_spa_stop();
    note_spa_start(9);
    let a = take_spa_start() == Some(9);
    let b = take_spa_start().is_none();
    note_spa_stop(9);
    let c = take_spa_stop() && !take_spa_stop();
    a && b && c
}

#[cfg(test)]
#[path = "spa_launch_test.rs"]
mod spa_launch_test;
