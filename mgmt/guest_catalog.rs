//! Spare guest catalog (ADR-019 slice 1, outside Proven Core).
//!
//! The Guests page is the boot policy for a latched RAYNU-SPARE image.
//! The row is that image. Start boots it. `linux_iso` inside the 8 GiB
//! window is the existing reinstall. A larger disk is refused. The free
//! tail of the VD is reported and is not placeable yet. UBUNTU0 is not
//! in these numbers. Windows is refused.
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** (ADR-002 / ADR-019).

use super::api::RestMethod;
use super::megaraid::PERC_IMAGE_BYTES;
use super::perc_boot_choice::{
    apply_spa_choice, perc_spa_choice, perc_spa_wait_required, PercChoiceApply, PercSpaChoice,
    CHOICE_BOOT, CHOICE_REINSTALL,
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
    /// Requested disk is past the slice-1 window. The tail stays unplaced.
    LargerThanWindow {
        disk_bytes: u64,
        placeable_bytes: u64,
    },
}

/// Pure result of [`decide`]. HTTP maps this onto the SPA choice latch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CatalogDecision {
    BootInstalled,
    InstallLinux { disk_bytes: u64 },
    Refuse(CatalogRefuse),
}

/// Numbers the Guests page prints. `placeable_bytes` is the slice-1 cap.
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
/// The window is still the placeable cap. A known spare smaller than the
/// window clamps the cap. The free tail is `spare - used` and is not a
/// second disk in this slice.
pub fn catalog_numbers(spare_bytes: u64, window_bytes: u64, installed: bool) -> CatalogNumbers {
    let placeable = if spare_bytes == 0 {
        window_bytes
    } else {
        window_bytes.min(spare_bytes)
    };
    let used = if installed { placeable } else { 0 };
    CatalogNumbers {
        spare_bytes,
        window_bytes,
        used_bytes: used,
        free_bytes: spare_bytes.saturating_sub(used),
        placeable_bytes: placeable,
    }
}

/// Guests policy. Does not touch the choice latch and does not format.
pub fn decide(installed: bool, placeable_bytes: u64, action: CatalogAction) -> CatalogDecision {
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
            if disk_bytes > placeable_bytes {
                CatalogDecision::Refuse(CatalogRefuse::LargerThanWindow {
                    disk_bytes,
                    placeable_bytes,
                })
            } else {
                CatalogDecision::InstallLinux { disk_bytes }
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

/// REST outcome. `Ready` carries a JSON body the HTTP layer writes as-is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GuestCatalogHttp {
    NotMine,
    Unauthorized,
    BadMethod,
    Ready {
        status: u16,
        len: usize,
        body: [u8; 576],
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
            let mut body = [0u8; 576];
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
    let numbers = catalog_numbers(spare, SLICE1_WINDOW_BYTES, installed);
    match decide(installed, numbers.placeable_bytes, action) {
        CatalogDecision::BootInstalled => store_choice(CHOICE_BOOT, installed),
        CatalogDecision::InstallLinux { .. } => store_choice(CHOICE_REINSTALL, installed),
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
    let mut buf = [0u8; 576];
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
    let n = catalog_numbers(spare, SLICE1_WINDOW_BYTES, installed);
    let choice = perc_spa_choice();
    let waiting = perc_spa_wait_required(installed, choice);
    let mut w = JsonBuf { b: buf, n: 0 };
    w.s("{\"spare_bytes\":");
    w.u(n.spare_bytes);
    w.s(",\"window_bytes\":");
    w.u(n.window_bytes);
    w.s(",\"used_bytes\":");
    w.u(n.used_bytes);
    w.s(",\"free_bytes\":");
    w.u(n.free_bytes);
    w.s(",\"placeable_bytes\":");
    w.u(n.placeable_bytes);
    w.s(",\"waiting\":");
    w.s(if waiting { "true" } else { "false" });
    w.s(",\"choice\":\"");
    w.s(choice_name(choice));
    w.s("\",\"guests\":[");
    if installed {
        let disk_mib = n.used_bytes / (1024 * 1024);
        w.s("{\"id\":1,\"name\":\"alpine\",\"state\":\"");
        w.s(guest_state(installed, choice));
        w.s("\",\"disk_mib\":");
        w.u(disk_mib);
        w.s(",\"image_type\":\"linux_iso\"}");
    }
    w.s("]}");
    w.n
}

fn choice_name(choice: PercSpaChoice) -> &'static str {
    match choice {
        PercSpaChoice::Pending => "pending",
        PercSpaChoice::BootInstalled => "boot",
        PercSpaChoice::CleanReinstall => "reinstall",
    }
}

fn guest_state(installed: bool, choice: PercSpaChoice) -> &'static str {
    match (installed, choice) {
        (true, PercSpaChoice::BootInstalled) => "booting",
        (true, PercSpaChoice::CleanReinstall) => "installing",
        (true, PercSpaChoice::Pending) => "installed",
        (false, _) => "absent",
    }
}

fn fill_window_json(buf: &mut [u8], disk_bytes: u64, placeable_bytes: u64) -> usize {
    let mut w = JsonBuf { b: buf, n: 0 };
    w.s("{\"ok\":false,\"reason\":\"window\",\"disk_bytes\":");
    w.u(disk_bytes);
    w.s(",\"placeable_bytes\":");
    w.u(placeable_bytes);
    w.s("}");
    w.n
}

struct JsonBuf<'a> {
    b: &'a mut [u8],
    n: usize,
}

impl JsonBuf<'_> {
    fn s(&mut self, t: &str) {
        let bytes = t.as_bytes();
        let room = self.b.len().saturating_sub(self.n);
        let take = bytes.len().min(room);
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
        && include_str!("../docs/adr/ADR-019.md").contains("Guests is the install and boot control")
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
        assert_eq!(n.placeable_bytes, SLICE1_WINDOW_BYTES);
        let empty = catalog_numbers(IRON_LD1_BYTES, SLICE1_WINDOW_BYTES, false);
        assert_eq!(empty.used_bytes, 0);
        assert_eq!(empty.free_bytes, IRON_LD1_BYTES);
        assert_eq!(empty.placeable_bytes, SLICE1_WINDOW_BYTES);
        let host = catalog_numbers(0, SLICE1_WINDOW_BYTES, false);
        assert_eq!(host.placeable_bytes, SLICE1_WINDOW_BYTES);
        assert_eq!(host.free_bytes, 0);
    }

    #[test]
    fn decide_boots_the_row_and_refuses_past_the_window() {
        let place = SLICE1_WINDOW_BYTES;
        assert_eq!(
            decide(true, place, CatalogAction::Start),
            CatalogDecision::BootInstalled
        );
        assert_eq!(
            decide(false, place, CatalogAction::Start),
            CatalogDecision::Refuse(CatalogRefuse::NoGuest)
        );
        assert_eq!(
            decide(true, place, CatalogAction::Windows { disk_mib: 8192 }),
            CatalogDecision::Refuse(CatalogRefuse::WindowsLater)
        );
        assert_eq!(
            decide(true, place, CatalogAction::Linux { disk_mib: 0 }),
            CatalogDecision::Refuse(CatalogRefuse::BadDisk)
        );
        assert_eq!(
            decide(true, place, CatalogAction::Linux { disk_mib: 8192 }),
            CatalogDecision::InstallLinux {
                disk_bytes: SLICE1_WINDOW_BYTES
            }
        );
        assert!(matches!(
            decide(true, place, CatalogAction::Linux { disk_mib: 10240 }),
            CatalogDecision::Refuse(CatalogRefuse::LargerThanWindow { .. })
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
