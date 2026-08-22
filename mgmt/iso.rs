//! M7.3 ISO deploy path (outside Proven Core).
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** (ADR-002 / ADR-009)
//! VERIFICATION: N/A
//!
//! Operator registers a distro ISO into the M7.2 image library, then binds a
//! **documented kernel-extract boot** path (existing bzImage/initrd staging +
//! `guest::load_bzimage_guest`) with an empty virtio-blk install target.
//! That extract path is the M7.3 **lab MVP**, not the product installer.
//! Product ISO install is typed + UEFI-first ([ADR-014](../docs/adr/ADR-014.md)):
//! `linux_iso` | `windows_iso` | `generic_uefi`. Catalog parse lives in
//! [`crate::mgmt::el_torito`] — parse is not attach, and attach is not VMLAUNCH.
//! `attach_cdrom_uefi` stays `UnsupportedOnFirmware`. Host attach is
//! [`attach_cdrom_host`] — parse + record a CD-ROM model. That is not guest
//! UEFI and not VMLAUNCH. Do not hard-wire SPA install to bzImage.

use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, Ordering};

use crate::audit::AuditEvent;
use crate::audit_log;

use super::api::{
    auth_allows, ApiReply, RestMethod, RestRequest, RestResponse, BRINGUP_AUTH_TOKEN,
};
use super::datastore::{ImageKind, ImageTable, StoreError};
use super::el_torito::{write_mock_efi_iso, ISO_SECTOR, MOCK_EFI_ISO_BYTES};
use super::guest_image::GuestImageType;

pub use super::el_torito::{parse_el_torito, ElToritoError, ElToritoImage};

/// Host / CI marker when the M7.3 ISO deploy gate passes.
pub const M7_ISO_OK_MARKER: &str = "RAYNU-V-M7-ISO-OK";

/// Linux ISO deploy path GAP closed in M7.3.
pub const ISO_GAP_NOTE: &str = "GAP(CLOSED M7.3): Linux ISO deploy path";

/// Documented MVP: kernel-extract boot (lab; not the product installer — ADR-014).
pub const ISO_EXTRACT_BOOT_NOTE: &str =
    "MVP: documented kernel-extract boot via bzImage/initrd staging (El Torito/CD-ROM deferred; product path ADR-014 UEFI-first)";

/// Default empty install disk size for the virtio-blk target (host/CI).
pub const DEFAULT_INSTALL_DISK_BYTES: u64 = 64 * 1024 * 1024;

/// Error from ISO deploy planning / attach.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IsoError {
    NotFound,
    BadState,
    InvalidId,
    UnsupportedOnFirmware,
    /// El Torito catalog missing, truncated, or not bootable.
    Catalog,
    /// Product ISO types require an EFI (0xEF) catalog entry.
    NotEfi,
    Store(StoreError),
}

/// Host / firmware CD-ROM attach state. Not guest-UEFI-live
/// (that stays [`attach_cdrom_uefi`] → `UnsupportedOnFirmware`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CdromAttachState {
    Detached,
    Parsed,
    AttachedHost,
    /// Firmware-facing boot image armed. Not VMLAUNCH and not OVMF.
    FirmwareArmed,
}

/// One host-side CD-ROM / boot-image record (ADR-014 Stage 1).
///
/// INVARIANTS:
/// - `state == AttachedHost` only after a successful [`attach_cdrom_host`]
/// - `efi` is the catalog platform flag; product types require `efi == true`
/// - Does not imply guest UEFI firmware or a VMLAUNCH CD
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CdromAttach {
    pub iso_id: u64,
    pub catalog_lba: u32,
    pub load_lba: u32,
    pub sector_count: u16,
    pub efi: bool,
    pub image_type: GuestImageType,
    pub state: CdromAttachState,
}

/// Fixed host CD-ROM table (no alloc). One slot per `iso_id`.
pub const CDROM_CAP: usize = 8;

#[derive(Debug, Clone, Copy)]
pub struct CdromTable {
    slots: [Option<CdromAttach>; CDROM_CAP],
}

impl CdromTable {
    pub const fn empty() -> Self {
        Self {
            slots: [None; CDROM_CAP],
        }
    }

    pub fn clear(&mut self) {
        self.slots = [None; CDROM_CAP];
    }

