//! E5 / M7.7 ISO install-to-disk path (outside Proven Core).
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** (ADR-002 / ADR-009)
//! VERIFICATION: N/A
//!
//! Builds on M7.3 (`IsoDeployPlan`): extract-boot bind + virtio-blk install
//! size → a phased **install-to-disk** contract (write disk → reboot → boot
//! from disk). Host/CI closes only the **scaffold**; iron prints
//! [`M7_ISO_INSTALL_OK_MARKER`] after real R640 proof.

use super::api::{
    auth_allows, ApiReply, RestMethod, RestRequest, RestResponse, BRINGUP_AUTH_TOKEN,
};
use super::datastore::{ImageKind, ImageTable};
use super::iso::{
    bind_extract_boot, configure_install_disk, extract_boot_surface_present,
    install_disk_surface_present, register_iso, IsoDeployPlan, IsoError, DEFAULT_INSTALL_DISK_BYTES,
};

/// Iron marker — firmware/COM2 after install-to-disk + reboot-to-disk on R640.
/// Host/CI smoke must **never** print this.
pub const M7_ISO_INSTALL_OK_MARKER: &str = "RAYNU-V-M7-ISO-INSTALL-OK";

/// Host / CI scaffold marker when runbook + package gate pass.
pub const M7_ISO_INSTALL_SCAFFOLD_MARKER: &str = "RAYNU-V-M7-ISO-INSTALL-SCAFFOLD-OK";

/// Open until QEMU then iron close install-to-disk.
pub const ISO_INSTALL_GAP_NOTE: &str = "GAP(OPEN M7.7): ISO install-to-disk + reboot-to-disk";

/// Documented MVP: extract-boot + empty virtio-blk → guest writes → reboot from disk.
pub const ISO_INSTALL_MVP_NOTE: &str =
    "MVP: extract-boot + virtio-blk install disk → guest write → reboot-to-disk (El Torito deferred)";

/// Honesty: scaffold cannot invent iron install evidence.
pub const ISO_INSTALL_HOST_LIMIT_NOTE: &str =
    "Latitude/QEMU host smoke cannot close RAYNU-V-M7-ISO-INSTALL-OK; real install proof required";

/// Phases toward E5 close (management-plane bookkeeping).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallPhase {
    /// No install plan yet.
    Idle,
    /// Deploy plan ready; launch contract issued (extract-boot + disk size).
    ContractReady,
    /// Guest (or host selftest) wrote install marker to virtio-blk.
    DiskWritten,
    /// Operator/host requested reboot-from-disk.
    RebootPending,
    /// Second boot observed from install disk (iron/QEMU close).
    BootedFromDisk,
}

/// Install-to-disk plan layered on [`IsoDeployPlan`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InstallToDiskPlan {
    pub deploy: IsoDeployPlan,
    pub phase: InstallPhase,
}

impl InstallToDiskPlan {
    pub const fn empty() -> Self {
        Self {
            deploy: IsoDeployPlan::empty(),
            phase: InstallPhase::Idle,
        }
    }

    pub fn is_contract_ready(&self) -> bool {
        self.deploy.is_ready() && self.phase != InstallPhase::Idle
    }

    pub fn is_install_complete(&self) -> bool {
        self.phase == InstallPhase::BootedFromDisk
    }
}

/// What the guest launch path must honor for extract-boot install MVP.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InstallLaunchContract {
    pub iso_id: u64,
    pub extract_bound: bool,
    pub install_disk_bytes: u64,
}

/// Probe-sized virtio-blk backing when no install contract is armed (M4.3).
pub const PROBE_DISK_BYTES: usize = 4096;

/// Armed across ExitBootServices: set by PRE-EBS `POST /iso/{id}/install`.
static mut ARMED_INSTALL: Option<InstallLaunchContract> = None;

/// Stash launch contract so post-EBS `virtio_blk::init` can size the install disk.
pub fn arm_install_launch_contract(contract: InstallLaunchContract) {
    // SAFETY: single-threaded boot / mgmt path.
    unsafe {
        ARMED_INSTALL = Some(contract);
    }
}

