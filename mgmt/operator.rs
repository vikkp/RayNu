//! ADR-021 E6. One operator account, saved on the spare.
//!
//! Pillar: [A]
//! Proven Core: **outside** (ADR-002)
//!
//! One sector, one LBA before the host-address sector. Never LBA 0.
//! Never UBUNTU0. The sector holds a name, a salt, and SHA-256 of
//! salt followed by the password. The password is not stored.
//!
//! Until that sector exists, `raynu-v-bringup` still opens the page.
//! That is the one-time bootstrap. After Save, the latch is refused
//! and a sign-in returns a session token. The next boot reads the
//! same sector and the latch stays refused.
//!
//! Host tests do not print an iron marker and do not mark the row lived.

use crate::mgmt::api::RestMethod;
use crate::mgmt::megaraid::{classify_ld_bytes, LdClass};
use sha2::{Digest, Sha256};

pub const MAGIC: &[u8; 8] = b"RAYNUOPR";
const VERSION: u8 = 1;
pub const NAME_MAX: usize = 32;
pub const PASS_MIN: usize = 8;
pub const PASS_MAX: usize = 64;
const OFF_NLEN: usize = 9;
const OFF_NAME: usize = 10;
const OFF_SALT: usize = 42;
const OFF_HASH: usize = 58;
const CHECK_OFF: usize = 508;
const SESSION_LEN: usize = 16;

/// Name, salt, and password verifier. No password bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Account {
    pub name: [u8; NAME_MAX],
    pub name_len: u8,
    pub salt: [u8; 16],
    pub hash: [u8; 32],
}

impl Account {
    pub fn name_str(&self) -> &str {
        let n = (self.name_len as usize).min(NAME_MAX);
        core::str::from_utf8(&self.name[..n]).unwrap_or("")
    }
}

/// LBA of the operator sector. One before the host-address sector.
/// `None` for anything that is not the spare, and never LBA 0.
pub fn operator_lba(spare_bytes: u64) -> Option<u64> {
    if classify_ld_bytes(spare_bytes) != LdClass::Spare {
        return None;
    }
    let addr = crate::mgmt::host_addr::addr_lba(spare_bytes)?;
    let lba = addr.checked_sub(1)?;
    if lba == 0 {
        None
    } else {
        Some(lba)
    }
}

/// SHA-256 of salt followed by the password. The password is not kept.
pub fn password_hash(salt: &[u8; 16], password: &str) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(salt);
    h.update(password.as_bytes());
    let dig = h.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&dig);
    out
}

/// Write `account` into `buf`. `false` when the name is not usable.
pub fn encode(account: &Account, buf: &mut [u8; 512]) -> bool {
    if !account_ok(account) {
        return false;
    }
    buf.fill(0);
    buf[..8].copy_from_slice(MAGIC);
    buf[8] = VERSION;
    let n = account.name_len as usize;
    buf[OFF_NLEN] = account.name_len;
    buf[OFF_NAME..OFF_NAME + n].copy_from_slice(&account.name[..n]);
    buf[OFF_SALT..OFF_SALT + 16].copy_from_slice(&account.salt);
    buf[OFF_HASH..OFF_HASH + 32].copy_from_slice(&account.hash);
    let sum = checksum(&buf[..CHECK_OFF]);
    buf[CHECK_OFF..CHECK_OFF + 4].copy_from_slice(&sum.to_le_bytes());
    true
}

