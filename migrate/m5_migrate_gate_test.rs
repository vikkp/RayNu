use super::*;

#[test]
fn m5_5_migrate_gate_passes() {
    assert!(
        migrate_surface_present(),
        "migrate must expose one-command import + marker"
    );
    assert!(
        migrate_audit_events_present(),
        "audit must define MigrateStarted/Completed/Failed"
    );
    assert!(
        migrate_assets_present(),
        "sample inventory or m5-migrate-smoke.sh incomplete"
    );
    assert!(
        prop_migrate_ten_plus(),
        "10+ guest one-command migrate failed"
    );
    assert!(run_m5_migrate_gate(), "M5.5 migrate gate failed");
    assert_eq!(M5_MIGRATE_OK_MARKER, "RAYNU-V-M5-MIGRATE-OK");
    println!("{M5_MIGRATE_OK_MARKER}");
}
