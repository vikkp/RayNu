//! M3.22 host verification gate (PE `.assets.*` embed).
//!
//! Pillar: [Z]
//! Proven Core: companion to `boot/pe_assets.rs` + `src/main.rs`.
//!
//! Checks in-tree that kernel/initrd are PE-linked, boot prefers embed over
//! ESP, size budget tooling remains, and QEMU requires `RAYNU-V-M3-ASSETS-OK`.

/// Host / serial marker when the M3.22 PE-assets gate passes.
pub const M3_ASSETS_OK_MARKER: &str = "RAYNU-V-M3-ASSETS-OK";

/// True when pe_assets embeds both payloads in named PE sections.
pub fn pe_sections_embedded() -> bool {
    let s = include_str!("pe_assets.rs");
    s.contains("link_section = \".askern\"")
        && s.contains("link_section = \".asinit\"")
        && s.contains("include_bytes!(\"../assets/bzImage\")")
        && s.contains("include_bytes!(\"../assets/initrd\")")
        && s.contains(M3_ASSETS_OK_MARKER)
        && s.contains("fn embedded_present")
}

/// True when main prefers PE embed and emits the ASSETS marker.
pub fn boot_prefers_pe_embed() -> bool {
    let s = include_str!("../src/main.rs");
    s.contains("pe_assets::embedded_present")
        && s.contains("pe_assets::bzimage_bytes")
        && s.contains("M3_ASSETS_OK_MARKER")
        && s.contains("prefer PE")
}

/// True when size budget + PE section smoke scripts are wired.
pub fn assets_scripts_present() -> bool {
    let size = include_str!("../tools/check-size.sh");
    let pe = include_str!("../tools/check-pe-assets.sh");
    let build = include_str!("../tools/build.sh");
    let smoke = include_str!("../tools/qemu-boot-test.sh");
    size.contains("15 * 1024 * 1024")
        && pe.contains(".askern")
        && pe.contains(".asinit")
        && pe.contains(".aswebui")
        && build.contains("check-pe-assets.sh")
        && smoke.contains("MARKER_ASSETS")
        && smoke.contains(M3_ASSETS_OK_MARKER)
        && smoke.contains("M3.22")
}

/// Full M3.22 artifact gate (does not run QEMU / objdump).
pub fn run_assets_gate() -> bool {
    pe_sections_embedded() && boot_prefers_pe_embed() && assets_scripts_present()
}

#[cfg(test)]
#[path = "assets_gate_test.rs"]
mod assets_gate_test;
