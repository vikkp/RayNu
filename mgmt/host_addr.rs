//! ADR-021 D9. The host management address, saved on the spare.
//!
//! Pillar: [Z]
//! Proven Core: **outside** (ADR-002)
//!
//! One sector, one LBA before the autostart sector. Never LBA 0. Never
//! UBUNTU0. A saved static address is what the next boot's standing SPA
//! binds. This boot's listener stays on the address it already chose.
//! An absent or bad sector keeps DHCP.
//!
//! Overview **Save address** stages the sector. **Use DHCP** stages a
//! sector that means DHCP. The page says the address is used on the next
//! boot.

use crate::mgmt::api::RestMethod;
use crate::mgmt::megaraid::{classify_ld_bytes, LdClass};
use crate::mgmt::mgmt_lease::ParkedMgmtLease;

pub const MAGIC: &[u8; 8] = b"RAYNUHST";
const VERSION: u8 = 1;
const FLAG_STATIC: u8 = 1;
const OFF_FLAGS: usize = 9;
const OFF_PREFIX: usize = 10;
const OFF_IP: usize = 12;
const OFF_GW: usize = 16;
const OFF_DNS: usize = 20;
const OFF_NAME: usize = 24;
const OFF_NLEN: usize = 56;
const CHECK_OFF: usize = 508;
pub const NAME_MAX: usize = 32;

/// A static management address. `name_len == 0` is no hostname.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StaticAddr {
    pub prefix: u8,
    pub ip: [u8; 4],
    pub gateway: [u8; 4],
    pub dns: [u8; 4],
    pub name: [u8; NAME_MAX],
    pub name_len: u8,
}

impl StaticAddr {
    pub fn name_str(&self) -> &str {
        let n = (self.name_len as usize).min(NAME_MAX);
        core::str::from_utf8(&self.name[..n]).unwrap_or("")
    }
}

/// What the next boot should bind. `Dhcp` is an explicit clear.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SavedAddr {
    Dhcp,
    Static(StaticAddr),
}

/// What [`choose`] tells the listener to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LeaseChoice {
    /// Use `lease`. IP, prefix, and gateway come from `addr`. MAC and port
    /// stay from the parked lease (or the device MAC when nothing is parked).
    Static {
        lease: ParkedMgmtLease,
        addr: StaticAddr,
    },
    /// No static record. Bind the parked DHCP lease.
    Parked(ParkedMgmtLease),
    /// No static record and no parked lease. The listener may DHCP.
    NeedDhcp,
}

/// LBA of the host-address sector. One before autostart. `None` for
/// anything that is not the spare, and never LBA 0.
pub fn addr_lba(spare_bytes: u64) -> Option<u64> {
    if classify_ld_bytes(spare_bytes) != LdClass::Spare {
        return None;
    }
    let auto = crate::mgmt::guest_autostart::autostart_lba(spare_bytes)?;
    let lba = auto.checked_sub(1)?;
    if lba == 0 {
        None
    } else {
        Some(lba)
    }
}

/// Write `saved` into `buf`. `false` when a static address is not usable.
pub fn encode(saved: SavedAddr, buf: &mut [u8; 512]) -> bool {
    buf.fill(0);
    buf[..8].copy_from_slice(MAGIC);
    buf[8] = VERSION;
    match saved {
        SavedAddr::Dhcp => {
            buf[OFF_FLAGS] = 0;
        }
        SavedAddr::Static(addr) => {
            if !static_ok(&addr) {
                return false;
            }
            buf[OFF_FLAGS] = FLAG_STATIC;
            buf[OFF_PREFIX] = addr.prefix;
            buf[OFF_IP..OFF_IP + 4].copy_from_slice(&addr.ip);
            buf[OFF_GW..OFF_GW + 4].copy_from_slice(&addr.gateway);
            buf[OFF_DNS..OFF_DNS + 4].copy_from_slice(&addr.dns);
            let n = (addr.name_len as usize).min(NAME_MAX);
            buf[OFF_NAME..OFF_NAME + n].copy_from_slice(&addr.name[..n]);
            buf[OFF_NLEN] = n as u8;
        }
    }
    let sum = checksum(&buf[..CHECK_OFF]);
    buf[CHECK_OFF..CHECK_OFF + 4].copy_from_slice(&sum.to_le_bytes());
    true
}

