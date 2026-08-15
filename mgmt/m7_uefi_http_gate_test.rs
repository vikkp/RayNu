use super::{
    prop_uefi_http_scaffold_package, run_m7_uefi_http_scaffold_gate, uefi_http_scripts_present,
    uefi_http_surface_present, M7_UEFI_HTTP_GATE_MARKER,
};
use crate::mgmt::http_listen::{M7_UEFI_HTTP_OK_MARKER, M7_UEFI_HTTP_SCAFFOLD_MARKER};

#[test]
fn m7_6_uefi_http_scaffold_passes() {
    assert_eq!(M7_UEFI_HTTP_GATE_MARKER, M7_UEFI_HTTP_SCAFFOLD_MARKER);
    assert_eq!(M7_UEFI_HTTP_OK_MARKER, "RAYNU-V-M7-UEFI-HTTP-OK");
    assert!(uefi_http_surface_present(), "listen + Tcp4 + main wiring");
    assert!(uefi_http_scripts_present(), "smoke + runbook must name M7.6");
    assert!(prop_uefi_http_scaffold_package());
    assert!(run_m7_uefi_http_scaffold_gate());
    // Scaffold only — never claim firmware OK from host tests.
    println!("{M7_UEFI_HTTP_SCAFFOLD_MARKER}");
}
