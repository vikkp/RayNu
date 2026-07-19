//! RayNu-V hypervisor library root.
//!
//! Pillars: [V] verified core · [Z] single binary · [D] iDRAC · [A] audit.
//! Governance: see `CLAUDE.md` and `docs/adr/`.

#![cfg_attr(not(test), no_std)]
#![allow(dead_code)]

// Kani injects its lib under cfg(kani); no_std crates must import it explicitly.
#[cfg(kani)]
extern crate kani;

#[path = "../boot/mod.rs"]
pub mod boot;

#[path = "../vmx/mod.rs"]
pub mod vmx;

#[path = "../memory/mod.rs"]
pub mod memory;

#[path = "../devices/mod.rs"]
pub mod devices;

#[path = "../sched/mod.rs"]
pub mod sched;

#[path = "../net/mod.rs"]
pub mod net;

#[path = "../audit/mod.rs"]
pub mod audit;

#[path = "../mgmt/mod.rs"]
pub mod mgmt;

#[path = "../migrate/mod.rs"]
pub mod migrate;

#[path = "../idrac/mod.rs"]
pub mod idrac;

#[path = "../arch/mod.rs"]
pub mod arch;

#[path = "../guest/mod.rs"]
pub mod guest;

/// Product identity banner printed on serial at boot.
pub const BOOT_BANNER: &str =
    "RayNu-V r640-hypervisor — formally verified bare-metal hypervisor";