/// `None` when the magic, version, checksum, or a static field is wrong.
/// A wrong static sector is absent, so the next boot stays on DHCP.
pub fn decode(buf: &[u8]) -> Option<SavedAddr> {
    if buf.len() < 512 || &buf[..8] != MAGIC || buf[8] != VERSION {
        return None;
    }
    let sum = u32::from_le_bytes(buf[CHECK_OFF..CHECK_OFF + 4].try_into().ok()?);
    if sum != checksum(&buf[..CHECK_OFF]) {
        return None;
    }
    match buf[OFF_FLAGS] {
        0 => Some(SavedAddr::Dhcp),
        FLAG_STATIC => {
            let name_len = buf[OFF_NLEN];
            if name_len as usize > NAME_MAX {
                return None;
            }
            let mut name = [0u8; NAME_MAX];
            let n = name_len as usize;
            name[..n].copy_from_slice(&buf[OFF_NAME..OFF_NAME + n]);
            if buf[OFF_NAME + n..OFF_NLEN].iter().any(|&b| b != 0) {
                return None;
            }
            let addr = StaticAddr {
                prefix: buf[OFF_PREFIX],
                ip: buf[OFF_IP..OFF_IP + 4].try_into().ok()?,
                gateway: buf[OFF_GW..OFF_GW + 4].try_into().ok()?,
                dns: buf[OFF_DNS..OFF_DNS + 4].try_into().ok()?,
                name,
                name_len,
            };
            if static_ok(&addr) {
                Some(SavedAddr::Static(addr))
            } else {
                None
            }
        }
        _ => None,
    }
}

fn static_ok(addr: &StaticAddr) -> bool {
    let n = addr.name_len as usize;
    n <= NAME_MAX
        && (1..=32).contains(&addr.prefix)
        && unicast(addr.ip)
        && (addr.gateway == [0, 0, 0, 0] || (unicast(addr.gateway) && addr.gateway != addr.ip))
        && (addr.dns == [0, 0, 0, 0] || unicast(addr.dns))
        && name_ok(&addr.name[..n])
}

fn unicast(ip: [u8; 4]) -> bool {
    ip[0] != 0 && ip[0] < 224 && ip != [255, 255, 255, 255]
}

fn name_ok(name: &[u8]) -> bool {
    if name.len() > NAME_MAX {
        return false;
    }
    if name.is_empty() {
        return true;
    }
    let last = name[name.len() - 1];
    if name[0] == b'.' || name[0] == b'-' || last == b'.' || last == b'-' {
        return false;
    }
    let mut prev_dot = false;
    for &b in name {
        if !(b.is_ascii_alphanumeric() || b == b'.' || b == b'-') {
            return false;
        }
        if b == b'.' {
            if prev_dot {
                return false;
            }
            prev_dot = true;
        } else {
            prev_dot = false;
        }
    }
    true
}

fn checksum(bytes: &[u8]) -> u32 {
    bytes
        .iter()
        .fold(0u32, |a, b| a.wrapping_add(u32::from(*b)))
}

struct Cell<T>(core::cell::UnsafeCell<T>);
// SAFETY: exclusive access is the matching spinlock.
// KANI-TARGET: host encode/decode, not this cell.
unsafe impl<T> Sync for Cell<T> {}

static ACTIVE: Cell<Option<StaticAddr>> = Cell(core::cell::UnsafeCell::new(None));
static ACTIVE_LOCK: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);
static SHOWN: Cell<SavedAddr> = Cell(core::cell::UnsafeCell::new(SavedAddr::Dhcp));
static SHOWN_LOCK: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);
static DIRTY: Cell<[u8; 512]> = Cell(core::cell::UnsafeCell::new([0u8; 512]));
static DIRTY_ON: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);
static DIRTY_GEN: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);

