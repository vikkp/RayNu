//! ADR-021 D10. Host wall clock from one SNTP reply.
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** (ADR-002)
//!
//! The hypervisor has a TSC and no battery clock worth trusting. One UDP
//! query to a public NTP server sets a unix time. TLS puts that time in
//! the first four bytes of `server_random`. The next audit event prints
//! it once. An absent reply leaves the clock unset.
//!
//! The server is Cloudflare's anycast `162.159.200.123` (UDP/123). The
//! lab gateway is not assumed to be an NTP server.

use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// Seconds between the NTP epoch (1900-01-01) and the Unix epoch.
pub const NTP_UNIX_OFFSET: u64 = 2_208_988_800;

/// Cloudflare time (time.cloudflare.com), so a blank DNS field still has
/// an address to query.
pub const NTP_SERVER: [u8; 4] = [162, 159, 200, 123];

const UNIX_MIN: u64 = 1_700_000_000;
const UNIX_MAX: u64 = 2_000_000_000;

static SYNCED: AtomicBool = AtomicBool::new(false);
static UNIX_AT: AtomicU64 = AtomicU64::new(0);
static TSC_AT: AtomicU64 = AtomicU64::new(0);
static STAMPED: AtomicBool = AtomicBool::new(false);

/// Client request. LI=0, VN=4, mode=3. Forty-eight bytes.
pub fn encode_request(buf: &mut [u8; 48]) {
    buf.fill(0);
    buf[0] = 0x23;
}

/// Unix seconds from a server reply. `None` when the packet is short, not
/// a server reply, stratum 0, or the time is outside 2023–2033.
pub fn decode_reply(buf: &[u8]) -> Option<u64> {
    if buf.len() < 48 {
        return None;
    }
    let mode = buf[0] & 0x07;
    let stratum = buf[1];
    if mode != 4 || stratum == 0 || stratum > 15 {
        return None;
    }
    let ntp = u32::from_be_bytes(buf[40..44].try_into().ok()?) as u64;
    let unix = ntp.checked_sub(NTP_UNIX_OFFSET)?;
    if (UNIX_MIN..=UNIX_MAX).contains(&unix) {
        Some(unix)
    } else {
        None
    }
}

/// Remember `unix` as the time at this TSC sample.
pub fn note_sync(unix: u64) {
    if !(UNIX_MIN..=UNIX_MAX).contains(&unix) {
        return;
    }
    TSC_AT.store(crate::arch::cpu::rdtsc(), Ordering::Release);
    UNIX_AT.store(unix, Ordering::Release);
    SYNCED.store(true, Ordering::Release);
}

pub fn synced() -> bool {
    SYNCED.load(Ordering::Acquire)
}

/// Unix seconds now, advanced by the TSC since [`note_sync`].
pub fn unix_now() -> Option<u64> {
    if !synced() {
        return None;
    }
    let unix = UNIX_AT.load(Ordering::Acquire);
    let then = TSC_AT.load(Ordering::Acquire);
    let hz = crate::boot::raynu_f_flag::tsc_hz();
    let hz = if hz < 1_000_000 { 2_100_000_000 } else { hz };
    let elapsed = crate::arch::cpu::rdtsc().wrapping_sub(then) / hz;
    Some(unix.saturating_add(elapsed))
}

/// Write the synced unix time into the TLS `gmt_unix_time` slot.
/// Unsynced leaves `random` alone.
pub fn stamp_server_random(random: &mut [u8; 32]) {
    let Some(unix) = unix_now() else {
        return;
    };
    random[..4].copy_from_slice(&(unix as u32).to_be_bytes());
}

/// First audit event after a sync prints the unix time once.
pub fn audit_stamp_once() {
    if !synced() || STAMPED.swap(true, Ordering::AcqRel) {
        return;
    }
    #[cfg(all(target_os = "uefi", feature = "uefi-bin"))]
    {
        use crate::boot::serial;
        serial::write_str_nowait("boot: D10 audit unix=");
        let mut buf = [0u8; 20];
        let n = fmt_u64(unix_now().unwrap_or(0), &mut buf);
        serial::write_str_nowait(core::str::from_utf8(&buf[..n]).unwrap_or("0"));
        serial::write_line_nowait("");
    }
}

