//! CLI, REST API, Web UI, VM lifecycle.
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** (ADR-002)
//! VERIFICATION: N/A
//!
//! M5.0: durable create / start / stop / destroy surface over a guest table.
//! Bring-up in `src/main.rs` remains the live VMLAUNCH path; this module is the
//! management-plane state machine those ops will drive (M5.1+).

use crate::audit_log;
use crate::audit::AuditEvent;

/// Host / CI marker when the M5.0 lifecycle gate passes.
pub const M5_LIFE_OK_MARKER: &str = "RAYNU-V-M5-LIFE-OK";

/// Max guests tracked by the management-plane table (M4 NVM spine = 4).
pub const MGMT_GUEST_CAP: usize = 8;

/// VM lifecycle state for the management plane (not Proven Core vCPU state).
///
/// Transitions (M5.0):
///   Defined → Running → Stopped → Destroyed
///   Defined → Destroyed (cancel before start)
///   Stopped → Running (restart)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmLifecycle {
    Defined,
    Running,
    Stopped,
    Destroyed,
}

/// Error from a lifecycle transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleError {
    InvalidGuest,
    Full,
    NotFound,
    BadState,
}

/// One guest slot in the management-plane registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VmRecord {
    pub guest_id: u64,
    pub state: VmLifecycle,
}

/// Fixed-capacity guest lifecycle table.
pub struct VmTable {
    slots: [Option<VmRecord>; MGMT_GUEST_CAP],
    len: usize,
}

impl VmTable {
    pub const fn new() -> Self {
        Self {
            slots: [None; MGMT_GUEST_CAP],
            len: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Look up a non-destroyed guest by id.
    pub fn get(&self, guest_id: u64) -> Option<&VmRecord> {
        self.slots.iter().flatten().find(|r| {
            r.guest_id == guest_id && r.state != VmLifecycle::Destroyed
        })
    }

    fn get_mut(&mut self, guest_id: u64) -> Option<&mut VmRecord> {
        for slot in self.slots.iter_mut() {
            if let Some(r) = slot {
                if r.guest_id == guest_id && r.state != VmLifecycle::Destroyed {
                    return Some(r);
                }
            }
        }
        None
    }

    /// Create a guest in `Defined` state. Emits `VmCreated`.
    pub fn create(&mut self, guest_id: u64) -> Result<(), LifecycleError> {
        if guest_id == 0 {
            return Err(LifecycleError::InvalidGuest);
        }
        if self.get(guest_id).is_some() {
            return Err(LifecycleError::BadState);
        }
        // Reuse a Destroyed slot if present; else take a free slot.
        for slot in self.slots.iter_mut() {
            match slot {
                None => {
                    *slot = Some(VmRecord {
                        guest_id,
                        state: VmLifecycle::Defined,
                    });
                    self.len += 1;
                    audit_log!(AuditEvent::VmCreated { guest_id });
                    return Ok(());
                }
                Some(r) if r.state == VmLifecycle::Destroyed => {
                    *r = VmRecord {
                        guest_id,
                        state: VmLifecycle::Defined,
                    };
                    self.len += 1;
                    audit_log!(AuditEvent::VmCreated { guest_id });
                    return Ok(());
                }
                _ => {}
            }
        }
        Err(LifecycleError::Full)
    }

    /// Defined | Stopped → Running. Emits `VmStarted`.
    pub fn start(&mut self, guest_id: u64) -> Result<(), LifecycleError> {
        let rec = self.get_mut(guest_id).ok_or(LifecycleError::NotFound)?;
        match rec.state {
            VmLifecycle::Defined | VmLifecycle::Stopped => {
                rec.state = VmLifecycle::Running;
                audit_log!(AuditEvent::VmStarted { guest_id });
                Ok(())
            }
            _ => Err(LifecycleError::BadState),
        }
    }

    /// Running → Stopped. Emits `VmStopped`.
    pub fn stop(&mut self, guest_id: u64) -> Result<(), LifecycleError> {
        let rec = self.get_mut(guest_id).ok_or(LifecycleError::NotFound)?;
        match rec.state {
            VmLifecycle::Running => {
                rec.state = VmLifecycle::Stopped;
                audit_log!(AuditEvent::VmStopped { guest_id });
                Ok(())
            }
            _ => Err(LifecycleError::BadState),
        }
    }

    /// Defined | Stopped → Destroyed. Emits `VmDestroyed`.
    ///
    /// Running guests must be stopped first (BadState).
    pub fn destroy(&mut self, guest_id: u64) -> Result<(), LifecycleError> {
        let rec = self.get_mut(guest_id).ok_or(LifecycleError::NotFound)?;
        match rec.state {
            VmLifecycle::Defined | VmLifecycle::Stopped => {
                rec.state = VmLifecycle::Destroyed;
                // len counts active (non-destroyed) slots.
                self.len = self.len.saturating_sub(1);
                audit_log!(AuditEvent::VmDestroyed { guest_id });
                Ok(())
            }
            _ => Err(LifecycleError::BadState),
        }
    }
}

impl Default for VmTable {
    fn default() -> Self {
        Self::new()
    }
}

/// Initial lifecycle for a freshly created guest.
pub fn initial_lifecycle() -> VmLifecycle {
    VmLifecycle::Defined
}

/// Host-testable lifecycle round-trip (Defined→Running→Stopped→Destroyed).
pub fn prop_lifecycle_roundtrip() -> bool {
    let mut t = VmTable::new();
    let gid = 1u64;
    if t.create(gid).is_err() {
        return false;
    }
    if t.get(gid).map(|r| r.state) != Some(VmLifecycle::Defined) {
        return false;
    }
    if t.start(gid).is_err() {
        return false;
    }
    if t.get(gid).map(|r| r.state) != Some(VmLifecycle::Running) {
        return false;
    }
    if t.stop(gid).is_err() {
        return false;
    }
    if t.get(gid).map(|r| r.state) != Some(VmLifecycle::Stopped) {
        return false;
    }
    if t.destroy(gid).is_err() {
        return false;
    }
    t.get(gid).is_none() && t.len() == 0
}

pub mod m5_life_gate;

pub use m5_life_gate::run_m5_life_gate;

#[cfg(test)]
#[path = "mgmt_test.rs"]
mod mgmt_test;
