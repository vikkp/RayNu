use super::*;
use crate::mgmt::VmTable;

#[test]
fn sample_inventory_has_ten_plus() {
    assert!(inventory_documents_vmdk_ovf());
    let mut specs = [ImportSpec {
        guest_id: 0,
        source: ImportSource::Vmdk,
        name: [0; 16],
        name_len: 0,
    }; MIGRATE_BATCH_CAP];
    let n = parse_inventory(SAMPLE_INVENTORY, &mut specs).unwrap();
    assert!(n >= MIGRATE_MIN_GUESTS);
    assert!(n >= 12);
}

#[test]
fn migrate_ten_plus_one_command() {
    assert!(prop_migrate_ten_plus());
}

#[test]
fn migrate_emits_audit_events() {
    let before = crate::audit::integrity::boot_ring_len_for_test();
    let mut table = VmTable::new();
    // Use guest ids 20..31 to avoid colliding with other parallel tests' creates.
    let inv = "\
20 vmdk a0
21 ovf a1
22 vmdk a2
23 ovf a3
24 vmdk a4
25 ovf a5
26 vmdk a6
27 ovf a7
28 vmdk a8
29 ovf a9
30 vmdk a10
";
    let r = migrate_one_command(99, inv, &mut table).unwrap();
    assert_eq!(r.imported, 11);
    let after = crate::audit::integrity::boot_ring_len_for_test();
    // MigrateStarted + 11× VmCreated + MigrateCompleted ≥ 13
    assert!(after >= before + 13, "expected migrate + lifecycle audit events");
}

#[test]
fn rejects_too_few() {
    let mut table = VmTable::new();
    let inv = "1 vmdk only\n2 ovf two\n";
    assert_eq!(
        migrate_one_command(2, inv, &mut table),
        Err(MigrateError::TooFewGuests)
    );
}

#[test]
fn gap_note_present() {
    assert!(VCENTER_API_GAP_NOTE.contains("vCenter"));
}
