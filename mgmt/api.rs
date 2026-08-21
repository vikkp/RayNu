//! M5.1 / M6.4 control plane: CLI + minimal REST over `VmTable`.
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** (ADR-002)
//! VERIFICATION: N/A
//!
//! Request/response shapes are host-testable dispatch over the M5.0 lifecycle
//! table. M6.4 closes REST auth with a bring-up mock token (`RAYNU-V-M6-AUTH-OK`).
//! M7.1 adds a minimal HTTP/1.1 wire codec + host TCP proof (`mgmt/http.rs`) —
//! still no HTTP crate; firmware NIC listen is stubbed until UEFI Tcp4/SNP.

use crate::audit_log;
use crate::audit::AuditEvent;

use super::{LifecycleError, VmLifecycle, VmSpec, VmTable, MGMT_GUEST_CAP};

/// Host / CI marker when the M5.1 API gate passes.
pub const M5_API_OK_MARKER: &str = "RAYNU-V-M5-API-OK";

/// Host / CI marker when the M6.4 REST auth gate passes.
pub const M6_AUTH_OK_MARKER: &str = "RAYNU-V-M6-AUTH-OK";

/// Closed auth GAP (was open stub through M5.1; closed in M6.4).
pub const AUTH_GAP_NOTE: &str = "GAP(CLOSED M6.4): REST auth stubbed → M6";

/// Bring-up mock REST token (documented; lab fallback when no ESP operator token).
pub const BRINGUP_AUTH_TOKEN: &str = "raynu-v-bringup";

/// Token source note for operators / CI.
pub const AUTH_TOKEN_SOURCE_NOTE: &str =
    "bring-up mock OR ESP EFI/RayNu/auth.token (Cruzer; E4 operator); BRINGUP_AUTH_TOKEN lab fallback";

/// Max UTF-8 bytes for an ESP / operator token.
pub const OPERATOR_TOKEN_CAP: usize = 64;

static mut OPERATOR_TOKEN: [u8; OPERATOR_TOKEN_CAP] = [0; OPERATOR_TOKEN_CAP];
static mut OPERATOR_TOKEN_LEN: usize = 0;

/// True when an operator token was armed (ESP or host test).
pub fn operator_token_armed() -> bool {
    unsafe { OPERATOR_TOKEN_LEN > 0 }
}

/// Clear operator token (tests / lab reset) — bring-up token allowed again.
pub fn clear_operator_token() {
    unsafe {
        OPERATOR_TOKEN = [0; OPERATOR_TOKEN_CAP];
        OPERATOR_TOKEN_LEN = 0;
    }
}

/// Arm an operator token (ESP `auth.token` contents). Empty / oversized → Err.
pub fn set_operator_token(bytes: &[u8]) -> Result<(), ()> {
    let t = core::str::from_utf8(bytes).map_err(|_| ())?;
    let t = t.trim();
    if t.is_empty() || t.len() > OPERATOR_TOKEN_CAP {
        return Err(());
    }
    if !t.bytes().all(|b| b.is_ascii_graphic()) {
        return Err(());
    }
    unsafe {
        OPERATOR_TOKEN[..t.len()].copy_from_slice(t.as_bytes());
        OPERATOR_TOKEN_LEN = t.len();
    }
    Ok(())
}

fn operator_token_str() -> Option<&'static str> {
    unsafe {
        let len = OPERATOR_TOKEN_LEN;
        if len == 0 || len > OPERATOR_TOKEN_CAP {
            None
        } else {
            core::str::from_utf8(&OPERATOR_TOKEN[..len]).ok()
        }
    }
}

/// Probe ESP for `auth.token` (E4 operator secret). PRE-EBS only.
///
/// Paths: `\\EFI\\RayNu\\auth.token` (Cruzer, next to `installdisk.bin`)
/// then `\\auth.token`.
#[cfg(target_os = "uefi")]
pub fn probe_operator_auth_token() {
    use crate::boot::serial;
    use uefi::boot;
    use uefi::fs::FileSystem;

    let image = boot::image_handle();
    let Ok(sfs) = boot::get_image_file_system(image) else {
        serial::write_line("boot: E4 auth: bring-up token (lab; no ESP FS)");
        return;
    };
    let mut fs = FileSystem::new(sfs);
    let loaded = load_token(&mut fs, "\\EFI\\RayNu\\auth.token")
        .or_else(|_| load_token(&mut fs, "\\auth.token"));
    match loaded {
        Ok(()) => serial::write_line("boot: E4 auth: ESP auth.token armed (bring-up disabled)"),
        Err(()) => serial::write_line("boot: E4 auth: bring-up token (lab; no auth.token)"),
    }
}

