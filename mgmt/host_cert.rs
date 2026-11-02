//! ADR-021 E4. An operator certificate for the standing page.
//!
//! Pillar: [A]
//! Proven Core: **outside** (ADR-002)
//!
//! Sixteen sectors, immediately before the operator-account sector.
//! Never LBA 0. Never UBUNTU0. The image holds one DER certificate and
//! one PKCS#8 RSA-2048 key. The key is not printed.
//!
//! Until that image exists, the standing listener keeps the lab
//! millicert. After Save, the next handshake presents the saved
//! certificate. This connection stays on the certificate it already
//! used. The listener is still TLS 1.2. TLS 1.3 is the open half of
//! the row.
//!
//! Host tests do not print an iron marker and do not mark the row lived.

use crate::mgmt::api::RestMethod;
use crate::mgmt::megaraid::{classify_ld_bytes, LdClass};
use rsa::pkcs8::DecodePrivateKey;
use rsa::traits::PublicKeyParts;
use rsa::RsaPrivateKey;

pub const MAGIC: &[u8; 8] = b"RAYNUTLS";
const VERSION: u8 = 1;
pub const CERT_MAX: usize = 1536;
pub const KEY_MAX: usize = 1400;
pub const WINDOW: u64 = 16;
const IMAGE: usize = 16 * 512;
const OFF_CERT: usize = 10;
const OFF_KEY: usize = 12;
const OFF_SUM: usize = 16;
const OFF_BODY: usize = 32;
const RSA2048: usize = 256;

/// LBA of the first certificate sector. Sixteen sectors before the
/// operator account. `None` for anything that is not the spare, and
/// never LBA 0.
pub fn cert_lba(spare_bytes: u64) -> Option<u64> {
    if classify_ld_bytes(spare_bytes) != LdClass::Spare {
        return None;
    }
    let op = crate::mgmt::operator::operator_lba(spare_bytes)?;
    let lba = op.checked_sub(WINDOW)?;
    if lba == 0 {
        None
    } else {
        Some(lba)
    }
}

struct State {
    cert: [u8; CERT_MAX],
    cert_len: usize,
    key: [u8; KEY_MAX],
    key_len: usize,
    on: bool,
    dirty: [u8; IMAGE],
    dirty_on: bool,
    dirty_gen: u32,
}

impl State {
    const fn empty() -> Self {
        Self {
            cert: [0; CERT_MAX],
            cert_len: 0,
            key: [0; KEY_MAX],
            key_len: 0,
            on: false,
            dirty: [0; IMAGE],
            dirty_on: false,
            dirty_gen: 0,
        }
    }
}

struct Cell(core::cell::UnsafeCell<State>);
// SAFETY: exclusive access is `STATE_LOCK` on firmware. Tests use a
// thread-local so one case cannot replace the millicert on another.
// KANI-TARGET: host encode/decode, not this cell.
unsafe impl Sync for Cell {}

#[cfg(not(test))]
static STATE: Cell = Cell(core::cell::UnsafeCell::new(State::empty()));
#[cfg(not(test))]
static STATE_LOCK: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);

#[cfg(test)]
thread_local! {
    static TEST_STATE: std::cell::RefCell<State> = std::cell::RefCell::new(State::empty());
}

fn with_state<R>(f: impl FnOnce(&mut State) -> R) -> R {
    #[cfg(test)]
    {
        TEST_STATE.with(|s| f(&mut s.borrow_mut()))
    }
    #[cfg(not(test))]
    {
        while STATE_LOCK.swap(true, core::sync::atomic::Ordering::Acquire) {
            core::hint::spin_loop();
        }
        // SAFETY: lock held. The cell is not shared outside this lock.
        // KANI-TARGET: host encode/decode, not this cell.
        let out = unsafe { f(&mut *STATE.0.get()) };
        STATE_LOCK.store(false, core::sync::atomic::Ordering::Release);
        out
    }
}

/// True when a saved certificate is what the next handshake will send.
pub fn operator_active() -> bool {
    with_state(|s| s.on)
}

/// Copy the saved certificate. `None` means the lab millicert.
pub fn copy_cert(out: &mut [u8]) -> Option<usize> {
    with_state(|s| {
        if !s.on || s.cert_len == 0 || s.cert_len > out.len() {
            return None;
        }
        out[..s.cert_len].copy_from_slice(&s.cert[..s.cert_len]);
        Some(s.cert_len)
    })
}

