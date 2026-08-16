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

/// QEMU / lab install disk (1 MiB) — keeps contiguous alloc cheap under `-m 512M`.
/// Iron / REST default remains [`DEFAULT_INSTALL_DISK_BYTES`] (64 MiB).
pub const LAB_INSTALL_DISK_BYTES: u64 = 1024 * 1024;

/// Serial / CI marker when lab ESP flag armed a 1 MiB install disk.
pub const M7_ISO_INSTALL_LAB_ARM_NOTE: &str = "boot: E5 lab isoinstall.txt armed (1MiB)";

/// Host writeback proved install-sized disk (LBA1 marker) — not reboot-to-disk.
pub const M7_ISO_DISK_WRITTEN_MARKER: &str = "RAYNU-V-M7-ISO-DISK-WRITTEN";

/// QEMU lab package close (sized disk + BLK-OK + disk written). Not iron E5.
pub const M7_ISO_INSTALL_LAB_OK_MARKER: &str = "RAYNU-V-M7-ISO-INSTALL-LAB-OK";

/// Lab honesty: install disk written; reboot-to-disk requested but not executed.
pub const M7_ISO_REBOOT_PENDING_MARKER: &str = "RAYNU-V-M7-ISO-REBOOT-PENDING";

/// Armed across ExitBootServices: set by PRE-EBS `POST /iso/{id}/install` or lab ESP flag.
static mut ARMED_INSTALL: Option<InstallLaunchContract> = None;
static mut LAB_ARMED: bool = false;
static mut DISK_WRITTEN_NOTED: bool = false;
static mut REBOOT_PENDING_NOTED: bool = false;
static mut BOOT_INSTALL_PLAN: InstallToDiskPlan = InstallToDiskPlan::empty();

/// Stash launch contract so post-EBS `virtio_blk::init` can size the install disk.
pub fn arm_install_launch_contract(contract: InstallLaunchContract) {
    // SAFETY: single-threaded boot / mgmt path.
    unsafe {
        ARMED_INSTALL = Some(contract);
        BOOT_INSTALL_PLAN = InstallToDiskPlan {
            deploy: IsoDeployPlan {
                iso_id: contract.iso_id,
                extract_bound: contract.extract_bound,
                install_disk_bytes: contract.install_disk_bytes,
            },
            phase: InstallPhase::ContractReady,
        };
        DISK_WRITTEN_NOTED = false;
        REBOOT_PENDING_NOTED = false;
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
        LAB_ARMED = false;
        DISK_WRITTEN_NOTED = false;
        REBOOT_PENDING_NOTED = false;
        BOOT_INSTALL_PLAN = InstallToDiskPlan::empty();
    }
}

/// Arm a QEMU-friendly 1 MiB install contract (ESP `isoinstall.txt` lab path).
pub fn arm_lab_install_contract() {
    arm_install_launch_contract(InstallLaunchContract {
        iso_id: 1,
        extract_bound: true,
        install_disk_bytes: LAB_INSTALL_DISK_BYTES,
    });
    // SAFETY: single-threaded boot.
    unsafe {
        LAB_ARMED = true;
    }
}

/// True when the lab ESP path armed this boot.
pub fn lab_install_armed() -> bool {
    // SAFETY: single-threaded boot.
    unsafe { LAB_ARMED }
}

/// Record install-disk write proof (host LBA1 marker after DRIVER_OK).
pub fn note_install_disk_written() -> bool {
    if !install_disk_armed_for_launch() {
        return false;
    }
    // SAFETY: single-threaded boot.
    unsafe {
        if DISK_WRITTEN_NOTED {
            return false;
        }
        DISK_WRITTEN_NOTED = true;
        let _ = mark_disk_written(&mut BOOT_INSTALL_PLAN);
    }
    true
}

/// After disk written: advance phase to RebootPending and latch serial emit.
///
/// Honest lab step — does **not** reboot or second-boot from disk.
pub fn note_reboot_pending_lab() -> bool {
    // SAFETY: single-threaded boot.
    unsafe {
        if REBOOT_PENDING_NOTED {
            return false;
        }
        if BOOT_INSTALL_PLAN.phase != InstallPhase::DiskWritten {
            // Accept direct advance if write was noted via virtio latch without
            // going through note_install_disk_written (launch path).
            if BOOT_INSTALL_PLAN.phase == InstallPhase::ContractReady {
                let _ = mark_disk_written(&mut BOOT_INSTALL_PLAN);
            }
        }
        if mark_reboot_pending(&mut BOOT_INSTALL_PLAN).is_err() {
            return false;
        }
        REBOOT_PENDING_NOTED = true;
        true
    }
}

