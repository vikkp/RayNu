use super::{
    ext_docs_present, ext_scripts_present, ext_surface_present, run_m6_ext_gate, M6_EXT_GATE_MARKER,
};
use crate::mgmt::ext::prop_external_audit_package;

#[test]
fn m6_9_ext_gate_passes() {
    assert_eq!(M6_EXT_GATE_MARKER, "RAYNU-V-M6-EXT-OK");
    assert!(ext_surface_present(), "mgmt/ext must embed M6.9 package");
    assert!(ext_docs_present(), "review/findings/runbook must be present");
    assert!(ext_scripts_present(), "m6-ext-smoke.sh must be present");
    assert!(
        prop_external_audit_package(),
        "external audit package prop must hold"
    );
    assert!(run_m6_ext_gate());
    println!("RAYNU-V-M6-EXT-OK");
}