/// Parse the saved PKCS#8 key. `None` means the lab key.
pub fn load_key() -> Option<RsaPrivateKey> {
    let key = with_state(|s| {
        if !s.on || s.key_len == 0 || s.key_len > KEY_MAX {
            return None;
        }
        let mut tmp = [0u8; KEY_MAX];
        tmp[..s.key_len].copy_from_slice(&s.key[..s.key_len]);
        Some((tmp, s.key_len))
    })?;
    let parsed = RsaPrivateKey::from_pkcs8_der(&key.0[..key.1]).ok()?;
    if parsed.size() == RSA2048 {
        Some(parsed)
    } else {
        None
    }
}

/// Install a decoded image. A bad image leaves the millicert in place.
pub fn restore(buf: &[u8]) -> bool {
    match decode(buf) {
        Some((cert, key)) => with_state(|s| {
            s.cert = [0; CERT_MAX];
            s.key = [0; KEY_MAX];
            s.cert[..cert.len()].copy_from_slice(cert);
            s.cert_len = cert.len();
            s.key[..key.len()].copy_from_slice(key);
            s.key_len = key.len();
            s.on = true;
            true
        }),
        None => with_state(|s| {
            s.on = false;
            s.cert_len = 0;
            s.key_len = 0;
            false
        }),
    }
}

fn decode(buf: &[u8]) -> Option<(&[u8], &[u8])> {
    if buf.len() < IMAGE || &buf[..8] != MAGIC || buf[8] != VERSION {
        return None;
    }
    let cert_len = u16::from_le_bytes(buf[OFF_CERT..OFF_CERT + 2].try_into().ok()?) as usize;
    let key_len = u16::from_le_bytes(buf[OFF_KEY..OFF_KEY + 2].try_into().ok()?) as usize;
    if cert_len == 0 || key_len == 0 || cert_len > CERT_MAX || key_len > KEY_MAX {
        return None;
    }
    if OFF_BODY + cert_len + key_len > IMAGE {
        return None;
    }
    let sum = u32::from_le_bytes(buf[OFF_SUM..OFF_SUM + 4].try_into().ok()?);
    if sum != checksum(&buf[OFF_BODY..OFF_BODY + cert_len + key_len]) {
        return None;
    }
    let cert = &buf[OFF_BODY..OFF_BODY + cert_len];
    let key = &buf[OFF_BODY + cert_len..OFF_BODY + cert_len + key_len];
    if !material_ok(cert, key) {
        return None;
    }
    Some((cert, key))
}

fn encode(cert: &[u8], key: &[u8], buf: &mut [u8; IMAGE]) -> bool {
    if !material_ok(cert, key) {
        return false;
    }
    buf.fill(0);
    buf[..8].copy_from_slice(MAGIC);
    buf[8] = VERSION;
    buf[OFF_CERT..OFF_CERT + 2].copy_from_slice(&(cert.len() as u16).to_le_bytes());
    buf[OFF_KEY..OFF_KEY + 2].copy_from_slice(&(key.len() as u16).to_le_bytes());
    buf[OFF_BODY..OFF_BODY + cert.len()].copy_from_slice(cert);
    buf[OFF_BODY + cert.len()..OFF_BODY + cert.len() + key.len()].copy_from_slice(key);
    let sum = checksum(&buf[OFF_BODY..OFF_BODY + cert.len() + key.len()]);
    buf[OFF_SUM..OFF_SUM + 4].copy_from_slice(&sum.to_le_bytes());
    true
}

fn material_ok(cert: &[u8], key: &[u8]) -> bool {
    if cert.first() != Some(&0x30) || cert.len() < 64 || cert.len() > CERT_MAX {
        return false;
    }
    if key.is_empty() || key.len() > KEY_MAX {
        return false;
    }
    match RsaPrivateKey::from_pkcs8_der(key) {
        Ok(parsed) => parsed.size() == RSA2048,
        Err(_) => false,
    }
}

fn checksum(bytes: &[u8]) -> u32 {
    bytes
        .iter()
        .fold(0u32, |a, b| a.wrapping_add(u32::from(*b)))
}

