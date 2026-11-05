//! One GET for the Overview poll (ADR-021 wave 5, E4 listener load).
//!
//! Pillar: [Z]
//! Proven Core: **outside** (ADR-002)
//!
//! The page used to poll `/perc/guests`, `/host/addr`, `/host/clock`,
//! `/auth/operator`, and `/host/cert` every 2 s. Firefox answered that
//! with its whole pool of six connections to one origin, and a reload
//! opened more while the old ones were still closing. The coexist
//! listener has ten sockets and no accept queue (smoltcp), so iron
//! `1da151be` reset the reload's SYN and `1cf82795` aborted a live
//! session to make room. One request per tick keeps the browser on one
//! or two connections. The five single routes stay for the action
//! buttons; this one is only the poll.

/// `GET` with a bearer. The five objects, verbatim, under one root.
pub const HOST_OVERVIEW_PATH: &str = "/host/overview";

/// Guests list (2048) plus address (512), operator (256), certificate
/// (128), clock (64), and the framing.
pub const OVERVIEW_BODY_CAP: usize = 3_200;

const KEYS: [&str; 5] = [
    "{\"ok\":true,\"guests\":",
    ",\"addr\":",
    ",\"clock\":",
    ",\"operator\":",
    ",\"cert\":",
];

/// Write the Overview object. Zero when any part did not fit, so a
/// truncated object is never sent.
pub fn overview_json(buf: &mut [u8]) -> usize {
    let mut n = 0usize;
    for (i, key) in KEYS.iter().enumerate() {
        let Some(k) = put(buf, n, key.as_bytes()) else {
            return 0;
        };
        n = k;
        let part = match i {
            0 => crate::mgmt::guest_catalog::list_json(&mut buf[n..]),
            1 => crate::mgmt::host_addr::status_body(&mut buf[n..]),
            2 => clock_part(&mut buf[n..]),
            3 => crate::mgmt::operator::status_body(&mut buf[n..], None),
            _ => crate::mgmt::host_cert::status_body(&mut buf[n..]),
        };
        if part == 0 {
            return 0;
        }
        n += part;
    }
    match put(buf, n, b"}") {
        Some(k) => k,
        None => 0,
    }
}

/// `host_clock::status_json` writes what fits and reports the count, so
/// a short buffer is detected here by the closing brace.
fn clock_part(buf: &mut [u8]) -> usize {
    let n = crate::mgmt::host_clock::status_json(buf);
    if n == 0 || buf[n - 1] != b'}' {
        return 0;
    }
    n
}

fn put(buf: &mut [u8], at: usize, bytes: &[u8]) -> Option<usize> {
    let end = at.checked_add(bytes.len())?;
    if end > buf.len() {
        return None;
    }
    buf[at..end].copy_from_slice(bytes);
    Some(end)
}

/// Host package: the route, the page's single poll, and the five parts
/// under one root. Host tests do not mark the row lived.
pub fn prop_host_overview() -> bool {
    let http = include_str!("http.rs");
    let html = include_str!("../assets/webui.html");
    let mut buf = [0u8; OVERVIEW_BODY_CAP];
    let n = overview_json(&mut buf);
    let body = core::str::from_utf8(&buf[..n]).unwrap_or("");
    n > 0
        && body.starts_with("{\"ok\":true,\"guests\":{")
        && body.contains(",\"addr\":{")
        && body.contains(",\"clock\":{")
        && body.contains(",\"operator\":{")
        && body.contains(",\"cert\":{")
        && body.ends_with("}}")
        && http.contains("host_overview::overview_json")
        && html.contains(HOST_OVERVIEW_PATH)
        && html.contains("function loadOverview")
        && html.contains("setInterval(loadOverview,2000)")
        && html.contains("if(r.status===401){if(!hold)setS(\"Sign in\");loadOp();return}")
        && !html.contains("setInterval(loadSpare")
        && !html.contains("setInterval(loadHostAddr")
        && !html.contains("setInterval(loadClock")
        && !html.contains("setInterval(loadOp")
        && !html.contains("setInterval(loadCert")
}

#[cfg(test)]
#[path = "host_overview_test.rs"]
mod host_overview_test;
