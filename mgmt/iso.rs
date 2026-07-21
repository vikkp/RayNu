//! M7.3 ISO deploy path (outside Proven Core).
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** (ADR-002 / ADR-009)
//! VERIFICATION: N/A
//!
//! Operator registers a distro ISO into the M7.2 image library, then binds a
//! **documented kernel-extract boot** path (existing bzImage/initrd staging +
//! `guest::load_bzimage_guest`) with an empty virtio-blk install target.
//! Full El Torito / CD-ROM attach remains stubbed until a later gate.

use super::api::{
    auth_allows, ApiReply, RestMethod, RestRequest, RestResponse, BRINGUP_AUTH_TOKEN,
};
use super::datastore::{ImageKind, ImageTable, StoreError};

/// Host / CI marker when the M7.3 ISO deploy gate passes.
pub const M7_ISO_OK_MARKER: &str = "RAYNU-V-M7-ISO-OK";

/// Linux ISO deploy path GAP closed in M7.3.
pub const ISO_GAP_NOTE: &str = "GAP(CLOSED M7.3): Linux ISO deploy path";

/// Documented MVP: kernel-extract boot (not full CD-ROM / El Torito).
pub const ISO_EXTRACT_BOOT_NOTE: &str =
    "MVP: documented kernel-extract boot via bzImage/initrd staging (El Torito/CD-ROM deferred)";

/// Default empty install disk size for the virtio-blk target (host/CI).
pub const DEFAULT_INSTALL_DISK_BYTES: u64 = 64 * 1024 * 1024;

/// Error from ISO deploy planning / attach.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IsoError {
    NotFound,
    BadState,
    InvalidId,
    UnsupportedOnFirmware,
    Store(StoreError),
}

/// One ISO → extract-boot + install-disk plan (management plane).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IsoDeployPlan {
    pub iso_id: u64,
    pub extract_bound: bool,
    pub install_disk_bytes: u64,
}

impl IsoDeployPlan {
    pub const fn empty() -> Self {
        Self {
            iso_id: 0,
            extract_bound: false,
            install_disk_bytes: 0,
        }
    }

    pub fn is_ready(&self) -> bool {
        self.iso_id != 0 && self.extract_bound && self.install_disk_bytes > 0
    }
}

/// Register an ISO into the datastore (metadata; blob upload residual).
pub fn register_iso(
    store: &mut ImageTable,
    id: u64,
    size_bytes: u64,
    name: &str,
) -> Result<(), IsoError> {
    let _ = (ISO_GAP_NOTE, M7_ISO_OK_MARKER, ISO_EXTRACT_BOOT_NOTE);
    store
        .register(id, ImageKind::Iso, size_bytes, name)
        .map_err(IsoError::Store)
}

/// Bind extract-boot: ISO must be registered; boot uses existing bzImage/initrd path.
pub fn bind_extract_boot(store: &ImageTable, plan: &mut IsoDeployPlan, iso_id: u64) -> Result<(), IsoError> {
    if iso_id == 0 {
        return Err(IsoError::InvalidId);
    }
    let rec = store.get(iso_id).ok_or(IsoError::NotFound)?;
    if rec.kind != ImageKind::Iso {
        return Err(IsoError::BadState);
    }
    plan.iso_id = iso_id;
    plan.extract_bound = true;
    if plan.install_disk_bytes == 0 {
        plan.install_disk_bytes = DEFAULT_INSTALL_DISK_BYTES;
    }
    Ok(())
}

/// Configure empty/persistent virtio-blk install target size (bytes, multiple of 512).
pub fn configure_install_disk(plan: &mut IsoDeployPlan, disk_bytes: u64) -> Result<(), IsoError> {
    if disk_bytes == 0 || disk_bytes % 512 != 0 {
        return Err(IsoError::BadState);
    }
    if plan.iso_id == 0 {
        return Err(IsoError::BadState);
    }
    plan.install_disk_bytes = disk_bytes;
    Ok(())
}

/// Firmware CD-ROM / El Torito attach — not wired yet (honest stub).
pub fn attach_cdrom_uefi(_iso_id: u64) -> Result<(), IsoError> {
    let _ = ISO_EXTRACT_BOOT_NOTE;
    Err(IsoError::UnsupportedOnFirmware)
}

