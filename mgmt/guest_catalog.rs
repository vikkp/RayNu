//! Spare guest catalog (ADR-019, outside Proven Core).
//!
//! The Guests page is the boot policy for a latched RAYNU-SPARE image.
//! The row is that image. Start boots it. `linux_iso` inside the 8 GiB
//! window is the existing reinstall. A larger disk that fits in the free
//! tail is a new `vda` at that size, starting after the window. UBUNTU0
//! is not in these numbers. Windows is refused. An empty latch is not a
//! wipe of the whole VD.
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** (ADR-002 / ADR-019).

use super::api::RestMethod;
use super::megaraid::{
    classify_ld_bytes, place_in_free_tail, LdClass, PERC_IMAGE_BYTES, SPARE_HIGH_RESERVE,
};
use super::perc_boot_choice::{
    apply_spa_choice, apply_tail_choice, perc_spa_choice, perc_spa_wait_required, PercChoiceApply,
    PercSpaChoice, CHOICE_BOOT, CHOICE_REINSTALL, CHOICE_TAIL,
};

/// `GET` — spare bytes, the window, and the installed guest if the latch is set.
pub const GUEST_CATALOG_PATH: &str = "/perc/guests";
/// `POST` — boot the installed 8 GiB guest. `setup-disk` stays withheld.
pub const GUEST_START_PATH: &str = "/perc/guests/1/start";
/// `POST` — boot the free-tail guest. `setup-disk` stays withheld.
pub const GUEST_TAIL_START_PATH: &str = "/perc/guests/2/start";
/// `POST` — stop the running guest. The page stays up. Another Start needs a new boot.
pub const GUEST_STOP_PATH: &str = "/perc/guests/stop";
const LINUX_PREFIX: &str = "/perc/guests/1/linux/";
const WINDOWS_PREFIX: &str = "/perc/guests/1/windows/";

/// Bytes the Guests page is allowed to place in slice 1.
pub const SLICE1_WINDOW_BYTES: u64 = PERC_IMAGE_BYTES;

/// What the operator asked the catalog to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CatalogAction {
    Start,
    Linux { disk_mib: u32 },
    Windows { disk_mib: u32 },
}

/// Why a Guests action does not become a boot choice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CatalogRefuse {
    /// Start, and the latch has no installed guest.
    NoGuest,
    /// `windows_iso` is on the form and is not installed by this firmware.
    WindowsLater,
    /// Disk size was zero or did not survive the MiB conversion.
    BadDisk,
    /// Requested disk is past the slice-1 window, and the tail is not placeable.
    LargerThanWindow {
        disk_bytes: u64,
        placeable_bytes: u64,
    },
    /// Requested disk does not fit in the free tail.
    LargerThanTail {
        disk_bytes: u64,
        placeable_bytes: u64,
    },
}

/// Pure result of [`decide`]. HTTP maps this onto the SPA choice latch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CatalogDecision {
    BootInstalled,
    InstallLinux {
        disk_bytes: u64,
    },
    /// New disk at `spare_off` on RAYNU-SPARE. The window is not this disk.
    PlaceTail {
        spare_off: u64,
        disk_bytes: u64,
    },
    Refuse(CatalogRefuse),
}

/// Numbers the Guests page prints.
///
/// `placeable_bytes` is the free tail when the spare size is known and a
/// guest is installed. Host and QEMU have no spare size, so they still
/// report the window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CatalogNumbers {
    pub spare_bytes: u64,
    pub window_bytes: u64,
    pub used_bytes: u64,
    pub free_bytes: u64,
    pub placeable_bytes: u64,
}

/// Spare size, the 8 GiB window, and whether the image latch is set.
///
/// `spare_bytes == 0` means the VD size was not read (host and QEMU).
/// The window is then the placeable cap. A known installed spare places
/// new disks in the free tail, so `placeable_bytes` is that tail minus
/// the NIC catalog and the mailbox probe. An empty latch does not offer
/// the whole VD.
pub fn catalog_numbers(spare_bytes: u64, window_bytes: u64, installed: bool) -> CatalogNumbers {
    let window_cap = if spare_bytes == 0 {
        window_bytes
    } else {
        window_bytes.min(spare_bytes)
    };
    let used = if installed { window_cap } else { 0 };
    let free = spare_bytes.saturating_sub(used);
    let reserve = if classify_ld_bytes(spare_bytes) == LdClass::Spare {
        SPARE_HIGH_RESERVE
    } else {
        0
    };
    let placeable = if spare_bytes > 0 && installed {
        free.saturating_sub(reserve)
    } else {
        window_cap
    };
    CatalogNumbers {
        spare_bytes,
        window_bytes,
        used_bytes: used,
        free_bytes: free,
        placeable_bytes: placeable,
    }
}

/// Guests policy with tails already on the spare and a library file.
///
/// `occupied_end == window_bytes` and `library_bytes == 0` matches [`decide`].
/// A later disk starts at `occupied_end`. The library, when held, shortens
/// the free tail from the high end.
pub fn decide_with_layout(
    spare_bytes: u64,
    window_bytes: u64,
    installed: bool,
    occupied_end: u64,
    library_bytes: u64,
    action: CatalogAction,
) -> CatalogDecision {
    if occupied_end == window_bytes && library_bytes == 0 {
        return decide(spare_bytes, window_bytes, installed, action);
    }
    match action {
        CatalogAction::Start => {
            if installed {
                CatalogDecision::BootInstalled
            } else {
                CatalogDecision::Refuse(CatalogRefuse::NoGuest)
            }
        }
        CatalogAction::Windows { .. } => CatalogDecision::Refuse(CatalogRefuse::WindowsLater),
        CatalogAction::Linux { disk_mib } => {
            let Some(disk_bytes) = mib_to_bytes(disk_mib) else {
                return CatalogDecision::Refuse(CatalogRefuse::BadDisk);
            };
            let window_cap = if spare_bytes == 0 {
                window_bytes
            } else {
                window_bytes.min(spare_bytes)
            };
            if disk_bytes <= window_cap {
                return CatalogDecision::InstallLinux { disk_bytes };
            }
            if let Some(place) = crate::mgmt::iso_library::place_guest_after_occupied(
                spare_bytes,
                window_bytes,
                installed,
                occupied_end,
                library_bytes,
                disk_bytes,
            ) {
                return CatalogDecision::PlaceTail {
                    spare_off: place.spare_off,
                    disk_bytes: place.disk_bytes,
                };
            }
            let limit = crate::mgmt::iso_library::library_floor(spare_bytes, library_bytes)
                .unwrap_or(0);
            let placeable = limit.saturating_sub(occupied_end);
            if spare_bytes > 0 && installed {
                CatalogDecision::Refuse(CatalogRefuse::LargerThanTail {
                    disk_bytes,
                    placeable_bytes: placeable,
                })
            } else {
                CatalogDecision::Refuse(CatalogRefuse::LargerThanWindow {
                    disk_bytes,
                    placeable_bytes: window_cap,
                })
            }
        }
    }
}

/// Guests policy. Does not touch the choice latch and does not format.
pub fn decide(
    spare_bytes: u64,
    window_bytes: u64,
    installed: bool,
    action: CatalogAction,
) -> CatalogDecision {
    match action {
        CatalogAction::Start => {
            if installed {
                CatalogDecision::BootInstalled
            } else {
                CatalogDecision::Refuse(CatalogRefuse::NoGuest)
            }
        }
        CatalogAction::Windows { .. } => CatalogDecision::Refuse(CatalogRefuse::WindowsLater),
        CatalogAction::Linux { disk_mib } => {
            let Some(disk_bytes) = mib_to_bytes(disk_mib) else {
                return CatalogDecision::Refuse(CatalogRefuse::BadDisk);
            };
            let numbers = catalog_numbers(spare_bytes, window_bytes, installed);
            let window_cap = if spare_bytes == 0 {
                window_bytes
            } else {
                window_bytes.min(spare_bytes)
            };
            if disk_bytes <= window_cap {
                return CatalogDecision::InstallLinux { disk_bytes };
            }
            if let Some(place) =
                place_in_free_tail(spare_bytes, window_bytes, installed, disk_bytes)
            {
                return CatalogDecision::PlaceTail {
                    spare_off: place.spare_off,
                    disk_bytes: place.disk_bytes,
                };
            }
            if spare_bytes > 0 && installed {
                CatalogDecision::Refuse(CatalogRefuse::LargerThanTail {
                    disk_bytes,
                    placeable_bytes: numbers.placeable_bytes,
                })
            } else {
                CatalogDecision::Refuse(CatalogRefuse::LargerThanWindow {
                    disk_bytes,
                    placeable_bytes: window_cap,
                })
            }
        }
    }
}

fn mib_to_bytes(disk_mib: u32) -> Option<u64> {
    if disk_mib == 0 {
        return None;
    }
    let bytes = u64::from(disk_mib).checked_mul(1024 * 1024)?;
    if bytes / (1024 * 1024) != u64::from(disk_mib) {
        return None;
    }
    Some(bytes)
}

