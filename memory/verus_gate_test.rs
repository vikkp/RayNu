use super::*;

#[test]
fn m3_15_verus_pin_gate_passes() {
    assert!(
        verus_pin_is_concrete(),
        "verus-version.toml must pin a concrete weekly Verus release"
    );
    assert!(
        verus_scripts_present(),
        "tools/install-verus.sh and tools/verus-smoke.sh must be present"
    );
    assert!(run_verus_pin_gate(), "M3.15 Verus pin gate failed");
    let v = pinned_verus_version().expect("pinned version");
    assert!(v.starts_with("0.2026."), "unexpected pin {v}");
    assert_eq!(M3_VERUS_OK_MARKER, "RAYNU-V-M3-VERUS-OK");
    println!("{M3_VERUS_OK_MARKER}");
}
