use super::{
    prop_create_vm_spec, prop_ops_ui_package, run_m7_ui_gate, ui_scripts_present, ui_surface_present,
    M7_UI_OK_MARKER, UI_GAP_NOTE, UI_RESIDUAL_NOTE,
};

#[test]
fn m7_4_ui_gate_passes() {
    assert_eq!(M7_UI_OK_MARKER, "RAYNU-V-M7-UI-OK");
    assert!(ui_surface_present(), "SPA must wire create-VM + media");
    assert!(ui_scripts_present(), "smoke + runbook must be present");
    assert!(prop_create_vm_spec(), "create-spec REST must hold");
    assert!(prop_ops_ui_package());
    assert!(run_m7_ui_gate());
    assert!(UI_GAP_NOTE.contains("CLOSED M7.4"));
    assert!(UI_RESIDUAL_NOTE.contains("console"));
    println!("RAYNU-V-M7-UI-OK");
}
