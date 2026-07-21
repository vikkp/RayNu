//! CLI, REST API, Web UI, VM lifecycle.
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** (ADR-002)
//! VERIFICATION: N/A
//!
//! M5.0: durable create / start / stop / destroy surface over a guest table.
//! M5.1: CLI + REST dispatch (`api`) over that table.
//! M5.2: embedded Web UI SPA (`webui`, PE `.aswebui`) drives list/start/stop.
//! M6.4: REST auth with bring-up mock token (`m6_auth_gate`).
//! M6.6: mock HA primary↔standby failover + harden checklist (`ha`, `m6_ha_gate`).
//! M6.7: fault injection suite (`fault`, `m6_fault_gate`).
//! M6.8: 72-hr soak thresholds (`soak`, `m6_soak_gate`).
//! M6.9: external audit + spec review (`ext`, `m6_ext_gate`).
//! Bring-up in `src/main.rs` remains the live VMLAUNCH path; this module is the
//! management-plane state machine those ops drive.

use crate::audit_log;
use crate::audit::AuditEvent;

/// Host / CI marker when the M5.0 lifecycle gate passes.
pub const M5_LIFE_OK_MARKER: &str = "RAYNU-V-M5-LIFE-OK";

/// Host / CI marker when the M5.1 API gate passes (re-export).
pub use api::M5_API_OK_MARKER;

/// Host / CI marker when the M5.2 Web UI gate passes (re-export).
pub use webui::M5_WEBUI_OK_MARKER;

/// Max guests tracked by the management-plane table (M5.5 migrate needs ≥10).
pub const MGMT_GUEST_CAP: usize = 16;

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

    /// Copy active (non-destroyed) records into `out`; returns count written.
    pub fn list(&self, out: &mut [Option<VmRecord>]) -> usize {
        let mut n = 0;
        for slot in self.slots.iter() {
            if let Some(r) = slot {
                if r.state != VmLifecycle::Destroyed {
                    if n < out.len() {
                        out[n] = Some(*r);
                        n += 1;
                    }
                }
            }
        }
        n
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

pub mod api;
pub mod ext;
pub mod fault;
pub mod ha;
pub mod m5_api_gate;
pub mod m5_life_gate;
pub mod m5_webui_gate;
pub mod m6_auth_gate;
pub mod m6_ext_gate;
pub mod m6_fault_gate;
pub mod m6_ha_gate;
pub mod m6_soak_gate;
pub mod m7_ship_gate;
pub mod ship;
pub mod soak;
pub mod webui;

pub use api::{
    dispatch_cli, dispatch_rest, parse_cli, parse_rest_method, prop_auth_deny_allow,
    prop_cli_rest_roundtrip, ApiReply, CliCommand, RestMethod, RestRequest, RestResponse,
    AUTH_GAP_NOTE, BRINGUP_AUTH_TOKEN, M6_AUTH_OK_MARKER,
};
pub use ext::{
    prop_external_audit_package, prop_findings_no_open_critical, prop_spec_review_filed,
    EXT_GAP_NOTE, M6_EXT_OK_MARKER,
};
pub use fault::{
    prop_corrupt_page_fail_closed, prop_drop_irq_fail_closed, prop_fault_suite,
    prop_kill_vcpu_recover, prop_net_partition_recover, FAULT_GAP_NOTE, M6_FAULT_OK_MARKER,
};
pub use ha::{
    dispatch_ha_rest, prop_ha_failover_restart, prop_security_harden_checklist, HaPair, HaRole,
    HA_GAP_NOTE, M6_HA_OK_MARKER,
};
pub use m5_api_gate::run_m5_api_gate;
pub use m5_life_gate::run_m5_life_gate;
pub use m5_webui_gate::run_m5_webui_gate;
pub use m6_auth_gate::{run_m6_auth_gate, M6_AUTH_GATE_MARKER};
pub use m6_ext_gate::{run_m6_ext_gate, M6_EXT_GATE_MARKER};
pub use m6_fault_gate::{run_m6_fault_gate, M6_FAULT_GATE_MARKER};
pub use m6_ha_gate::{run_m6_ha_gate, M6_HA_GATE_MARKER};
pub use m6_soak_gate::{run_m6_soak_gate, M6_SOAK_GATE_MARKER};
pub use m7_ship_gate::{run_m7_ship_gate, M7_SHIP_GATE_MARKER};
pub use ship::{
    prop_release_kit_package, M7_SHIP_OK_MARKER, SHIP_GAP_NOTE,
};
pub use soak::{
    prop_soak_72h_thresholds, run_soak_simulation, thresholds_met, SoakMetrics, SOAK_GAP_NOTE,
    SOAK_TARGET_HOURS, M6_SOAK_OK_MARKER,
};
pub use webui::{
    dispatch_webui_action, load_webui, prop_webui_list_start_stop, WebUiAction,
};

#[cfg(test)]
#[path = "mgmt_test.rs"]
mod mgmt_test;