    pub fn get(&self, iso_id: u64) -> Option<CdromAttach> {
        self.slots.iter().copied().flatten().find(|a| a.iso_id == iso_id)
    }

    pub fn insert(&mut self, attach: CdromAttach) -> Result<(), IsoError> {
        if attach.iso_id == 0 {
            return Err(IsoError::InvalidId);
        }
        for slot in self.slots.iter_mut() {
            if let Some(existing) = slot {
                if existing.iso_id == attach.iso_id {
                    *slot = Some(attach);
                    return Ok(());
                }
            }
        }
        for slot in self.slots.iter_mut() {
            if slot.is_none() {
                *slot = Some(attach);
                return Ok(());
            }
        }
        Err(IsoError::Store(StoreError::Full))
    }

    pub fn attached_count(&self) -> usize {
        self.slots
            .iter()
            .copied()
            .flatten()
            .filter(|a| {
                matches!(
                    a.state,
                    CdromAttachState::AttachedHost | CdromAttachState::FirmwareArmed
                )
            })
            .count()
    }

    pub fn firmware_armed_count(&self) -> usize {
        self.slots
            .iter()
            .copied()
            .flatten()
            .filter(|a| a.state == CdromAttachState::FirmwareArmed)
            .count()
    }
}

/// Firmware-facing El Torito boot image (no alloc; metadata only).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FirmwareBootImage {
    pub iso_id: u64,
    pub catalog_lba: u32,
    pub load_lba: u32,
    pub sector_count: u16,
    pub efi: bool,
}

/// JUSTIFICATION (global state): the HTTP listen loops already take
/// `IsoDeployPlan` / `InstallToDiskPlan`. Adding another argument would
/// touch HOST-NIC FIN-close. One host CD-ROM table, spinlock for `cargo test`.
struct HostCdrom(UnsafeCell<CdromTable>);

// SAFETY: exclusive access is enforced by `HOST_CDROM_LOCK`.
// KANI-TARGET: management-plane table; outside Proven Core.
unsafe impl Sync for HostCdrom {}

static HOST_CDROM: HostCdrom = HostCdrom(UnsafeCell::new(CdromTable::empty()));
static HOST_CDROM_LOCK: AtomicBool = AtomicBool::new(false);

fn with_host_cdrom<R>(f: impl FnOnce(&mut CdromTable) -> R) -> R {
    while HOST_CDROM_LOCK.swap(true, Ordering::Acquire) {
        core::hint::spin_loop();
    }
    // SAFETY: lock held; exclusive mutable access to the host CD-ROM table.
    // KANI-TARGET: host CD-ROM table mutex (mgmt plane).
    let out = unsafe { f(&mut *HOST_CDROM.0.get()) };
    HOST_CDROM_LOCK.store(false, Ordering::Release);
    out
}