/// List JSON, including `cap` and `note`. Captions live here so the SPA
/// shell stays inside `HTTP_RESPONSE_CAP`.
const CATALOG_BODY_CAP: usize = 2048;

/// REST outcome. `Ready` carries a JSON body the HTTP layer writes as-is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GuestCatalogHttp {
    NotMine,
    Unauthorized,
    BadMethod,
    Ready {
        status: u16,
        len: usize,
        body: [u8; CATALOG_BODY_CAP],
        /// `Some` when this response stored a SPA choice. The HTTP layer audits it.
        audit_choice: Option<u8>,
    },
}

/// Authenticated Guests routes. Host tests have no image latch, so Start
/// is `noguest` and a fitting `linux_iso` is `noimage`. A disk past the
/// window is `window` even without a latch.
pub fn guest_catalog_rest(method: RestMethod, path: &str, authed: bool) -> GuestCatalogHttp {
    if let Some(nic) = parse_nic_route(path) {
        return nic_rest(method, nic, authed);
    }
    let action = match parse_guest_route(path) {
        None => return GuestCatalogHttp::NotMine,
        Some(GuestRoute::List) => {
            if !authed {
                return GuestCatalogHttp::Unauthorized;
            }
            if !matches!(method, RestMethod::Get) {
                return GuestCatalogHttp::BadMethod;
            }
            let mut body = [0u8; CATALOG_BODY_CAP];
            let n = fill_list_json(&mut body);
            return ready(200, &body[..n], None);
        }
        Some(GuestRoute::Start) => CatalogAction::Start,
        Some(GuestRoute::StartTail) => {
            if !authed {
                return GuestCatalogHttp::Unauthorized;
            }
            if !matches!(method, RestMethod::Post) {
                return GuestCatalogHttp::BadMethod;
            }
            return start_tail();
        }
        Some(GuestRoute::StartDisk(off)) => {
            if !authed {
                return GuestCatalogHttp::Unauthorized;
            }
            if !matches!(method, RestMethod::Post) {
                return GuestCatalogHttp::BadMethod;
            }
            return start_disk_at(off);
        }
        Some(GuestRoute::Stop) => {
            if !authed {
                return GuestCatalogHttp::Unauthorized;
            }
            if !matches!(method, RestMethod::Post) {
                return GuestCatalogHttp::BadMethod;
            }
            return stop_guest();
        }
        Some(GuestRoute::Linux(mib)) => CatalogAction::Linux { disk_mib: mib },
        Some(GuestRoute::Windows(mib)) => CatalogAction::Windows { disk_mib: mib },
        Some(GuestRoute::Bad) => {
            if !authed {
                return GuestCatalogHttp::Unauthorized;
            }
            return GuestCatalogHttp::BadMethod;
        }
    };
    if !authed {
        return GuestCatalogHttp::Unauthorized;
    }
    if !matches!(method, RestMethod::Post) {
        return GuestCatalogHttp::BadMethod;
    }
    apply_action(action)
}

enum GuestRoute {
    List,
    Start,
    StartTail,
    StartDisk(u64),
    Stop,
    Linux(u32),
    Windows(u32),
    Bad,
}

fn parse_guest_route(path: &str) -> Option<GuestRoute> {
    if path == GUEST_CATALOG_PATH {
        return Some(GuestRoute::List);
    }
    if path == GUEST_START_PATH {
        return Some(GuestRoute::Start);
    }
    if path == GUEST_TAIL_START_PATH {
        return Some(GuestRoute::StartTail);
    }
    if let Some(off) = parse_disk_off(path) {
        return Some(GuestRoute::StartDisk(off));
    }
    if path == GUEST_STOP_PATH {
        return Some(GuestRoute::Stop);
    }
    if let Some(rest) = path.strip_prefix(LINUX_PREFIX) {
        return Some(match parse_mib(rest) {
            Some(mib) => GuestRoute::Linux(mib),
            None => GuestRoute::Bad,
        });
    }
    if let Some(rest) = path.strip_prefix(WINDOWS_PREFIX) {
        return Some(match parse_mib(rest) {
            Some(mib) => GuestRoute::Windows(mib),
            None => GuestRoute::Bad,
        });
    }
    if path.starts_with("/perc/guests/") {
        return Some(GuestRoute::Bad);
    }
    None
}

enum NicRoute {
    Save {
        spare_off: u64,
        mode: crate::mgmt::guest_nic::NicMode,
        prefix: u8,
        ipv4: [u8; 4],
        gateway: [u8; 4],
        dns: [u8; 4],
    },
    Detach(u64),
    Discover(u64),
    Bad,
}

fn parse_nic_route(path: &str) -> Option<NicRoute> {
    let rest = path.strip_prefix("/perc/nic/")?;
    let mut parts = rest.split('/');
    let off = parse_u64_token(parts.next()?)?;
    let kind = parts.next()?;
    if kind == "detach" && parts.next().is_none() {
        return Some(NicRoute::Detach(off));
    }
    if kind == "dhcp" && parts.next().is_none() {
        return Some(NicRoute::Discover(off));
    }
    let mode = match kind {
        "m" => crate::mgmt::guest_nic::NicMode::Manual,
        "d" => crate::mgmt::guest_nic::NicMode::DhcpPinned,
        _ => return Some(NicRoute::Bad),
    };
    let ipv4 = parse_ipv4(parts.next()?)?;
    let prefix = parse_u64_token(parts.next()?)?;
    if prefix > 32 {
        return Some(NicRoute::Bad);
    }
    let gateway = parse_ipv4(parts.next()?)?;
    let dns = parse_ipv4(parts.next()?)?;
    if parts.next().is_some() {
        return Some(NicRoute::Bad);
    }
    Some(NicRoute::Save {
        spare_off: off,
        mode,
        prefix: prefix as u8,
        ipv4,
        gateway,
        dns,
    })
}

fn parse_u64_token(s: &str) -> Option<u64> {
    if s.is_empty() || s.len() > 20 || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse().ok()
}

fn parse_ipv4(s: &str) -> Option<[u8; 4]> {
    let mut out = [0u8; 4];
    let mut n = 0usize;
    for part in s.split('.') {
        if n == 4 {
            return None;
        }
        if part.is_empty() || part.len() > 3 || !part.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let v: u16 = part.parse().ok()?;
        if v > 255 {
            return None;
        }
        out[n] = v as u8;
        n += 1;
    }
    if n == 4 {
        Some(out)
    } else {
        None
    }
}

fn discover_nic(spare_off: u64) -> GuestCatalogHttp {
    let saved = crate::mgmt::guest_nic::panel_snapshot()
        .get(spare_off)
        .map(|r| r.mac);
    let mac = crate::mgmt::guest_dhcp::discover_mac(saved, spare_off);
    let offer = crate::mgmt::guest_dhcp::preview_offer(mac);
    let mut body = [0u8; 192];
    let mut w = JsonBuf::new(&mut body);
    match offer {
        Some(fields) => {
            w.s("{\"ok\":true,\"saved\":false,\"mac\":\"");
            write_mac(&mut w, mac);
            w.s("\",\"ip\":\"");
            write_ip(&mut w, fields.ipv4);
            w.s("\",\"px\":");
            w.u(u64::from(fields.prefix));
            w.s(",\"gw\":\"");
            write_ip(&mut w, fields.gateway);
            w.s("\",\"dns\":\"");
            write_ip(&mut w, fields.dns);
            w.s("\"}");
        }
        None => {
            w.s("{\"ok\":false,\"reason\":\"noffer\",\"mac\":\"");
            write_mac(&mut w, mac);
            w.s("\"}");
        }
    }
    let n = if w.fit { w.n } else { 0 };
    ready(200, &body[..n], None)
}

fn nic_rest(method: RestMethod, route: NicRoute, authed: bool) -> GuestCatalogHttp {
    if !authed {
        return GuestCatalogHttp::Unauthorized;
    }
    if !matches!(method, RestMethod::Post) {
        return GuestCatalogHttp::BadMethod;
    }
    let spare = crate::mgmt::megaraid::perc_spare_bytes();
    match route {
        NicRoute::Bad => GuestCatalogHttp::BadMethod,
        NicRoute::Discover(spare_off) => discover_nic(spare_off),
        NicRoute::Detach(spare_off) => {
            if crate::mgmt::guest_nic::panel_detach(spare_off) {
                crate::audit_log!(crate::audit::AuditEvent::GuestNicDetached { spare_off });
                ready(200, b"{\"ok\":true,\"detached\":true}", None)
            } else {
                ready(404, b"{\"ok\":false,\"reason\":\"nonic\"}", None)
            }
        }
        NicRoute::Save {
            spare_off,
            mode,
            prefix,
            ipv4,
            gateway,
            dns,
        } => {
            match crate::mgmt::guest_nic::panel_save(
                spare,
                crate::mgmt::guest_nic::NicDraft {
                    spare_off,
                    mode,
                    prefix,
                    ipv4,
                    gateway,
                    dns,
                },
            ) {
                Ok(rec) => {
                    crate::audit_log!(crate::audit::AuditEvent::GuestNicSaved {
                        spare_off,
                        mode: mode as u8,
                    });
                    let mut body = [0u8; 96];
                    let mut w = JsonBuf::new(&mut body);
                    w.s("{\"ok\":true,\"mac\":\"");
                    write_mac(&mut w, rec.mac);
                    w.s("\"}");
                    let n = if w.fit { w.n } else { 0 };
                    ready(200, &body[..n], None)
                }
                Err(_) => ready(400, b"{\"ok\":false,\"reason\":\"addr\"}", None),
            }
        }
    }
}