#[derive(Clone, Copy)]
struct Using {
    known: bool,
    mode_static: bool,
    prefix: u8,
    ip: [u8; 4],
    gateway: [u8; 4],
    dns: [u8; 4],
    name: [u8; NAME_MAX],
    name_len: u8,
}

impl Using {
    const fn empty() -> Self {
        Self {
            known: false,
            mode_static: false,
            prefix: 0,
            ip: [0; 4],
            gateway: [0; 4],
            dns: [0; 4],
            name: [0; NAME_MAX],
            name_len: 0,
        }
    }
}

static USING: Cell<Using> = Cell(core::cell::UnsafeCell::new(Using::empty()));
static USING_LOCK: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);

fn with_lock<T, R>(lock: &core::sync::atomic::AtomicBool, cell: &Cell<T>, f: impl FnOnce(&mut T) -> R) -> R {
    while lock.swap(true, core::sync::atomic::Ordering::Acquire) {
        core::hint::spin_loop();
    }
    // SAFETY: lock held. The cell is not shared outside this lock.
    // KANI-TARGET: host encode/decode, not this cell.
    let out = unsafe { f(&mut *cell.0.get()) };
    lock.store(false, core::sync::atomic::Ordering::Release);
    out
}

/// Install a decoded sector. This boot's listener uses the static record.
/// A bad image leaves DHCP. Edits from the page do not call this.
pub fn restore(buf: &[u8]) -> bool {
    match decode(buf) {
        Some(SavedAddr::Static(addr)) => {
            with_lock(&ACTIVE_LOCK, &ACTIVE, |s| *s = Some(addr));
            with_lock(&SHOWN_LOCK, &SHOWN, |s| *s = SavedAddr::Static(addr));
            true
        }
        Some(SavedAddr::Dhcp) | None => {
            with_lock(&ACTIVE_LOCK, &ACTIVE, |s| *s = None);
            with_lock(&SHOWN_LOCK, &SHOWN, |s| *s = SavedAddr::Dhcp);
            false
        }
    }
}

pub fn active_static() -> Option<StaticAddr> {
    with_lock(&ACTIVE_LOCK, &ACTIVE, |s| *s)
}

/// The record the page shows. A Save lands here and does not move
/// [`active_static`].
pub fn shown() -> SavedAddr {
    with_lock(&SHOWN_LOCK, &SHOWN, |s| *s)
}

fn stage_locked(saved: SavedAddr) -> bool {
    let mut buf = [0u8; 512];
    if !encode(saved, &mut buf) {
        return false;
    }
    // SAFETY: `SHOWN_LOCK` is held by the caller.
    // KANI-TARGET: host sector round-trip, not this cell.
    unsafe {
        *DIRTY.0.get() = buf;
    }
    DIRTY_GEN.fetch_add(1, core::sync::atomic::Ordering::Release);
    DIRTY_ON.store(true, core::sync::atomic::Ordering::Release);
    true
}

/// Stage `addr` for the next boot. This boot's listener is unchanged.
pub fn save(addr: StaticAddr) -> bool {
    with_lock(&SHOWN_LOCK, &SHOWN, |s| {
        if !static_ok(&addr) {
            return false;
        }
        *s = SavedAddr::Static(addr);
        stage_locked(SavedAddr::Static(addr))
    })
}

/// Next boot uses DHCP again. This boot's listener is unchanged.
pub fn clear() -> bool {
    with_lock(&SHOWN_LOCK, &SHOWN, |s| {
        *s = SavedAddr::Dhcp;
        stage_locked(SavedAddr::Dhcp)
    })
}

