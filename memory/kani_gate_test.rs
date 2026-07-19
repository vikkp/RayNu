use super::*;

#[test]
fn m3_21_kani_gate_passes() {
    assert!(
        kani_harnesses_present(),
        "M2.6 Kani harnesses must exist with unwind(16) and Kani MAP_CAP=8"
    );
    assert!(
        kani_pin_present(),
        "kani-version.toml must pin kani-verifier 0.67.0"
    );
    assert!(
        kani_ci_hard_fail_present(),
        "CI must hard-fail via tools/kani-smoke.sh --lib"
    );
    assert!(run_kani_gate(), "M3.21 Kani gate failed");
    assert_eq!(M3_KANI_OK_MARKER, "RAYNU-V-M3-KANI-OK");
    println!("{M3_KANI_OK_MARKER}");
}