/// `None` when the magic, version, checksum, or name is wrong.
pub fn decode(buf: &[u8]) -> Option<Account> {
    if buf.len() < 512 || &buf[..8] != MAGIC || buf[8] != VERSION {
        return None;
    }
    let sum = u32::from_le_bytes(buf[CHECK_OFF..CHECK_OFF + 4].try_into().ok()?);
    if sum != checksum(&buf[..CHECK_OFF]) {
        return None;
    }
    let name_len = buf[OFF_NLEN];
    if name_len as usize > NAME_MAX || name_len == 0 {
        return None;
    }
    let n = name_len as usize;
    let mut name = [0u8; NAME_MAX];
    name[..n].copy_from_slice(&buf[OFF_NAME..OFF_NAME + n]);
    if buf[OFF_NAME + n..OFF_SALT].iter().any(|&b| b != 0) {
        return None;
    }
    let account = Account {
        name,
        name_len,
        salt: buf[OFF_SALT..OFF_SALT + 16].try_into().ok()?,
        hash: buf[OFF_HASH..OFF_HASH + 32].try_into().ok()?,
    };
    if account_ok(&account) {
        Some(account)
    } else {
        None
    }
}

fn account_ok(account: &Account) -> bool {
    let n = account.name_len as usize;
    n > 0 && n <= NAME_MAX && name_ok(&account.name[..n]) && account.hash.iter().any(|&b| b != 0)
}

fn name_ok(name: &[u8]) -> bool {
    if name.is_empty() || name.len() > NAME_MAX {
        return false;
    }
    name.iter()
        .all(|&b| b.is_ascii_alphanumeric() || b == b'.' || b == b'_' || b == b'-')
        && name[0] != b'.'
        && name[0] != b'-'
}

fn password_ok(password: &str) -> bool {
    let n = password.len();
    (PASS_MIN..=PASS_MAX).contains(&n)
        && password
            .bytes()
            .all(|b| b.is_ascii_graphic() && b != b'/' && b != b'\\')
}

fn checksum(bytes: &[u8]) -> u32 {
    bytes
        .iter()
        .fold(0u32, |a, b| a.wrapping_add(u32::from(*b)))
}

struct State {
    account: Option<Account>,
    session: [u8; SESSION_LEN],
    session_on: bool,
    dirty: [u8; 512],
    dirty_on: bool,
    dirty_gen: u32,
}

impl State {
    const fn empty() -> Self {
        Self {
            account: None,
            session: [0; SESSION_LEN],
            session_on: false,
            dirty: [0; 512],
            dirty_on: false,
            dirty_gen: 0,
        }
    }
}

// Firmware is one thread. Host tests run in parallel, so each test thread
// keeps its own account. A Save on one thread must not refuse the lab
// latch on another.
struct Cell(core::cell::UnsafeCell<State>);
// SAFETY: exclusive access is `STATE_LOCK` on firmware. Tests use a
// thread-local instead of this cell.
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

static SALT_N: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(1);

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

/// True once a sector or a Save has installed an account.
pub fn account_armed() -> bool {
    with_state(|s| s.account.is_some())
}

/// Install a decoded sector. A bad image leaves the latch in place.
pub fn restore(buf: &[u8]) -> bool {
    match decode(buf) {
        Some(account) => with_state(|s| {
            s.account = Some(account);
            s.session_on = false;
            true
        }),
        None => with_state(|s| {
            s.account = None;
            s.session_on = false;
            false
        }),
    }
}

/// Bearer matches the session issued by the last successful sign-in.
/// The password itself is not a bearer. The lab latch is not a bearer
/// once an account is armed.
pub fn session_matches(token: &str) -> bool {
    let Some(got) = hex_decode_16(token) else {
        return false;
    };
    with_state(|s| s.session_on && ct_eq(&got, &s.session))
}

fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut d = 0u8;
    for i in 0..a.len() {
        d |= a[i] ^ b[i];
    }
    d == 0
}

fn fresh_salt() -> [u8; 16] {
    let n = SALT_N.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
    let t = crate::arch::cpu::rdtsc();
    let mut salt = [0u8; 16];
    salt[..8].copy_from_slice(&n.to_le_bytes());
    salt[8..].copy_from_slice(&t.to_le_bytes());
    salt
}