pub fn fmt_u64(mut v: u64, buf: &mut [u8; 20]) -> usize {
    if v == 0 {
        buf[0] = b'0';
        return 1;
    }
    let mut i = 20;
    while v > 0 {
        i -= 1;
        buf[i] = b'0' + (v % 10) as u8;
        v /= 10;
    }
    let n = 20 - i;
    buf.copy_within(i..20, 0);
    n
}

/// `{"ok":true,"synced":false}` or the unix time the page shows.
pub fn status_json(buf: &mut [u8]) -> usize {
    if unix_now().is_none() {
        let text = b"{\"ok\":true,\"synced\":false}";
        let n = text.len().min(buf.len());
        buf[..n].copy_from_slice(&text[..n]);
        return n;
    }
    let unix = unix_now().unwrap_or(0);
    let mut tmp = [0u8; 20];
    let digits = fmt_u64(unix, &mut tmp);
    let prefix = b"{\"ok\":true,\"synced\":true,\"unix\":";
    let mut n = 0;
    for b in prefix {
        if n >= buf.len() {
            return n;
        }
        buf[n] = *b;
        n += 1;
    }
    for b in &tmp[..digits] {
        if n >= buf.len() {
            return n;
        }
        buf[n] = *b;
        n += 1;
    }
    if n < buf.len() {
        buf[n] = b'}';
        n += 1;
    }
    n
}

/// Host package: the codec, the TLS slot, and the page are in tree.
/// Host tests do not send a UDP packet and do not mark the row lived.
pub fn prop_host_clock() -> bool {
    let listen = include_str!("host_nic_listen.rs");
    let tls = include_str!("tls12.rs");
    let html = include_str!("../assets/webui.html");
    let http = include_str!("http.rs");
    listen.contains("boot: D10 ntp ")
        && listen.contains("162.159.200.123")
        && listen.contains("d10_ntp_tick")
        && tls.contains("stamp_server_random")
        && html.contains("id=\"host-clock\"")
        && html.contains("/host/clock")
        && http.contains("/host/clock")
}

#[cfg(test)]
pub fn clear_for_test() {
    SYNCED.store(false, Ordering::Release);
    STAMPED.store(false, Ordering::Release);
    UNIX_AT.store(0, Ordering::Release);
    TSC_AT.store(0, Ordering::Release);
}

#[cfg(test)]
mod host_clock_test {
    use super::*;

    fn reply(ntp_secs: u32, mode: u8, stratum: u8) -> [u8; 48] {
        let mut buf = [0u8; 48];
        buf[0] = mode;
        buf[1] = stratum;
        buf[40..44].copy_from_slice(&ntp_secs.to_be_bytes());
        buf
    }

    #[test]
    fn a_server_reply_becomes_unix_time_and_stamps_tls() {
        let _ = clear_for_test();
        let mut req = [0u8; 48];
        encode_request(&mut req);
        assert_eq!(req[0], 0x23);
        assert!(decode_reply(&req).is_none());
        let unix = 1_793_404_800u64;
        let ntp = (unix + NTP_UNIX_OFFSET) as u32;
        assert_eq!(decode_reply(&reply(ntp, 4, 3)), Some(unix));
        assert!(decode_reply(&reply(ntp, 3, 3)).is_none(), "a request is not a reply");
        assert!(decode_reply(&reply(ntp, 4, 0)).is_none());
        assert!(decode_reply(&reply(100, 4, 1)).is_none());
        note_sync(unix);
        assert!(synced());
        let now = unix_now().unwrap();
        assert!(now >= unix && now < unix + 5, "now={now}");
        let mut random = [0x11u8; 32];
        stamp_server_random(&mut random);
        assert_eq!(&random[..4], &(now as u32).to_be_bytes());
        assert_eq!(random[4], 0x11);
        let mut json = [0u8; 64];
        let n = status_json(&mut json);
        let text = core::str::from_utf8(&json[..n]).unwrap();
        assert!(text.contains("\"synced\":true"), "{text}");
        assert!(prop_host_clock());
        clear_for_test();
        let mut random = [0x22u8; 32];
        stamp_server_random(&mut random);
        assert_eq!(random[0], 0x22);
    }
}
