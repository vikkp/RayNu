//! PRE-EBS SNP lease, reused by the native NIC after BOOT-OK (ADR-013 Phase D).
//!
//! Pillar: [Z]
//! Proven Core: **outside**
//!
//! Firmware SNP is never polled after EBS. The IPv4 lease (and MAC) are copied
//! here during the PRE-EBS window so BCM5720 listen can bind the same station
//! address the operator already curled.

use core::sync::atomic::{AtomicBool, Ordering};

/// IPv4 lease captured from SNP DHCP (or host tests).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParkedMgmtLease {
    pub ip: [u8; 4],
    pub prefix: u8,
    pub router: [u8; 4],
    pub has_router: bool,
    pub mac: [u8; 6],
    pub port: u16,
}

// JUSTIFICATION: BSP-only; written once PRE-EBS, read after BOOT-OK.
static LOCK: AtomicBool = AtomicBool::new(false);
static mut LEASE: Option<ParkedMgmtLease> = None;

fn with_lease<R>(f: impl FnOnce(&mut Option<ParkedMgmtLease>) -> R) -> Option<R> {
    if LOCK.swap(true, Ordering::Acquire) {
        return None;
    }
    // SAFETY: exclusive flag; single hart; LEASE is the parked copy.
    // KANI-TARGET: host tests store/load mocked octets, not this lock path.
    let out = unsafe { f(&mut *core::ptr::addr_of_mut!(LEASE)) };
    LOCK.store(false, Ordering::Release);
    Some(out)
}

/// Store the PRE-EBS lease. Overwrites a previous copy.
pub fn store(lease: ParkedMgmtLease) {
    let _ = with_lease(|slot| *slot = Some(lease));
}

/// Parked lease, if SNP DHCP succeeded.
pub fn load() -> Option<ParkedMgmtLease> {
    with_lease(|slot| *slot).flatten()
}

/// Usable parked lease, if any.
pub fn load_usable() -> Option<ParkedMgmtLease> {
    load().filter(lease_is_usable)
}

/// BCM5720 `prefer_mac`. Zero octets means pick by link (no SNP lease).
/// Iron `6ba076cc`: SNP DHCP failed → skip analog/coexist → Phase B idle.
pub fn prefer_mac() -> [u8; 6] {
    load_usable().map(|l| l.mac).unwrap_or([0; 6])
}

/// True when `prefix` is a plausible IPv4 prefix and `ip` is not unspecified.
pub fn lease_is_usable(lease: &ParkedMgmtLease) -> bool {
    lease.prefix > 0
        && lease.prefix <= 32
        && lease.port != 0
        && lease.ip != [0, 0, 0, 0]
        && lease.mac.iter().any(|&b| b != 0)
}

#[cfg(test)]
#[path = "mgmt_lease_test.rs"]
mod mgmt_lease_test;
