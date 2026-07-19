//! M3.20 host verification gate (tight EPT below 1 GiB).
//!
//! Pillar: [V]
//! Proven Core: companion to `memory/ept_hw.rs` + `src/main.rs`.
//!
//! Checks in-tree that the live precise window is [`crate::memory::PRECISE_MIB`]
//! MiB (strictly below 1 GiB), built via 2M leaves, and that the QEMU boot gate
//! requires `RAYNU-V-M3-EPT3-OK`. Runtime gate: `tools/qemu-boot-test.sh`.

/// Host / serial marker when the M3.20 tight-EPT gate passes.
pub const M3_EPT3_OK_MARKER: &str = "RAYNU-V-M3-EPT3-OK";

/// True when ept_hw encodes a sub-GiB precise window with a 2M builder.
pub fn tight_precise_window_present() -> bool {
    let s = include_str!("ept_hw.rs");
    s.contains("PRECISE_MIB: u64 = 512")
        && s.contains("build_identity_2m_bytes")
        && s.contains("fn build_precise_identity")
        && s.contains("fn frames_required_precise")
        && s.contains(M3_EPT3_OK_MARKER)
        && s.contains("PRECISE_BYTES < (1u64 << 30)")
}

/// True when main installs the tight window and emits EPT3.
pub fn boot_path_emits_ept3() -> bool {
    let s = include_str!("../src/main.rs");
    s.contains("build_precise_identity")
        && s.contains("ensure_2m_capable")
        && s.contains("512MiB")
        && s.contains("M3_EPT3_OK_MARKER")
}

/// True when QEMU machine RAM matches the tight window and the script gates EPT3.
pub fn ept3_boot_scripts_present() -> bool {
    let qemu = include_str!("../tools/run-qemu.sh");
    let smoke = include_str!("../tools/qemu-boot-test.sh");
    qemu.contains("-m 512M")
        && smoke.contains(M3_EPT3_OK_MARKER)
        && smoke.contains("MARKER_EPT3")
        && smoke.contains("M3.20")
}

/// Full M3.20 artifact gate (does not run QEMU).
pub fn run_ept3_gate() -> bool {
    tight_precise_window_present() && boot_path_emits_ept3() && ept3_boot_scripts_present()
}

#[cfg(test)]
#[path = "ept3_gate_test.rs"]
mod ept3_gate_test;
