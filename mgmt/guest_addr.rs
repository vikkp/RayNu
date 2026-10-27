//! M8.9.5: the saved address for the installed Ubuntu (outside Proven Core).
//!
//! Pillar: [Z] [D]
//! Proven Core: **outside** (ADR-002 / ADR-013 Appendix C).
//! VERIFICATION: L1 host tests. Host/CI never print an iron marker.
//!
//! `cloud-init` is disabled on this disk. The address reaches the guest as
//! a systemd credential in SMBIOS Type 11 (`io.systemd.credential.binary:
//! network.network.10-raynu=<base64 .network>`), which PID 1 imports and
//! `systemd-network-generator` writes to `/run/systemd/network/` before
//! `systemd-networkd` starts. The file matches the guest's MAC, so the
//! address lands on the virtio NIC and nowhere else. The guest disk is not
//! written; the table is this boot only. The earlier kernel `ip=` token
//! stays as a second view (Documentation/admin-guide/kernel-parameters.txt);
//! iron `02e0acba` showed it does not configure this Ubuntu by itself.
//!
//! ADR-013: the guest uses the saved address. Get from DHCP pinned a lease
//! and manual entry typed one; both are a static `Address=` here. RayNu-V
//! does not run a second DHCP client on that MAC.

use core::sync::atomic::{AtomicU8, Ordering};

use crate::mgmt::guest_nic::{EGRESS_PHY_MAC, SPA_MAC};
use crate::mgmt::guest_nic_start::StartNic;

/// Credential name. `systemd-network-generator` strips `network.network.`
/// and writes `/run/systemd/network/10-raynu.network`.
pub const NETWORK_CREDENTIAL_NAME: &[u8] = b"network.network.10-raynu";
/// Largest `.network` text we emit (match + address + gateway + DNS).
pub const NETWORK_FILE_CAP: usize = 160;
/// Largest Type 11 string: prefix + name + `=` + base64 of the file.
pub const NETWORK_CREDENTIAL_CAP: usize = 320;

/// The `.network` file text and its Type 11 credential string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NetworkCredential {
    pub file: [u8; NETWORK_FILE_CAP],
    pub file_len: usize,
    pub string: [u8; NETWORK_CREDENTIAL_CAP],
    pub string_len: usize,
}

impl NetworkCredential {
    pub fn file_str(&self) -> &str {
        core::str::from_utf8(&self.file[..self.file_len]).unwrap_or("")
    }

    pub fn string_bytes(&self) -> &[u8] {
        &self.string[..self.string_len]
    }

    pub fn string_str(&self) -> &str {
        core::str::from_utf8(self.string_bytes()).unwrap_or("")
    }
}

/// Iron `d588148f`: Ubuntu 26.04 ships `systemd-network-generator.service`
/// **disabled** (`preset: enabled`; netplan is their renderer), so PID 1
/// imported `network.network.10-raynu` and nothing read it. systemd 256+
/// also imports `systemd.unit-dropin.<unit>~<name>` credentials and writes
/// them to `/run/systemd/system/<unit>.d/<name>.conf` before loading units.
/// A `Wants=` drop-in on `sysinit.target` pulls the generator in whatever
/// its enable state. Runtime only; the guest disk is not written.
pub const GENERATOR_DROPIN_CREDENTIAL_NAME: &[u8] = b"systemd.unit-dropin.sysinit.target~10-raynu";
pub const GENERATOR_DROPIN_FILE: &[u8] = b"[Unit]\nWants=systemd-network-generator.service\n";
/// Largest Type 11 string for the drop-in credential.
pub const GENERATOR_DROPIN_CREDENTIAL_CAP: usize = 160;

/// Type 11 string that makes the guest run `systemd-network-generator`.
/// Constant text; `None` only if the cap is wrong.
pub fn generator_dropin_credential(out: &mut [u8]) -> Option<usize> {
    let mut n = 0usize;
    if !push(out, &mut n, crate::raynu_f::smbios::CREDENTIAL_BINARY_PREFIX)
        || !push(out, &mut n, GENERATOR_DROPIN_CREDENTIAL_NAME)
        || !push(out, &mut n, b"=")
    {
        return None;
    }
    let enc = base64_encode(GENERATOR_DROPIN_FILE, &mut out[n..])?;
    Some(n + enc)
}