fn parse_disk_off(path: &str) -> Option<u64> {
    let rest = path.strip_prefix("/perc/guests/disk/")?;
    let off = rest.strip_suffix("/start")?;
    if off.is_empty() || off.len() > 20 || !off.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    off.parse().ok()
}

fn parse_mib(s: &str) -> Option<u32> {
    if s.is_empty() || s.len() > 7 || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse().ok()
}

fn apply_action(action: CatalogAction) -> GuestCatalogHttp {
    let installed = crate::mgmt::megaraid::perc_image_boot_latched();
    let spare = crate::mgmt::megaraid::perc_spare_bytes();
    let library = crate::mgmt::iso_library::held_bytes();
    let occupied = occupied_guest_end();
    match decide_with_layout(
        spare,
        SLICE1_WINDOW_BYTES,
        installed,
        occupied,
        library,
        action,
    ) {
        CatalogDecision::BootInstalled => store_choice(CHOICE_BOOT, installed),
        CatalogDecision::InstallLinux { .. } => store_choice(CHOICE_REINSTALL, installed),
        CatalogDecision::PlaceTail {
            spare_off,
            disk_bytes,
        } => store_tail(installed, spare, spare_off, disk_bytes),
        CatalogDecision::Refuse(CatalogRefuse::NoGuest) => {
            ready(409, b"{\"ok\":false,\"reason\":\"noguest\"}", None)
        }
        CatalogDecision::Refuse(CatalogRefuse::WindowsLater) => {
            ready(409, b"{\"ok\":false,\"reason\":\"windows\"}", None)
        }
        CatalogDecision::Refuse(CatalogRefuse::BadDisk) => {
            ready(409, b"{\"ok\":false,\"reason\":\"baddisk\"}", None)
        }
        CatalogDecision::Refuse(CatalogRefuse::LargerThanWindow {
            disk_bytes,
            placeable_bytes,
        }) => {
            let mut body = [0u8; 576];
            let n = fill_window_json(&mut body, disk_bytes, placeable_bytes);
            ready(409, &body[..n], None)
        }
        CatalogDecision::Refuse(CatalogRefuse::LargerThanTail {
            disk_bytes,
            placeable_bytes,
        }) => {
            let mut body = [0u8; 576];
            let n = fill_tail_json(&mut body, disk_bytes, placeable_bytes);
            ready(409, &body[..n], None)
        }
    }
}

fn start_tail() -> GuestCatalogHttp {
    use super::perc_boot_choice::{apply_tail_boot_choice, PercChoiceApply, VmcsGuest, CHOICE_TAIL_BOOT};
    let off = crate::mgmt::megaraid::PERC_IMAGE_BYTES;
    if let Some(body) = held_by_other(VmcsGuest::Tail(off)) {
        return ready(200, body, None);
    }
    let restarted = crate::mgmt::perc_boot_choice::rearm_guest_after_stop();
    let installed = crate::mgmt::megaraid::perc_image_boot_latched();
    let spare = crate::mgmt::megaraid::perc_spare_bytes();
    let disk_bytes = known_tail_bytes(spare, installed);
    match apply_tail_boot_choice(installed, spare, disk_bytes) {
        PercChoiceApply::Accepted => {
            if restarted {
                crate::mgmt::perc_boot_choice::note_perc_relaunch();
            }
            crate::boot::raynu_f_flag::request_installed_boot();
            crate::mgmt::guest_nic_start::on_accepted_start(
                crate::mgmt::megaraid::PERC_IMAGE_BYTES,
            );
            let mut body = [0u8; 128];
            let mut w = JsonBuf::new(&mut body);
            w.s("{\"ok\":true,\"choice\":\"tailboot\",\"spare_off\":");
            w.u(crate::mgmt::megaraid::PERC_IMAGE_BYTES);
            w.s(",\"disk_bytes\":");
            w.u(disk_bytes);
            w.s("}");
            let n = if w.fit { w.n } else { 0 };
            ready(200, &body[..n], Some(CHOICE_TAIL_BOOT))
        }
        PercChoiceApply::NoImage => ready(409, b"{\"ok\":false,\"reason\":\"notail\"}", None),
        PercChoiceApply::AlreadyChosen => ready(409, b"{\"ok\":false,\"reason\":\"chosen\"}", None),
    }
}

fn start_disk_at(spare_off: u64) -> GuestCatalogHttp {
    use super::perc_boot_choice::{apply_tail_boot_at, PercChoiceApply, CHOICE_TAIL_BOOT};
    let installed = crate::mgmt::megaraid::perc_image_boot_latched();
    let spare = crate::mgmt::megaraid::perc_spare_bytes();
    let (tails, n) = discovered_tails();
    let Some(disk) = tails[..n].iter().find(|d| d.spare_off == spare_off) else {
        return ready(409, b"{\"ok\":false,\"reason\":\"nodisk\"}", None);
    };
    if let Some(body) = held_by_other(crate::mgmt::perc_boot_choice::VmcsGuest::Tail(spare_off))
    {
        return ready(200, body, None);
    }
    let restarted = crate::mgmt::perc_boot_choice::rearm_guest_after_stop();
    match apply_tail_boot_at(installed, spare, disk.spare_off, disk.disk_bytes) {
        PercChoiceApply::Accepted => {
            if restarted {
                crate::mgmt::perc_boot_choice::note_perc_relaunch();
            }
            crate::boot::raynu_f_flag::request_installed_boot();
            crate::mgmt::guest_nic_start::on_accepted_start(disk.spare_off);
            let mut body = [0u8; 192];
            let mut w = JsonBuf::new(&mut body);
            w.s("{\"ok\":true,\"choice\":\"tailboot\",\"spare_off\":");
            w.u(disk.spare_off);
            w.s(",\"disk_bytes\":");
            w.u(disk.disk_bytes);
            w.s("}");
            let len = if w.fit { w.n } else { 0 };
            ready(200, &body[..len], Some(CHOICE_TAIL_BOOT))
        }
        PercChoiceApply::NoImage => ready(409, b"{\"ok\":false,\"reason\":\"notail\"}", None),
        PercChoiceApply::AlreadyChosen => ready(409, b"{\"ok\":false,\"reason\":\"chosen\"}", None),
    }
}

fn stop_guest() -> GuestCatalogHttp {
    if crate::mgmt::perc_boot_choice::request_guest_stop() {
        ready(200, b"{\"ok\":true,\"choice\":\"stop\"}", None)
    } else {
        ready(409, b"{\"ok\":false,\"reason\":\"idle\"}", None)
    }
}

fn store_tail(
    installed: bool,
    spare_bytes: u64,
    spare_off: u64,
    disk_bytes: u64,
) -> GuestCatalogHttp {
    match apply_tail_choice(installed, spare_bytes, spare_off, disk_bytes) {
        PercChoiceApply::Accepted => {
            let _ = crate::mgmt::iso_library::choose_held_library();
            crate::boot::raynu_f_flag::request_from_spa();
            let mut body = [0u8; 576];
            let n = fill_tail_ok(&mut body, spare_off, disk_bytes);
            ready(200, &body[..n], Some(CHOICE_TAIL))
        }
        PercChoiceApply::NoImage => ready(409, b"{\"ok\":false,\"reason\":\"noimage\"}", None),
        PercChoiceApply::AlreadyChosen => ready(409, b"{\"ok\":false,\"reason\":\"chosen\"}", None),
    }
}

fn store_choice(choice: u8, installed: bool) -> GuestCatalogHttp {
    if choice == CHOICE_BOOT {
        if let Some(body) = held_by_other(crate::mgmt::perc_boot_choice::VmcsGuest::Window) {
            return ready(200, body, None);
        }
    }
    let restarted = if choice == CHOICE_BOOT {
        crate::mgmt::perc_boot_choice::rearm_guest_after_stop()
    } else {
        false
    };
    match apply_spa_choice(installed, choice) {
        PercChoiceApply::Accepted => {
            if restarted {
                crate::mgmt::perc_boot_choice::note_perc_relaunch();
            }
            if choice == CHOICE_REINSTALL {
                let _ = crate::mgmt::iso_library::choose_held_library();
                crate::boot::raynu_f_flag::request_from_spa();
            } else {
                crate::boot::raynu_f_flag::request_installed_boot();
                crate::mgmt::guest_nic_start::on_accepted_start(0);
            }
            let body: &[u8] = if choice == CHOICE_REINSTALL {
                b"{\"ok\":true,\"choice\":\"reinstall\"}"
            } else {
                b"{\"ok\":true,\"choice\":\"boot\"}"
            };
            ready(200, body, Some(choice))
        }
        PercChoiceApply::NoImage => ready(409, b"{\"ok\":false,\"reason\":\"noimage\"}", None),
        PercChoiceApply::AlreadyChosen => ready(409, b"{\"ok\":false,\"reason\":\"chosen\"}", None),
    }
}

