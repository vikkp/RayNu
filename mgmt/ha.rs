//! M6.6 mock HA pair + security harden checklist (outside Proven Core).
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** (ADR-002)
//! VERIFICATION: N/A
//!
//! Single-host primary↔standby failover over `VmTable` (bring-up mock).
//! Not cross-host live migrate (that is M6.3). External soak/faults → M6.7+.

use crate::audit_log;
use crate::audit::AuditEvent;

use super::api::{
    auth_allows, dispatch_rest, BRINGUP_AUTH_TOKEN, RestMethod, RestRequest, RestResponse,
};
use super::{LifecycleError, VmLifecycle, VmTable, MGMT_GUEST_CAP};

/// Host / CI marker when the M6.6 HA / harden gate passes.
pub const M6_HA_OK_MARKER: &str = "RAYNU-V-M6-HA-OK";

/// HA / harden GAP closed in M6.6.
pub const HA_GAP_NOTE: &str = "GAP(CLOSED M6.6): HA / security harden";

/// Role in the mock HA pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HaRole {
    Primary,
    Standby,
}

impl HaRole {
    pub fn tag(self) -> u8 {
        match self {
            HaRole::Primary => 0,
            HaRole::Standby => 1,
        }
    }
}

/// Error from a mock HA transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HaError {
    WrongRole,
    Lifecycle(LifecycleError),
}

/// Primary + standby `VmTable` pair with one active role.
pub struct HaPair {
    pub primary: VmTable,
    pub standby: VmTable,
    pub active: HaRole,
}

impl HaPair {
    pub const fn new() -> Self {
        Self {
            primary: VmTable::new(),
            standby: VmTable::new(),
            active: HaRole::Primary,
        }
    }

    /// Table currently serving guests.
    pub fn active_table(&self) -> &VmTable {
        match self.active {
            HaRole::Primary => &self.primary,
            HaRole::Standby => &self.standby,
        }
    }

    pub fn active_table_mut(&mut self) -> &mut VmTable {
        match self.active {
            HaRole::Primary => &mut self.primary,
            HaRole::Standby => &mut self.standby,
        }
    }

    /// Fail over Primary → Standby: recreate active guests on standby, clear primary.
    ///
    /// Running guests restart as Running on standby (uses existing stop/start path).
    pub fn failover_to_standby(&mut self) -> Result<u32, HaError> {
        self.failover(HaRole::Standby)
    }

    /// Fail over Standby → Primary (symmetric reverse hop).
    pub fn failover_to_primary(&mut self) -> Result<u32, HaError> {
        self.failover(HaRole::Primary)
    }

    fn failover(&mut self, to: HaRole) -> Result<u32, HaError> {
        let _ = (HA_GAP_NOTE, M6_HA_OK_MARKER);
        if self.active == to {
            return Err(HaError::WrongRole);
        }
        let from = self.active;
        audit_log!(AuditEvent::HaFailoverStarted {
            from_role: from.tag(),
            to_role: to.tag(),
        });

        // Snapshot source guests before mutating either table.
        let mut snap = [None; MGMT_GUEST_CAP];
        let n = match from {
            HaRole::Primary => self.primary.list(&mut snap),
            HaRole::Standby => self.standby.list(&mut snap),
        };

        let mut transferred: u32 = 0;
        for i in 0..n {
            let Some(rec) = snap[i] else {
                continue;
            };
            let gid = rec.guest_id;
            let was_running = rec.state == VmLifecycle::Running;

            // Quiesce + remove from source (Running must stop before destroy).
            {
                let src = match from {
                    HaRole::Primary => &mut self.primary,
                    HaRole::Standby => &mut self.standby,
                };
                if was_running {
                    src.stop(gid).map_err(HaError::Lifecycle)?;
                }
                src.destroy(gid).map_err(HaError::Lifecycle)?;
            }

            // Recreate on destination; Defined/Running restart as Running.
            let dest = match to {
                HaRole::Primary => &mut self.primary,
                HaRole::Standby => &mut self.standby,
            };
            dest.create(gid).map_err(HaError::Lifecycle)?;
            match rec.state {
                VmLifecycle::Running | VmLifecycle::Defined => {
                    dest.start(gid).map_err(HaError::Lifecycle)?;
                }
                VmLifecycle::Stopped => {
                    dest.start(gid).map_err(HaError::Lifecycle)?;
                    dest.stop(gid).map_err(HaError::Lifecycle)?;
                }
                VmLifecycle::Destroyed => {}
            }
            transferred += 1;
        }

        self.active = to;
        audit_log!(AuditEvent::HaFailoverCompleted {
            guest_count: transferred,
        });
        Ok(transferred)
    }
}

impl Default for HaPair {
    fn default() -> Self {
        Self::new()
    }
}

