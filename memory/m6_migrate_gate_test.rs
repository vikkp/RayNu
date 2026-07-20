use super::{
    ept_model_has_migrate_xfer, ept_spec_closes_migrate_xfer, migrate_xfer_scripts_present,
    run_m6_migrate_gate, M6_MIGRATE_XFER_OK_MARKER,
};
use crate::memory::ept::prop_page_transfer_preserves_exclusive;

#[test]
fn m6_3_migrate_xfer_gate_passes() {
    assert_eq!(M6_MIGRATE_XFER_OK_MARKER, "RAYNU-V-M6-MIGRATE-XFER-OK");
    assert!(
        ept_model_has_migrate_xfer(),
        "ept_model must embed M6.3 migrate-xfer artifacts"
    );
    assert!(
        ept_spec_closes_migrate_xfer(),
        "ept_spec/ept_proof must close live migration page transfer GAP"
    );
    assert!(
        migrate_xfer_scripts_present(),
        "verus-migrate-xfer-smoke.sh must be present"
    );
    assert!(
        prop_page_transfer_preserves_exclusive(),
        "live transfer_page must preserve exclusivity"
    );
    assert!(run_m6_migrate_gate());
    println!("RAYNU-V-M6-MIGRATE-XFER-OK");
}