/// Staged sector for one WRITE(16). `None` when the page has not edited it.
pub fn copy_dirty() -> Option<(u32, [u8; 512])> {
    if !DIRTY_ON.load(core::sync::atomic::Ordering::Acquire) {
        return None;
    }
    while SHOWN_LOCK.swap(true, core::sync::atomic::Ordering::Acquire) {
        core::hint::spin_loop();
    }
    let gen = DIRTY_GEN.load(core::sync::atomic::Ordering::Acquire);
    // SAFETY: lock held.
    // KANI-TARGET: host sector round-trip, not this cell.
    let sector = unsafe { *DIRTY.0.get() };
    SHOWN_LOCK.store(false, core::sync::atomic::Ordering::Release);
    Some((gen, sector))
}

pub fn ack_dirty(gen: u32) {
    if DIRTY_GEN.load(core::sync::atomic::Ordering::Acquire) == gen {
        DIRTY_ON.store(false, core::sync::atomic::Ordering::Release);
    }
}

/// Static record replaces the lease address and keeps the MAC.
/// No record returns the parked lease, or [`LeaseChoice::NeedDhcp`].
pub fn choose(mac: [u8; 6], port: u16, parked: Option<ParkedMgmtLease>) -> LeaseChoice {
    if let Some(addr) = active_static() {
        let mut lease = parked.unwrap_or(ParkedMgmtLease {
            ip: [0; 4],
            prefix: 0,
            router: [0; 4],
            has_router: false,
            mac,
            port,
        });
        if lease.mac.iter().all(|&b| b == 0) {
            lease.mac = mac;
        }
        if lease.port == 0 {
            lease.port = port;
        }
        lease.ip = addr.ip;
        lease.prefix = addr.prefix;
        lease.router = addr.gateway;
        lease.has_router = addr.gateway != [0, 0, 0, 0];
        return LeaseChoice::Static { lease, addr };
    }
    match parked {
        Some(lease) => LeaseChoice::Parked(lease),
        None => LeaseChoice::NeedDhcp,
    }
}

/// This boot bound `addr`. Does not change the staged next-boot record.
pub fn note_using_static(addr: StaticAddr) {
    with_lock(&USING_LOCK, &USING, |u| {
        *u = Using {
            known: true,
            mode_static: true,
            prefix: addr.prefix,
            ip: addr.ip,
            gateway: addr.gateway,
            dns: addr.dns,
            name: addr.name,
            name_len: addr.name_len,
        };
    });
}

/// This boot bound a DHCP lease. Hostname stays empty.
pub fn note_using_dhcp(lease: ParkedMgmtLease) {
    with_lock(&USING_LOCK, &USING, |u| {
        *u = Using {
            known: true,
            mode_static: false,
            prefix: lease.prefix,
            ip: lease.ip,
            gateway: if lease.has_router { lease.router } else { [0; 4] },
            dns: [0; 4],
            name: [0; NAME_MAX],
            name_len: 0,
        };
    });
}

fn using_now() -> Using {
    with_lock(&USING_LOCK, &USING, |u| *u)
}

/// REST answer. The body is owned so the HTTP codec can copy it out.
pub enum HostAddrHttp {
    NotMine,
    Unauthorized,
    Bad,
    Json { status: u16, len: usize, body: [u8; 512] },
}

/// `GET /host/addr`, `POST /host/addr/<ip>/<px>/<gw>/<dns>/<name>`,
/// `POST /host/addr/clear`. A Save does not rebind the live listener.
pub fn host_addr_rest(method: RestMethod, path: &str, authed: bool) -> HostAddrHttp {
    if path != "/host/addr" && path != "/host/addr/clear" && !path.starts_with("/host/addr/") {
        return HostAddrHttp::NotMine;
    }
    if !authed {
        return HostAddrHttp::Unauthorized;
    }
    if path == "/host/addr" {
        return if matches!(method, RestMethod::Get) {
            status_json(200)
        } else {
            HostAddrHttp::Bad
        };
    }
    if !matches!(method, RestMethod::Post) {
        return HostAddrHttp::Bad;
    }
    if path == "/host/addr/clear" {
        if !clear() {
            return HostAddrHttp::Bad;
        }
        crate::mgmt::megaraid::flush_host_addr();
        crate::audit_log!(crate::audit::AuditEvent::HostAddrSaved { mode: 0, prefix: 0 });
        return status_json(200);
    }
    let Some(addr) = parse_save(path) else {
        return HostAddrHttp::Bad;
    };
    if !save(addr) {
        return HostAddrHttp::Bad;
    }
    crate::mgmt::megaraid::flush_host_addr();
    crate::audit_log!(crate::audit::AuditEvent::HostAddrSaved {
        mode: 1,
        prefix: addr.prefix,
    });
    status_json(200)
}

