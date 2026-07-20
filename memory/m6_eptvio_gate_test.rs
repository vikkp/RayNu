use super::{
    ept_model_has_eptvio, ept_spec_closes_eptvio, eptvio_scripts_present, run_m6_eptvio_gate,
    M6_EPTVIO_OK_MARKER,
};
use crate::memory::ept::prop_violation_preserves_exclusive;

#[test]
fn m6_0_eptvio_gate_passes() {
    assert!(
        ept_model_has_eptvio(),
        "ept_model must carry EptViolationDisposition / theorem / marker"
    );
    assert!(
        ept_spec_closes_eptvio(),
        "ept_spec/ept_proof must close EPT-violation GAP for M6.0"
    );
    assert!(eptvio_scripts_present(), "verus-eptvio-smoke.sh must be present");
    assert!(
        prop_violation_preserves_exclusive(),
        "live violation dispositions must preserve exclusivity"
    );
    assert!(run_m6_eptvio_gate());
    assert_eq!(M6_EPTVIO_OK_MARKER, "RAYNU-V-M6-EPTVIO-OK");
    println!("{M6_EPTVIO_OK_MARKER}");
}
