//! M6.6 host verification gate (HA failover + security harden).
//!
//! Pillar: [Z] [A]
//! Proven Core: outside (companion to `mgmt/ha` — not a boot path).
//!
//! Checks mock primary→standby failover, harden checklist (auth fail-closed,
//! safe defaults), closed GAP, marker, and smoke/CI wiring.

use super::ha::{
    prop_ha_failover_restart, prop_security_harden_checklist, HA_GAP_NOTE, M6_HA_OK_MARKER,
};

/// Host / CI marker when the M6.6 HA gate passes.
pub const M6_HA_GATE_MARKER: &str = M6_HA_OK_MARKER;

/// True when HA module exposes failover, closed GAP, and marker.
pub fn ha_surface_present() -> bool {
    let s = include_str!("ha.rs");
    s.contains("fn failover_to_standby(")
        && s.contains("fn prop_ha_failover_restart(")
        && s.contains("fn prop_security_harden_checklist(")
        && s.contains("fn dispatch_ha_rest(")
        && s.contains("struct HaPair")
        && s.contains(M6_HA_OK_MARKER)
        && s.contains(HA_GAP_NOTE)
        && HA_GAP_NOTE.contains("CLOSED M6.6")
        && !include_str!("api.rs").contains("Auth stub: always allows")
}

/// True when audit HA failover events exist.
pub fn audit_ha_events_present() -> bool {
    let s = include_str!("../audit/integrity.rs");
    s.contains("HaFailoverStarted")
        && s.contains("HaFailoverCompleted")
        && s.contains("from_role")
        && s.contains("guest_count")
}

/// True when the M6.6 smoke script is present.
pub fn ha_scripts_present() -> bool {
    let smoke = include_str!("../tools/m6-ha-smoke.sh");
    smoke.contains(M6_HA_OK_MARKER)
        && smoke.contains("m6_6_ha_gate_passes")
        && smoke.contains("prop_ha_failover_restart")
        && smoke.contains("prop_security_harden_checklist")
}

/// True when the HA runbook is present.
pub fn ha_runbook_present() -> bool {
    let rb = include_str!("../docs/runbooks/ha.md");
    rb.contains("RAYNU-V-M6-HA-OK")
        && rb.contains("failover_to_standby")
        && rb.contains("primary")
        && rb.contains("standby")
}

/// Full M6.6 artifact + failover + harden gate.
pub fn run_m6_ha_gate() -> bool {
    ha_surface_present()
        && audit_ha_events_present()
        && ha_scripts_present()
        && ha_runbook_present()
        && prop_ha_failover_restart()
        && prop_security_harden_checklist()
}

#[cfg(test)]
#[path = "m6_ha_gate_test.rs"]
mod m6_ha_gate_test;
