//! M7.1 minimal HTTP/1.1 mgmt plane codec (outside Proven Core).
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** (ADR-002 / ADR-009)
//! VERIFICATION: N/A
//!
//! Parses plaintext HTTP/1.1 into the existing REST shapes and serves the
//! embedded SPA. TLS is deferred (lab HTTP MVP — ADR-009). Firmware NIC/TCP
//! listen is stubbed until UEFI SNP/Tcp4 lands; host `cfg(test)` proves a
//! real TCP listener + browser-shaped exchange.

use super::api::{
    auth_allows, dispatch_rest, ApiReply, RestMethod, RestRequest, RestResponse, BRINGUP_AUTH_TOKEN,
};
use super::datastore::{dispatch_store_rest, ImageTable};
use super::webui::{load_webui, webui_raw_bytes};
use super::VmTable;

/// Host / CI marker when the M7.1 HTTP mgmt gate passes.
pub const M7_HTTP_OK_MARKER: &str = "RAYNU-V-M7-HTTP-OK";

/// Network HTTPS/HTTP mgmt GAP closed in M7.1.
pub const HTTP_GAP_NOTE: &str = "GAP(CLOSED M7.1): Network HTTPS/HTTP mgmt";

/// Lab note: plaintext HTTP allowed; TLS follows (size-boxed).
pub const HTTP_LAB_NOTE: &str =
    "lab MVP: plaintext HTTP (TLS deferred under ADR-003/ADR-009 size budget)";

/// Default lab bind (host tests / QEMU user-net docs).
pub const MGMT_HTTP_DEFAULT_PORT: u16 = 8443;

/// HTTP parse failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpParseError {
    Truncated,
    BadRequestLine,
    BadMethod,
    UnsupportedVersion,
}

/// One parsed HTTP/1.1 request (wire → REST / SPA).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParsedHttpRequest<'a> {
    pub method: RestMethod,
    pub path: &'a str,
    pub auth_token: Option<&'a str>,
    /// `GET /` or `GET /index.html` → serve SPA (no auth).
    pub is_spa: bool,
}

/// Extract `Bearer` token from an `Authorization` header value.
pub fn extract_bearer_token(authorization_value: &str) -> Option<&str> {
    let v = authorization_value.trim();
    let rest = v.strip_prefix("Bearer ").or_else(|| v.strip_prefix("bearer "))?;
    let tok = rest.trim();
    if tok.is_empty() {
        None
    } else {
        Some(tok)
    }
}

/// Scan raw header block for `Authorization:` and return Bearer token.
pub fn auth_token_from_headers(headers: &str) -> Option<&str> {
    for line in headers.lines() {
        let line = line.trim();
        if let Some(rest) = line
            .strip_prefix("Authorization:")
            .or_else(|| line.strip_prefix("authorization:"))
        {
            return extract_bearer_token(rest);
        }
    }
    None
}

/// Parse a complete HTTP/1.1 request (headers ended; body ignored for MVP).
pub fn parse_http_request(raw: &str) -> Result<ParsedHttpRequest<'_>, HttpParseError> {
    let raw = raw.trim_start_matches('\u{feff}');
    let (req_line, rest) = raw.split_once("\r\n").or_else(|| raw.split_once('\n')).ok_or(HttpParseError::Truncated)?;
    let mut parts = req_line.split_whitespace();
    let method_s = parts.next().ok_or(HttpParseError::BadRequestLine)?;
    let path = parts.next().ok_or(HttpParseError::BadRequestLine)?;
    let version = parts.next().ok_or(HttpParseError::BadRequestLine)?;
    if !version.starts_with("HTTP/1.") {
        return Err(HttpParseError::UnsupportedVersion);
    }
    let method = match method_s {
        "GET" => RestMethod::Get,
        "POST" => RestMethod::Post,
        "DELETE" => RestMethod::Delete,
        _ => return Err(HttpParseError::BadMethod),
    };
    let headers = rest.split("\r\n\r\n").next().unwrap_or(rest);
    let headers = headers.split("\n\n").next().unwrap_or(headers);
    let path_only = path.split('?').next().unwrap_or(path);
    let is_spa = matches!(method, RestMethod::Get) && (path_only == "/" || path_only == "/index.html");
    let auth_token = auth_token_from_headers(headers);
    Ok(ParsedHttpRequest {
        method,
        path: path_only,
        auth_token,
        is_spa,
    })
}

fn status_text(code: u16) -> &'static str {
    match code {
        200 => "OK",
        201 => "Created",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        500 => "Internal Server Error",
        _ => "OK",
    }
}

fn reply_body(reply: Option<ApiReply>) -> &'static str {
    match reply {
        Some(ApiReply::Ok) => "{\"ok\":true}",
        Some(ApiReply::Listed { .. }) => "{\"ok\":true,\"listed\":true}",
        Some(ApiReply::Record { .. }) => "{\"ok\":true,\"record\":true}",
        Some(ApiReply::Image { .. }) => "{\"ok\":true,\"image\":true}",
        None => "",
    }
}

/// Format a minimal HTTP/1.1 response into `out`. Returns bytes written.
pub fn format_http_response(
    status: u16,
    content_type: &str,
    body: &[u8],
    out: &mut [u8],
) -> Option<usize> {
    let reason = status_text(status);
    let header = HeaderBuf::new(status, reason, content_type, body.len());
    let need = header.len() + body.len();
    if out.len() < need {
        return None;
    }
    out[..header.len()].copy_from_slice(header.as_bytes());
    out[header.len()..need].copy_from_slice(body);
    Some(need)
}

