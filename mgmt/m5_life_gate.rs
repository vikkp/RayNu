//! M5.0 host verification gate (VM lifecycle API).
//!
//! Pillar: [Z] [A]
//! Proven Core: outside (companion to `mgmt/` — not a boot path).
//!
//! Checks that `mgmt` carries create/start/stop/destroy, that audit events for
//! those ops exist, and that the lifecycle round-trip property holds.

use super::{prop_lifecycle_roundtrip, M5_LIFE_OK_MARKER};

/// True when lifecycle transition APIs are present in `mgmt/mod.rs`.
pub fn mgmt_lifecycle_api_present() -> bool {
    let s = include_str!("mod.rs");
    s.contains("enum VmLifecycle")
        && s.contains("Destroyed")
        && s.contains("struct VmTable")
        && s.contains("fn create(")
        && s.contains("fn start(")
        && s.contains("fn stop(")
        && s.contains("fn destroy(")
        && s.contains(M5_LIFE_OK_MARKER)
}

/// True when audit events for lifecycle ops are present.
pub fn audit_lifecycle_events_present() -> bool {
    let s = include_str!("../audit/integrity.rs");
    s.contains("VmCreated")
        && s.contains("VmStarted")
        && s.contains("VmStopped")
        && s.contains("VmDestroyed")
}

/// True when the M5.0 smoke script is present.
pub fn life_scripts_present() -> bool {
    let smoke = include_str!("../tools/m5-life-smoke.sh");
    smoke.contains(M5_LIFE_OK_MARKER)
        && smoke.contains("m5_0_life_gate_passes")
        && smoke.contains("create_start_stop_destroy_roundtrip")
}

/// Full M5.0 artifact + round-trip gate.
pub fn run_m5_life_gate() -> bool {
    mgmt_lifecycle_api_present()
        && audit_lifecycle_events_present()
        && life_scripts_present()
        && prop_lifecycle_roundtrip()
}

#[cfg(test)]
#[path = "m5_life_gate_test.rs"]
mod m5_life_gate_test;
