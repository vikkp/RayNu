use super::{
    prop_r640_scaffold_package, r640_honesty_holds, r640_scripts_present, run_m7_r640_scaffold_gate,
    ship_kit_names_r640_efi, M7_R640_OK_MARKER, M7_R640_SCAFFOLD_MARKER, R640_GAP_NOTE,
    R640_HOST_LIMIT_NOTE,
};

#[test]
fn m7_5_r640_scaffold_passes() {
    assert_eq!(M7_R640_OK_MARKER, "RAYNU-V-R640-BOOT-OK");
    assert_eq!(M7_R640_SCAFFOLD_MARKER, "RAYNU-V-M7-R640-SCAFFOLD-OK");
    assert!(ship_kit_names_r640_efi(), "ship kit must name r640-hypervisor.efi");
    assert!(r640_scripts_present(), "runbook + evidence + smoke must be present");
    assert!(r640_honesty_holds(), "GAP must stay open; host limit note required");
    assert!(prop_r640_scaffold_package());
    assert!(run_m7_r640_scaffold_gate());
    assert!(!R640_GAP_NOTE.contains("CLOSED"));
    assert!(R640_HOST_LIMIT_NOTE.contains("cannot close"));
    // Scaffold only — never claim iron boot from host tests.
    println!("{M7_R640_SCAFFOLD_MARKER}");
}