fn issue_session(account: &Account) -> [u8; 16] {
    let n = SALT_N.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
    let t = crate::arch::cpu::rdtsc();
    let mut h = Sha256::new();
    h.update(account.salt);
    h.update(account.hash);
    h.update(n.to_le_bytes());
    h.update(t.to_le_bytes());
    let dig = h.finalize();
    let mut token = [0u8; SESSION_LEN];
    token.copy_from_slice(&dig[..SESSION_LEN]);
    with_state(|s| {
        s.session = token;
        s.session_on = true;
    });
    token
}

fn stage(account: &Account) -> bool {
    let mut buf = [0u8; 512];
    if !encode(account, &mut buf) {
        return false;
    }
    with_state(|s| {
        s.dirty = buf;
        s.dirty_gen = s.dirty_gen.wrapping_add(1);
        s.dirty_on = true;
        s.account = Some(*account);
    });
    true
}

/// Staged sector for one WRITE(16). `None` when the page has not saved.
pub fn copy_dirty() -> Option<(u32, [u8; 512])> {
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
pub enum OperatorHttp {
    NotMine,
    Unauthorized,
    Bad,
    Json {
        status: u16,
        len: usize,
        body: [u8; 256],
    },
}

/// `GET /auth/operator` needs no bearer. `POST /auth/login` checks the
/// password. `POST /auth/operator` saves a new account: the lab latch
/// is enough when none is saved, and the session is required after that.
pub fn operator_rest(
    method: RestMethod,
    path: &str,
    token: Option<&str>,
    body: &str,
) -> OperatorHttp {
    if path != "/auth/operator" && path != "/auth/login" {
        return OperatorHttp::NotMine;
    }
    if path == "/auth/operator" && matches!(method, RestMethod::Get) {
        return status_json(200, None);
    }
    if !matches!(method, RestMethod::Post) {
        return OperatorHttp::Bad;
    }
    if path == "/auth/login" {
        let Some((name, password)) = split_body(body) else {
            return OperatorHttp::Bad;
        };
        let Some(account) = with_state(|s| s.account) else {
            return OperatorHttp::Unauthorized;
        };
        if account.name_str() != name || !password_matches(&account, password) {
            return OperatorHttp::Unauthorized;
        }
        let session = issue_session(&account);
        return status_json(200, Some(session));
    }
    let armed = account_armed();
    let allowed = if armed {
        token.is_some_and(session_matches)
    } else {
        crate::mgmt::api::auth_allows(token)
    };
    if !allowed {
        return OperatorHttp::Unauthorized;
    }
    let Some((name, password)) = split_body(body) else {
        return OperatorHttp::Bad;
    };
    if !password_ok(password) || !name_ok(name.as_bytes()) {
        return OperatorHttp::Bad;
    }
    let salt = fresh_salt();
    let mut name_buf = [0u8; NAME_MAX];
    name_buf[..name.len()].copy_from_slice(name.as_bytes());
    let account = Account {
        name: name_buf,
        name_len: name.len() as u8,
        salt,
        hash: password_hash(&salt, password),
    };
    if !stage(&account) {
        return OperatorHttp::Bad;
    }
    crate::mgmt::megaraid::flush_operator();
    crate::audit_log!(crate::audit::AuditEvent::OperatorAccountSet {
        name_len: account.name_len,
    });
    let session = issue_session(&account);
    status_json(200, Some(session))
}

fn password_matches(account: &Account, password: &str) -> bool {
    if !password_ok(password) {
        return false;
    }
    let got = password_hash(&account.salt, password);
    ct_eq(&got, &account.hash)
}

fn split_body(body: &str) -> Option<(&str, &str)> {
    let body = body.trim_end_matches(['\r', '\n', ' ']);
    if body.len() > NAME_MAX + 1 + PASS_MAX {
        return None;
    }
    let (name, password) = body.split_once('\n')?;
    let name = name.trim_end_matches('\r');
    let password = password.trim_end_matches('\r');
    if name.is_empty() || password.is_empty() || password.contains('\n') {
        return None;
    }
    Some((name, password))
}

fn status_json(status: u16, session: Option<[u8; SESSION_LEN]>) -> OperatorHttp {
    let account = with_state(|s| s.account);
    let mut body = [0u8; 256];
    let mut w = JsonBuf::new(&mut body);
    w.s("{\"ok\":true,\"set\":");
    match account {
        None => w.s("false"),
        Some(account) => {
            w.s("true,\"name\":\"");
            w.s(account.name_str());
            w.s("\"");
        }
    }
    if let Some(token) = session {
        w.s(",\"token\":\"");
        w.hex(&token);
        w.s("\"");
    }
    w.s("}");
    let len = if w.fit { w.n } else { 0 };
    OperatorHttp::Json { status, len, body }
}

fn hex_decode_16(s: &str) -> Option<[u8; 16]> {
    if s.len() != SESSION_LEN * 2 {
        return None;
    }
    let bytes = s.as_bytes();
    let mut out = [0u8; 16];
    for i in 0..SESSION_LEN {
        let hi = hex_val(bytes[i * 2])?;
        let lo = hex_val(bytes[i * 2 + 1])?;
        out[i] = (hi << 4) | lo;
    }
    Some(out)
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
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

    fn hex(&mut self, bytes: &[u8]) {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        for b in bytes {
            let pair = [HEX[(b >> 4) as usize], HEX[(b & 0xf) as usize]];
            let s = core::str::from_utf8(&pair).unwrap_or("00");
            self.s(s);
        }
    }
}

/// Host package: the sector, the page, and the latch rule are in tree.
/// Host tests do not print an iron marker and do not mark the row lived.
pub fn prop_operator() -> bool {
    let http = include_str!("http.rs");
    let mega = include_str!("megaraid.rs");
    let coexist = include_str!("host_nic_coexist.rs");
    let wait = include_str!("perc_boot_choice.rs");
    let html = include_str!("../assets/webui.html");
    let api = include_str!("api.rs");
    http.contains("operator_rest")
        && api.contains("operator::account_armed()")
        && api.contains("operator::session_matches")
        && mega.contains("fn perc_load_operator(")
        && mega.contains("fn flush_operator_uefi(")
        && mega.contains("perc_load_operator(")
        && mega.contains("boot: E6 operator")
        && coexist.contains("flush_operator()")
        && wait.contains("flush_operator()")
        && html.contains("id=\"btn-op-save\"")
        && html.contains("/auth/operator")
        && html.contains("/auth/login")
        && html.contains("function loadOp")
}

#[cfg(test)]
pub fn reset_for_test() {
    with_state(|s| *s = State::empty());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mgmt::api::{auth_allows, BRINGUP_AUTH_TOKEN};

    fn lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn sample(password: &str) -> Account {
        let salt = [7u8; 16];
        let mut name = [0u8; NAME_MAX];
        name[..6].copy_from_slice(b"vikkp1");
        Account {
            name,
            name_len: 6,
            salt,
            hash: password_hash(&salt, password),
        }
    }

    #[test]
    fn sector_round_trip_and_spare_lba() {
        let _g = lock();
        let account = sample("correct-horse");
        let mut buf = [0u8; 512];
        assert!(encode(&account, &mut buf));
        assert_eq!(decode(&buf), Some(account));
        buf[OFF_HASH] ^= 1;
        assert!(decode(&buf).is_none());
        assert!(!encode(
            &Account {
                name_len: 0,
                ..account
            },
            &mut [0u8; 512]
        ));
        let spare = crate::mgmt::megaraid::IRON_LD1_BYTES;
        let lba = operator_lba(spare).unwrap();
        let addr = crate::mgmt::host_addr::addr_lba(spare).unwrap();
        assert_eq!(lba, addr - 1);
        assert_ne!(lba, 0);
        assert!(operator_lba(400 * 1024 * 1024 * 1024).is_none());
        assert!(prop_operator());
    }

    #[test]
    fn latch_works_until_save_then_only_the_session() {
        let _g = lock();
        reset_for_test();
        assert!(auth_allows(Some(BRINGUP_AUTH_TOKEN)));
        assert!(!account_armed());
        let open = operator_rest(RestMethod::Get, "/auth/operator", None, "");
        match open {
            OperatorHttp::Json { status, body, len } => {
                assert_eq!(status, 200);
                let s = core::str::from_utf8(&body[..len]).unwrap();
                assert!(s.contains("\"set\":false"));
                assert!(!s.contains("token"));
            }
            _ => panic!("status"),
        }
        assert!(matches!(
            operator_rest(
                RestMethod::Post,
                "/auth/operator",
                None,
                "vikkp1\ncorrect-horse"
            ),
            OperatorHttp::Unauthorized
        ));
        let saved = operator_rest(
            RestMethod::Post,
            "/auth/operator",
            Some(BRINGUP_AUTH_TOKEN),
            "vikkp1\ncorrect-horse",
        );
        let token_owned = match saved {
            OperatorHttp::Json { status, body, len } => {
                assert_eq!(status, 200);
                let s = core::str::from_utf8(&body[..len]).unwrap();
                assert!(s.contains("\"set\":true"));
                assert!(s.contains("\"name\":\"vikkp1\""));
                token_of(s)
            }
            _ => panic!("save"),
        };
        let token = token_owned.as_str();
        assert!(account_armed());
        assert!(!auth_allows(Some(BRINGUP_AUTH_TOKEN)));
        assert!(auth_allows(Some(token)));
        assert!(matches!(
            operator_rest(
                RestMethod::Post,
                "/auth/login",
                Some(BRINGUP_AUTH_TOKEN),
                "vikkp1\nwrong-password",
            ),
            OperatorHttp::Unauthorized
        ));
        let again = operator_rest(
            RestMethod::Post,
            "/auth/login",
            None,
            "vikkp1\ncorrect-horse",
        );
        let signed = match again {
            OperatorHttp::Json { status, body, len } => {
                assert_eq!(status, 200);
                token_of(core::str::from_utf8(&body[..len]).unwrap())
            }
            _ => panic!("login"),
        };
        assert!(!auth_allows(Some(token)));
        assert!(auth_allows(Some(signed.as_str())));
        assert!(matches!(
            operator_rest(
                RestMethod::Post,
                "/auth/operator",
                Some(BRINGUP_AUTH_TOKEN),
                "vikkp1\nanother-pass",
            ),
            OperatorHttp::Unauthorized
        ));
        assert!(matches!(
            operator_rest(
                RestMethod::Post,
                "/auth/operator",
                Some(signed.as_str()),
                "nope\nshort",
            ),
            OperatorHttp::Bad
        ));
        let changed = operator_rest(
            RestMethod::Post,
            "/auth/operator",
            Some(signed.as_str()),
            "vikkp1\nanother-pass",
        );
        assert!(matches!(changed, OperatorHttp::Json { status: 200, .. }));
        assert!(matches!(
            operator_rest(
                RestMethod::Post,
                "/auth/login",
                None,
                "vikkp1\ncorrect-horse"
            ),
            OperatorHttp::Unauthorized
        ));
        let sector = copy_dirty().unwrap().1;
        reset_for_test();
        assert!(auth_allows(Some(BRINGUP_AUTH_TOKEN)));
        assert!(restore(&sector));
        assert!(account_armed());
        assert!(!auth_allows(Some(BRINGUP_AUTH_TOKEN)));
        assert!(matches!(
            operator_rest(
                RestMethod::Post,
                "/auth/login",
                None,
                "vikkp1\nanother-pass"
            ),
            OperatorHttp::Json { status: 200, .. }
        ));
        reset_for_test();
        assert!(auth_allows(Some(BRINGUP_AUTH_TOKEN)));
    }

    fn token_of(json: &str) -> String {
        let rest = json.split("\"token\":\"").nth(1).unwrap();
        rest.split('"').next().unwrap().to_string()
    }
}