/// systemd `.network` for the saved NIC: match on the guest MAC, static
/// address, gateway and DNS when saved. `None` for the same records
/// [`ip_token`] refuses (SPA/egress/multicast MAC, no address, bad prefix).
pub fn network_file(nic: &StartNic, out: &mut [u8]) -> Option<usize> {
    if nic.mac == SPA_MAC || nic.mac == EGRESS_PHY_MAC || nic.mac[0] & 1 != 0 {
        return None;
    }
    if nic.egress_phy != EGRESS_PHY_MAC {
        return None;
    }
    if nic.ipv4 == [0, 0, 0, 0] || nic.ipv4 == [255, 255, 255, 255] {
        return None;
    }
    if nic.prefix == 0 || nic.prefix > 32 {
        return None;
    }
    let mut n = 0usize;
    let ok = push(out, &mut n, b"[Match]\nMACAddress=")
        && push_mac(out, &mut n, nic.mac)
        && push(out, &mut n, b"\n\n[Network]\nAddress=")
        && push_ip(out, &mut n, nic.ipv4)
        && push(out, &mut n, b"/")
        && push_u8(out, &mut n, nic.prefix)
        && push(out, &mut n, b"\n");
    if !ok {
        return None;
    }
    if nic.gateway != [0, 0, 0, 0] {
        if !(push(out, &mut n, b"Gateway=")
            && push_ip(out, &mut n, nic.gateway)
            && push(out, &mut n, b"\n"))
        {
            return None;
        }
    }
    if nic.dns != [0, 0, 0, 0] {
        if !(push(out, &mut n, b"DNS=") && push_ip(out, &mut n, nic.dns) && push(out, &mut n, b"\n"))
        {
            return None;
        }
    }
    Some(n)
}

/// Type 11 string carrying the `.network` as a binary systemd credential.
pub fn network_credential(nic: &StartNic) -> Option<NetworkCredential> {
    let mut file = [0u8; NETWORK_FILE_CAP];
    let file_len = network_file(nic, &mut file)?;
    let mut string = [0u8; NETWORK_CREDENTIAL_CAP];
    let mut n = 0usize;
    if !push(&mut string, &mut n, crate::raynu_f::smbios::CREDENTIAL_BINARY_PREFIX)
        || !push(&mut string, &mut n, NETWORK_CREDENTIAL_NAME)
        || !push(&mut string, &mut n, b"=")
    {
        return None;
    }
    let enc = base64_encode(&file[..file_len], &mut string[n..])?;
    n += enc;
    Some(NetworkCredential {
        file,
        file_len,
        string,
        string_len: n,
    })
}

/// Standard base64 with `=` padding (RFC 4648 §4). Returns the byte count
/// written, or `None` when `out` is too small.
pub fn base64_encode(input: &[u8], out: &mut [u8]) -> Option<usize> {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let need = input.len().div_ceil(3) * 4;
    if out.len() < need {
        return None;
    }
    let mut o = 0usize;
    for chunk in input.chunks(3) {
        let b0 = chunk[0];
        let b1 = chunk.get(1).copied().unwrap_or(0);
        let b2 = chunk.get(2).copied().unwrap_or(0);
        out[o] = T[usize::from(b0 >> 2)];
        out[o + 1] = T[usize::from(((b0 & 0x03) << 4) | (b1 >> 4))];
        out[o + 2] = if chunk.len() > 1 {
            T[usize::from(((b1 & 0x0f) << 2) | (b2 >> 6))]
        } else {
            b'='
        };
        out[o + 3] = if chunk.len() > 2 {
            T[usize::from(b2 & 0x3f)]
        } else {
            b'='
        };
        o += 4;
    }
    Some(o)
}

fn push_mac(buf: &mut [u8], n: &mut usize, mac: [u8; 6]) -> bool {
    const H: &[u8; 16] = b"0123456789abcdef";
    for (i, b) in mac.iter().enumerate() {
        if i > 0 && !push(buf, n, b":") {
            return false;
        }
        let pair = [H[usize::from(b >> 4)], H[usize::from(b & 0xf)]];
        if !push(buf, n, &pair) {
            return false;
        }
    }
    true
}

/// Kernel `ip=` text. `len` is the used prefix of `bytes`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IpToken {
    pub bytes: [u8; 96],
    pub len: usize,
}

impl IpToken {
    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.bytes[..self.len]).unwrap_or("")
    }
}

static HELD: AtomicU8 = AtomicU8::new(0);
static TOKEN_BYTES: [AtomicU8; 96] = [const { AtomicU8::new(0) }; 96];
static AMEND_LOGGED: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);