/// Peek armed contract without clearing (host tests / diagnostics).
pub fn peek_armed_install_contract() -> Option<InstallLaunchContract> {
    // SAFETY: single-threaded boot / mgmt path.
    unsafe { ARMED_INSTALL }
}

/// Take armed contract (clear after read). Prefer [`disk_bytes_for_virtio_launch`] at init.
pub fn take_armed_install_contract() -> Option<InstallLaunchContract> {
    // SAFETY: single-threaded boot / mgmt path.
    unsafe { ARMED_INSTALL.take() }
}

/// Clear armed contract (tests / cancel).
pub fn clear_armed_install_contract() {
    // SAFETY: single-threaded boot / mgmt path.
    unsafe {
        ARMED_INSTALL = None;
    }
}

/// Disk bytes for live `virtio_blk::init`: armed install size or M4.3 probe (4 KiB).
pub fn disk_bytes_for_virtio_launch() -> usize {
    match peek_armed_install_contract() {
        Some(c)
            if c.extract_bound
                && c.install_disk_bytes >= PROBE_DISK_BYTES as u64
                && c.install_disk_bytes % 512 == 0 =>
        {
            c.install_disk_bytes as usize
        }
        _ => PROBE_DISK_BYTES,
    }
}

/// True when launch will use an install-sized disk (not the 4 KiB probe).
pub fn install_disk_armed_for_launch() -> bool {
    disk_bytes_for_virtio_launch() > PROBE_DISK_BYTES
}

/// Build launch contract from a ready M7.3 deploy plan.
pub fn launch_contract(deploy: &IsoDeployPlan) -> Result<InstallLaunchContract, IsoError> {
    if !deploy.is_ready() {
        return Err(IsoError::BadState);
    }
    Ok(InstallLaunchContract {
        iso_id: deploy.iso_id,
        extract_bound: deploy.extract_bound,
        install_disk_bytes: deploy.install_disk_bytes,
    })
}

/// Start install-to-disk from a ready deploy plan (or bind one first).
pub fn begin_install_to_disk(
    store: &ImageTable,
    install: &mut InstallToDiskPlan,
    iso_id: u64,
) -> Result<InstallLaunchContract, IsoError> {
    let _ = (
        M7_ISO_INSTALL_OK_MARKER,
        M7_ISO_INSTALL_SCAFFOLD_MARKER,
        ISO_INSTALL_GAP_NOTE,
        ISO_INSTALL_MVP_NOTE,
        ISO_INSTALL_HOST_LIMIT_NOTE,
    );
    if !install.deploy.is_ready() || install.deploy.iso_id != iso_id {
        bind_extract_boot(store, &mut install.deploy, iso_id)?;
        configure_install_disk(&mut install.deploy, DEFAULT_INSTALL_DISK_BYTES)?;
    }
    let contract = launch_contract(&install.deploy)?;
    install.phase = InstallPhase::ContractReady;
    arm_install_launch_contract(contract);
    Ok(contract)
}

/// Record that the install disk received a guest/host write proof.
pub fn mark_disk_written(install: &mut InstallToDiskPlan) -> Result<(), IsoError> {
    if install.phase != InstallPhase::ContractReady && install.phase != InstallPhase::DiskWritten {
        return Err(IsoError::BadState);
    }
    install.phase = InstallPhase::DiskWritten;
    Ok(())
}

/// Request reboot-from-disk after a successful disk write.
pub fn mark_reboot_pending(install: &mut InstallToDiskPlan) -> Result<(), IsoError> {
    if install.phase != InstallPhase::DiskWritten {
        return Err(IsoError::BadState);
    }
    install.phase = InstallPhase::RebootPending;
    Ok(())
}

