//! M5.1 control plane: CLI + minimal REST over `VmTable`.
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** (ADR-002)
//! VERIFICATION: N/A
//!
//! No TCP stack and no HTTP crate — request/response shapes are host-testable
//! dispatch over the M5.0 lifecycle table. Auth is stubbed
//! (`GAP: REST auth → M6`).

use super::{LifecycleError, VmLifecycle, VmTable, MGMT_GUEST_CAP};

/// Host / CI marker when the M5.1 API gate passes.
pub const M5_API_OK_MARKER: &str = "RAYNU-V-M5-API-OK";

/// Documented auth gap: REST accepts any (or missing) token until M6.
pub const AUTH_GAP_NOTE: &str = "GAP: REST auth stubbed → M6";

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
    /// Present but ignored — stub accepts all (`GAP: REST auth → M6`).
    pub auth_token: Option<&'a str>,
}

/// REST status + reply body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RestResponse {
    pub status: u16,
    pub reply: Option<ApiReply>,
}

/// Auth stub: always allows. Documented GAP → M6.
#[inline]
pub fn auth_allows(_token: Option<&str>) -> bool {
    // GAP: REST auth stubbed → M6
    let _ = AUTH_GAP_NOTE;
    true
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
    if !auth_allows(req.auth_token) {
        return RestResponse {
            status: 401,
            reply: None,
        };
    }

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
            auth_token: None, // stub auth
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
            auth_token: Some("ignored"),
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
            auth_token: None,
        },
    );
    if del.status != 200 || del.reply != Some(ApiReply::Ok) {
        return false;
    }

    // CLI list empty
    match dispatch_cli(&mut t, CliCommand::List) {
        Ok(ApiReply::Listed { count: 0 }) => t.get(7).is_none() && AUTH_GAP_NOTE.contains("M6"),
        _ => false,
    }
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