fn ready(status: u16, body: &[u8], audit_choice: Option<u8>) -> GuestCatalogHttp {
    let mut buf = [0u8; CATALOG_BODY_CAP];
    let n = body.len().min(buf.len());
    buf[..n].copy_from_slice(&body[..n]);
    GuestCatalogHttp::Ready {
        status,
        len: n,
        body: buf,
        audit_choice,
    }
}

/// Window plus any free-tail disk already known. The library starts after this.
pub fn occupied_guest_end() -> u64 {
    let (tails, n) = discovered_tails();
    let mut end = occupied_from_tails(&tails[..n]);
    if let Some((off, bytes)) = crate::mgmt::perc_boot_choice::armed_tail() {
        let armed_end = off.saturating_add(bytes);
        if armed_end > end {
            end = armed_end;
        }
    }
    end
}

fn fill_list_json(buf: &mut [u8]) -> usize {
    let installed = crate::mgmt::megaraid::perc_image_boot_latched();
    let spare = crate::mgmt::megaraid::perc_spare_bytes();
    let numbers = catalog_numbers(spare, SLICE1_WINDOW_BYTES, installed);
    let choice = perc_spa_choice();
    let waiting = perc_spa_wait_required(installed, choice);
    let (tails, n) = discovered_tails();
    write_list_json(buf, numbers, waiting, choice, installed, &tails[..n])
}

/// One GPT header sector at guest LBA 1 of a disk that starts at the 8 GiB mark.
/// `backup` LBA is at offset 32. Returns the guest disk size, or `None`.
pub fn tail_bytes_from_gpt(sector: &[u8], spare_bytes: u64) -> Option<u64> {
    if sector.len() < 40 || &sector[..8] != b"EFI PART" {
        return None;
    }
    let backup = u64::from_le_bytes(sector[32..40].try_into().ok()?);
    let bytes = backup.checked_add(1)?.checked_mul(512)?;
    if bytes <= SLICE1_WINDOW_BYTES || bytes % 512 != 0 {
        return None;
    }
    if SLICE1_WINDOW_BYTES.saturating_add(bytes) > spare_bytes {
        return None;
    }
    Some(bytes)
}

/// One installed guest disk. `spare_off` is its first byte on RAYNU-SPARE.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FoundDisk {
    pub spare_off: u64,
    pub disk_bytes: u64,
}

pub const GUEST_DISK_MAX: usize = 4;

/// Disk size from a GPT header at `cursor`, if the disk ends at or before `limit`.
///
/// `lba1` is the header sector (guest LBA 1). The backup LBA is the last
/// sector of that disk, not a spare LBA. A disk that is not larger than the
/// 8 GiB window is not a tail.
pub fn disk_bytes_at(cursor: u64, lba1: &[u8], limit: u64) -> Option<u64> {
    if lba1.len() < 40 || &lba1[..8] != b"EFI PART" || cursor % 512 != 0 || limit < cursor {
        return None;
    }
    let backup = u64::from_le_bytes(lba1[32..40].try_into().ok()?);
    let bytes = backup.checked_add(1)?.checked_mul(512)?;
    if bytes <= SLICE1_WINDOW_BYTES || bytes % 512 != 0 {
        return None;
    }
    let end = cursor.checked_add(bytes)?;
    if end > limit {
        return None;
    }
    Some(bytes)
}

/// Walk GPT disks from `start` until a header is missing or the library floor.
///
/// `read_lba1(cursor, sector)` fills guest LBA 1 of the disk that begins at
/// `cursor`. The first hit on this spare is the Alpine tail at 8 GiB. The
/// next hit is the disk after it.
pub fn walk_gpt_disks(
    start: u64,
    limit: u64,
    mut read_lba1: impl FnMut(u64, &mut [u8; 512]) -> bool,
) -> ([FoundDisk; GUEST_DISK_MAX], usize) {
    let mut out = [FoundDisk {
        spare_off: 0,
        disk_bytes: 0,
    }; GUEST_DISK_MAX];
    let mut n = 0usize;
    let mut cursor = start;
    while n < GUEST_DISK_MAX && cursor.saturating_add(1024) <= limit {
        let mut sector = [0u8; 512];
        if !read_lba1(cursor, &mut sector) {
            break;
        }
        let Some(bytes) = disk_bytes_at(cursor, &sector, limit) else {
            break;
        };
        out[n] = FoundDisk {
            spare_off: cursor,
            disk_bytes: bytes,
        };
        n += 1;
        cursor = cursor.saturating_add(bytes);
    }
    (out, n)
}

fn known_tail_bytes(spare: u64, installed: bool) -> u64 {
    if let Some((_, bytes)) = crate::mgmt::perc_boot_choice::armed_tail() {
        bytes
    } else {
        discover_tail_bytes(spare, installed)
    }
}

/// Every GPT tail after the 8 GiB window, in order. Host tests see none.
fn discovered_tails() -> ([FoundDisk; GUEST_DISK_MAX], usize) {
    #[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
    {
        cached_tails()
    }
    #[cfg(not(all(target_os = "uefi", feature = "uefi-bin")))]
    {
        (
            [FoundDisk {
                spare_off: 0,
                disk_bytes: 0,
            }; GUEST_DISK_MAX],
            0,
        )
    }
}

/// First free byte after the window and every discovered tail.
fn occupied_from_tails(tails: &[FoundDisk]) -> u64 {
    let mut end = SLICE1_WINDOW_BYTES;
    for d in tails {
        if d.disk_bytes == 0 {
            continue;
        }
        let disk_end = d.spare_off.saturating_add(d.disk_bytes);
        if disk_end > end {
            end = disk_end;
        }
    }
    end
}

fn discover_tail_bytes(spare: u64, installed: bool) -> u64 {
    if !installed || spare == 0 {
        return 0;
    }
    let (tails, n) = discovered_tails();
    if n == 0 {
        0
    } else {
        tails[0].disk_bytes
    }
}

#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
fn cached_tails() -> ([FoundDisk; GUEST_DISK_MAX], usize) {
    use core::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
    static PROBED: AtomicBool = AtomicBool::new(false);
    static N: AtomicU8 = AtomicU8::new(0);
    static OFFS: [AtomicU64; GUEST_DISK_MAX] = [const { AtomicU64::new(0) }; GUEST_DISK_MAX];
    static BYTES: [AtomicU64; GUEST_DISK_MAX] = [const { AtomicU64::new(0) }; GUEST_DISK_MAX];
    if !PROBED.load(Ordering::Acquire) {
        let spare = crate::mgmt::megaraid::perc_spare_bytes();
        let installed = crate::mgmt::megaraid::perc_image_boot_latched();
        let limit = if installed {
            crate::mgmt::iso_library::library_floor(
                spare,
                crate::mgmt::iso_library::held_bytes(),
            )
            .unwrap_or(spare)
        } else {
            0
        };
        let (found, n) = walk_gpt_disks(SLICE1_WINDOW_BYTES, limit, |cursor, sector| {
            let lba = cursor / 512 + 1;
            crate::mgmt::megaraid::perc_spare_read(lba, sector)
        });
        let n = n.min(GUEST_DISK_MAX);
        for i in 0..n {
            OFFS[i].store(found[i].spare_off, Ordering::Release);
            BYTES[i].store(found[i].disk_bytes, Ordering::Release);
        }
        N.store(n as u8, Ordering::Release);
        PROBED.store(true, Ordering::Release);
    }
    let n = (N.load(Ordering::Acquire) as usize).min(GUEST_DISK_MAX);
    let mut out = [FoundDisk {
        spare_off: 0,
        disk_bytes: 0,
    }; GUEST_DISK_MAX];
    for i in 0..n {
        out[i] = FoundDisk {
            spare_off: OFFS[i].load(Ordering::Acquire),
            disk_bytes: BYTES[i].load(Ordering::Acquire),
        };
    }
    (out, n)
}

