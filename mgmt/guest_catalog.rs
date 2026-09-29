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
/// `POST` — boot the installed guest. `setup-disk` stays withheld.
pub const GUEST_START_PATH: &str = "/perc/guests/1/start";
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
const CATALOG_BODY_CAP: usize = 768;

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
    match decide(spare, SLICE1_WINDOW_BYTES, installed, action) {
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

fn fill_list_json(buf: &mut [u8]) -> usize {
    let installed = crate::mgmt::megaraid::perc_image_boot_latched();
    let spare = crate::mgmt::megaraid::perc_spare_bytes();
    let numbers = catalog_numbers(spare, SLICE1_WINDOW_BYTES, installed);
    let choice = perc_spa_choice();
    let waiting = perc_spa_wait_required(installed, choice);
    write_list_json(buf, numbers, waiting, choice, installed)
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
) -> usize {
    let mut w = JsonBuf::new(buf);
    w.s("{\"spare_bytes\":");
    w.u(numbers.spare_bytes);
    w.s(",\"window_bytes\":");
    w.u(numbers.window_bytes);
    w.s(",\"used_bytes\":");
    w.u(numbers.used_bytes);
    w.s(",\"free_bytes\":");
    w.u(numbers.free_bytes);
    w.s(",\"placeable_bytes\":");
    w.u(numbers.placeable_bytes);
    w.s(",\"waiting\":");
    w.s(if waiting { "true" } else { "false" });
    w.s(",\"choice\":\"");
    w.s(choice_name(choice));
    w.s("\",\"cap\":\"Spare ");
    w.u(numbers.spare_bytes / (1024 * 1024));
    w.s(" MiB. Used ");
    w.u(numbers.used_bytes / (1024 * 1024));
    w.s(". Free ");
    w.u(numbers.free_bytes / (1024 * 1024));
    w.s(". Placeable ");
    w.u(numbers.placeable_bytes / (1024 * 1024));
    w.s(".\",\"note\":\"");
    w.s(catalog_note(waiting, choice));
    w.s("\",\"guests\":[");
    if installed {
        let disk_mib = numbers.used_bytes / (1024 * 1024);
        w.s("{\"id\":1,\"name\":\"alpine\",\"state\":\"");
        w.s(guest_state(installed, choice));
        w.s("\",\"disk_mib\":");
        w.u(disk_mib);
        w.s(",\"image_type\":\"linux_iso\"}");
    }
    w.s("]}");
    if w.fit {
        w.n
    } else {
        0
    }
}

fn catalog_note(waiting: bool, choice: PercSpaChoice) -> &'static str {
    if waiting {
        "Start boots the installed guest. A larger Linux disk uses the free tail. 8192 MiB reinstalls the window."
    } else {
        match choice {
            PercSpaChoice::BootInstalled => {
                "Booting the installed guest. setup-disk stays withheld."
            }
            PercSpaChoice::CleanReinstall => "Installing linux_iso on the 8 GiB window.",
            PercSpaChoice::Tail { .. } => {
                "Installing linux_iso on a new free-tail disk. The 8 GiB window stays."
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
    }
}

fn guest_state(installed: bool, choice: PercSpaChoice) -> &'static str {
    match (installed, choice) {
        (true, PercSpaChoice::BootInstalled) => "booting",
        (true, PercSpaChoice::CleanReinstall) => "installing",
        (true, PercSpaChoice::Tail { .. }) => "installing",
        (true, PercSpaChoice::Pending) => "installed",
        (false, _) => "absent",
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
        let n = write_list_json(&mut buf, n, true, PercSpaChoice::Pending, true);
        let body = core::str::from_utf8(&buf[..n]).unwrap_or("");
        assert!(body.ends_with("]}"), "{body}");
        assert!(n < buf.len(), "{n}");
        assert!(
            body.contains(
                "\"cap\":\"Spare 3022592 MiB. Used 8192. Free 3014400. Placeable 3014400.\""
            ),
            "{body}"
        );
        assert!(body.contains("free tail"), "{body}");
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