#[cfg(target_os = "uefi")]
fn load_token(fs: &mut uefi::fs::FileSystem, path: &str) -> Result<(), ()> {
    use uefi::CString16;
    let Ok(p) = CString16::try_from(path) else {
        return Err(());
    };
    let Ok(data) = fs.read(p.as_ref()) else {
        return Err(());
    };
    set_operator_token(&data)
}

#[cfg(not(target_os = "uefi"))]
pub fn probe_operator_auth_token() {}

/// CLI verb over the management plane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CliCommand {
    Create { guest_id: u64 },
    Start { guest_id: u64 },
    Stop { guest_id: u64 },
    Destroy { guest_id: u64 },
    List,
}

/// CLI / REST parse error (distinct from lifecycle transition errors).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiParseError {
    Empty,
    UnknownVerb,
    MissingGuestId,
    BadGuestId,
    BadMethod,
    BadPath,
}

/// Successful control-plane reply (CLI or REST body shape).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiReply {
    Ok,
    Listed { count: usize },
    Record {
        guest_id: u64,
        state: VmLifecycle,
        cpu: u8,
        ram_mib: u32,
        disk_mib: u32,
        iso_id: u64,
    },
    /// M7.2 image library record (`/images/{id}`).
    Image {
        id: u64,
        kind_tag: u8,
        size_bytes: u64,
    },
}

/// HTTP method subset used by the REST control plane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestMethod {
    Get,
    Post,
    Delete,
}

/// One REST request (path + optional auth token).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RestRequest<'a> {
    pub method: RestMethod,
    pub path: &'a str,
    /// Required for REST: Bearer token (bring-up mock or ESP operator token).
    pub auth_token: Option<&'a str>,
}

/// REST status + reply body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RestResponse {
    pub status: u16,
    pub reply: Option<ApiReply>,
}

/// Method tag for audit AuthAllowed / AuthDenied (stable small ints).
fn rest_method_tag(m: RestMethod) -> u8 {
    match m {
        RestMethod::Get => 1,
        RestMethod::Post => 2,
        RestMethod::Delete => 3,
    }
}

/// M6.4 / E4: allow bring-up mock **unless** an operator token is armed, then
/// only the operator token (honest step beyond the toy for iron).
#[inline]
pub fn auth_allows(token: Option<&str>) -> bool {
    let _ = AUTH_GAP_NOTE;
    let _ = AUTH_TOKEN_SOURCE_NOTE;
    let _ = M6_AUTH_OK_MARKER;
    let Some(t) = token else {
        return false;
    };
    if let Some(op) = operator_token_str() {
        return t == op;
    }
    t == BRINGUP_AUTH_TOKEN
}