/// `ip=<client>::<gw>:<mask>:::off[:<dns>]`.
///
/// Empty gateway and empty device are adjacent colons. `off` means the
/// kernel does not start DHCP. A zero DNS is omitted. `0.0.0.0` and
/// `255.255.255.255` are not a client address.
pub fn ip_token(nic: &StartNic) -> Option<IpToken> {
    if nic.mac == SPA_MAC || nic.mac == EGRESS_PHY_MAC || nic.mac[0] & 1 != 0 {
        return None;
    }
    if nic.egress_phy != EGRESS_PHY_MAC {
        return None;
    }
    if nic.ipv4 == [0, 0, 0, 0] || nic.ipv4 == [255, 255, 255, 255] {
        return None;
    }
    let mask = mask_of(nic.prefix)?;
    let mut bytes = [0u8; 96];
    let mut n = 0usize;
    if !push(&mut bytes, &mut n, b"ip=") {
        return None;
    }
    if !push_ip(&mut bytes, &mut n, nic.ipv4) {
        return None;
    }
    if !push(&mut bytes, &mut n, b"::") {
        return None;
    }
    if nic.gateway != [0, 0, 0, 0] && !push_ip(&mut bytes, &mut n, nic.gateway) {
        return None;
    }
    if !push(&mut bytes, &mut n, b":") {
        return None;
    }
    if !push_ip(&mut bytes, &mut n, mask) {
        return None;
    }
    if !push(&mut bytes, &mut n, b":::off") {
        return None;
    }
    if nic.dns != [0, 0, 0, 0] {
        if !push(&mut bytes, &mut n, b":") {
            return None;
        }
        if !push_ip(&mut bytes, &mut n, nic.dns) {
            return None;
        }
    }
    Some(IpToken { bytes, len: n })
}

/// Remember that this Start has a token. The bytes are rebuilt from the plan.
pub fn hold_token(nic: &StartNic) {
    match ip_token(nic) {
        Some(t) => {
            for (i, b) in TOKEN_BYTES.iter().enumerate() {
                let v = if i < t.len { t.bytes[i] } else { 0 };
                b.store(v, Ordering::Relaxed);
            }
            HELD.store(t.len as u8, Ordering::Release);
        }
        None => clear_token(),
    }
}

pub fn clear_token() {
    HELD.store(0, Ordering::Release);
}

/// Copy the held token. Returns 0 when this Start has no address.
pub fn copy_token(out: &mut [u8]) -> usize {
    let n = held_len().min(out.len());
    for i in 0..n {
        out[i] = TOKEN_BYTES[i].load(Ordering::Relaxed);
    }
    n
}

/// Insert the held `ip=` token into a `linux` / `linuxefi` line that names
/// `root=` and does not already contain `ip=`.
///
/// The disk bytes are not written. The insert fits only when `buf` has
/// enough trailing zero bytes to grow. Otherwise the buffer is unchanged.
pub fn amend_linux_view(buf: &mut [u8]) -> bool {
    let n = held_len();
    if n == 0 || n > 96 {
        return false;
    }
    let mut token = [0u8; 96];
    let n = copy_token(&mut token);
    if n == 0 {
        return false;
    }
    let token = &token[..n];
    let need = 1 + token.len();
    let slack = buf.iter().rev().take_while(|b| **b == 0).count();
    if slack < need {
        return false;
    }
    let mut i = 0usize;
    while i < buf.len() {
        if buf[i] == 0 {
            break;
        }
        let start = i;
        while i < buf.len() && buf[i] != 0 && buf[i] != b'\n' {
            i += 1;
        }
        let end = i;
        if line_wants_ip(&buf[start..end]) {
            buf.copy_within(end..buf.len() - need, end + need);
            buf[end] = b' ';
            buf[end + 1..end + 1 + token.len()].copy_from_slice(token);
            note_amend();
            return true;
        }
        if i < buf.len() && buf[i] == b'\n' {
            i += 1;
        }
    }
    false
}

fn line_wants_ip(line: &[u8]) -> bool {
    let s = trim(line);
    if !has_tok(s, b"linuxefi") && !has_tok(s, b"linux") {
        return false;
    }
    contains(s, b"root=") && !contains(s, b"ip=")
}

fn trim(s: &[u8]) -> &[u8] {
    let mut i = 0;
    while i < s.len() && (s[i] == b' ' || s[i] == b'\t') {
        i += 1;
    }
    &s[i..]
}

fn has_tok(s: &[u8], token: &[u8]) -> bool {
    s.len() >= token.len()
        && s.starts_with(token)
        && (s.len() == token.len() || s[token.len()] == b' ' || s[token.len()] == b'\t')
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty() && hay.windows(needle.len()).any(|w| w == needle)
}

