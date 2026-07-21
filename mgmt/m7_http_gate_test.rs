use super::{
    http_listen_present, http_scripts_present, http_surface_present, run_m7_http_gate,
    M7_HTTP_GATE_MARKER,
};
use crate::mgmt::http::prop_http_mgmt_package;

#[test]
fn m7_1_http_gate_passes() {
    assert_eq!(M7_HTTP_GATE_MARKER, "RAYNU-V-M7-HTTP-OK");
    assert!(http_surface_present(), "mgmt/http must embed M7.1 package");
    assert!(http_listen_present(), "listen stub + host TCP must be present");
    assert!(http_scripts_present(), "smoke + runbook must be present");
    assert!(prop_http_mgmt_package(), "http mgmt package prop must hold");
    assert!(run_m7_http_gate());
    println!("RAYNU-V-M7-HTTP-OK");
}