/// Record second boot from the install disk (QEMU/iron close step).
pub fn mark_booted_from_disk(install: &mut InstallToDiskPlan) -> Result<(), IsoError> {
    if install.phase != InstallPhase::RebootPending {
        return Err(IsoError::BadState);
    }
    install.phase = InstallPhase::BootedFromDisk;
    Ok(())
}

/// True when virtio-blk accepts the default install-disk size (capacity surface).
pub fn install_disk_capacity_ok(disk_bytes: u64) -> bool {
    disk_bytes > 0
        && disk_bytes % 512 == 0
        && disk_bytes >= DEFAULT_INSTALL_DISK_BYTES
        && devices_install_capacity_surface()
}

fn devices_install_capacity_surface() -> bool {
    let blk = include_str!("../devices/virtio_blk.rs");
    blk.contains("fn capacity_sectors_for(")
        && blk.contains("DEFAULT_INSTALL_DISK_BYTES")
        && blk.contains("unsafe fn init(")
}

/// True when guest extract-boot + install-disk surfaces exist for the contract.
pub fn install_launch_surfaces_present() -> bool {
    extract_boot_surface_present()
        && install_disk_surface_present()
        && devices_install_capacity_surface()
        && include_str!("../guest/linux_boot.rs").contains("fn load_bzimage_guest")
}

enum InstallOp {
    Status,
    Begin { id: u64 },
}

fn parse_u64(s: &str) -> Option<u64> {
    let mut n: u64 = 0;
    if s.is_empty() {
        return None;
    }
    for b in s.bytes() {
        if !(b'0'..=b'9').contains(&b) {
            return None;
        }
        n = n.checked_mul(10)?.checked_add(u64::from(b - b'0'))?;
    }
    Some(n)
}

fn route_install(method: RestMethod, path: &str) -> Result<InstallOp, ()> {
    let path = path.trim().trim_end_matches('/');
    if path == "/iso/install" {
        return match method {
            RestMethod::Get => Ok(InstallOp::Status),
            _ => Err(()),
        };
    }
    let rest = path.strip_prefix("/iso/").ok_or(())?;
    let mut segs = rest.split('/');
    let id_s = segs.next().ok_or(())?;
    let id = parse_u64(id_s).ok_or(())?;
    let action = segs.next();
    if segs.next().is_some() {
        return Err(());
    }
    match (method, action) {
        (RestMethod::Post, Some("install")) => Ok(InstallOp::Begin { id }),
        _ => Err(()),
    }
}

/// REST: `POST /iso/{id}/install` begins install-to-disk contract;
/// `GET /iso/install` returns Listed count `1` when contract ready (not iron-closed).
pub fn dispatch_iso_install_rest(
    store: &mut ImageTable,
    install: &mut InstallToDiskPlan,
    req: RestRequest<'_>,
) -> RestResponse {
    if !auth_allows(req.auth_token) {
        return RestResponse {
            status: 401,
            reply: None,
        };
    }
    match route_install(req.method, req.path) {
        Ok(InstallOp::Status) => RestResponse {
            status: 200,
            reply: Some(ApiReply::Listed {
                count: if install.is_contract_ready() { 1 } else { 0 },
            }),
        },
        Ok(InstallOp::Begin { id }) => {
            if store.get(id).is_none() {
                if let Err(e) = register_iso(store, id, 0, "distro.iso") {
                    return match e {
                        IsoError::Store(_) => RestResponse {
                            status: 409,
                            reply: None,
                        },
                        _ => RestResponse {
                            status: 500,
                            reply: None,
                        },
                    };
                }
            } else if store.get(id).map(|r| r.kind) != Some(ImageKind::Iso) {
                return RestResponse {
                    status: 409,
                    reply: None,
                };
            }
            match begin_install_to_disk(store, install, id) {
                Ok(_) => RestResponse {
                    status: 201,
                    reply: Some(ApiReply::Ok),
                },
                Err(IsoError::NotFound) => RestResponse {
                    status: 404,
                    reply: None,
                },
                Err(IsoError::BadState) | Err(IsoError::InvalidId) => RestResponse {
                    status: 409,
                    reply: None,
                },
                Err(_) => RestResponse {
                    status: 500,
                    reply: None,
                },
            }
        }
        Err(()) => RestResponse {
            status: 400,
            reply: None,
        },
    }
}

