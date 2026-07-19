use super::*;

#[test]
fn m3_20_ept3_gate_passes() {
    assert!(
        tight_precise_window_present(),
        "ept_hw must encode 512 MiB precise window + 2M builder + EPT3 marker"
    );
    assert!(
        boot_path_emits_ept3(),
        "main.rs must build tight precise EPT and emit RAYNU-V-M3-EPT3-OK"
    );
    assert!(
        ept3_boot_scripts_present(),
        "run-qemu.sh must use -m 512M; qemu-boot-test.sh must require EPT3"
    );
    assert!(run_ept3_gate(), "M3.20 EPT3 gate failed");
    assert_eq!(M3_EPT3_OK_MARKER, "RAYNU-V-M3-EPT3-OK");
    println!("{M3_EPT3_OK_MARKER}");
}