fn note_amend() {
    if AMEND_LOGGED.swap(true, Ordering::AcqRel) {
        return;
    }
    #[cfg(feature = "uefi-bin")]
    crate::boot::serial::write_line(
        "boot: M8.9 ip= on linux line (this boot only; disk unchanged)",
    );
}

/// Length of the token held for the last accepted Start. Zero means none.
pub fn held_len() -> usize {
    HELD.load(Ordering::Acquire) as usize
}

fn mask_of(prefix: u8) -> Option<[u8; 4]> {
    if prefix == 0 || prefix > 32 {
        return None;
    }
    let v = u32::MAX << (32 - prefix);
    Some(v.to_be_bytes())
}

fn push(buf: &mut [u8], n: &mut usize, s: &[u8]) -> bool {
    if *n + s.len() > buf.len() {
        return false;
    }
    buf[*n..*n + s.len()].copy_from_slice(s);
    *n += s.len();
    true
}

fn push_ip(buf: &mut [u8], n: &mut usize, ip: [u8; 4]) -> bool {
    for (i, oct) in ip.iter().enumerate() {
        if i > 0 && !push(buf, n, b".") {
            return false;
        }
        if !push_u8(buf, n, *oct) {
            return false;
        }
    }
    true
}

fn push_u8(buf: &mut [u8], n: &mut usize, v: u8) -> bool {
    let mut tmp = [0u8; 3];
    let s: &[u8] = if v >= 100 {
        tmp[0] = b'0' + v / 100;
        tmp[1] = b'0' + (v / 10) % 10;
        tmp[2] = b'0' + v % 10;
        &tmp[..3]
    } else if v >= 10 {
        tmp[0] = b'0' + v / 10;
        tmp[1] = b'0' + v % 10;
        &tmp[..2]
    } else {
        tmp[0] = b'0' + v;
        &tmp[..1]
    };
    push(buf, n, s)
}

#[cfg(test)]
mod guest_addr_test {
    use super::*;
    use crate::mgmt::guest_nic::mint_mac;

    fn nic() -> StartNic {
        StartNic {
            spare_off: 0,
            mac: mint_mac(0),
            ipv4: [10, 99, 99, 40],
            prefix: 24,
            gateway: [10, 99, 99, 1],
            dns: [1, 1, 1, 1],
            egress_phy: EGRESS_PHY_MAC,
        }
    }

    #[test]
    fn token_is_the_saved_address() {
        let tok = ip_token(&nic()).unwrap();
        assert_eq!(
            tok.as_str(),
            "ip=10.99.99.40::10.99.99.1:255.255.255.0:::off:1.1.1.1"
        );
        let mut no_dns = nic();
        no_dns.dns = [0, 0, 0, 0];
        assert_eq!(
            ip_token(&no_dns).unwrap().as_str(),
            "ip=10.99.99.40::10.99.99.1:255.255.255.0:::off"
        );
        let mut no_gw = nic();
        no_gw.gateway = [0, 0, 0, 0];
        assert_eq!(
            ip_token(&no_gw).unwrap().as_str(),
            "ip=10.99.99.40:::255.255.255.0:::off:1.1.1.1"
        );
        let mut wide = nic();
        wide.prefix = 16;
        assert!(ip_token(&wide).unwrap().as_str().contains("255.255.0.0"));
        let mut bad = nic();
        bad.ipv4 = [0, 0, 0, 0];
        assert!(ip_token(&bad).is_none());
        bad = nic();
        bad.prefix = 0;
        assert!(ip_token(&bad).is_none());
        bad = nic();
        bad.mac = SPA_MAC;
        assert!(ip_token(&bad).is_none());
        hold_token(&nic());
        assert_eq!(held_len(), tok.len);
        clear_token();
        assert_eq!(held_len(), 0);
        hold_token(&nic());
        let mut view = [0u8; 128];
        let line = b"linux /vmlinuz root=/dev/mapper/ubuntu--vg-ubuntu--lv ro\n";
        view[..line.len()].copy_from_slice(line);
        assert!(amend_linux_view(&mut view));
        let text = core::str::from_utf8(&view).unwrap();
        assert!(text.contains("ip=10.99.99.40::10.99.99.1:255.255.255.0:::off:1.1.1.1"));
        assert!(text.contains("root=/dev/mapper/ubuntu--vg-ubuntu--lv"));
        let mut tight = *line;
        assert!(!amend_linux_view(&mut tight));
        clear_token();
        let src = include_str!("guest_addr.rs");
        let disk = ["ext4", "_write"].concat();
        let grub = ["grub", ".cfg"].concat();
        assert!(!src.contains(&disk));
        assert!(!src.contains(&grub));
    }