fn stage(cert: &[u8], key: &[u8]) -> bool {
    let mut image = [0u8; IMAGE];
    if !encode(cert, key, &mut image) {
        return false;
    }
    with_state(|s| {
        s.cert = [0; CERT_MAX];
        s.key = [0; KEY_MAX];
        s.cert[..cert.len()].copy_from_slice(cert);
        s.cert_len = cert.len();
        s.key[..key.len()].copy_from_slice(key);
        s.key_len = key.len();
        s.on = true;
        s.dirty = image;
        s.dirty_gen = s.dirty_gen.wrapping_add(1);
        s.dirty_on = true;
    });
    true
}

/// Staged image for sixteen WRITE(16)s. `None` when the page has not saved.
pub fn copy_dirty() -> Option<(u32, [u8; IMAGE])> {
    with_state(|s| {
        if s.dirty_on {
            Some((s.dirty_gen, s.dirty))
        } else {
            None
        }
    })
}

pub fn ack_dirty(gen: u32) {
    with_state(|s| {
        if s.dirty_gen == gen {
            s.dirty_on = false;
        }
    });
}

/// REST answer. The body is owned so the HTTP codec can copy it out.
pub enum HostCertHttp {
    NotMine,
    Unauthorized,
    Bad,
    Json {
        status: u16,
        len: usize,
        body: [u8; 128],
    },
}

/// `GET /host/cert`, `POST /host/cert` with a PEM certificate and a
/// PKCS#8 PEM key, `POST /host/cert/clear`. The key is not echoed.
pub fn host_cert_rest(method: RestMethod, path: &str, authed: bool, body: &str) -> HostCertHttp {
    if path != "/host/cert" && path != "/host/cert/clear" {
        return HostCertHttp::NotMine;
    }
    if !authed {
        return HostCertHttp::Unauthorized;
    }
    if path == "/host/cert" && matches!(method, RestMethod::Get) {
        return status_json(200);
    }
    if !matches!(method, RestMethod::Post) {
        return HostCertHttp::Bad;
    }
    if path == "/host/cert/clear" {
        with_state(|s| {
            s.on = false;
            s.cert_len = 0;
            s.key_len = 0;
            s.dirty = [0; IMAGE];
            s.dirty_gen = s.dirty_gen.wrapping_add(1);
            s.dirty_on = true;
        });
        crate::mgmt::megaraid::flush_host_cert();
        crate::audit_log!(crate::audit::AuditEvent::HostCertSaved {
            source: 0,
            cert_len: 0,
        });
        return status_json(200);
    }
    let mut cert = [0u8; CERT_MAX];
    let mut key = [0u8; KEY_MAX];
    let Some(cert_len) = pem_der(
        body,
        "-----BEGIN CERTIFICATE-----",
        "-----END CERTIFICATE-----",
        &mut cert,
    ) else {
        return HostCertHttp::Bad;
    };
    let Some(key_len) = pem_der(
        body,
        "-----BEGIN PRIVATE KEY-----",
        "-----END PRIVATE KEY-----",
        &mut key,
    ) else {
        return HostCertHttp::Bad;
    };
    if !stage(&cert[..cert_len], &key[..key_len]) {
        return HostCertHttp::Bad;
    }
    crate::mgmt::megaraid::flush_host_cert();
    crate::audit_log!(crate::audit::AuditEvent::HostCertSaved {
        source: 1,
        cert_len: cert_len as u16,
    });
    status_json(200)
}

fn status_json(status: u16) -> HostCertHttp {
    let (on, cert_len) = with_state(|s| (s.on, s.cert_len));
    let mut body = [0u8; 128];
    let text = if on {
        // cert_len fits the fixed sentence below; the digits are written by hand.
        None
    } else {
        Some("{\"ok\":true,\"source\":\"millicert\",\"tls\":\"1.2\"}")
    };
    let n = if let Some(text) = text {
        let b = text.as_bytes();
        body[..b.len()].copy_from_slice(b);
        b.len()
    } else {
        let mut w = JsonBuf::new(&mut body);
        w.s("{\"ok\":true,\"source\":\"operator\",\"tls\":\"1.2\",\"cert\":");
        w.u(cert_len as u64);
        w.s("}");
        if w.fit {
            w.n
        } else {
            0
        }
    };
    HostCertHttp::Json {
        status,
        len: n,
        body,
    }
}