/// Tiny fixed header builder (no alloc).
struct HeaderBuf {
    buf: [u8; 256],
    len: usize,
}

impl HeaderBuf {
    fn new(status: u16, reason: &str, content_type: &str, body_len: usize) -> Self {
        let mut h = Self {
            buf: [0u8; 256],
            len: 0,
        };
        h.push_str("HTTP/1.1 ");
        h.push_u16(status);
        h.push_str(" ");
        h.push_str(reason);
        h.push_str("\r\nContent-Type: ");
        h.push_str(content_type);
        h.push_str("\r\nContent-Length: ");
        h.push_usize(body_len);
        h.push_str("\r\nConnection: close\r\n\r\n");
        h
    }
    fn push_str(&mut self, s: &str) {
        let b = s.as_bytes();
        let n = b.len().min(self.buf.len().saturating_sub(self.len));
        self.buf[self.len..self.len + n].copy_from_slice(&b[..n]);
        self.len += n;
    }
    fn push_u16(&mut self, mut v: u16) {
        let mut tmp = [0u8; 5];
        let mut i = 5;
        if v == 0 {
            self.push_str("0");
            return;
        }
        while v > 0 && i > 0 {
            i -= 1;
            tmp[i] = b'0' + (v % 10) as u8;
            v /= 10;
        }
        let s = core::str::from_utf8(&tmp[i..]).unwrap_or("0");
        self.push_str(s);
    }
    fn push_usize(&mut self, mut v: usize) {
        let mut tmp = [0u8; 20];
        let mut i = 20;
        if v == 0 {
            self.push_str("0");
            return;
        }
        while v > 0 && i > 0 {
            i -= 1;
            tmp[i] = b'0' + (v % 10) as u8;
            v /= 10;
        }
        let s = core::str::from_utf8(&tmp[i..]).unwrap_or("0");
        self.push_str(s);
    }
    fn as_bytes(&self) -> &[u8] {
        &self.buf[..self.len]
    }
    fn len(&self) -> usize {
        self.len
    }
}

/// Handle one HTTP exchange against `VmTable` + image library (SPA or REST).
/// Writes the response into `out`; returns `Some(n)` bytes written.
pub fn handle_http_request(
    table: &mut VmTable,
    images: &mut ImageTable,
    raw: &str,
    out: &mut [u8],
) -> Option<usize> {
    let _ = (HTTP_GAP_NOTE, M7_HTTP_OK_MARKER, HTTP_LAB_NOTE);
    let parsed = match parse_http_request(raw) {
        Ok(p) => p,
        Err(_) => {
            return format_http_response(400, "text/plain; charset=utf-8", b"bad request", out);
        }
    };
    if parsed.is_spa {
        let _ = load_webui();
        let body = webui_raw_bytes();
        return format_http_response(200, "text/html; charset=utf-8", body, out);
    }
    let req = RestRequest {
        method: parsed.method,
        path: parsed.path,
        auth_token: parsed.auth_token,
    };
    let resp: RestResponse = if parsed.path == "/images" || parsed.path.starts_with("/images/") {
        dispatch_store_rest(images, req)
    } else {
        dispatch_rest(table, req)
    };
    let body = reply_body(resp.reply);
    let ctype = if body.is_empty() {
        "text/plain; charset=utf-8"
    } else {
        "application/json"
    };
    format_http_response(resp.status, ctype, body.as_bytes(), out)
}

/// True when HTTP package props hold (codec + auth wire + SPA + gap).
pub fn prop_http_mgmt_package() -> bool {
    let _ = BRINGUP_AUTH_TOKEN;
    let raw = "GET / HTTP/1.1\r\nHost: localhost\r\n\r\n";
    let p = parse_http_request(raw).expect("spa get");
    if !p.is_spa {
        return false;
    }
    let raw_rest = "GET /vms HTTP/1.1\r\nAuthorization: Bearer raynu-v-bringup\r\n\r\n";
    let p2 = parse_http_request(raw_rest).expect("rest get");
    if p2.is_spa || p2.auth_token != Some(BRINGUP_AUTH_TOKEN) {
        return false;
    }
    if extract_bearer_token("Bearer raynu-v-bringup") != Some(BRINGUP_AUTH_TOKEN) {
        return false;
    }
    if !auth_allows(Some(BRINGUP_AUTH_TOKEN)) {
        return false;
    }
    let mut table = VmTable::new();
    let mut images = ImageTable::new();
    let mut out = [0u8; 8192];
    let n = handle_http_request(&mut table, &mut images, raw, &mut out).unwrap_or(0);
    if n == 0 {
        return false;
    }
    let s = core::str::from_utf8(&out[..n]).unwrap_or("");
    s.contains("HTTP/1.1 200")
        && s.contains("text/html")
        && HTTP_GAP_NOTE.contains("CLOSED M7.1")
        && M7_HTTP_OK_MARKER == "RAYNU-V-M7-HTTP-OK"
        && HTTP_LAB_NOTE.contains("plaintext HTTP")
}

#[cfg(test)]
#[path = "http_test.rs"]
mod http_test;