/// Parse a single CLI line: `create|start|stop|destroy <id>` or `list`.
pub fn parse_cli(line: &str) -> Result<CliCommand, ApiParseError> {
    let line = line.trim();
    if line.is_empty() {
        return Err(ApiParseError::Empty);
    }
    let mut parts = line.split_whitespace();
    let verb = parts.next().ok_or(ApiParseError::Empty)?;
    match verb {
        "list" => {
            if parts.next().is_some() {
                return Err(ApiParseError::UnknownVerb);
            }
            Ok(CliCommand::List)
        }
        "create" | "start" | "stop" | "destroy" => {
            let id_s = parts.next().ok_or(ApiParseError::MissingGuestId)?;
            if parts.next().is_some() {
                return Err(ApiParseError::UnknownVerb);
            }
            let guest_id = parse_u64(id_s).ok_or(ApiParseError::BadGuestId)?;
            Ok(match verb {
                "create" => CliCommand::Create { guest_id },
                "start" => CliCommand::Start { guest_id },
                "stop" => CliCommand::Stop { guest_id },
                _ => CliCommand::Destroy { guest_id },
            })
        }
        _ => Err(ApiParseError::UnknownVerb),
    }
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

/// Dispatch a parsed CLI command against the VM table.
pub fn dispatch_cli(table: &mut VmTable, cmd: CliCommand) -> Result<ApiReply, LifecycleError> {
    match cmd {
        CliCommand::Create { guest_id } => {
            table.create(guest_id)?;
            Ok(ApiReply::Ok)
        }
        CliCommand::Start { guest_id } => {
            table.start(guest_id)?;
            Ok(ApiReply::Ok)
        }
        CliCommand::Stop { guest_id } => {
            table.stop(guest_id)?;
            Ok(ApiReply::Ok)
        }
        CliCommand::Destroy { guest_id } => {
            table.destroy(guest_id)?;
            Ok(ApiReply::Ok)
        }
        CliCommand::List => {
            let mut buf = [None; MGMT_GUEST_CAP];
            let count = table.list(&mut buf);
            Ok(ApiReply::Listed { count })
        }
    }
}

/// Parse REST method token (`GET` / `POST` / `DELETE`).
pub fn parse_rest_method(s: &str) -> Result<RestMethod, ApiParseError> {
    match s.trim() {
        "GET" | "get" => Ok(RestMethod::Get),
        "POST" | "post" => Ok(RestMethod::Post),
        "DELETE" | "delete" => Ok(RestMethod::Delete),
        _ => Err(ApiParseError::BadMethod),
    }
}

/// Map REST request → CLI-equivalent op, then run it.
///
/// Routes:
/// - `GET  /vms`              → list
/// - `GET  /vms/{id}`         → get one
/// - `POST /vms/{id}`         → create (default spec)
/// - `POST /vms/{id}/spec/{cpu}/{ram_mib}/{disk_mib}/{iso_id}` → create with fields (M7.4)
/// - `POST /vms/{id}/start`   → start
/// - `POST /vms/{id}/stop`    → stop
/// - `DELETE /vms/{id}`       → destroy
pub fn dispatch_rest(table: &mut VmTable, req: RestRequest<'_>) -> RestResponse {
    let tag = rest_method_tag(req.method);
    if !auth_allows(req.auth_token) {
        audit_log!(AuditEvent::AuthDenied { method_tag: tag });
        return RestResponse {
            status: 401,
            reply: None,
        };
    }
    audit_log!(AuditEvent::AuthAllowed { method_tag: tag });

    match route_rest(req.method, req.path) {
        Ok(RestOp::List) => match dispatch_cli(table, CliCommand::List) {
            Ok(reply) => RestResponse {
                status: 200,
                reply: Some(reply),
            },
            Err(_) => RestResponse {
                status: 500,
                reply: None,
            },
        },
        Ok(RestOp::Get { guest_id }) => match table.get(guest_id) {
            Some(r) => RestResponse {
                status: 200,
                reply: Some(ApiReply::Record {
                    guest_id: r.guest_id,
                    state: r.state,
                    cpu: r.cpu,
                    ram_mib: r.ram_mib,
                    disk_mib: r.disk_mib,
                    iso_id: r.iso_id,
                }),
            },
            None => RestResponse {
                status: 404,
                reply: None,
            },
        },
        Ok(RestOp::Create { guest_id }) => {
            rest_lifecycle(table, CliCommand::Create { guest_id }, 201)
        }
        Ok(RestOp::CreateSpec { guest_id, spec }) => match table.create_with_spec(guest_id, spec) {
            Ok(()) => RestResponse {
                status: 201,
                reply: Some(ApiReply::Ok),
            },
            Err(LifecycleError::NotFound) => RestResponse {
                status: 404,
                reply: None,
            },
            Err(LifecycleError::BadState) | Err(LifecycleError::InvalidGuest) => RestResponse {
                status: 409,
                reply: None,
            },
            Err(LifecycleError::Full) => RestResponse {
                status: 507,
                reply: None,
            },
        },
        Ok(RestOp::Start { guest_id }) => {
            let resp = rest_lifecycle(table, CliCommand::Start { guest_id }, 200);
            if resp.status == 200 {
                // Queue only. VMLAUNCH runs on the next coexist scheduler quantum.
                super::spa_launch::note_spa_start(guest_id);
            }
            resp
        }
        Ok(RestOp::Stop { guest_id }) => {
            let resp = rest_lifecycle(table, CliCommand::Stop { guest_id }, 200);
            if resp.status == 200 {
                super::spa_launch::note_spa_stop(guest_id);
            }
            resp
        }
        Ok(RestOp::Destroy { guest_id }) => {
            rest_lifecycle(table, CliCommand::Destroy { guest_id }, 200)
        }
        Err(ApiParseError::BadPath) | Err(ApiParseError::BadGuestId) => RestResponse {
            status: 400,
            reply: None,
        },
        Err(_) => RestResponse {
            status: 400,
            reply: None,
        },
    }
}

enum RestOp {
    List,
    Get { guest_id: u64 },
    Create { guest_id: u64 },
    CreateSpec { guest_id: u64, spec: VmSpec },
    Start { guest_id: u64 },
    Stop { guest_id: u64 },
    Destroy { guest_id: u64 },
}

fn rest_lifecycle(table: &mut VmTable, cmd: CliCommand, ok_status: u16) -> RestResponse {
    match dispatch_cli(table, cmd) {
        Ok(reply) => RestResponse {
            status: ok_status,
            reply: Some(reply),
        },
        Err(LifecycleError::NotFound) => RestResponse {
            status: 404,
            reply: None,
        },
        Err(LifecycleError::BadState) | Err(LifecycleError::InvalidGuest) => RestResponse {
            status: 409,
            reply: None,
        },
        Err(LifecycleError::Full) => RestResponse {
            status: 507,
            reply: None,
        },
    }
}

fn route_rest(method: RestMethod, path: &str) -> Result<RestOp, ApiParseError> {
    let path = path.trim().trim_end_matches('/');
    if path == "/vms" {
        return match method {
            RestMethod::Get => Ok(RestOp::List),
            _ => Err(ApiParseError::BadPath),
        };
    }
    // /vms/{id} or /vms/{id}/start|stop or /vms/{id}/spec/{cpu}/{ram}/{disk}/{iso}
    let rest = path.strip_prefix("/vms/").ok_or(ApiParseError::BadPath)?;
    let mut segs = rest.split('/');
    let id_s = segs.next().ok_or(ApiParseError::BadPath)?;
    let guest_id = parse_u64(id_s).ok_or(ApiParseError::BadGuestId)?;
    let action = segs.next();
    match (method, action) {
        (RestMethod::Get, None) => {
            if segs.next().is_some() {
                return Err(ApiParseError::BadPath);
            }
            Ok(RestOp::Get { guest_id })
        }
        (RestMethod::Post, None) => {
            if segs.next().is_some() {
                return Err(ApiParseError::BadPath);
            }
            Ok(RestOp::Create { guest_id })
        }
        (RestMethod::Post, Some("start")) => {
            if segs.next().is_some() {
                return Err(ApiParseError::BadPath);
            }
            Ok(RestOp::Start { guest_id })
        }
        (RestMethod::Post, Some("stop")) => {
            if segs.next().is_some() {
                return Err(ApiParseError::BadPath);
            }
            Ok(RestOp::Stop { guest_id })
        }
        (RestMethod::Post, Some("spec")) => {
            let cpu = parse_u64(segs.next().ok_or(ApiParseError::BadPath)?)
                .ok_or(ApiParseError::BadPath)?;
            let ram = parse_u64(segs.next().ok_or(ApiParseError::BadPath)?)
                .ok_or(ApiParseError::BadPath)?;
            let disk = parse_u64(segs.next().ok_or(ApiParseError::BadPath)?)
                .ok_or(ApiParseError::BadPath)?;
            let iso = parse_u64(segs.next().ok_or(ApiParseError::BadPath)?)
                .ok_or(ApiParseError::BadPath)?;
            if segs.next().is_some() {
                return Err(ApiParseError::BadPath);
            }
            if cpu > 64 || ram > u64::from(u32::MAX) || disk > u64::from(u32::MAX) {
                return Err(ApiParseError::BadPath);
            }
            Ok(RestOp::CreateSpec {
                guest_id,
                spec: VmSpec {
                    cpu: cpu as u8,
                    ram_mib: ram as u32,
                    disk_mib: disk as u32,
                    iso_id: iso,
                },
            })
        }
        (RestMethod::Delete, None) => {
            if segs.next().is_some() {
                return Err(ApiParseError::BadPath);
            }
            Ok(RestOp::Destroy { guest_id })
        }
        _ => Err(ApiParseError::BadPath),
    }
}

/// Host-testable CLI + REST round-trip over one guest.
pub fn prop_cli_rest_roundtrip() -> bool {
    let mut t = VmTable::new();
    let tok = Some(BRINGUP_AUTH_TOKEN);

    // CLI create → start → stop
    let create = match parse_cli("create 7") {
        Ok(c) => c,
        Err(_) => return false,
    };
    if dispatch_cli(&mut t, create) != Ok(ApiReply::Ok) {
        return false;
    }
    if dispatch_cli(&mut t, CliCommand::Start { guest_id: 7 }) != Ok(ApiReply::Ok) {
        return false;
    }
    if dispatch_cli(&mut t, CliCommand::Stop { guest_id: 7 }) != Ok(ApiReply::Ok) {
        return false;
    }

    // REST list sees one guest
    let list = dispatch_rest(
        &mut t,
        RestRequest {
            method: RestMethod::Get,
            path: "/vms",
            auth_token: tok,
        },
    );
    if list.status != 200 || list.reply != Some(ApiReply::Listed { count: 1 }) {
        return false;
    }

    // REST get
    let get = dispatch_rest(
        &mut t,
        RestRequest {
            method: RestMethod::Get,
            path: "/vms/7",
            auth_token: tok,
        },
    );
    if get.status != 200
        || get.reply
            != Some(ApiReply::Record {
                guest_id: 7,
                state: VmLifecycle::Stopped,
                cpu: 1,
                ram_mib: 512,
                disk_mib: 1024,
                iso_id: 0,
            })
    {
        return false;
    }

    // REST destroy
    let del = dispatch_rest(
        &mut t,
        RestRequest {
            method: RestMethod::Delete,
            path: "/vms/7",
            auth_token: tok,
        },
    );
    if del.status != 200 || del.reply != Some(ApiReply::Ok) {
        return false;
    }

    // CLI list empty
    match dispatch_cli(&mut t, CliCommand::List) {
        Ok(ApiReply::Listed { count: 0 }) => {
            t.get(7).is_none() && AUTH_GAP_NOTE.contains("CLOSED M6.4")
        }
        _ => false,
    }
}

/// M6.4 / E4: missing/wrong token → 401; bring-up OK when no operator token;
/// operator token overrides bring-up when armed.
pub fn prop_auth_deny_allow() -> bool {
    clear_operator_token();
    let mut t = VmTable::new();

    let denied_none = dispatch_rest(
        &mut t,
        RestRequest {
            method: RestMethod::Post,
            path: "/vms/11",
            auth_token: None,
        },
    );
    if denied_none.status != 401 || t.get(11).is_some() {
        return false;
    }

    let denied_bad = dispatch_rest(
        &mut t,
        RestRequest {
            method: RestMethod::Post,
            path: "/vms/11",
            auth_token: Some("wrong-token"),
        },
    );
    if denied_bad.status != 401 || t.get(11).is_some() {
        return false;
    }

    let allowed = dispatch_rest(
        &mut t,
        RestRequest {
            method: RestMethod::Post,
            path: "/vms/11",
            auth_token: Some(BRINGUP_AUTH_TOKEN),
        },
    );
    if allowed.status != 201 || t.get(11).map(|r| r.state) != Some(VmLifecycle::Defined) {
        return false;
    }

    if set_operator_token(b"iron-e4-secret").is_err() {
        return false;
    }
    if auth_allows(Some(BRINGUP_AUTH_TOKEN)) {
        clear_operator_token();
        return false;
    }
    if !auth_allows(Some("iron-e4-secret")) {
        clear_operator_token();
        return false;
    }
    clear_operator_token();

    !auth_allows(None)
        && !auth_allows(Some("anything"))
        && auth_allows(Some(BRINGUP_AUTH_TOKEN))
        && AUTH_GAP_NOTE.contains("CLOSED M6.4")
        && AUTH_TOKEN_SOURCE_NOTE.contains("BRINGUP_AUTH_TOKEN")
        && AUTH_TOKEN_SOURCE_NOTE.contains("auth.token")
        && M6_AUTH_OK_MARKER == "RAYNU-V-M6-AUTH-OK"
}

/// True when CLI verbs parse as documented.
pub fn prop_cli_verbs_parse() -> bool {
    matches!(
        parse_cli("create 1"),
        Ok(CliCommand::Create { guest_id: 1 })
    ) && matches!(parse_cli("start 2"), Ok(CliCommand::Start { guest_id: 2 }))
        && matches!(parse_cli("stop 3"), Ok(CliCommand::Stop { guest_id: 3 }))
        && matches!(
            parse_cli("destroy 4"),
            Ok(CliCommand::Destroy { guest_id: 4 })
        )
        && matches!(parse_cli("list"), Ok(CliCommand::List))
        && parse_cli("nope").is_err()
}

#[cfg(test)]
#[path = "api_test.rs"]
mod api_test;