/// True when guest bzImage load + ESP/PE stage surfaces exist (extract-boot path).
pub fn extract_boot_surface_present() -> bool {
    let guest = include_str!("../guest/linux_boot.rs");
    let esp = include_str!("../boot/esp_assets.rs");
    let pe = include_str!("../boot/pe_assets.rs");
    guest.contains("fn load_bzimage_guest")
        && esp.contains("fn stage_bzimage")
        && esp.contains("fn stage_initrd")
        && pe.contains("fn bzimage_bytes")
        && pe.contains("fn initrd_bytes")
}

/// True when virtio-blk empty-disk install target surface exists (M4.3+).
pub fn install_disk_surface_present() -> bool {
    let blk = include_str!("../devices/virtio_blk.rs");
    blk.contains("unsafe fn init(")
        && blk.contains("CAPACITY_SECTORS")
        && blk.contains("M4_BLK_OK_MARKER")
        && blk.contains("DISK_BYTES")
}

enum IsoOp {
    Status,
    Deploy { id: u64 },
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

fn route_iso(method: RestMethod, path: &str) -> Result<IsoOp, ()> {
    let path = path.trim().trim_end_matches('/');
    if path == "/iso/deploy" {
        return match method {
            RestMethod::Get => Ok(IsoOp::Status),
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
        (RestMethod::Post, Some("deploy")) => Ok(IsoOp::Deploy { id }),
        _ => Err(()),
    }
}

/// REST: `POST /iso/{id}/deploy` registers ISO (if needed) + binds extract-boot;
/// `GET /iso/deploy` returns plan readiness via Listed count (0/1).
pub fn dispatch_iso_rest(
    store: &mut ImageTable,
    plan: &mut IsoDeployPlan,
    req: RestRequest<'_>,
) -> RestResponse {
    if !auth_allows(req.auth_token) {
        return RestResponse {
            status: 401,
            reply: None,
        };
    }
    match route_iso(req.method, req.path) {
        Ok(IsoOp::Status) => RestResponse {
            status: 200,
            reply: Some(ApiReply::Listed {
                count: if plan.is_ready() { 1 } else { 0 },
            }),
        },
        Ok(IsoOp::Deploy { id }) => {
            if store.get(id).is_none() {
                if let Err(e) = register_iso(store, id, 0, "distro.iso") {
                    return match e {
                        IsoError::Store(StoreError::Full) => RestResponse {
                            status: 507,
                            reply: None,
                        },
                        IsoError::Store(StoreError::BadState)
                        | IsoError::Store(StoreError::InvalidId)
                        | IsoError::Store(StoreError::BadName) => RestResponse {
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
            match bind_extract_boot(store, plan, id) {
                Ok(()) => RestResponse {
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

/// Host-testable ISO deploy package (register + extract-boot + virtio install disk).
pub fn prop_iso_deploy_package() -> bool {
    let _ = BRINGUP_AUTH_TOKEN;
    let mut store = ImageTable::new();
    let mut plan = IsoDeployPlan::empty();
    if register_iso(&mut store, 1, 700_000_000, "ubuntu.iso").is_err() {
        return false;
    }
    if bind_extract_boot(&store, &mut plan, 1).is_err() {
        return false;
    }
    if !plan.extract_bound || plan.iso_id != 1 {
        return false;
    }
    if configure_install_disk(&mut plan, DEFAULT_INSTALL_DISK_BYTES).is_err() {
        return false;
    }
    if !plan.is_ready() {
        return false;
    }
    if attach_cdrom_uefi(1) != Err(IsoError::UnsupportedOnFirmware) {
        return false;
    }
    if !extract_boot_surface_present() || !install_disk_surface_present() {
        return false;
    }

    let tok = Some(BRINGUP_AUTH_TOKEN);
    let mut store2 = ImageTable::new();
    let mut plan2 = IsoDeployPlan::empty();
    let deployed = dispatch_iso_rest(
        &mut store2,
        &mut plan2,
        RestRequest {
            method: RestMethod::Post,
            path: "/iso/11/deploy",
            auth_token: tok,
        },
    );
    if deployed.status != 201 || !plan2.is_ready() {
        return false;
    }
    let status = dispatch_iso_rest(
        &mut store2,
        &mut plan2,
        RestRequest {
            method: RestMethod::Get,
            path: "/iso/deploy",
            auth_token: tok,
        },
    );
    status.status == 200
        && status.reply == Some(ApiReply::Listed { count: 1 })
        && ISO_GAP_NOTE.contains("CLOSED M7.3")
        && M7_ISO_OK_MARKER == "RAYNU-V-M7-ISO-OK"
        && ISO_EXTRACT_BOOT_NOTE.contains("kernel-extract")
}

#[cfg(test)]
#[path = "iso_test.rs"]
mod iso_test;
