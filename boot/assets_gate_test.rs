use super::*;

#[test]
fn m3_22_assets_gate_passes() {
    assert!(
        pe_sections_embedded(),
        "pe_assets.rs must PE-link bzImage+initrd as .askern/.asinit"
    );
    assert!(
        boot_prefers_pe_embed(),
        "main.rs must prefer PE embed and emit RAYNU-V-M3-ASSETS-OK"
    );
    assert!(
        assets_scripts_present(),
        "build/check-size/check-pe-assets/qemu-boot-test must wire M3.22"
    );
    assert!(run_assets_gate(), "M3.22 ASSETS gate failed");
    assert_eq!(M3_ASSETS_OK_MARKER, "RAYNU-V-M3-ASSETS-OK");
    println!("{M3_ASSETS_OK_MARKER}");
}
