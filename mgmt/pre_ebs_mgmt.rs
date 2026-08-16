//! Durable PRE-EBS management-plane tables (outside Proven Core).
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** (ADR-009 / ADR-012)
//!
//! Tcp4 and SNP listen previously allocated fresh `VmTable` / `ImageTable` per
//! HTTP exchange, so create/list could not survive across curls in the PRE-EBS
//! window. This module holds one shared session for the listen lifetime.

use super::datastore::ImageTable;
use super::iso::IsoDeployPlan;
use super::iso_install::InstallToDiskPlan;
use super::VmTable;

/// Shared mgmt state across PRE-EBS HTTP exchanges.
pub struct PreEbsMgmt {
    pub vms: VmTable,
    pub images: ImageTable,
    pub iso_plan: IsoDeployPlan,
    pub iso_install: InstallToDiskPlan,
}

impl PreEbsMgmt {
    pub const fn new() -> Self {
        Self {
            vms: VmTable::new(),
            images: ImageTable::new(),
            iso_plan: IsoDeployPlan::empty(),
            iso_install: InstallToDiskPlan::empty(),
        }
    }
}

static mut PRE_EBS: PreEbsMgmt = PreEbsMgmt::new();
static mut PRE_EBS_ARMED: bool = false;

/// Reset shared tables (start of listen window / tests).
pub fn reset_pre_ebs_mgmt() {
    // SAFETY: single-threaded boot / host tests with --test-threads=1 for this path.
    unsafe {
        PRE_EBS = PreEbsMgmt::new();
        PRE_EBS_ARMED = true;
    }
}

/// True after [`reset_pre_ebs_mgmt`] until clear.
pub fn pre_ebs_mgmt_armed() -> bool {
    unsafe { PRE_EBS_ARMED }
}

/// Clear armed flag (does not wipe tables — call reset to wipe).
pub fn clear_pre_ebs_mgmt_flag() {
    unsafe {
        PRE_EBS_ARMED = false;
    }
}

/// Borrow shared PRE-EBS mgmt for one HTTP exchange.
///
/// SAFETY: caller must be single-threaded (UEFI boot / host unit test).
pub unsafe fn with_pre_ebs_mgmt<R>(f: impl FnOnce(&mut PreEbsMgmt) -> R) -> R {
    if !PRE_EBS_ARMED {
        PRE_EBS = PreEbsMgmt::new();
        PRE_EBS_ARMED = true;
    }
    f(&mut *core::ptr::addr_of_mut!(PRE_EBS))
}

/// Host package: shared state survives two create/get exchanges.
pub fn prop_pre_ebs_mgmt_durable() -> bool {
    reset_pre_ebs_mgmt();
    unsafe {
        with_pre_ebs_mgmt(|m| {
            if m.vms.create(3).is_err() {
                return false;
            }
            true
        });
        let ok = with_pre_ebs_mgmt(|m| m.vms.get(3).is_some());
        clear_pre_ebs_mgmt_flag();
        reset_pre_ebs_mgmt();
        clear_pre_ebs_mgmt_flag();
        ok
    }
}

#[cfg(test)]
mod pre_ebs_mgmt_test {
    use super::*;

    #[test]
    fn pre_ebs_mgmt_survives_two_borrows() {
        assert!(prop_pre_ebs_mgmt_durable());
    }
}
