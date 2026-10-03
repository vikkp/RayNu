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
use super::megaraid::{place_in_free_tail, PERC_IMAGE_BYTES};
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
/// new disks in the free tail, so `placeable_bytes` is that tail. An
/// empty latch does not offer the whole VD.
pub fn catalog_numbers(spare_bytes: u64, window_bytes: u64, installed: bool) -> CatalogNumbers {
    let window_cap = if spare_bytes == 0 {
        window_bytes
    } else {
        window_bytes.min(spare_bytes)
    };
    let used = if installed { window_cap } else { 0 };
    let free = spare_bytes.saturating_sub(used);
    let placeable = if spare_bytes > 0 && installed {
        free
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
const CATALOG_BODY_CAP: usize = 1280;

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

fn parse_mib(s: &str) -> Option<u32> {
    if s.is_empty() || s.len() > 7 || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse().ok()
}

fn apply_action(action: CatalogAction) -> GuestCatalogHttp {
    let installed = crate::mgmt::megaraid::perc_image_boot_latched();
    let spare = crate::mgmt::megaraid::perc_spare_bytes();
    let tail = if let Some((_, bytes)) = crate::mgmt::perc_boot_choice::armed_tail() {
        bytes
    } else {
        known_tail_bytes(spare, installed)
    };
    let library = crate::mgmt::iso_library::held_bytes();
    let occupied = SLICE1_WINDOW_BYTES.saturating_add(tail);
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
    use super::perc_boot_choice::{apply_tail_boot_choice, guest_stopped, PercChoiceApply, CHOICE_TAIL_BOOT};
    if guest_stopped() {
        return ready(409, b"{\"ok\":false,\"reason\":\"stopped\"}", None);
    }
    let installed = crate::mgmt::megaraid::perc_image_boot_latched();
    let spare = crate::mgmt::megaraid::perc_spare_bytes();
    let disk_bytes = known_tail_bytes(spare, installed);
    match apply_tail_boot_choice(installed, spare, disk_bytes) {
        PercChoiceApply::Accepted => {
            crate::boot::raynu_f_flag::request_installed_boot();
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
    match apply_tail_choice(installed, spare_bytes, disk_bytes) {
        PercChoiceApply::Accepted => {
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
    match apply_spa_choice(installed, choice) {
        PercChoiceApply::Accepted => {
            if choice == CHOICE_REINSTALL {
                crate::boot::raynu_f_flag::request_from_spa();
            } else {
                crate::boot::raynu_f_flag::request_installed_boot();
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
    let installed = crate::mgmt::megaraid::perc_image_boot_latched();
    let spare = crate::mgmt::megaraid::perc_spare_bytes();
    let tail = if let Some((_, bytes)) = crate::mgmt::perc_boot_choice::armed_tail() {
        bytes
    } else {
        discover_tail_bytes(spare, installed)
    };
    SLICE1_WINDOW_BYTES.saturating_add(tail)
}

fn fill_list_json(buf: &mut [u8]) -> usize {
    let installed = crate::mgmt::megaraid::perc_image_boot_latched();
    let spare = crate::mgmt::megaraid::perc_spare_bytes();
    let numbers = catalog_numbers(spare, SLICE1_WINDOW_BYTES, installed);
    let choice = perc_spa_choice();
    let waiting = perc_spa_wait_required(installed, choice);
    let tail = if let Some((_, bytes)) = crate::mgmt::perc_boot_choice::armed_tail() {
        bytes
    } else {
        discover_tail_bytes(spare, installed)
    };
    write_list_json(buf, numbers, waiting, choice, installed, tail)
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

fn known_tail_bytes(spare: u64, installed: bool) -> u64 {
    if let Some((_, bytes)) = crate::mgmt::perc_boot_choice::armed_tail() {
        bytes
    } else {
        discover_tail_bytes(spare, installed)
    }
}

fn discover_tail_bytes(spare: u64, installed: bool) -> u64 {
    if !installed || spare == 0 {
        return 0;
    }
    #[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
    {
        cached_tail_probe(spare)
    }
    #[cfg(not(all(target_os = "uefi", feature = "uefi-bin")))]
    {
        let _ = spare;
        0
    }
}

#[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
fn cached_tail_probe(spare: u64) -> u64 {
    use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    static PROBED: AtomicBool = AtomicBool::new(false);
    static FOUND: AtomicU64 = AtomicU64::new(0);
    if PROBED.load(Ordering::Acquire) {
        return FOUND.load(Ordering::Acquire);
    }
    let lba = SLICE1_WINDOW_BYTES / 512 + 1;
    let mut sector = [0u8; 512];
    let found = if crate::mgmt::megaraid::perc_spare_read(lba, &mut sector) {
        tail_bytes_from_gpt(&sector, spare).unwrap_or(0)
    } else {
        0
    };
    FOUND.store(found, Ordering::Release);
    PROBED.store(true, Ordering::Release);
    found
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
    tail_bytes: u64,
) -> usize {
    let stopped = crate::mgmt::perc_boot_choice::guest_stopped();
    let library = crate::mgmt::iso_library::held_bytes();
    let used = numbers
        .used_bytes
        .saturating_add(tail_bytes)
        .saturating_add(library);
    let reserve = if library > 0 { 512 } else { 0 };
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
    if installed {
        let pending = matches!(choice, PercSpaChoice::Pending) && !stopped;
        let active = matches!(
            choice,
            PercSpaChoice::BootInstalled | PercSpaChoice::CleanReinstall
        ) && !stopped;
        w.s("{\"id\":1,\"name\":\"alpine\",\"state\":\"");
        w.s(if stopped {
            "stopped"
        } else {
            window_state(choice)
        });
        w.s("\",\"disk_mib\":");
        w.u(numbers.used_bytes / (1024 * 1024));
        w.s(",\"image_type\":\"linux_iso\",\"start\":");
        w.s(if pending { "true" } else { "false" });
        w.s(",\"stop\":");
        w.s(if active { "true" } else { "false" });
        w.s("}");
    }
    if tail_bytes > 0 {
        if installed {
            w.s(",");
        }
        let pending = matches!(choice, PercSpaChoice::Pending) && !stopped;
        let active = matches!(
            choice,
            PercSpaChoice::Tail { .. } | PercSpaChoice::TailBoot { .. }
        ) && !stopped;
        w.s("{\"id\":2,\"name\":\"alpine\",\"state\":\"");
        w.s(if stopped {
            "stopped"
        } else {
            tail_row_state(choice)
        });
        w.s("\",\"disk_mib\":");
        w.u(tail_bytes / (1024 * 1024));
        w.s(",\"image_type\":\"linux_iso\",\"start\":");
        w.s(if pending { "true" } else { "false" });
        w.s(",\"stop\":");
        w.s(if active { "true" } else { "false" });
        w.s("}");
    }
    w.s("]}");
    if w.fit {
        w.n
    } else {
        0
    }
}

fn catalog_note(waiting: bool, choice: PercSpaChoice, stopped: bool) -> &'static str {
    if stopped {
        return "Guest stopped. F11 to start another.";
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
    fn numbers_show_the_tail_and_cap_the_window() {
        let n = catalog_numbers(IRON_LD1_BYTES, SLICE1_WINDOW_BYTES, true);
        assert_eq!(n.spare_bytes, IRON_LD1_BYTES);
        assert_eq!(n.used_bytes, SLICE1_WINDOW_BYTES);
        assert_eq!(n.free_bytes, IRON_LD1_BYTES - SLICE1_WINDOW_BYTES);
        assert_eq!(n.placeable_bytes, IRON_LD1_BYTES - SLICE1_WINDOW_BYTES);
        let empty = catalog_numbers(IRON_LD1_BYTES, SLICE1_WINDOW_BYTES, false);
        assert_eq!(empty.used_bytes, 0);
        assert_eq!(empty.free_bytes, IRON_LD1_BYTES);
        assert_eq!(empty.placeable_bytes, SLICE1_WINDOW_BYTES);
        let host = catalog_numbers(0, SLICE1_WINDOW_BYTES, false);
        assert_eq!(host.placeable_bytes, SLICE1_WINDOW_BYTES);
        assert_eq!(host.free_bytes, 0);
        let mut buf = [0u8; CATALOG_BODY_CAP];
        let ten = 10_240u64 * 1024 * 1024;
        let n = write_list_json(&mut buf, n, true, PercSpaChoice::Pending, true, ten);
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
        assert!(body.contains("\"start\":true"), "{body}");
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