    #[test]
    fn network_credential_is_the_saved_address_as_a_systemd_network_file() {
        let n = nic();
        let cred = network_credential(&n).unwrap();
        let mac = n.mac;
        let want = format!(
            "[Match]\nMACAddress={:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}\n\n[Network]\nAddress=10.99.99.40/24\nGateway=10.99.99.1\nDNS=1.1.1.1\n",
            mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
        );
        assert_eq!(cred.file_str(), want);
        // The Type 11 string is the binary-credential form systemd imports.
        let s = cred.string_str();
        assert!(s.starts_with("io.systemd.credential.binary:network.network.10-raynu="));
        let b64 = &s["io.systemd.credential.binary:network.network.10-raynu=".len()..];
        assert_eq!(b64, std_base64(want.as_bytes()));
        assert!(!cred.string_bytes().contains(&0));
        assert!(!cred.string_bytes().contains(&b'\n'));
        assert!(cred.string_len <= NETWORK_CREDENTIAL_CAP);

        // No gateway and no DNS drop their lines; no second DHCP client.
        let mut bare = nic();
        bare.gateway = [0, 0, 0, 0];
        bare.dns = [0, 0, 0, 0];
        let c = network_credential(&bare).unwrap();
        assert!(!c.file_str().contains("Gateway="));
        assert!(!c.file_str().contains("DNS="));
        assert!(!c.file_str().contains("DHCP="));
        let mut wide = nic();
        wide.prefix = 16;
        assert!(network_credential(&wide).unwrap().file_str().contains("Address=10.99.99.40/16\n"));

        // The same records ip_token refuses.
        let mut bad = nic();
        bad.ipv4 = [0, 0, 0, 0];
        assert!(network_credential(&bad).is_none());
        bad = nic();
        bad.prefix = 33;
        assert!(network_credential(&bad).is_none());
        bad = nic();
        bad.mac = EGRESS_PHY_MAC;
        assert!(network_credential(&bad).is_none());
        bad = nic();
        bad.mac[0] |= 1;
        assert!(network_credential(&bad).is_none());

        // base64 known answers (RFC 4648 §10).
        let mut out = [0u8; 16];
        assert_eq!(base64_encode(b"", &mut out), Some(0));
        assert_eq!(base64_encode(b"f", &mut out).map(|n| &out[..n]), Some(&b"Zg=="[..]));
        let mut out = [0u8; 16];
        assert_eq!(base64_encode(b"fo", &mut out).map(|n| &out[..n]), Some(&b"Zm8="[..]));
        let mut out = [0u8; 16];
        assert_eq!(base64_encode(b"foo", &mut out).map(|n| &out[..n]), Some(&b"Zm9v"[..]));
        let mut out = [0u8; 16];
        assert_eq!(base64_encode(b"foobar", &mut out).map(|n| &out[..n]), Some(&b"Zm9vYmFy"[..]));
        let mut tight = [0u8; 3];
        assert_eq!(base64_encode(b"f", &mut tight), None);

        // The drop-in that makes Ubuntu run the generator (iron d588148f:
        // the unit is disabled there). Binary credential, runtime unit dir.
        let mut d = [0u8; GENERATOR_DROPIN_CREDENTIAL_CAP];
        let n = generator_dropin_credential(&mut d).unwrap();
        let s = core::str::from_utf8(&d[..n]).unwrap();
        let head = "io.systemd.credential.binary:systemd.unit-dropin.sysinit.target~10-raynu=";
        assert!(s.starts_with(head));
        assert_eq!(&s[head.len()..], std_base64(b"[Unit]\nWants=systemd-network-generator.service\n"));
        assert!(!d[..n].contains(&0));
        assert!(!s.contains("enable"));
    }

    fn std_base64(input: &[u8]) -> String {
        const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut s = String::new();
        for c in input.chunks(3) {
            let v = (u32::from(c[0]) << 16)
                | (u32::from(*c.get(1).unwrap_or(&0)) << 8)
                | u32::from(*c.get(2).unwrap_or(&0));
            s.push(T[(v >> 18) as usize & 63] as char);
            s.push(T[(v >> 12) as usize & 63] as char);
            s.push(if c.len() > 1 { T[(v >> 6) as usize & 63] as char } else { '=' });
            s.push(if c.len() > 2 { T[v as usize & 63] as char } else { '=' });
        }
        s
    }
}