/// REST over the HA pair (auth required; same bring-up token as M6.4).
///
/// Routes:
/// - `GET  /ha`           → status (active role as Listed.count: 0=Primary, 1=Standby)
/// - `POST /ha/failover`  → failover away from current active role
pub fn dispatch_ha_rest(pair: &mut HaPair, req: RestRequest<'_>) -> RestResponse {
    let tag = match req.method {
        RestMethod::Get => 1u8,
        RestMethod::Post => 2u8,
        RestMethod::Delete => 3u8,
    };
    if !auth_allows(req.auth_token) {
        audit_log!(AuditEvent::AuthDenied { method_tag: tag });
        return RestResponse {
            status: 401,
            reply: None,
        };
    }
    audit_log!(AuditEvent::AuthAllowed { method_tag: tag });

    let path = req.path.trim().trim_end_matches('/');
    match (req.method, path) {
        (RestMethod::Get, "/ha") => RestResponse {
            status: 200,
            reply: Some(super::api::ApiReply::Listed {
                count: pair.active.tag() as usize,
            }),
        },
        (RestMethod::Post, "/ha/failover") => {
            let to = match pair.active {
                HaRole::Primary => HaRole::Standby,
                HaRole::Standby => HaRole::Primary,
            };
            match pair.failover(to) {
                Ok(_) => RestResponse {
                    status: 200,
                    reply: Some(super::api::ApiReply::Ok),
                },
                Err(HaError::WrongRole) => RestResponse {
                    status: 409,
                    reply: None,
                },
                Err(HaError::Lifecycle(_)) => RestResponse {
                    status: 500,
                    reply: None,
                },
            }
        }
        _ => RestResponse {
            status: 400,
            reply: None,
        },
    }
}

/// Host-testable: primary guest survives failover as Running on standby.
pub fn prop_ha_failover_restart() -> bool {
    let mut pair = HaPair::new();
    let gid = 3u64;
    if pair.primary.create(gid).is_err() {
        return false;
    }
    if pair.primary.start(gid).is_err() {
        return false;
    }
    if pair.active != HaRole::Primary {
        return false;
    }
    let n = match pair.failover_to_standby() {
        Ok(c) => c,
        Err(_) => return false,
    };
    if n != 1 || pair.active != HaRole::Standby {
        return false;
    }
    if pair.primary.get(gid).is_some() {
        return false;
    }
    if pair.standby.get(gid).map(|r| r.state) != Some(VmLifecycle::Running) {
        return false;
    }
    // Symmetric reverse hop.
    let n2 = match pair.failover_to_primary() {
        Ok(c) => c,
        Err(_) => return false,
    };
    n2 == 1
        && pair.active == HaRole::Primary
        && pair.standby.get(gid).is_none()
        && pair.primary.get(gid).map(|r| r.state) == Some(VmLifecycle::Running)
        && HA_GAP_NOTE.contains("CLOSED M6.6")
        && M6_HA_OK_MARKER == "RAYNU-V-M6-HA-OK"
}

/// Host-testable security harden checklist (auth, fail-closed, safe defaults).
pub fn prop_security_harden_checklist() -> bool {
    // Auth required (M6.4 surface still holds).
    if auth_allows(None) || auth_allows(Some("wrong")) || !auth_allows(Some(BRINGUP_AUTH_TOKEN))
    {
        return false;
    }

    // Privileged REST fail-closed: no token → 401, table unchanged.
    let mut t = VmTable::new();
    let denied = dispatch_rest(
        &mut t,
        RestRequest {
            method: RestMethod::Post,
            path: "/vms/9",
            auth_token: None,
        },
    );
    if denied.status != 401 || t.get(9).is_some() || t.len() != 0 {
        return false;
    }

    // HA REST also fail-closed.
    let mut pair = HaPair::new();
    let ha_denied = dispatch_ha_rest(
        &mut pair,
        RestRequest {
            method: RestMethod::Post,
            path: "/ha/failover",
            auth_token: None,
        },
    );
    if ha_denied.status != 401 || pair.active != HaRole::Primary {
        return false;
    }

    // Safe defaults: guest 0 invalid; destroy while Running → BadState.
    let mut safe = VmTable::new();
    if safe.create(0) != Err(LifecycleError::InvalidGuest) {
        return false;
    }
    if safe.create(2).is_err() || safe.start(2).is_err() {
        return false;
    }
    if safe.destroy(2) != Err(LifecycleError::BadState) {
        return false;
    }
    if safe.stop(2).is_err() || safe.destroy(2).is_err() {
        return false;
    }

    // Auth'd HA failover works.
    let mut pair2 = HaPair::new();
    if pair2.primary.create(5).is_err() || pair2.primary.start(5).is_err() {
        return false;
    }
    let ok = dispatch_ha_rest(
        &mut pair2,
        RestRequest {
            method: RestMethod::Post,
            path: "/ha/failover",
            auth_token: Some(BRINGUP_AUTH_TOKEN),
        },
    );
    ok.status == 200
        && pair2.active == HaRole::Standby
        && pair2.standby.get(5).map(|r| r.state) == Some(VmLifecycle::Running)
        && !include_str!("api.rs").contains("Auth stub: always allows")
}

#[cfg(test)]
#[path = "ha_test.rs"]
mod ha_test;