/// `cap` and `note` are the Guests sentences. The HTML shell prints them.
/// Returns 0 when the caption does not fit, so a short buffer cannot publish
/// a cut-off object.
fn write_list_json(
    buf: &mut [u8],
    numbers: CatalogNumbers,
    waiting: bool,
    choice: PercSpaChoice,
    installed: bool,
    tails: &[FoundDisk],
) -> usize {
    let stopped = crate::mgmt::perc_boot_choice::guest_stopped();
    let library = crate::mgmt::iso_library::held_bytes();
    let iso_ready = crate::mgmt::iso_library::library_complete();
    let tail_bytes: u64 = tails.iter().map(|d| d.disk_bytes).sum();
    let used = numbers
        .used_bytes
        .saturating_add(tail_bytes)
        .saturating_add(library);
    let reserve = if classify_ld_bytes(numbers.spare_bytes) == LdClass::Spare {
        SPARE_HIGH_RESERVE
    } else if library > 0 {
        512
    } else {
        0
    };
    let free = numbers.spare_bytes.saturating_sub(used).saturating_sub(reserve);
    let placeable = if numbers.spare_bytes > 0 && installed {
        free
    } else {
        numbers.placeable_bytes
    };
    let mut w = JsonBuf::new(buf);
    w.s("{\"spare_bytes\":");
    w.u(numbers.spare_bytes);
    w.s(",\"window_bytes\":");
    w.u(numbers.window_bytes);
    w.s(",\"used_bytes\":");
    w.u(used);
    w.s(",\"free_bytes\":");
    w.u(free);
    w.s(",\"placeable_bytes\":");
    w.u(placeable);
    w.s(",\"waiting\":");
    w.s(if waiting { "true" } else { "false" });
    w.s(",\"choice\":\"");
    w.s(choice_name(choice));
    w.s("\",\"cap\":\"Spare ");
    w.u(numbers.spare_bytes / (1024 * 1024));
    w.s(" MiB. Used ");
    w.u(used / (1024 * 1024));
    w.s(". Free ");
    w.u(free / (1024 * 1024));
    w.s(". Placeable ");
    w.u(placeable / (1024 * 1024));
    w.s(".\",\"note\":\"");
    w.s(catalog_note(waiting, choice, stopped));
    w.s("\",\"guests\":[");
    let window_on = installed
        && !stopped
        && matches!(
            choice,
            PercSpaChoice::BootInstalled | PercSpaChoice::CleanReinstall
        );
    let tail_on = !stopped
        && matches!(
            choice,
            PercSpaChoice::Tail { .. } | PercSpaChoice::TailBoot { .. }
        );
    let iso_on = iso_ready && crate::mgmt::iso_library::present_as_cd() && !stopped;
    if installed {
        let (start, stop) = row_power(window_on);
        w.s("{\"id\":1,\"name\":\"alpine\",\"state\":\"");
        w.s(if stopped {
            "stopped"
        } else {
            window_state(choice)
        });
        w.s("\",\"disk_mib\":");
        w.u(numbers.used_bytes / (1024 * 1024));
        w.s(",\"image_type\":\"linux_iso\",\"start\":");
        w.s(if start { "true" } else { "false" });
        w.s(",\"stop\":");
        w.s(if stop { "true" } else { "false" });
        w.s(",\"boot\":\"/perc/guests/1/start\"}");
    }
    let armed_off = match choice {
        PercSpaChoice::Tail { .. } | PercSpaChoice::TailBoot { .. } => {
            crate::mgmt::perc_boot_choice::armed_tail().map(|(off, _)| off)
        }
        _ => None,
    };
    let mut next_id = if installed { 2u64 } else { 1 };
    let mut any_guest = installed;
    for disk in tails {
        if disk.disk_bytes == 0 {
            continue;
        }
        if any_guest {
            w.s(",");
        }
        any_guest = true;
        let mine = armed_off == Some(disk.spare_off);
        let active = mine && tail_on;
        let (start, stop) = row_power(active);
        w.s("{\"id\":");
        w.u(next_id);
        next_id += 1;
        w.s(",\"name\":\"disk\",\"state\":\"");
        w.s(if stopped {
            "stopped"
        } else if !mine {
            "installed"
        } else {
            tail_row_state(choice)
        });
        w.s("\",\"disk_mib\":");
        w.u(disk.disk_bytes / (1024 * 1024));
        w.s(",\"off_mib\":");
        w.u(disk.spare_off / (1024 * 1024));
        w.s(",\"image_type\":\"linux_iso\",\"start\":");
        w.s(if start { "true" } else { "false" });
        w.s(",\"stop\":");
        w.s(if stop { "true" } else { "false" });
        w.s(",\"boot\":\"/perc/guests/disk/");
        w.u(disk.spare_off);
        w.s("/start\"}");
    }
    if iso_ready {
        if any_guest {
            w.s(",");
        }
        let chosen = crate::mgmt::iso_library::present_as_cd();
        let (start, stop) = row_power(iso_on);
        w.s("{\"id\":");
        w.u(next_id);
        w.s(",\"name\":\"ubuntu\",\"state\":\"");
        w.s(if chosen { "loading" } else { "waiting" });
        w.s("\",\"disk_mib\":");
        w.u(library / (1024 * 1024));
        w.s(",\"image_type\":\"ubuntu\",\"start\":");
        w.s(if start { "true" } else { "false" });
        w.s(",\"stop\":");
        w.s(if stop { "true" } else { "false" });
        w.s(",\"act\":\"ISO\"}");
    }
    w.s("]");
    write_nic_json(&mut w);
    crate::mgmt::host_link::write_json(&mut w);
    w.s("}");
    if w.fit {
        w.n
    } else {
        0
    }
}

/// Start and Stop follow this guest alone. Another guest being on does not
/// change this row.
pub fn row_power(this_on: bool) -> (bool, bool) {
    (!this_on, this_on)
}

/// `Some` when the CPU is already running a different guest. This guest is
/// left off. The guest that is on is left on.
fn held_by_other(want: crate::mgmt::perc_boot_choice::VmcsGuest) -> Option<&'static [u8]> {
    use crate::mgmt::perc_boot_choice::{vmcs_guest, VmcsGuest};
    match vmcs_guest() {
        VmcsGuest::None => None,
        owner if owner == want => Some(b"{\"ok\":true,\"running\":true}"),
        _ => Some(b"{\"ok\":true,\"busy\":true}"),
    }
}

fn write_nic_json(w: &mut JsonBuf<'_>) {
    let cat = crate::mgmt::guest_nic::panel_snapshot();
    let mut any = false;
    for i in 0..8 {
        let Some(rec) = cat.slot(i) else {
            continue;
        };
        if !any {
            w.s(",\"nic\":[");
            any = true;
        } else {
            w.s(",");
        }
        w.s("{\"off\":");
        w.u(rec.spare_off);
        w.s(",\"mac\":\"");
        write_mac(w, rec.mac);
        w.s("\",\"m\":\"");
        w.s(if rec.mode == crate::mgmt::guest_nic::NicMode::DhcpPinned {
            "d"
        } else {
            "m"
        });
        w.s("\",\"ip\":\"");
        write_ip(w, rec.ipv4);
        w.s("\",\"px\":");
        w.u(u64::from(rec.prefix));
        w.s(",\"gw\":\"");
        write_ip(w, rec.gateway);
        w.s("\",\"dns\":\"");
        write_ip(w, rec.dns);
        w.s("\"}");
    }
    if any {
        w.s("]");
    }
}

fn write_mac(w: &mut JsonBuf<'_>, mac: [u8; 6]) {
    for (i, b) in mac.iter().enumerate() {
        if i > 0 {
            w.s(":");
        }
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut s = [0u8; 2];
        s[0] = HEX[(b >> 4) as usize];
        s[1] = HEX[(b & 0xf) as usize];
        w.s(core::str::from_utf8(&s).unwrap_or("00"));
    }
}

fn write_ip(w: &mut JsonBuf<'_>, ip: [u8; 4]) {
    w.u(u64::from(ip[0]));
    w.s(".");
    w.u(u64::from(ip[1]));
    w.s(".");
    w.u(u64::from(ip[2]));
    w.s(".");
    w.u(u64::from(ip[3]));
}

fn catalog_note(waiting: bool, choice: PercSpaChoice, stopped: bool) -> &'static str {
    if stopped {
        return "Guest stopped. Start boots it again. The hypervisor stays up.";
    }
    if waiting {
        "Start boots one installed guest. A larger Linux disk uses the free tail. 8192 MiB reinstalls the window."
    } else {
        match choice {
            PercSpaChoice::BootInstalled => {
                "Booting the 8 GiB guest. setup-disk stays withheld."
            }
            PercSpaChoice::CleanReinstall => "Installing linux_iso on the 8 GiB window.",
            PercSpaChoice::Tail { .. } => {
                "Installing linux_iso on a new free-tail disk. The 8 GiB window stays."
            }
            PercSpaChoice::TailBoot { .. } => {
                "Booting the free-tail guest. setup-disk stays withheld."
            }
            PercSpaChoice::Pending => {
                "No latched spare image. A new disk waits until one is present."
            }
        }
    }
}

fn choice_name(choice: PercSpaChoice) -> &'static str {
    match choice {
        PercSpaChoice::Pending => "pending",
        PercSpaChoice::BootInstalled => "boot",
        PercSpaChoice::CleanReinstall => "reinstall",
        PercSpaChoice::Tail { .. } => "tail",
        PercSpaChoice::TailBoot { .. } => "tailboot",
    }
}

fn window_state(choice: PercSpaChoice) -> &'static str {
    match choice {
        PercSpaChoice::BootInstalled => "booting",
        PercSpaChoice::CleanReinstall => "installing",
        PercSpaChoice::Tail { .. } | PercSpaChoice::TailBoot { .. } => "stopped",
        PercSpaChoice::Pending => "installed",
    }
}

fn tail_row_state(choice: PercSpaChoice) -> &'static str {
    match choice {
        PercSpaChoice::Tail { .. } => "installing",
        PercSpaChoice::TailBoot { .. } => "booting",
        _ => "installed",
    }
}

