//! M5.6 host verification gate (Dell Tier‑1 iDRAC health + topology).
//!
//! Pillar: [D]
//! Proven Core: outside (ADR-005 companion to `idrac/`).
//!
//! Checks mock Redfish thermal/fan/PSU, SMBIOS/ACPI topology surface, Tier‑2
//! GAP documentation, and smoke script presence.

use super::{
    prop_tier1_health_and_topology, LIVE_REDFISH_GAP_NOTE, M5_IDRAC_OK_MARKER, MOCK_REDFISH,
    MOCK_TOPOLOGY, TIER2_GAP_NOTE,
};

/// True when idrac module exposes Tier‑1 health + topology + marker.
pub fn idrac_surface_present() -> bool {
    let s = include_str!("mod.rs");
    s.contains("fn read_tier1_health(")
        && s.contains("fn parse_topology(")
        && s.contains("fn read_topology_mock(")
        && s.contains("struct Tier1Health")
        && s.contains("struct TopologySnapshot")
        && s.contains("IdracTier::Tier1")
        && s.contains(M5_IDRAC_OK_MARKER)
        && s.contains(LIVE_REDFISH_GAP_NOTE)
        && s.contains(TIER2_GAP_NOTE)
}

/// True when mock Redfish / topology assets document Tier‑1 fields.
pub fn idrac_assets_present() -> bool {
    MOCK_REDFISH.contains("Temperatures")
        && MOCK_REDFISH.contains("Fans")
        && MOCK_REDFISH.contains("PowerSupplies")
        && MOCK_REDFISH.contains("\"Health\": \"OK\"")
        && MOCK_TOPOLOGY.contains("dimm ")
        && MOCK_TOPOLOGY.contains("cpu ")
        && MOCK_TOPOLOGY.contains("numa ")
        && MOCK_TOPOLOGY.contains("slit ")
}

/// True when the M5.6 smoke script is present.
pub fn idrac_scripts_present() -> bool {
    let smoke = include_str!("../tools/m5-idrac-smoke.sh");
    smoke.contains(M5_IDRAC_OK_MARKER)
        && smoke.contains("m5_6_idrac_gate_passes")
        && smoke.contains("tier1_health_and_topology")
        && smoke.contains("mock_redfish.json")
        && smoke.contains("mock_topology.txt")
}

/// Full M5.6 artifact + Tier‑1 health/topology gate.
pub fn run_m5_idrac_gate() -> bool {
    idrac_surface_present()
        && idrac_assets_present()
        && idrac_scripts_present()
        && prop_tier1_health_and_topology()
}

#[cfg(test)]
#[path = "m5_idrac_gate_test.rs"]
mod m5_idrac_gate_test;