fn parse_save(path: &str) -> Option<StaticAddr> {
    let rest = path.strip_prefix("/host/addr/")?;
    let mut parts = rest.split('/');
    let ip = parse_ipv4(parts.next()?)?;
    let prefix = parse_u8(parts.next()?)?;
    let gateway = parse_ipv4(parts.next()?)?;
    let dns = parse_ipv4(parts.next()?)?;
    let name_tok = parts.next()?;
    if parts.next().is_some() {
        return None;
    }
    let name_bytes = if name_tok == "-" { "" } else { name_tok };
    if !name_ok(name_bytes.as_bytes()) {
        return None;
    }
    let mut name = [0u8; NAME_MAX];
    let n = name_bytes.len();
    name[..n].copy_from_slice(name_bytes.as_bytes());
    let addr = StaticAddr {
        prefix,
        ip,
        gateway,
        dns,
        name,
        name_len: n as u8,
    };
    if static_ok(&addr) {
        Some(addr)
    } else {
        None
    }
}

fn parse_u8(s: &str) -> Option<u8> {
    if s.is_empty() || s.len() > 3 || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse().ok()
}

fn parse_ipv4(s: &str) -> Option<[u8; 4]> {
    let mut out = [0u8; 4];
    let mut n = 0usize;
    for part in s.split('.') {
        if n == 4 || part.is_empty() || part.len() > 3 || !part.bytes().all(|b| b.is_ascii_digit())
        {
            return None;
        }
        let v: u16 = part.parse().ok()?;
        if v > 255 {
            return None;
        }
        out[n] = v as u8;
        n += 1;
    }
    if n == 4 {
        Some(out)
    } else {
        None
    }
}

fn status_json(status: u16) -> HostAddrHttp {
    let mut body = [0u8; 512];
    let mut w = JsonBuf::new(&mut body);
    w.s("{\"ok\":true,\"saved\":");
    write_saved(&mut w, shown());
    w.s(",\"using\":");
    write_using(&mut w, using_now());
    w.s(",\"next\":\"reboot\"}");
    let len = if w.fit { w.n } else { 0 };
    HostAddrHttp::Json { status, len, body }
}

fn write_saved(w: &mut JsonBuf<'_>, saved: SavedAddr) {
    match saved {
        SavedAddr::Dhcp => w.s("{\"mode\":\"dhcp\"}"),
        SavedAddr::Static(addr) => {
            w.s("{\"mode\":\"static\",\"ip\":\"");
            write_ip(w, addr.ip);
            w.s("\",\"px\":");
            w.u(u64::from(addr.prefix));
            w.s(",\"gw\":\"");
            write_ip(w, addr.gateway);
            w.s("\",\"dns\":\"");
            write_ip(w, addr.dns);
            w.s("\",\"name\":\"");
            w.s(addr.name_str());
            w.s("\"}");
        }
    }
}

fn write_using(w: &mut JsonBuf<'_>, using: Using) {
    if !using.known {
        w.s("{\"mode\":\"dhcp\"}");
        return;
    }
    w.s(if using.mode_static {
        "{\"mode\":\"static\",\"ip\":\""
    } else {
        "{\"mode\":\"dhcp\",\"ip\":\""
    });
    write_ip(w, using.ip);
    w.s("\",\"px\":");
    w.u(u64::from(using.prefix));
    w.s(",\"gw\":\"");
    write_ip(w, using.gateway);
    w.s("\",\"dns\":\"");
    write_ip(w, using.dns);
    w.s("\",\"name\":\"");
    let n = (using.name_len as usize).min(NAME_MAX);
    if let Ok(name) = core::str::from_utf8(&using.name[..n]) {
        w.s(name);
    }
    w.s("\"}");
}