fn fill_window_json(buf: &mut [u8], disk_bytes: u64, placeable_bytes: u64) -> usize {
    let mut w = JsonBuf::new(buf);
    w.s("{\"ok\":false,\"reason\":\"window\",\"disk_bytes\":");
    w.u(disk_bytes);
    w.s(",\"placeable_bytes\":");
    w.u(placeable_bytes);
    w.s("}");
    w.n
}

fn fill_tail_json(buf: &mut [u8], disk_bytes: u64, placeable_bytes: u64) -> usize {
    let mut w = JsonBuf::new(buf);
    w.s("{\"ok\":false,\"reason\":\"tail\",\"disk_bytes\":");
    w.u(disk_bytes);
    w.s(",\"placeable_bytes\":");
    w.u(placeable_bytes);
    w.s("}");
    w.n
}

fn fill_tail_ok(buf: &mut [u8], spare_off: u64, disk_bytes: u64) -> usize {
    let mut w = JsonBuf::new(buf);
    w.s("{\"ok\":true,\"choice\":\"tail\",\"spare_off\":");
    w.u(spare_off);
    w.s(",\"disk_bytes\":");
    w.u(disk_bytes);
    w.s("}");
    w.n
}

struct JsonBuf<'a> {
    b: &'a mut [u8],
    n: usize,
    fit: bool,
}

impl<'a> JsonBuf<'a> {
    fn new(b: &'a mut [u8]) -> Self {
        Self {
            b,
            n: 0,
            fit: true,
        }
    }

    fn s(&mut self, t: &str) {
        let bytes = t.as_bytes();
        let room = self.b.len().saturating_sub(self.n);
        let take = bytes.len().min(room);
        if take < bytes.len() {
            self.fit = false;
        }
        self.b[self.n..self.n + take].copy_from_slice(&bytes[..take]);
        self.n += take;
    }

    fn u(&mut self, mut v: u64) {
        if v == 0 {
            self.s("0");
            return;
        }
        let mut tmp = [0u8; 20];
        let mut i = 20;
        while v > 0 {
            i -= 1;
            tmp[i] = b'0' + (v % 10) as u8;
            v /= 10;
        }
        let s = core::str::from_utf8(&tmp[i..]).unwrap_or("0");
        self.s(s);
    }
}

impl crate::mgmt::host_link::JsonSink for JsonBuf<'_> {
    fn s(&mut self, t: &str) {
        JsonBuf::s(self, t)
    }
    fn u(&mut self, v: u64) {
        JsonBuf::u(self, v)
    }
}

/// Host package: Guests owns the choice. Overview no longer posts it.
/// Host tests do not print an iron marker.
pub fn prop_guest_catalog() -> bool {
    let html = include_str!("../assets/webui.html");
    let http = include_str!("http.rs");
    html.contains("spare-list")
        && html.contains(GUEST_START_PATH)
        && html.contains("/perc/guests/2/start")
        && html.contains("/perc/guests/stop")
        && html.contains("/perc/guests/1/linux/")
        && html.contains("/perc/guests/1/windows/")
        && !html.contains("btn-perc-boot")
        && http.contains("guest_catalog_rest")
        && html.contains("b.note")
        && html.contains("id=\"n-ip\"")
        && html.contains("btn-nic")
        && html.contains("btn-dhcp")
        && html.contains("/dhcp")
        && html.contains("Offer filled")
        && html.contains("btn-det")
        && html.contains("btn-vnic-x")
        && html.contains("function closeVnic")
        && html.contains("a.disabled=!vm.start")
        && html.contains("c.disabled=!vm.stop")
        && include_str!("../vmx/launch.rs").contains("relaunch_perc_guest_from_spa")
        && html.contains("b.cap")
        && include_str!("guest_catalog.rs").contains("free tail")
        && include_str!("../docs/adr/ADR-019.md").contains("free tail")
        && !include_str!("guest_catalog.rs").contains("println!(\"RAYNU-V-M8")
}

#[cfg(test)]
mod guest_catalog_test {
    use super::*;
    use crate::mgmt::http::{handle_http_request, HTTP_RESPONSE_CAP};
    use crate::mgmt::iso::IsoDeployPlan;
    use crate::mgmt::iso_install::InstallToDiskPlan;
    use crate::mgmt::megaraid::IRON_LD1_BYTES;
    use crate::mgmt::perc_boot_choice::clear_perc_spa_choice_for_test;
    use crate::mgmt::{ImageTable, VmTable};

    #[test]
    fn row_power_stays_on_the_row() {
        assert_eq!(row_power(false), (true, false));
        assert_eq!(row_power(true), (false, true));
    }

    #[test]
    fn m893_offer_fills_fields_and_does_not_save() {
        crate::mgmt::guest_nic::clear_panel_for_test();
        crate::mgmt::guest_dhcp::inject_offer_for_test(Some(crate::mgmt::guest_dhcp::DhcpFields {
            ipv4: [10, 99, 99, 50],
            prefix: 24,
            gateway: [10, 99, 99, 1],
            dns: [1, 1, 1, 1],
        }));
        let (st, body) = nic_exchange(
            "POST /perc/nic/0/dhcp HTTP/1.1\r\nAuthorization: Bearer raynu-v-bringup\r\n\r\n",
        );
        assert_eq!(st, 200, "{body}");
        assert!(body.contains("\"saved\":false"), "{body}");
        assert!(body.contains("\"ip\":\"10.99.99.50\""), "{body}");
        assert!(body.contains("\"px\":24"), "{body}");
        let (st, list) = nic_exchange(
            "GET /perc/guests HTTP/1.1\r\nAuthorization: Bearer raynu-v-bringup\r\n\r\n",
        );
        assert_eq!(st, 200, "{list}");
        assert!(!list.contains("\"nic\""), "{list}");
        crate::mgmt::guest_dhcp::inject_offer_for_test(None);
        let (st, body) = nic_exchange(
            "POST /perc/nic/0/dhcp HTTP/1.1\r\nAuthorization: Bearer raynu-v-bringup\r\n\r\n",
        );
        assert_eq!(st, 200, "{body}");
        assert!(body.contains("\"reason\":\"noffer\""), "{body}");
        assert!(!body.contains("\"ip\""), "{body}");
        crate::mgmt::guest_nic::clear_panel_for_test();
    }

    #[test]
    fn d5_link_rows_ride_the_guest_list() {
        // ADR-021 D5: both LOM ports in `/perc/guests`, rendered on the
        // Overview page. Nothing is emitted until the sampler has a row,
        // so host and QEMU bodies are byte-for-byte what they were.
        use crate::mgmt::host_link::{observe, reset_for_test, LinkSample};
        crate::mgmt::guest_nic::clear_panel_for_test();
        reset_for_test();
        let list = "GET /perc/guests HTTP/1.1\r\nAuthorization: Bearer raynu-v-bringup\r\n\r\n";
        let (st, body) = nic_exchange(list);
        assert_eq!(st, 200, "{body}");
        assert!(!body.contains("\"link\""), "{body}");
        let spa = LinkSample {
            up: true,
            mbps: 1000,
            full_duplex: true,
            bmsr: 0x796d,
            driven: true,
        };
        let lan = LinkSample {
            up: false,
            mbps: 0,
            full_duplex: false,
            bmsr: 0x7949,
            driven: false,
        };
        assert!(observe(0, [0xb0, 0x26, 0x28, 0x5c, 0x5a, 0x38], spa).is_none());
        assert!(observe(1, [0xb0, 0x26, 0x28, 0x5c, 0x5a, 0x39], lan).is_none());
        let (st, body) = nic_exchange(list);
        assert_eq!(st, 200, "{body}");
        assert!(
            body.contains(
                "\"link\":[{\"lom\":3,\"func\":0,\"role\":\"spa\",\"mac\":\"b0:26:28:5c:5a:38\",\"seen\":true,\"driven\":true,\"up\":true,\"mbps\":1000,\"duplex\":\"full\",\"changes\":0}"
            ),
            "{body}"
        );
        assert!(
            body.contains(
                "{\"lom\":4,\"func\":1,\"role\":\"guest-lan\",\"mac\":\"b0:26:28:5c:5a:39\",\"seen\":true,\"driven\":false,\"up\":false,\"mbps\":0,\"duplex\":\"-\",\"changes\":0}]}"
            ),
            "{body}"
        );
        // A flip counts and the body still fits with a vNIC row and two
        // guests on the spare (the fullest list the SPA can ask for).
        let lit = LinkSample { up: true, driven: true, ..spa };
        assert!(observe(1, [0; 6], lit).is_some());
        let save = "POST /perc/nic/0/m/10.99.99.145/24/10.99.99.1/1.1.1.1 HTTP/1.1\r\nAuthorization: Bearer raynu-v-bringup\r\n\r\n";
        let (st, _) = nic_exchange(save);
        assert_eq!(st, 200);
        let ten = 10_240u64 * 1024 * 1024;
        let disks = [
            FoundDisk {
                spare_off: SLICE1_WINDOW_BYTES,
                disk_bytes: ten,
            },
            FoundDisk {
                spare_off: SLICE1_WINDOW_BYTES + ten,
                disk_bytes: ten,
            },
        ];
        let mut buf = [0u8; CATALOG_BODY_CAP];
        let n = write_list_json(
            &mut buf,
            catalog_numbers(IRON_LD1_BYTES, SLICE1_WINDOW_BYTES, true),
            true,
            PercSpaChoice::Pending,
            true,
            &disks,
        );
        let full = core::str::from_utf8(&buf[..n]).unwrap_or("");
        assert!(n > 0 && n + 128 < CATALOG_BODY_CAP, "{n} {full}");
        assert!(full.contains("\"nic\":["), "{full}");
        assert!(full.contains("\"changes\":1}]}"), "{full}");
        assert!(full.contains("\"mbps\":1000,\"duplex\":\"full\",\"changes\":1"), "{full}");
        let html = include_str!("../assets/webui.html");
        assert!(html.contains("id=\"ports\""), "Overview lists both ports");
        assert!(html.contains("function paintPorts"));
        assert!(html.contains("b.link"));
        assert!(html.contains("not driven yet"));
        reset_for_test();
        crate::mgmt::guest_nic::clear_panel_for_test();
    }