/// Reset the process-local host CD-ROM table (host tests).
pub fn reset_host_cdrom() {
    with_host_cdrom(|t| t.clear());
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

/// Host El Torito CD-ROM attach (ADR-014 Stage 1).
///
/// INVARIANTS:
/// - Parses `iso` via [`parse_el_torito`]; does not mutate `iso`
/// - Rejects lab `linux_bzimage` and `iso_id == 0`
/// - Product types require `efi == true` on the catalog
/// - On success, returned `state` is [`CdromAttachState::AttachedHost`]
/// - Does not VMLAUNCH and does not change [`attach_cdrom_uefi`]
///
/// VERIFICATION: L0 (outside Proven Core) — runtime checks in the E5 Stage 1 gate
pub fn attach_cdrom_host(
    iso: &[u8],
    iso_id: u64,
    image_type: GuestImageType,
) -> Result<CdromAttach, IsoError> {
    if iso_id == 0 {
        return Err(IsoError::InvalidId);
    }
    if image_type.is_lab_only() {
        return Err(IsoError::BadState);
    }
    let img = parse_el_torito(iso).map_err(|_| IsoError::Catalog)?;
    if !img.efi {
        return Err(IsoError::NotEfi);
    }
    audit_log!(AuditEvent::CdromAttached {
        iso_id,
        load_lba: u64::from(img.load_lba),
    });
    Ok(CdromAttach {
        iso_id,
        catalog_lba: img.catalog_lba,
        load_lba: img.load_lba,
        sector_count: img.sector_count,
        efi: img.efi,
        image_type,
        state: CdromAttachState::AttachedHost,
    })
}

/// Firmware-facing 2048-byte CD sector read (El Torito / ECMA-119).
///
/// INVARIANTS:
/// - Does not allocate
/// - Does not mutate `iso`
/// - Rejects an LBA that is not fully inside `iso`
pub fn cdrom_read_sector(iso: &[u8], lba: u32) -> Result<[u8; ISO_SECTOR], IsoError> {
    let start = (lba as usize).saturating_mul(ISO_SECTOR);
    let end = start.saturating_add(ISO_SECTOR);
    if end > iso.len() {
        return Err(IsoError::Catalog);
    }
    let mut out = [0u8; ISO_SECTOR];
    out.copy_from_slice(&iso[start..end]);
    Ok(out)
}

/// Resolve the firmware boot image from a host attach + ISO bytes.
///
/// INVARIANTS:
/// - Requires `AttachedHost` or `FirmwareArmed`
/// - Product types require `efi == true`
/// - Every catalog `sector_count` sector from `load_lba` must be present
/// - Does not VMLAUNCH and does not load OVMF
pub fn firmware_boot_image(
    iso: &[u8],
    attach: &CdromAttach,
) -> Result<FirmwareBootImage, IsoError> {
    if attach.iso_id == 0 {
        return Err(IsoError::InvalidId);
    }
    if !matches!(
        attach.state,
        CdromAttachState::AttachedHost | CdromAttachState::FirmwareArmed
    ) {
        return Err(IsoError::BadState);
    }
    if attach.image_type.is_lab_only() {
        return Err(IsoError::BadState);
    }
    if !attach.efi {
        return Err(IsoError::NotEfi);
    }
    if attach.sector_count == 0 {
        return Err(IsoError::Catalog);
    }
    let last = (attach.load_lba as u64)
        .checked_add(u64::from(attach.sector_count))
        .and_then(|n| n.checked_sub(1))
        .ok_or(IsoError::Catalog)?;
    if last > u64::from(u32::MAX) {
        return Err(IsoError::Catalog);
    }
    let _ = cdrom_read_sector(iso, attach.load_lba)?;
    let _ = cdrom_read_sector(iso, last as u32)?;
    Ok(FirmwareBootImage {
        iso_id: attach.iso_id,
        catalog_lba: attach.catalog_lba,
        load_lba: attach.load_lba,
        sector_count: attach.sector_count,
        efi: attach.efi,
    })
}

/// Arm a firmware-facing CD from an existing host attach (ADR-014 Stage 2).
///
/// INVARIANTS:
/// - Requires [`CdromAttachState::AttachedHost`] (or already `FirmwareArmed`)
/// - Re-parses El Torito and requires catalog `load_lba` to match the host record
/// - On success, returned `state` is [`CdromAttachState::FirmwareArmed`]
/// - Does not change [`attach_cdrom_uefi`] and does not VMLAUNCH
pub fn attach_cdrom_firmware(
    iso: &[u8],
    host: CdromAttach,
) -> Result<CdromAttach, IsoError> {
    if host.state != CdromAttachState::AttachedHost
        && host.state != CdromAttachState::FirmwareArmed
    {
        return Err(IsoError::BadState);
    }
    let parsed = parse_el_torito(iso).map_err(|_| IsoError::Catalog)?;
    if parsed.load_lba != host.load_lba
        || parsed.catalog_lba != host.catalog_lba
        || parsed.sector_count != host.sector_count
    {
        return Err(IsoError::Catalog);
    }
    if !parsed.efi {
        return Err(IsoError::NotEfi);
    }
    let _ = firmware_boot_image(iso, &host)?;
    audit_log!(AuditEvent::CdromFirmwareArmed {
        iso_id: host.iso_id,
        load_lba: u64::from(host.load_lba),
    });
    Ok(CdromAttach {
        state: CdromAttachState::FirmwareArmed,
        efi: parsed.efi,
        ..host
    })
}

/// True when `path` is the host CD-ROM attach REST surface.
pub fn is_iso_attach_path(path: &str) -> bool {
    let path = path.trim().trim_end_matches('/');
    if path == "/iso/attach" {
        return true;
    }
    let Some(rest) = path.strip_prefix("/iso/") else {
        return false;
    };
    let mut segs = rest.split('/');
    let Some(id_s) = segs.next() else {
        return false;
    };
    if parse_u64(id_s).is_none() {
        return false;
    }
    segs.next() == Some("attach")
}

/// True when `path` is the firmware-CD arm REST surface.
pub fn is_iso_firmware_path(path: &str) -> bool {
    let path = path.trim().trim_end_matches('/');
    if path == "/iso/firmware" {
        return true;
    }
    let Some(rest) = path.strip_prefix("/iso/") else {
        return false;
    };
    let mut segs = rest.split('/');
    let Some(id_s) = segs.next() else {
        return false;
    };
    if parse_u64(id_s).is_none() {
        return false;
    }
    segs.next() == Some("firmware") && segs.next().is_none()
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

/// True when virtio-blk empty-disk install target surface exists (M4.3+ / E5).
pub fn install_disk_surface_present() -> bool {
    let blk = include_str!("../devices/virtio_blk.rs");
    blk.contains("unsafe fn init(")
        && blk.contains("CAPACITY_SECTORS")
        && blk.contains("M4_BLK_OK_MARKER")
        && blk.contains("DISK_BYTES")
        && blk.contains("fn capacity_sectors_for(")
        && blk.contains("DEFAULT_INSTALL_DISK_BYTES")
}

enum IsoOp {
    Status,
    Deploy { id: u64 },
}

enum IsoAttachOp {
    Status,
    Attach {
        id: u64,
        image_type: GuestImageType,
    },
}

enum IsoFirmwareOp {
    Status,
    Arm { id: u64 },
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

fn route_iso_attach(method: RestMethod, path: &str) -> Result<IsoAttachOp, ()> {
    let path = path.trim().trim_end_matches('/');
    if path == "/iso/attach" {
        return match method {
            RestMethod::Get => Ok(IsoAttachOp::Status),
            _ => Err(()),
        };
    }
    let rest = path.strip_prefix("/iso/").ok_or(())?;
    let mut segs = rest.split('/');
    let id = parse_u64(segs.next().ok_or(())?).ok_or(())?;
    if segs.next() != Some("attach") {
        return Err(());
    }
    let image_type = match segs.next() {
        None => GuestImageType::LinuxIso,
        Some(tag) => GuestImageType::parse(tag).ok_or(())?,
    };
    if segs.next().is_some() {
        return Err(());
    }
    match method {
        RestMethod::Post => Ok(IsoAttachOp::Attach { id, image_type }),
        _ => Err(()),
    }
}

fn register_iso_if_needed(store: &mut ImageTable, id: u64) -> Result<(), IsoError> {
    if store.get(id).is_none() {
        register_iso(store, id, 0, "distro.iso")?;
        return Ok(());
    }
    if store.get(id).map(|r| r.kind) != Some(ImageKind::Iso) {
        return Err(IsoError::BadState);
    }
    Ok(())
}

fn iso_err_status(e: IsoError) -> u16 {
    match e {
        IsoError::Store(StoreError::Full) => 507,
        IsoError::NotFound => 404,
        IsoError::BadState
        | IsoError::InvalidId
        | IsoError::Catalog
        | IsoError::NotEfi
        | IsoError::Store(StoreError::BadState)
        | IsoError::Store(StoreError::InvalidId)
        | IsoError::Store(StoreError::BadName) => 409,
        IsoError::UnsupportedOnFirmware => 409,
        IsoError::Store(_) => 500,
    }
}

/// REST: `POST /iso/{id}/attach[/{type}]` registers ISO if needed, parses the
/// host mock EFI El Torito prefix (blob upload residual), and records
/// [`CdromAttachState::AttachedHost`]. `GET /iso/attach` returns attached count.
/// Does not VMLAUNCH. Does not flip [`attach_cdrom_uefi`].
pub fn dispatch_iso_attach_rest(
    store: &mut ImageTable,
    cdrom: &mut CdromTable,
    req: RestRequest<'_>,
) -> RestResponse {
    if !auth_allows(req.auth_token) {
        return RestResponse {
            status: 401,
            reply: None,
        };
    }
    match route_iso_attach(req.method, req.path) {
        Ok(IsoAttachOp::Status) => RestResponse {
            status: 200,
            reply: Some(ApiReply::Listed {
                count: cdrom.attached_count(),
            }),
        },
        Ok(IsoAttachOp::Attach { id, image_type }) => {
            if let Err(e) = register_iso_if_needed(store, id) {
                return RestResponse {
                    status: iso_err_status(e),
                    reply: None,
                };
            }
            let mut buf = [0u8; MOCK_EFI_ISO_BYTES];
            if write_mock_efi_iso(&mut buf).is_err() {
                return RestResponse {
                    status: 500,
                    reply: None,
                };
            }
            match attach_cdrom_host(&buf, id, image_type) {
                Ok(rec) => match cdrom.insert(rec) {
                    Ok(()) => RestResponse {
                        status: 201,
                        reply: Some(ApiReply::Ok),
                    },
                    Err(e) => RestResponse {
                        status: iso_err_status(e),
                        reply: None,
                    },
                },
                Err(e) => RestResponse {
                    status: iso_err_status(e),
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

/// HTTP listen path: attach against the process-local host CD-ROM table.
pub fn dispatch_iso_attach_locked(store: &mut ImageTable, req: RestRequest<'_>) -> RestResponse {
    with_host_cdrom(|cdrom| dispatch_iso_attach_rest(store, cdrom, req))
}

fn route_iso_firmware(method: RestMethod, path: &str) -> Result<IsoFirmwareOp, ()> {
    let path = path.trim().trim_end_matches('/');
    if path == "/iso/firmware" {
        return match method {
            RestMethod::Get => Ok(IsoFirmwareOp::Status),
            _ => Err(()),
        };
    }
    let rest = path.strip_prefix("/iso/").ok_or(())?;
    let mut segs = rest.split('/');
    let id = parse_u64(segs.next().ok_or(())?).ok_or(())?;
    if segs.next() != Some("firmware") || segs.next().is_some() {
        return Err(());
    }
    match method {
        RestMethod::Post => Ok(IsoFirmwareOp::Arm { id }),
        _ => Err(()),
    }
}

/// REST: `POST /iso/{id}/firmware` arms firmware CD from an existing host
/// attach. `GET /iso/firmware` returns FirmwareArmed count.
/// Does not VMLAUNCH. Does not flip [`attach_cdrom_uefi`].
pub fn dispatch_iso_firmware_rest(
    store: &mut ImageTable,
    cdrom: &mut CdromTable,
    req: RestRequest<'_>,
) -> RestResponse {
    let _ = store;
    if !auth_allows(req.auth_token) {
        return RestResponse {
            status: 401,
            reply: None,
        };
    }
    match route_iso_firmware(req.method, req.path) {
        Ok(IsoFirmwareOp::Status) => RestResponse {
            status: 200,
            reply: Some(ApiReply::Listed {
                count: cdrom.firmware_armed_count(),
            }),
        },
        Ok(IsoFirmwareOp::Arm { id }) => {
            let host = match cdrom.get(id) {
                Some(a) => a,
                None => {
                    return RestResponse {
                        status: 409,
                        reply: None,
                    };
                }
            };
            let mut buf = [0u8; MOCK_EFI_ISO_BYTES];
            if write_mock_efi_iso(&mut buf).is_err() {
                return RestResponse {
                    status: 500,
                    reply: None,
                };
            }
            match attach_cdrom_firmware(&buf, host) {
                Ok(rec) => match cdrom.insert(rec) {
                    Ok(()) => RestResponse {
                        status: 201,
                        reply: Some(ApiReply::Ok),
                    },
                    Err(e) => RestResponse {
                        status: iso_err_status(e),
                        reply: None,
                    },
                },
                Err(e) => RestResponse {
                    status: iso_err_status(e),
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

/// HTTP listen path: firmware arm against the process-local host CD-ROM table.
pub fn dispatch_iso_firmware_locked(store: &mut ImageTable, req: RestRequest<'_>) -> RestResponse {
    with_host_cdrom(|cdrom| dispatch_iso_firmware_rest(store, cdrom, req))
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
