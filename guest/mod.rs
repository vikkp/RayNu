//! Guest bring-up helpers outside the Proven Core (ADR-002).
//!
//! Pillar: [Z]
//! Linux boot protocol packing lives here — not in `memory/` / `vmx/`.

pub mod linux_boot;

pub use linux_boot::{
    claim_load_pages, load_synthetic_guest, pack_boot_params, BootLoadInfo, M3_LOAD_OK_MARKER,
    SETUP_HEADER_MAGIC,
};