    #[test]
    fn m892_save_is_still_there_on_the_next_list() {
        crate::mgmt::guest_nic::clear_panel_for_test();
        let save = "POST /perc/nic/0/m/10.99.99.40/24/10.99.99.1/1.1.1.1 HTTP/1.1\r\nAuthorization: Bearer raynu-v-bringup\r\n\r\n";
        let (st, body) = nic_exchange(save);
        assert_eq!(st, 200, "{body}");
        assert!(body.contains("\"ok\":true"), "{body}");
        let mac = body.split("\"mac\":\"").nth(1).unwrap_or("").split('"').next().unwrap_or("");
        assert_eq!(mac.len(), 17, "{body}");
        let list = "GET /perc/guests HTTP/1.1\r\nAuthorization: Bearer raynu-v-bringup\r\n\r\n";
        let (st, body) = nic_exchange(list);
        assert_eq!(st, 200, "{body}");
        assert!(body.contains("\"ip\":\"10.99.99.40\""), "{body}");
        assert!(body.contains(mac), "{body}");
        let (st, again) = nic_exchange(list);
        assert_eq!(st, 200, "{again}");
        assert!(again.contains(mac), "{again}");
        let again_save = "POST /perc/nic/0/d/10.99.99.41/22/0.0.0.0/0.0.0.0 HTTP/1.1\r\nAuthorization: Bearer raynu-v-bringup\r\n\r\n";
        let (st, body) = nic_exchange(again_save);
        assert_eq!(st, 200, "{body}");
        assert!(body.contains(mac), "{body}");
        let (st, body) = nic_exchange(list);
        assert!(body.contains("\"ip\":\"10.99.99.41\""), "{body}");
        assert!(body.contains("\"m\":\"d\""), "{body}");
        let detach = "POST /perc/nic/0/detach HTTP/1.1\r\nAuthorization: Bearer raynu-v-bringup\r\n\r\n";
        let (st, body) = nic_exchange(detach);
        assert_eq!(st, 200, "{body}");
        let (st, body) = nic_exchange(list);
        assert_eq!(st, 200, "{body}");
        assert!(!body.contains("\"nic\""), "{body}");
        let (st, _) = nic_exchange(
            "POST /perc/nic/0/m/10.99.99.40/24/10.99.99.1/1.1.1.1 HTTP/1.1\r\n\r\n",
        );
        assert_eq!(st, 401);
        crate::mgmt::guest_nic::clear_panel_for_test();
    }

    fn nic_exchange(raw: &str) -> (u16, String) {
        let mut table = VmTable::new();
        let mut images = ImageTable::new();
        let mut iso_plan = IsoDeployPlan::empty();
        let mut iso_install = InstallToDiskPlan::empty();
        let mut out = [0u8; HTTP_RESPONSE_CAP];
        let n = handle_http_request(
            &mut table,
            &mut images,
            &mut iso_plan,
            &mut iso_install,
            raw,
            &mut out,
        )
        .unwrap_or(0);
        let s = core::str::from_utf8(&out[..n]).unwrap_or("").to_string();
        let status = s.split_whitespace().nth(1).unwrap_or("0").parse().unwrap_or(0);
        (status, s)
    }

    #[test]
    fn numbers_show_the_tail_and_cap_the_window() {
        let n = catalog_numbers(IRON_LD1_BYTES, SLICE1_WINDOW_BYTES, true);
        assert_eq!(n.spare_bytes, IRON_LD1_BYTES);
        assert_eq!(n.used_bytes, SLICE1_WINDOW_BYTES);
        assert_eq!(n.free_bytes, IRON_LD1_BYTES - SLICE1_WINDOW_BYTES);
        assert_eq!(
            n.placeable_bytes,
            IRON_LD1_BYTES - SLICE1_WINDOW_BYTES - SPARE_HIGH_RESERVE
        );
        let empty = catalog_numbers(IRON_LD1_BYTES, SLICE1_WINDOW_BYTES, false);
        assert_eq!(empty.used_bytes, 0);
        assert_eq!(empty.free_bytes, IRON_LD1_BYTES);
        assert_eq!(empty.placeable_bytes, SLICE1_WINDOW_BYTES);
        let host = catalog_numbers(0, SLICE1_WINDOW_BYTES, false);
        assert_eq!(host.placeable_bytes, SLICE1_WINDOW_BYTES);
        assert_eq!(host.free_bytes, 0);
        crate::mgmt::iso_library::clear_library_for_test();
        let mut buf = [0u8; CATALOG_BODY_CAP];
        let ten = 10_240u64 * 1024 * 1024;
        let disks = [FoundDisk {
            spare_off: SLICE1_WINDOW_BYTES,
            disk_bytes: ten,
        }];
        let n = write_list_json(&mut buf, n, true, PercSpaChoice::Pending, true, &disks);
        let body = core::str::from_utf8(&buf[..n]).unwrap_or("");
        assert!(body.ends_with("]}"), "{body}");
        assert!(n < buf.len(), "{n}");
        assert!(
            body.contains(
                "\"cap\":\"Spare 3022592 MiB. Used 18432. Free 3004160. Placeable 3004160.\""
            ),
            "{body}"
        );
        assert!(body.contains("\"id\":1"), "{body}");
        assert!(body.contains("\"state\":\"installed\""), "{body}");
        assert!(body.contains("\"disk_mib\":10240"), "{body}");
        assert!(body.contains("\"off_mib\":8192"), "{body}");
        assert!(
            body.contains("\"boot\":\"/perc/guests/disk/8589934592/start\""),
            "{body}"
        );
        assert!(body.contains("\"start\":true"), "{body}");
        let mut gpt = [0u8; 512];
        gpt[..8].copy_from_slice(b"EFI PART");
        let backup_lba = (ten / 512) - 1;
        gpt[32..40].copy_from_slice(&backup_lba.to_le_bytes());
        let limit = IRON_LD1_BYTES;
        let (walked, wn) = walk_gpt_disks(SLICE1_WINDOW_BYTES, limit, |cursor, sector| {
            if cursor == SLICE1_WINDOW_BYTES || cursor == SLICE1_WINDOW_BYTES + ten {
                sector.copy_from_slice(&gpt);
                true
            } else {
                false
            }
        });
        assert_eq!(wn, 2, "{wn}");
        assert_eq!(walked[1].spare_off, SLICE1_WINDOW_BYTES + ten);
        assert_eq!(walked[1].disk_bytes, ten);
        let mut buf2 = [0u8; CATALOG_BODY_CAP];
        let n2 = write_list_json(
            &mut buf2,
            catalog_numbers(IRON_LD1_BYTES, SLICE1_WINDOW_BYTES, true),
            true,
            PercSpaChoice::Pending,
            true,
            &walked[..wn],
        );
        let two = core::str::from_utf8(&buf2[..n2]).unwrap_or("");
        assert!(n2 > 0 && n2 < buf2.len(), "{n2} {two}");
        assert!(
            two.contains("\"boot\":\"/perc/guests/disk/19327352832/start\""),
            "{two}"
        );
        assert!(two.contains("\"off_mib\":18432"), "{two}");
        assert!(body.contains("free tail"), "{body}");
        let mut sector = [0u8; 512];
        sector[..8].copy_from_slice(b"EFI PART");
        let backup = (ten / 512) - 1;
        sector[32..40].copy_from_slice(&backup.to_le_bytes());
        assert_eq!(
            tail_bytes_from_gpt(&sector, IRON_LD1_BYTES),
            Some(ten)
        );
        assert!(tail_bytes_from_gpt(&sector, SLICE1_WINDOW_BYTES).is_none());
        assert!(
            crate::mgmt::webui::webui_len() + 256 <= crate::mgmt::http::HTTP_RESPONSE_CAP
        );
    }