fn write_ip(w: &mut JsonBuf<'_>, ip: [u8; 4]) {
    w.u(u64::from(ip[0]));
    w.s(".");
    w.u(u64::from(ip[1]));
    w.s(".");
    w.u(u64::from(ip[2]));
    w.s(".");
    w.u(u64::from(ip[3]));
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

/// Host package: the sector, the page, and the next-boot bind are in tree.
/// Host tests do not print an iron marker and do not mark the row lived.
pub fn prop_host_addr() -> bool {
    let http = include_str!("http.rs");
    let listen = include_str!("host_nic_listen.rs");
    let mega = include_str!("megaraid.rs");
    let coexist = include_str!("host_nic_coexist.rs");
    let wait = include_str!("perc_boot_choice.rs");
    let html = include_str!("../assets/webui.html");
    http.contains("host_addr_rest")
        && listen.contains("host_addr::choose(")
        && listen.contains("boot: D9 host addr static ")
        && listen.contains("boot: D9 host addr dhcp")
        && mega.contains("fn perc_load_host_addr(")
        && mega.contains("fn flush_host_addr_uefi(")
        && mega.contains("perc_load_host_addr(")
        && coexist.contains("flush_host_addr()")
        && wait.contains("flush_host_addr()")
        && html.contains("id=\"btn-host-addr\"")
        && html.contains("\"/host/addr/\"+")
        && html.contains("/host/addr/clear")
        && html.contains("Used on the next boot")
        && html.contains("function loadHostAddr")
}

#[cfg(test)]
pub fn test_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

#[cfg(test)]
pub fn reset_for_test() {
    restore(&[0u8; 512]);
    DIRTY_ON.store(false, core::sync::atomic::Ordering::Release);
    with_lock(&USING_LOCK, &USING, |u| *u = Using::empty());
}

#[cfg(test)]
mod host_addr_test {
    use super::*;
    use crate::mgmt::megaraid::IRON_LD1_BYTES;

    fn lab() -> StaticAddr {
        let mut name = [0u8; NAME_MAX];
        let n = b"raynu";
        name[..n.len()].copy_from_slice(n);
        StaticAddr {
            prefix: 24,
            ip: [10, 99, 99, 50],
            gateway: [10, 99, 99, 1],
            dns: [1, 1, 1, 1],
            name,
            name_len: n.len() as u8,
        }
    }

    fn parked() -> ParkedMgmtLease {
        ParkedMgmtLease {
            ip: [10, 99, 99, 144],
            prefix: 24,
            router: [10, 99, 99, 1],
            has_router: true,
            mac: [0xb0, 0x26, 0x28, 0x5c, 0x5a, 0x38],
            port: 8443,
        }
    }

    #[test]
    fn the_sector_sits_one_before_autostart_and_never_on_ubuntu() {
        let lba = addr_lba(IRON_LD1_BYTES).unwrap();
        let auto = crate::mgmt::guest_autostart::autostart_lba(IRON_LD1_BYTES).unwrap();
        assert_eq!(lba, auto - 1);
        assert_ne!(lba, 0);
        assert!(addr_lba(400 * 1024 * 1024 * 1024).is_none());
        let mut buf = [0u8; 512];
        assert!(decode(&buf).is_none());
        assert!(encode(SavedAddr::Dhcp, &mut buf));
        assert_eq!(decode(&buf), Some(SavedAddr::Dhcp));
        assert!(encode(SavedAddr::Static(lab()), &mut buf));
        buf[20] ^= 0xff;
        assert!(decode(&buf).is_none());
        buf[20] ^= 0xff;
        assert_eq!(decode(&buf), Some(SavedAddr::Static(lab())));
    }

    #[test]
    fn a_static_record_replaces_the_lease_ip_and_keeps_the_mac() {
        let _guard = super::test_lock();
        super::reset_for_test();
        let mut buf = [0u8; 512];
        assert!(encode(SavedAddr::Static(lab()), &mut buf));
        assert!(restore(&buf));
        match choose(parked().mac, 8443, Some(parked())) {
            LeaseChoice::Static { lease, addr } => {
                assert_eq!(lease.ip, [10, 99, 99, 50]);
                assert_eq!(lease.prefix, 24);
                assert_eq!(lease.router, [10, 99, 99, 1]);
                assert!(lease.has_router);
                assert_eq!(lease.mac, parked().mac);
                assert_eq!(lease.port, 8443);
                assert_eq!(addr.dns, [1, 1, 1, 1]);
                assert_eq!(addr.name_str(), "raynu");
            }
            other => panic!("expected static, got {other:?}"),
        }
        match choose([0x02, 0, 0, 0, 0, 1], 8443, None) {
            LeaseChoice::Static { lease, .. } => {
                assert_eq!(lease.mac, [0x02, 0, 0, 0, 0, 1]);
                assert_eq!(lease.ip, [10, 99, 99, 50]);
            }
            other => panic!("expected static from the device mac, got {other:?}"),
        }
    }

    #[test]
    fn an_absent_record_keeps_dhcp_and_a_save_does_not_move_this_boot() {
        let _guard = super::test_lock();
        super::reset_for_test();
        assert!(!restore(&[0u8; 512]));
        assert!(matches!(
            choose(parked().mac, 8443, Some(parked())),
            LeaseChoice::Parked(l) if l.ip == [10, 99, 99, 144] && l.mac == parked().mac
        ));
        assert!(matches!(
            choose(parked().mac, 8443, None),
            LeaseChoice::NeedDhcp
        ));
        note_using_dhcp(parked());
        assert!(save(lab()));
        assert!(active_static().is_none(), "this boot stays on DHCP");
        assert!(matches!(shown(), SavedAddr::Static(_)));
        let (gen, sector) = copy_dirty().unwrap();
        assert_eq!(decode(&sector), Some(SavedAddr::Static(lab())));
        ack_dirty(gen);
        assert!(copy_dirty().is_none());
        match choose(parked().mac, 8443, Some(parked())) {
            LeaseChoice::Parked(l) => assert_eq!(l.ip, [10, 99, 99, 144]),
            other => panic!("save must not rebind, got {other:?}"),
        }
        assert!(clear());
        let (_, cleared) = copy_dirty().unwrap();
        assert_eq!(decode(&cleared), Some(SavedAddr::Dhcp));
    }

    #[test]
    fn bad_addresses_and_names_are_refused() {
        let mut addr = lab();
        addr.ip = [224, 0, 0, 1];
        assert!(!encode(SavedAddr::Static(addr), &mut [0u8; 512]));
        addr = lab();
        addr.ip = [0, 0, 0, 0];
        assert!(!encode(SavedAddr::Static(addr), &mut [0u8; 512]));
        addr = lab();
        addr.prefix = 0;
        assert!(!encode(SavedAddr::Static(addr), &mut [0u8; 512]));
        addr = lab();
        addr.name_len = 33;
        assert!(!encode(SavedAddr::Static(addr), &mut [0u8; 512]));
        assert!(parse_save("/host/addr/10.99.99.50/24/10.99.99.1/1.1.1.1/raynu.lab").is_some());
        assert!(parse_save("/host/addr/10.99.99.50/24/0.0.0.0/0.0.0.0/-").is_some());
        assert!(parse_save("/host/addr/10.99.99.50/24/10.99.99.1/1.1.1.1/has space").is_none());
        assert!(parse_save("/host/addr/10.99.99.50/24/10.99.99.1/1.1.1.1/.raynu").is_none());
        assert!(parse_save("/host/addr/10.99.99.50/24/10.99.99.50/1.1.1.1/raynu").is_none());
    }

    #[test]
    fn the_page_reports_saved_and_using_separately() {
        let _guard = super::test_lock();
        super::reset_for_test();
        note_using_dhcp(parked());
        let HostAddrHttp::Json { status, len, body } =
            host_addr_rest(RestMethod::Get, "/host/addr", true)
        else {
            panic!("expected json");
        };
        let text = core::str::from_utf8(&body[..len]).unwrap();
        assert_eq!(status, 200);
        assert!(text.contains("\"saved\":{\"mode\":\"dhcp\"}"), "{text}");
        assert!(text.contains("\"using\":{\"mode\":\"dhcp\",\"ip\":\"10.99.99.144\""), "{text}");
        assert!(text.contains("\"next\":\"reboot\""), "{text}");
        let HostAddrHttp::Json { status, len, body } = host_addr_rest(
            RestMethod::Post,
            "/host/addr/10.99.99.50/24/10.99.99.1/1.1.1.1/raynu",
            true,
        ) else {
            panic!("expected json");
        };
        assert_eq!(status, 200);
        let text = core::str::from_utf8(&body[..len]).unwrap();
        assert!(text.contains("\"mode\":\"static\",\"ip\":\"10.99.99.50\""), "{text}");
        assert!(text.contains("\"name\":\"raynu\""), "{text}");
        assert!(text.contains("\"using\":{\"mode\":\"dhcp\",\"ip\":\"10.99.99.144\""), "{text}");
        assert!(matches!(
            host_addr_rest(RestMethod::Get, "/host/addr", false),
            HostAddrHttp::Unauthorized
        ));
        assert!(matches!(
            host_addr_rest(RestMethod::Post, "/host/addr/clear", false),
            HostAddrHttp::Unauthorized
        ));
        assert!(matches!(
            host_addr_rest(
                RestMethod::Post,
                "/host/addr/10.99.99.50/24/10.99.99.1/1.1.1.1/not a name",
                true,
            ),
            HostAddrHttp::Bad
        ));
        let HostAddrHttp::Json { .. } =
            host_addr_rest(RestMethod::Post, "/host/addr/clear", true)
        else {
            panic!("clear");
        };
        assert!(matches!(shown(), SavedAddr::Dhcp));
        assert!(prop_host_addr());
    }

    #[test]
    fn http_save_does_not_rebind_and_the_next_boot_would() {
        let _guard = super::test_lock();
        super::reset_for_test();
        note_using_dhcp(parked());
        let mut table = crate::mgmt::VmTable::new();
        let mut images = crate::mgmt::datastore::ImageTable::new();
        let mut iso_plan = crate::mgmt::iso::IsoDeployPlan::empty();
        let mut iso_install = crate::mgmt::iso_install::InstallToDiskPlan::empty();
        let mut out = [0u8; crate::mgmt::http::HTTP_RESPONSE_CAP];
        let n = crate::mgmt::http::handle_http_request(
            &mut table,
            &mut images,
            &mut iso_plan,
            &mut iso_install,
            "POST /host/addr/10.99.99.50/24/10.99.99.1/1.1.1.1/raynu HTTP/1.1\r\nAuthorization: Bearer raynu-v-bringup\r\n\r\n",
            &mut out,
        )
        .unwrap();
        let text = core::str::from_utf8(&out[..n]).unwrap();
        assert!(text.contains("200"), "{text}");
        assert!(text.contains("10.99.99.50"), "{text}");
        assert!(text.contains("10.99.99.144"), "{text}");
        assert!(active_static().is_none());
        let (gen, sector) = copy_dirty().unwrap();
        ack_dirty(gen);
        assert!(restore(&sector));
        match choose(parked().mac, 8443, Some(parked())) {
            LeaseChoice::Static { lease, .. } => {
                assert_eq!(lease.ip, [10, 99, 99, 50]);
                assert_eq!(lease.mac, parked().mac);
            }
            other => panic!("next boot should be static, got {other:?}"),
        }
    }
}
