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

use super::{LifecycleError, VmLifecycle, VmTable, MGMT_GUEST_CAP};

/// Host / CI marker when the M5.1 API gate passes.
pub const M5_API_OK_MARKER: &str = "RAYNU-V-M5-API-OK";

/// Host / CI marker when the M6.4 REST auth gate passes.
pub const M6_AUTH_OK_MARKER: &str = "RAYNU-V-M6-AUTH-OK";

/// Closed auth GAP (was open stub through M5.1; closed in M6.4).
pub const AUTH_GAP_NOTE: &str = "GAP(CLOSED M6.4): REST auth stubbed → M6";

/// Bring-up mock REST token (documented; replace for production).
pub const BRINGUP_AUTH_TOKEN: &str = "raynu-v-bringup";

/// Token source note for operators / CI.
pub const AUTH_TOKEN_SOURCE_NOTE: &str =
    "bring-up mock: BRINGUP_AUTH_TOKEN (M6.4; replace for production)";

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
    Record { guest_id: u64, state: VmLifecycle },
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
    /// Required for M6.4: must equal [`BRINGUP_AUTH_TOKEN`] (bring-up mock).
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

/// M6.4: allow only the documented bring-up mock token.
#[inline]
pub fn auth_allows(token: Option<&str>) -> bool {
    let _ = AUTH_GAP_NOTE;
    let _ = AUTH_TOKEN_SOURCE_NOTE;
    let _ = M6_AUTH_OK_MARKER;
    matches!(token, Some(t) if t == BRINGUP_AUTH_TOKEN)
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
/// - `POST /vms/{id}`         → create
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
        Ok(RestOp::Start { guest_id }) => {
            rest_lifecycle(table, CliCommand::Start { guest_id }, 200)
        }
        Ok(RestOp::Stop { guest_id }) => rest_lifecycle(table, CliCommand::Stop { guest_id }, 200),
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
    // /vms/{id} or /vms/{id}/start|stop
    let rest = path.strip_prefix("/vms/").ok_or(ApiParseError::BadPath)?;
    let mut segs = rest.split('/');
    let id_s = segs.next().ok_or(ApiParseError::BadPath)?;
    let guest_id = parse_u64(id_s).ok_or(ApiParseError::BadGuestId)?;
    let action = segs.next();
    if segs.next().is_some() {
        return Err(ApiParseError::BadPath);
    }
    match (method, action) {
        (RestMethod::Get, None) => Ok(RestOp::Get { guest_id }),
        (RestMethod::Post, None) => Ok(RestOp::Create { guest_id }),
        (RestMethod::Post, Some("start")) => Ok(RestOp::Start { guest_id }),
        (RestMethod::Post, Some("stop")) => Ok(RestOp::Stop { guest_id }),
        (RestMethod::Delete, None) => Ok(RestOp::Destroy { guest_id }),
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

/// M6.4: missing/wrong token → 401 and no mutation; good token → create OK.
pub fn prop_auth_deny_allow() -> bool {
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

    !auth_allows(None)
        && !auth_allows(Some("anything"))
        && auth_allows(Some(BRINGUP_AUTH_TOKEN))
        && AUTH_GAP_NOTE.contains("CLOSED M6.4")
        && AUTH_TOKEN_SOURCE_NOTE.contains("BRINGUP_AUTH_TOKEN")
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