fn pem_der(text: &str, begin: &str, end: &str, out: &mut [u8]) -> Option<usize> {
    let start = text.find(begin)? + begin.len();
    let rest = &text[start..];
    let stop = rest.find(end)?;
    b64_decode(&rest[..stop], out)
}

fn b64_decode(text: &str, out: &mut [u8]) -> Option<usize> {
    fn val(c: u8) -> Option<u8> {
        match c {
            b'A'..=b'Z' => Some(c - b'A'),
            b'a'..=b'z' => Some(c - b'a' + 26),
            b'0'..=b'9' => Some(c - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let mut raw = [0u8; 4096];
    let mut nraw = 0usize;
    for b in text.bytes() {
        if b.is_ascii_whitespace() {
            continue;
        }
        if nraw >= raw.len() {
            return None;
        }
        raw[nraw] = b;
        nraw += 1;
    }
    if nraw == 0 || nraw % 4 != 0 {
        return None;
    }
    let mut n = 0usize;
    let mut i = 0usize;
    while i < nraw {
        let (c, cpad) = if raw[i + 2] == b'=' {
            (0, true)
        } else {
            (val(raw[i + 2])?, false)
        };
        let (d, dpad) = if raw[i + 3] == b'=' {
            (0, true)
        } else {
            (val(raw[i + 3])?, false)
        };
        if cpad && !dpad {
            return None;
        }
        let a = val(raw[i])?;
        let b = val(raw[i + 1])?;
        let v = ((a as u32) << 18) | ((b as u32) << 12) | ((c as u32) << 6) | (d as u32);
        let bytes = [
            ((v >> 16) & 0xff) as u8,
            ((v >> 8) & 0xff) as u8,
            (v & 0xff) as u8,
        ];
        let take = if dpad {
            if cpad {
                1
            } else {
                2
            }
        } else {
            3
        };
        if n + take > out.len() {
            return None;
        }
        out[n..n + take].copy_from_slice(&bytes[..take]);
        n += take;
        i += 4;
    }
    Some(n)
}

struct JsonBuf<'a> {
    b: &'a mut [u8],
    n: usize,
    fit: bool,
}

impl<'a> JsonBuf<'a> {
    fn new(b: &'a mut [u8]) -> Self {
        Self { b, n: 0, fit: true }
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

/// Host package: the image, the page, and the TLS identity are in tree.
/// Host tests do not print an iron marker and do not mark the row lived.
pub fn prop_host_cert() -> bool {
    let http = include_str!("http.rs");
    let tls = include_str!("tls12.rs");
    let mega = include_str!("megaraid.rs");
    let coexist = include_str!("host_nic_coexist.rs");
    let html = include_str!("../assets/webui.html");
    http.contains("host_cert_rest")
        && tls.contains("host_cert::copy_cert")
        && tls.contains("TLS 1.2")
        && mega.contains("fn perc_load_host_cert(")
        && mega.contains("fn flush_host_cert_uefi(")
        && mega.contains("boot: E4 cert")
        && coexist.contains("flush_host_cert()")
        && html.contains("id=\"btn-cert\"")
        && html.contains("/host/cert")
        && html.contains("function saveCert")
}

#[cfg(test)]
pub fn reset_for_test() {
    with_state(|s| *s = State::empty());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn lab() -> (&'static [u8], &'static [u8]) {
        (
            include_bytes!("../assets/tls/lab.crt.der"),
            include_bytes!("../assets/tls/lab.key.der"),
        )
    }

    fn pem_of(begin: &str, end: &str, der: &[u8]) -> String {
        const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut b64 = String::new();
        let mut i = 0;
        while i < der.len() {
            let b0 = der[i];
            let b1 = if i + 1 < der.len() { der[i + 1] } else { 0 };
            let b2 = if i + 2 < der.len() { der[i + 2] } else { 0 };
            let v = ((b0 as u32) << 16) | ((b1 as u32) << 8) | b2 as u32;
            b64.push(T[((v >> 18) & 63) as usize] as char);
            b64.push(T[((v >> 12) & 63) as usize] as char);
            if i + 1 < der.len() {
                b64.push(T[((v >> 6) & 63) as usize] as char);
            } else {
                b64.push('=');
            }
            if i + 2 < der.len() {
                b64.push(T[(v & 63) as usize] as char);
            } else {
                b64.push('=');
            }
            i += 3;
            if b64.len() % 65 == 64 {
                b64.push('\n');
            }
        }
        format!("{begin}\n{b64}\n{end}\n")
    }

    #[test]
    fn sector_window_is_before_the_operator_account() {
        let spare = crate::mgmt::megaraid::IRON_LD1_BYTES;
        let lba = cert_lba(spare).unwrap();
        let op = crate::mgmt::operator::operator_lba(spare).unwrap();
        assert_eq!(lba, op - WINDOW);
        assert_ne!(lba, 0);
        assert!(cert_lba(400 * 1024 * 1024 * 1024).is_none());
        assert!(prop_host_cert());
    }

    #[test]
    fn pem_save_replaces_the_millicert_until_clear() {
        let _g = lock();
        reset_for_test();
        assert!(!operator_active());
        let (cert, key) = lab();
        let mut image = [0u8; IMAGE];
        assert!(encode(cert, key, &mut image));
        assert!(restore(&image));
        let mut got = [0u8; CERT_MAX];
        assert_eq!(copy_cert(&mut got), Some(cert.len()));
        assert_eq!(&got[..cert.len()], cert);
        assert!(load_key().is_some());
        reset_for_test();
        let body = format!(
            "{}{}",
            pem_of(
                "-----BEGIN CERTIFICATE-----",
                "-----END CERTIFICATE-----",
                cert
            ),
            pem_of(
                "-----BEGIN PRIVATE KEY-----",
                "-----END PRIVATE KEY-----",
                key
            )
        );
        assert!(matches!(
            host_cert_rest(RestMethod::Post, "/host/cert", false, &body),
            HostCertHttp::Unauthorized
        ));
        let saved = host_cert_rest(RestMethod::Post, "/host/cert", true, &body);
        match saved {
            HostCertHttp::Json { status, body, len } => {
                assert_eq!(status, 200);
                let s = core::str::from_utf8(&body[..len]).unwrap();
                assert!(s.contains("\"source\":\"operator\""));
                assert!(s.contains("\"tls\":\"1.2\""));
            }
            _ => panic!("save"),
        }
        assert!(operator_active());
        let staged = copy_dirty().unwrap().1;
        reset_for_test();
        assert!(!operator_active());
        assert!(restore(&staged));
        assert_eq!(copy_cert(&mut got), Some(cert.len()));
        assert!(matches!(
            host_cert_rest(RestMethod::Post, "/host/cert/clear", true, ""),
            HostCertHttp::Json { status: 200, .. }
        ));
        assert!(!operator_active());
        reset_for_test();
    }

    #[test]
    fn listener_sends_the_saved_certificate() {
        let _g = lock();
        reset_for_test();
        let (cert, key) = lab();
        let mut flipped = [0u8; 1536];
        flipped[..cert.len()].copy_from_slice(cert);
        flipped[40] ^= 0x5a;
        let mut image = [0u8; IMAGE];
        assert!(encode(&flipped[..cert.len()], key, &mut image));
        assert!(restore(&image));
        let ch = [
            0x16, 0x03, 0x03, 0x00, 0x33, 0x01, 0x00, 0x00, 0x2f, 0x03, 0x03, 0x00, 0x01, 0x02,
            0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10,
            0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e,
            0x1f, 0x00, 0x00, 0x02, 0xc0, 0x2f, 0x01, 0x00, 0x00, 0x04, 0x00, 0x17, 0x00, 0x00,
        ];
        let mut listen = crate::mgmt::tls12::Tls12Listen::new();
        assert_eq!(listen.feed_tcp(&ch), ch.len());
        let mut out = [0u8; 4096];
        let wn = listen.drain_tcp(&mut out);
        assert!(wn > 50, "flight {}", listen.debug_state());
        assert!(out[..wn]
            .windows(cert.len())
            .any(|w| w == &flipped[..cert.len()]));
        reset_for_test();
        let mut again = crate::mgmt::tls12::Tls12Listen::new();
        assert_eq!(again.feed_tcp(&ch), ch.len());
        let wn = again.drain_tcp(&mut out);
        assert!(wn > 50);
        assert!(out[..wn].windows(cert.len()).any(|w| w == cert));
        reset_for_test();
    }
}
