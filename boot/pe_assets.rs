//! PE-linked guest assets (M3.22 / ADR-003).
//!
//! Pillar: [Z]
//! Proven Core: **outside** (ADR-002)
//!
//! Embeds the bring-up bzImage + initrd as PE/COFF sections on the single
//! `.efi`. Short 8-character section names (COFF limit) alias ADR-003's
//! long names:
//!
//! | Section   | ADR-003 name        |
//! |-----------|---------------------|
//! | `.askern` | `.assets.kernel`    |
//! | `.asinit` | `.assets.initrd`    |
//!
//! Boot prefers these blobs; ESP `\EFI\BOOT\*` remains the split-mode fallback.
//! zstd / webui / schemas stay deferred while the binary is under the 15 MB
//! target (`tools/check-size.sh`).

/// COM1 / host marker when PE-embedded assets are present and preferred.
pub const M3_ASSETS_OK_MARKER: &str = "RAYNU-V-M3-ASSETS-OK";

/// PE section name for the embedded bzImage (ADR-003 `.assets.kernel`).
pub const SECTION_KERNEL: &str = ".askern";
/// PE section name for the embedded initrd (ADR-003 `.assets.initrd`).
pub const SECTION_INITRD: &str = ".asinit";

// `include_bytes!(…).len()` is const — keeps the array sized to the asset.
#[link_section = ".askern"]
#[used]
static PE_BZIMAGE: [u8; include_bytes!("../assets/bzImage").len()] =
    *include_bytes!("../assets/bzImage");

#[link_section = ".asinit"]
#[used]
static PE_INITRD: [u8; include_bytes!("../assets/initrd").len()] =
    *include_bytes!("../assets/initrd");

/// Embedded bzImage bytes (always present when `assets/bzImage` is in-tree).
pub fn bzimage_bytes() -> Option<&'static [u8]> {
    if PE_BZIMAGE.is_empty() {
        None
    } else {
        Some(&PE_BZIMAGE[..])
    }
}

/// Embedded initrd bytes (always present when `assets/initrd` is in-tree).
pub fn initrd_bytes() -> Option<&'static [u8]> {
    if PE_INITRD.is_empty() {
        None
    } else {
        Some(&PE_INITRD[..])
    }
}

/// True when both kernel and initrd PE sections carry non-empty payloads.
pub fn embedded_present() -> bool {
    bzimage_bytes().is_some() && initrd_bytes().is_some()
}

/// Byte length of the embedded kernel (host gate / size notes).
pub fn bzimage_len() -> usize {
    PE_BZIMAGE.len()
}

/// Byte length of the embedded initrd.
pub fn initrd_len() -> usize {
    PE_INITRD.len()
}

#[cfg(test)]
#[path = "pe_assets_test.rs"]
mod pe_assets_test;
