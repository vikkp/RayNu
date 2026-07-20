//! M5.5 host verification gate (VMware / vCenter import).
//!
//! Pillar: [A] [Z]
//! Proven Core: outside (ADR-007 companion to `migrate/`).
//!
//! Checks one-command ≥10-guest import, VMDK/OVF inventory, migrate audit
//! events, and smoke script presence.

use super::{
    inventory_documents_vmdk_ovf, prop_migrate_ten_plus, M5_MIGRATE_OK_MARKER, MIGRATE_MIN_GUESTS,
    SAMPLE_INVENTORY, VCENTER_API_GAP_NOTE,
};

/// True when migrate module exposes one-command import + marker.
pub fn migrate_surface_present() -> bool {
    let s = include_str!("mod.rs");
    s.contains("fn migrate_one_command(")
        && s.contains("fn parse_inventory(")
        && s.contains("ImportSource")
        && s.contains("Vmdk")
        && s.contains("Ovf")
        && s.contains("MIGRATE_MIN_GUESTS")
        && s.contains(M5_MIGRATE_OK_MARKER)
        && s.contains(VCENTER_API_GAP_NOTE)
}

/// True when audit events for migrate start/complete/fail exist.
pub fn migrate_audit_events_present() -> bool {
    let s = include_str!("../audit/integrity.rs");
    s.contains("MigrateStarted")
        && s.contains("MigrateCompleted")
        && s.contains("MigrateFailed")
}

/// True when sample inventory and smoke script are present.
pub fn migrate_assets_present() -> bool {
    let smoke = include_str!("../tools/m5-migrate-smoke.sh");
    inventory_documents_vmdk_ovf()
        && SAMPLE_INVENTORY.lines().filter(|l| {
            let t = l.trim();
            !t.is_empty() && !t.starts_with('#')
        }).count()
            >= MIGRATE_MIN_GUESTS
        && smoke.contains(M5_MIGRATE_OK_MARKER)
        && smoke.contains("m5_5_migrate_gate_passes")
        && smoke.contains("migrate_ten_plus_one_command")
}

/// Full M5.5 artifact + 10+ import gate.
pub fn run_m5_migrate_gate() -> bool {
    migrate_surface_present()
        && migrate_audit_events_present()
        && migrate_assets_present()
        && prop_migrate_ten_plus()
}

#[cfg(test)]
#[path = "m5_migrate_gate_test.rs"]
mod m5_migrate_gate_test;
