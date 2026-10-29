//! RayNu-V hypervisor library root.
//!
//! Pillars: [V] verified core · [Z] single binary · [D] iDRAC · [A] audit.
//! Governance: see `CLAUDE.md` and `docs/adr/`.

#![cfg_attr(not(test), no_std)]
#![allow(dead_code)]

#[cfg(feature = "uefi-bin")]
extern crate alloc;

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

#[path = "../raynu_f/mod.rs"]
pub mod raynu_f;

/// Product identity banner printed on serial at boot.
pub const BOOT_BANNER: &str = "RayNu-V r640-hypervisor — formally verified bare-metal hypervisor";

/// Git SHA baked in by `tools/build.sh` (`RAYNU_BUILD_SHA`, from
/// `GITHUB_SHA` in CI). Printed under the M0 banner so a COM2 log names the
/// artifact that ran — iron `8b6ed1a` could not be told apart from the
/// `f229d14` probe by behaviour alone. `local` when built without it.
/// Diagnostic only; no behaviour keys off it. build sha banner.
pub const BUILD_SHA: &str = match option_env!("RAYNU_BUILD_SHA") {
    Some(s) => s,
    None => "local",
};

/// CI run id baked in by `tools/build.sh` (`RAYNU_BUILD_RUN`, from
/// `GITHUB_RUN_ID`). Empty on a local build. Diagnostic only. build run banner.
pub const BUILD_RUN: &str = match option_env!("RAYNU_BUILD_RUN") {
    Some(s) => s,
    None => "",
};

/// A CI run id is digits only. Empty and anything else stay off the link
/// and off the `run=` token, so a local build prints `build: sha=` alone.
pub fn build_run_is_linkable(run: &str) -> bool {
    !run.is_empty() && run.bytes().all(|b| b.is_ascii_digit()) && run.len() <= 20
}

/// `build: sha=<sha>` and, when [`build_run_is_linkable`], ` run=<id>`.
/// `build: sha=` is always the first token. Returns bytes written.
pub fn format_build_banner_from(buf: &mut [u8], sha: &str, run: &str) -> usize {
    let mut n = push_bytes(buf, 0, b"build: sha=");
    n = push_bytes(buf, n, sha.as_bytes());
    if build_run_is_linkable(run) {
        n = push_bytes(buf, n, b" run=");
        n = push_bytes(buf, n, run.as_bytes());
    }
    n
}

/// The banner for this binary.
pub fn format_build_banner(buf: &mut [u8]) -> usize {
    format_build_banner_from(buf, BUILD_SHA, BUILD_RUN)
}

fn push_bytes(buf: &mut [u8], at: usize, src: &[u8]) -> usize {
    let mut n = at;
    for &b in src {
        if n >= buf.len() {
            break;
        }
        buf[n] = b;
        n += 1;
    }
    n
}

#[cfg(test)]
mod build_banner_tests {
    use super::{build_run_is_linkable, format_build_banner_from};

    fn line(sha: &str, run: &str) -> String {
        let mut buf = [0u8; 80];
        let n = format_build_banner_from(&mut buf, sha, run);
        String::from_utf8(buf[..n].to_vec()).unwrap()
    }

    #[test]
    fn banner_keeps_sha_first_and_omits_an_empty_run() {
        let s = line("local", "");
        assert_eq!(s, "build: sha=local");
        assert!(!s.contains("run="));
        assert!(!build_run_is_linkable(""));
        assert!(!build_run_is_linkable("local"));
    }

    #[test]
    fn banner_appends_a_digit_run() {
        let s = line("aeb2d79bb179", "40687568692");
        assert_eq!(s, "build: sha=aeb2d79bb179 run=40687568692");
        assert!(s.starts_with("build: sha="));
        assert!(build_run_is_linkable("40687568692"));
    }
}
