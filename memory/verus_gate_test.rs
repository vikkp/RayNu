use super::*;

#[test]
fn m3_15_verus_pin_gate_passes() {
    assert!(
        verus_pin_is_concrete(),
        "verus-version.toml must freeze tag + 40-char commit + 64-char sha256_linux"
    );
    assert!(
        verus_scripts_present(),
        "install/smoke scripts must enforce sha256 and reject latest"
    );
    assert!(run_verus_pin_gate(), "M3.15 Verus pin gate failed");
    let v = pinned_verus_version().expect("pinned version");
    assert!(v.starts_with("0.2026."), "unexpected pin {v}");
    assert_eq!(M3_VERUS_OK_MARKER, "RAYNU-V-M3-VERUS-OK");
    println!("{M3_VERUS_OK_MARKER}");
}

#[test]
fn pin_rejects_floating_channels() {
    let s = include_str!("../verus-version.toml");
    assert!(!s.contains("unpinned-scaffold"));
    assert!(s.contains("sha256_linux = \""));
    assert!(s.contains("commit = \""));
    // Pin field values must not be floating channels (policy prose may mention "latest").
    let tag = super::toml_string("tag").expect("tag");
    let asset = super::toml_string("asset_linux").expect("asset");
    assert!(!tag.contains("latest") && !tag.contains("rolling"), "tag={tag}");
    assert!(!asset.contains("latest"), "asset={asset}");
}