    #[test]
    fn waiting_iso_is_a_row_until_guests_chooses_it() {
        crate::mgmt::iso_library::clear_library_for_test();
        clear_perc_spa_choice_for_test();
        let sector = [0x11u8; 512];
        assert!(crate::mgmt::iso_library::accept_upload(0, 1024, &sector).is_ok());
        assert!(!crate::mgmt::iso_library::library_complete());
        let numbers = catalog_numbers(IRON_LD1_BYTES, SLICE1_WINDOW_BYTES, true);
        let ten = 10_240u64 * 1024 * 1024;
        let disks = [FoundDisk {
            spare_off: SLICE1_WINDOW_BYTES,
            disk_bytes: ten,
        }];
        let mut buf = [0u8; CATALOG_BODY_CAP];
        let n = write_list_json(&mut buf, numbers, true, PercSpaChoice::Pending, true, &disks);
        let early = core::str::from_utf8(&buf[..n]).unwrap_or("");
        assert!(!early.contains("\"id\":3"), "{early}");
        assert!(crate::mgmt::iso_library::accept_upload(512, 1024, &sector).is_ok());
        assert!(crate::mgmt::iso_library::library_complete());
        assert!(!crate::mgmt::iso_library::present_as_cd());
        let n = write_list_json(&mut buf, numbers, true, PercSpaChoice::Pending, true, &disks);
        assert!(n > 0, "catalog caption did not fit");
        let body = core::str::from_utf8(&buf[..n]).unwrap_or("");
        assert!(body.contains("\"id\":1"), "{body}");
        assert!(body.contains("\"id\":2"), "{body}");
        assert!(body.contains("\"id\":3"), "{body}");
        assert!(body.contains("\"state\":\"waiting\""), "{body}");
        assert!(body.contains("\"image_type\":\"ubuntu\""), "{body}");
        assert!(body.contains("\"start\":true"), "{body}");
        assert!(crate::mgmt::iso_library::choose_held_library());
        let n = write_list_json(&mut buf, numbers, true, PercSpaChoice::Pending, true, &disks);
        let chosen = core::str::from_utf8(&buf[..n]).unwrap_or("");
        assert!(
            chosen.contains("\"state\":\"loading\",\"disk_mib\":0,\"image_type\":\"ubuntu\",\"start\":false"),
            "{chosen}"
        );
        crate::mgmt::iso_library::clear_library_for_test();
        clear_perc_spa_choice_for_test();
    }

    #[test]
    fn decide_boots_the_row_and_places_the_tail() {
        let spare = IRON_LD1_BYTES;
        let window = SLICE1_WINDOW_BYTES;
        assert_eq!(
            decide(spare, window, true, CatalogAction::Start),
            CatalogDecision::BootInstalled
        );
        assert_eq!(
            decide(0, window, false, CatalogAction::Start),
            CatalogDecision::Refuse(CatalogRefuse::NoGuest)
        );
        assert_eq!(
            decide(
                spare,
                window,
                true,
                CatalogAction::Windows { disk_mib: 8192 }
            ),
            CatalogDecision::Refuse(CatalogRefuse::WindowsLater)
        );
        assert_eq!(
            decide(spare, window, true, CatalogAction::Linux { disk_mib: 0 }),
            CatalogDecision::Refuse(CatalogRefuse::BadDisk)
        );
        assert_eq!(
            decide(spare, window, true, CatalogAction::Linux { disk_mib: 8192 }),
            CatalogDecision::InstallLinux {
                disk_bytes: SLICE1_WINDOW_BYTES
            }
        );
        assert_eq!(
            decide(
                spare,
                window,
                true,
                CatalogAction::Linux { disk_mib: 10240 }
            ),
            CatalogDecision::PlaceTail {
                spare_off: SLICE1_WINDOW_BYTES,
                disk_bytes: 10_240u64 * 1024 * 1024
            }
        );
        assert!(matches!(
            decide(0, window, false, CatalogAction::Linux { disk_mib: 10240 }),
            CatalogDecision::Refuse(CatalogRefuse::LargerThanWindow { .. })
        ));
        assert!(matches!(
            decide(
                spare,
                window,
                true,
                CatalogAction::Linux {
                    disk_mib: 4_000_000
                }
            ),
            CatalogDecision::Refuse(CatalogRefuse::LargerThanTail { .. })
        ));
        assert!(prop_guest_catalog());
    }

    #[test]
    fn m88_next_disk_starts_after_the_existing_tail() {
        let spare = IRON_LD1_BYTES;
        let window = SLICE1_WINDOW_BYTES;
        let ten = 10_240u64 * 1024 * 1024;
        let occupied = window + ten;
        assert_eq!(
            decide_with_layout(
                spare,
                window,
                true,
                occupied,
                0,
                CatalogAction::Linux { disk_mib: 20480 }
            ),
            CatalogDecision::PlaceTail {
                spare_off: occupied,
                disk_bytes: 20_480u64 * 1024 * 1024
            }
        );
        let iso = 2u64 * 1024 * 1024 * 1024;
        assert!(matches!(
            decide_with_layout(
                spare,
                window,
                true,
                crate::mgmt::iso_library::library_floor(spare, iso).unwrap(),
                iso,
                CatalogAction::Linux { disk_mib: 10240 }
            ),
            CatalogDecision::Refuse(CatalogRefuse::LargerThanTail { .. })
        ));
    }

    fn exchange(raw: &str) -> (u16, String) {
        crate::mgmt::guest_nic::clear_panel_for_test();
        clear_perc_spa_choice_for_test();
        let mut table = VmTable::new();
        let mut images = ImageTable::new();
        let mut iso_plan = IsoDeployPlan::empty();
        let mut iso_install = InstallToDiskPlan::empty();
        let mut out = [0u8; HTTP_RESPONSE_CAP];
        let n = handle_http_request(
            &mut table,
            &mut images,
            &mut iso_plan,
            &mut iso_install,
            raw,
            &mut out,
        )
        .unwrap_or(0);
        let s = core::str::from_utf8(&out[..n]).unwrap_or("").to_string();
        let status = s
            .split_whitespace()
            .nth(1)
            .unwrap_or("0")
            .parse()
            .unwrap_or(0);
        (status, s)
    }

    #[test]
    fn http_lists_an_empty_catalog_and_refuses_the_form_default() {
        let (st, body) = exchange("GET /perc/guests HTTP/1.1\r\n\r\n");
        assert_eq!(st, 401, "{body}");
        let (st, body) =
            exchange("GET /perc/guests HTTP/1.1\r\nAuthorization: Bearer raynu-v-bringup\r\n\r\n");
        assert_eq!(st, 200, "{body}");
        assert!(body.contains("\"waiting\":false"), "{body}");
        assert!(body.contains("\"guests\":[]"), "{body}");
        assert!(body.contains("\"placeable_bytes\":8589934592"), "{body}");
        assert!(
            body.contains("\"cap\":\"Spare 0 MiB. Used 0. Free 0. Placeable 8192.\""),
            "{body}"
        );
        assert!(body.contains("\"note\":\"No latched spare image."), "{body}");
        let (st, body) = exchange(
            "POST /perc/guests/1/start HTTP/1.1\r\nAuthorization: Bearer raynu-v-bringup\r\n\r\n",
        );
        assert_eq!(st, 409, "{body}");
        assert!(body.contains("noguest"), "{body}");
        let (st, body) = exchange(
            "POST /perc/guests/2/start HTTP/1.1\r\nAuthorization: Bearer raynu-v-bringup\r\n\r\n",
        );
        assert_eq!(st, 409, "{body}");
        assert!(body.contains("notail"), "{body}");
        let (st, body) = exchange(
            "POST /perc/guests/disk/19327352832/start HTTP/1.1\r\nAuthorization: Bearer raynu-v-bringup\r\n\r\n",
        );
        assert_eq!(st, 409, "{body}");
        assert!(body.contains("nodisk"), "{body}");
        let (st, body) = exchange(
            "POST /perc/guests/stop HTTP/1.1\r\nAuthorization: Bearer raynu-v-bringup\r\n\r\n",
        );
        assert_eq!(st, 409, "{body}");
        assert!(body.contains("idle"), "{body}");
        let (st, body) = exchange(
            "POST /perc/guests/1/linux/10240 HTTP/1.1\r\nAuthorization: Bearer raynu-v-bringup\r\n\r\n",
        );
        assert_eq!(st, 409, "{body}");
        assert!(body.contains("\"reason\":\"window\""), "{body}");
        assert!(body.contains("\"placeable_bytes\":8589934592"), "{body}");
        let (st, body) = exchange(
            "POST /perc/guests/1/linux/8192 HTTP/1.1\r\nAuthorization: Bearer raynu-v-bringup\r\n\r\n",
        );
        assert_eq!(st, 409, "{body}");
        assert!(body.contains("noimage"), "{body}");
        let (st, body) = exchange(
            "POST /perc/guests/1/windows/8192 HTTP/1.1\r\nAuthorization: Bearer raynu-v-bringup\r\n\r\n",
        );
        assert_eq!(st, 409, "{body}");
        assert!(body.contains("windows"), "{body}");
    }
}