/// Host-testable install-to-disk **scaffold** package (not iron close).
pub fn prop_iso_install_package() -> bool {
    let _ = BRINGUP_AUTH_TOKEN;
    clear_armed_install_contract();
    if !install_launch_surfaces_present() {
        return false;
    }
    if !install_disk_capacity_ok(DEFAULT_INSTALL_DISK_BYTES) {
        return false;
    }
    if disk_bytes_for_virtio_launch() != PROBE_DISK_BYTES {
        return false;
    }

    let mut store = ImageTable::new();
    let mut install = InstallToDiskPlan::empty();
    if register_iso(&mut store, 7, 800_000_000, "debian.iso").is_err() {
        return false;
    }
    let contract = match begin_install_to_disk(&store, &mut install, 7) {
        Ok(c) => c,
        Err(_) => return false,
    };
    if contract.iso_id != 7
        || !contract.extract_bound
        || contract.install_disk_bytes != DEFAULT_INSTALL_DISK_BYTES
    {
        return false;
    }
    if install.phase != InstallPhase::ContractReady {
        return false;
    }
    if peek_armed_install_contract() != Some(contract) {
        return false;
    }
    if disk_bytes_for_virtio_launch() != DEFAULT_INSTALL_DISK_BYTES as usize {
        return false;
    }
    if !install_disk_armed_for_launch() {
        return false;
    }
    // Live path must name the wire helper (include_str gate in scaffold).
    if !include_str!("../src/main.rs").contains("disk_bytes_for_virtio_launch") {
        return false;
    }
    if !include_str!("../src/main.rs").contains("allocate_contiguous") {
        return false;
    }
    if mark_disk_written(&mut install).is_err() {
        return false;
    }
    if mark_reboot_pending(&mut install).is_err() {
        return false;
    }
    if mark_booted_from_disk(&mut install).is_err() {
        return false;
    }
    if !install.is_install_complete() {
        return false;
    }

    // Phase machine must reject out-of-order advances.
    let mut bad = InstallToDiskPlan::empty();
    if mark_reboot_pending(&mut bad).is_ok() {
        return false;
    }

    clear_armed_install_contract();
    let tok = Some(BRINGUP_AUTH_TOKEN);
    let mut store2 = ImageTable::new();
    let mut install2 = InstallToDiskPlan::empty();
    let began = dispatch_iso_install_rest(
        &mut store2,
        &mut install2,
        RestRequest {
            method: RestMethod::Post,
            path: "/iso/21/install",
            auth_token: tok,
        },
    );
    if began.status != 201 || !install2.is_contract_ready() {
        return false;
    }
    if disk_bytes_for_virtio_launch() != DEFAULT_INSTALL_DISK_BYTES as usize {
        return false;
    }
    let status = dispatch_iso_install_rest(
        &mut store2,
        &mut install2,
        RestRequest {
            method: RestMethod::Get,
            path: "/iso/install",
            auth_token: tok,
        },
    );
    let ok = status.status == 200
        && status.reply == Some(ApiReply::Listed { count: 1 })
        && ISO_INSTALL_GAP_NOTE.contains("OPEN M7.7")
        && ISO_INSTALL_MVP_NOTE.contains("reboot-to-disk")
        && ISO_INSTALL_HOST_LIMIT_NOTE.contains("cannot close")
        && M7_ISO_INSTALL_SCAFFOLD_MARKER == "RAYNU-V-M7-ISO-INSTALL-SCAFFOLD-OK"
        && M7_ISO_INSTALL_OK_MARKER == "RAYNU-V-M7-ISO-INSTALL-OK";
    clear_armed_install_contract();
    ok
}

#[cfg(test)]
#[path = "iso_install_test.rs"]
mod iso_install_test;
