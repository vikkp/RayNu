//! M3.21 host verification gate (Kani CI hard-fail).
//!
//! Pillar: [V]
//! Proven Core: companion to M2.6 harnesses in `ept_test` / `frame_allocator_test`.
//!
//! Checks in-tree that Kani is pinned, harnesses exist, CI is hard-fail, and
//! the smoke script emits `RAYNU-V-M3-KANI-OK`. Runtime: `./tools/kani-smoke.sh`.

/// Host / CI marker when the M3.21 Kani gate passes.
pub const M3_KANI_OK_MARKER: &str = "RAYNU-V-M3-KANI-OK";

/// True when both M2.6 Kani harnesses are present with unwind bounds.
pub fn kani_harnesses_present() -> bool {
    let ept = include_str!("ept_test.rs");
    let alloc = include_str!("frame_allocator_test.rs");
    let ept_hw = include_str!("ept.rs");
    ept.contains("fn kani_no_double_map_same_hpa")
        && ept.contains("#[kani::unwind(16)]")
        && alloc.contains("fn kani_alloc_no_alias_double_free_rejected")
        && alloc.contains("#[kani::unwind(16)]")
        && ept_hw.contains("#[cfg(kani)]")
        && ept_hw.contains("const MAP_CAP: usize = 8")
}

/// True when kani-version.toml pins an exact verifier version.
pub fn kani_pin_present() -> bool {
    let s = include_str!("../kani-version.toml");
    s.contains("version = \"0.67.0\"")
        && s.contains(M3_KANI_OK_MARKER)
        && s.contains("kani_no_double_map_same_hpa")
}

/// True when CI + smoke script hard-fail on the two harnesses (lib-only).
pub fn kani_ci_hard_fail_present() -> bool {
    let ci = include_str!("../.github/workflows/ci.yml");
    let smoke = include_str!("../tools/kani-smoke.sh");
    !ci.contains("kani-soft")
        && !ci.contains("soft-fail M2.6")
        && ci.contains("kani-smoke.sh")
        && ci.contains("M3.21")
        && smoke.contains("--lib --tests")
        && smoke.contains(M3_KANI_OK_MARKER)
        && smoke.contains("kani_no_double_map_same_hpa")
        && smoke.contains("kani_alloc_no_alias_double_free_rejected")
}

/// Full M3.21 artifact gate (does not run Kani).
pub fn run_kani_gate() -> bool {
    kani_harnesses_present() && kani_pin_present() && kani_ci_hard_fail_present()
}

#[cfg(test)]
#[path = "kani_gate_test.rs"]
mod kani_gate_test;