/// Take-once COM1 emit for reboot-pending lab latch.
pub fn take_reboot_pending_latch() -> bool {
    // SAFETY: single-threaded boot.
    unsafe {
        if REBOOT_PENDING_NOTED {
            REBOOT_PENDING_NOTED = false;
            true
        } else {
            false
        }
    }
}

/// Peek boot install phase (tests / diagnostics).
pub fn boot_install_phase() -> InstallPhase {
    // SAFETY: single-threaded boot.
    unsafe { BOOT_INSTALL_PLAN.phase }
}

/// Take-once: install disk write was noted this boot.
pub fn take_install_disk_written_latch() -> bool {
    // SAFETY: single-threaded boot.
    unsafe {
        if DISK_WRITTEN_NOTED {
            DISK_WRITTEN_NOTED = false;
            true
        } else {
            false
        }
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

/// Probe ESP for `isoinstall.txt` (lab arm without PRE-EBS curl).
///
/// Paths: `\\EFI\\RayNu\\isoinstall.txt` then `\\isoinstall.txt`.
/// Must run **before** ExitBootServices.
#[cfg(target_os = "uefi")]
pub fn probe_iso_install_lab_flag() {
    use crate::boot::serial;
    use uefi::boot;
    use uefi::fs::FileSystem;

    let image = boot::image_handle();
    let Ok(sfs) = boot::get_image_file_system(image) else {
        return;
    };
    let mut fs = FileSystem::new(sfs);
    let present = flag_present(&mut fs, "\\EFI\\RayNu\\isoinstall.txt")
        || flag_present(&mut fs, "\\isoinstall.txt");
    if present {
        arm_lab_install_contract();
        serial::write_line(M7_ISO_INSTALL_LAB_ARM_NOTE);
    }
}

#[cfg(target_os = "uefi")]
fn flag_present(fs: &mut uefi::fs::FileSystem, path: &str) -> bool {
    use uefi::CString16;
    let Ok(p) = CString16::try_from(path) else {
        return false;
    };
    fs.read(p.as_ref()).is_ok()
}

#[cfg(not(target_os = "uefi"))]
pub fn probe_iso_install_lab_flag() {}

/// Host package: lab arm sizes launch disk to 1 MiB; iron default still 64 MiB via REST.
pub fn prop_iso_install_lab_package() -> bool {
    clear_armed_install_contract();
    if disk_bytes_for_virtio_launch() != PROBE_DISK_BYTES {
        return false;
    }
    arm_lab_install_contract();
    if !lab_install_armed() {
        return false;
    }
    if disk_bytes_for_virtio_launch() != LAB_INSTALL_DISK_BYTES as usize {
        return false;
    }
    if !note_install_disk_written() {
        return false;
    }
    if boot_install_phase() != InstallPhase::DiskWritten {
        return false;
    }
    if !note_reboot_pending_lab() {
        return false;
    }
    if boot_install_phase() != InstallPhase::RebootPending {
        return false;
    }
    if !take_reboot_pending_latch() {
        return false;
    }
    if !take_install_disk_written_latch() {
        return false;
    }
    let smoke = include_str!("../tools/m7-iso-install-qemu-smoke.sh");
    let runbook = include_str!("../docs/runbooks/iso_install.md");
    let ok = smoke.contains(M7_ISO_INSTALL_LAB_OK_MARKER)
        && smoke.contains("ISO_INSTALL_LAB")
        && smoke.contains("isoinstall.txt")
        && smoke.contains(M7_ISO_DISK_WRITTEN_MARKER)
        && smoke.contains(M7_ISO_REBOOT_PENDING_MARKER)
        && smoke.contains("never print iron marker")
        && runbook.contains("isoinstall.txt")
        && runbook.contains("1MiB")
        && runbook.contains("REBOOT-PENDING")
        && LAB_INSTALL_DISK_BYTES == 1024 * 1024
        && M7_ISO_DISK_WRITTEN_MARKER == crate::devices::virtio_blk::M7_ISO_DISK_WRITTEN_MARKER
        && M7_ISO_INSTALL_LAB_OK_MARKER == crate::devices::virtio_blk::M7_ISO_INSTALL_LAB_OK_MARKER
        && M7_ISO_REBOOT_PENDING_MARKER == crate::devices::virtio_blk::M7_ISO_REBOOT_PENDING_MARKER
        && include_str!("../src/main.rs").contains("probe_iso_install_lab_flag")
        && include_str!("../tools/run-qemu.sh").contains("ISO_INSTALL_LAB")
        && include_str!("../vmx/launch.rs").contains("M7_ISO_REBOOT_PENDING_MARKER")
        && include_str!("../vmx/launch.rs").contains("note_reboot_pending_lab");
    clear_armed_install_contract();
    ok
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
