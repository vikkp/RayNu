//! Guest bring-up helpers outside the Proven Core (ADR-002).
//!
//! Pillar: [Z]
//! Linux boot protocol packing lives here — not in `memory/` / `vmx/`.

pub mod linux_boot;

pub use linux_boot::{
    build_minimal_bzimage, claim_load_pages, load_bzimage_guest, load_synthetic_guest,
    pack_boot_params, parse_bzimage, BootLoadInfo, BzImageInfo, M3_BZIMAGE_OK_MARKER,
    M3_LOAD_OK_MARKER, MINIMAL_BZIMAGE_CAP, SETUP_HEADER_MAGIC,
};
