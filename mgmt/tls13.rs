//! TLS 1.3 server flight for the coexist listener (ADR-021 E4).
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** (ADR-002 / ADR-018). Do not touch VMX/EPT.
//!
//! One cipher: `TLS_AES_128_GCM_SHA256` (RFC 8446). The key share is
//! secp256r1, which is already in the firmware binary. Browsers send
//! X25519 in the first ClientHello, so the server answers with
//! HelloRetryRequest and the client sends P-256 on the second hello.
//! CertificateVerify is RSASSA-PSS (`rsa_pss_rsae_sha256`). rustls and
//! ring stay out of `uefi-bin`. A host rustls client is the proof.
//! Iron E4 stays in flight until a boot prints `boot: E4 cert rd`
//! `operator` and a client completes this handshake.
//!
//! RFC 8446: §4.1.2 ClientHello, §4.1.3 HelloRetryRequest, §4.4
//! Certificate / CertificateVerify / Finished, §5.2 record protection,
//! §7.1 key schedule.

use super::{
    be16, read_u16, sha256, MixRng, Tls12Listen, CT_ALERT, CT_APP, CT_CCS, CT_HS, HS_CERT, HS_FIN,
    HS_SH, PROTO13, ST_13FIN, ST_APP, ST_CH2,
};
use aes_gcm::aead::{AeadInPlace, KeyInit};
use aes_gcm::{Aes128Gcm, Key, Nonce};
use hmac::{Hmac, Mac};
use p256::ecdh::EphemeralSecret;
use p256::elliptic_curve::sec1::ToEncodedPoint;
use p256::PublicKey;
use rsa::pss::SigningKey;
use rsa::sha2::Sha256 as RsaSha256;
use rsa::signature::{RandomizedSigner, SignatureEncoding};
use sha2::{Digest, Sha256};

type HmacSha256 = Hmac<Sha256>;

const CS_AES128_GCM: u16 = 0x1301;
const GRP_P256: u16 = 0x0017;
const EXT_VERSIONS: u16 = 0x002b;
const EXT_GROUPS: u16 = 0x000a;
const EXT_SIGALGS: u16 = 0x000d;
const EXT_KEY_SHARE: u16 = 0x0033;
const SIG_PSS_SHA256: u16 = 0x0804;
const VER_13: u16 = 0x0304;

/// SHA-256("HelloRetryRequest"). RFC 8446 §4.1.3.
pub(super) const HRR_RANDOM: [u8; 32] = [
    0xCF, 0x21, 0xAD, 0x74, 0xE5, 0x9A, 0x61, 0x11, 0xBE, 0x1D, 0x8C, 0x02, 0x1E, 0x65, 0xB8, 0x91,
    0xC2, 0xA2, 0x11, 0x16, 0x7A, 0xBB, 0x8C, 0x5E, 0x07, 0x9E, 0x09, 0xE2, 0xC8, 0xA8, 0x33, 0x9C,
];

const CERT_VERIFY_CTX: &[u8] = b"TLS 1.3, server CertificateVerify";

/// Host/CI only. Not an iron marker and not `RAYNU-V-M8-TLS-OK`.
pub(super) const E4_TLS13_HOST_OK: &str = "RAYNU-V-E4-TLS13-HOST-OK";

pub(super) struct Ch13 {
    pub share: Option<[u8; 65]>,
    pub group_p256: bool,
    pub sid: [u8; 32],
    pub sid_len: u8,
}

pub(super) enum HelloClass {
    Bad,
    Tls12 { offered_13: bool },
    Tls13(Ch13),
}

fn hmac_sha256(key: &[u8]) -> Option<HmacSha256> {
    <HmacSha256 as Mac>::new_from_slice(key).ok()
}

pub(super) fn hkdf_extract(salt: &[u8], ikm: &[u8]) -> [u8; 32] {
    let mut mac = hmac_sha256(salt).expect("hmac salt");
    mac.update(ikm);
    let d = mac.finalize().into_bytes();
    let mut o = [0u8; 32];
    o.copy_from_slice(&d);
    o
}

pub(super) fn hkdf_expand(prk: &[u8], info: &[u8], out: &mut [u8]) -> bool {
    if out.is_empty() || out.len() > 32 || prk.len() != 32 {
        return false;
    }
    let Some(mut mac) = hmac_sha256(prk) else {
        return false;
    };
    mac.update(info);
    mac.update(&[1]);
    let d = mac.finalize().into_bytes();
    out.copy_from_slice(&d[..out.len()]);
    true
}

fn expand_label(secret: &[u8], label: &[u8], context: &[u8], out: &mut [u8]) -> bool {
    let full = 6 + label.len();
    if full > 255 || context.len() > 255 || out.len() > 32 {
        return false;
    }
    let mut info = [0u8; 160];
    info[0] = (out.len() >> 8) as u8;
    info[1] = out.len() as u8;
    info[2] = full as u8;
    info[3..9].copy_from_slice(b"tls13 ");
    let mut i = 9;
    info[i..i + label.len()].copy_from_slice(label);
    i += label.len();
    info[i] = context.len() as u8;
    i += 1;
    info[i..i + context.len()].copy_from_slice(context);
    i += context.len();
    hkdf_expand(secret, &info[..i], out)
}

fn derive_secret(secret: &[u8], label: &[u8], transcript: &[u8]) -> Option<[u8; 32]> {
    let mut out = [0u8; 32];
    if expand_label(secret, label, transcript, &mut out) {
        Some(out)
    } else {
        None
    }
}

fn traffic_key_iv(secret: &[u8]) -> Option<([u8; 16], [u8; 12])> {
    let mut key = [0u8; 16];
    let mut iv = [0u8; 12];
    if expand_label(secret, b"key", b"", &mut key) && expand_label(secret, b"iv", b"", &mut iv) {
        Some((key, iv))
    } else {
        None
    }
}

fn finished_verify(base: &[u8], transcript: &[u8]) -> Option<[u8; 32]> {
    let mut key = [0u8; 32];
    if !expand_label(base, b"finished", b"", &mut key) {
        return None;
    }
    let Some(mut mac) = hmac_sha256(&key) else {
        return None;
    };
    mac.update(transcript);
    let d = mac.finalize().into_bytes();
    let mut o = [0u8; 32];
    o.copy_from_slice(&d);
    key.fill(0);
    Some(o)
}

fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut d = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        d |= x ^ y;
    }
    d == 0
}

fn th_update(th: &mut Option<Sha256>, m: &[u8]) -> bool {
    match th.as_mut() {
        Some(h) => {
            h.update(m);
            true
        }
        None => false,
    }
}

fn th_hash(th: &Option<Sha256>) -> Option<[u8; 32]> {
    let d = th.as_ref()?.clone().finalize();
    let mut o = [0u8; 32];
    o.copy_from_slice(&d);
    Some(o)
}

impl Tls12Listen {
    pub(super) fn classify_hello(&self, body: &[u8]) -> HelloClass {
        let Some(view) = walk_hello(body) else {
            return HelloClass::Bad;
        };
        if view.tls13 && view.sig_pss {
            HelloClass::Tls13(Ch13 {
                share: view.share,
                group_p256: view.group_p256,
                sid: view.sid,
                sid_len: view.sid_len,
            })
        } else {
            HelloClass::Tls12 {
                offered_13: view.offered_13,
            }
        }
    }

    pub(super) fn start_tls13(&mut self, body: &[u8], whole: &[u8], ch: Ch13) -> bool {
        self.offered_tls13 = true;
        self.sid = ch.sid;
        self.sid_len = ch.sid_len;
        self.th = Some(Sha256::new());
        if !th_update(&mut self.th, whole) {
            return false;
        }
        if let Some(pt) = ch.share {
            self.peer_point = pt;
            self.peer_ok = true;
            self.proto = PROTO13;
            if !self.send_tls13_flight() {
                return false;
            }
            self.state = ST_13FIN;
            true
        } else if ch.group_p256 {
            self.proto = PROTO13;
            if !self.send_hrr() {
                return false;
            }
            self.state = ST_CH2;
            true
        } else {
            self.th = None;
            self.proto = 0;
            self.accept_tls12_hello(body, whole)
        }
    }

    pub(super) fn second_tls13_hello(&mut self, body: &[u8], whole: &[u8]) -> bool {
        let HelloClass::Tls13(ch) = self.classify_hello(body) else {
            return false;
        };
        let Some(pt) = ch.share else {
            return false;
        };
        self.sid = ch.sid;
        self.sid_len = ch.sid_len;
        self.peer_point = pt;
        self.peer_ok = true;
        if !th_update(&mut self.th, whole) {
            return false;
        }
        if !self.send_tls13_flight() {
            return false;
        }
        self.state = ST_13FIN;
        true
    }

    pub(super) fn finish_tls13(&mut self, verify: &[u8]) -> bool {
        if !ct_eq(verify, &self.expect_fin) {
            return false;
        }
        self.client_key = self.app_client_key;
        self.client_iv = self.app_client_iv;
        self.read_seq = 0;
        self.state = ST_APP;
        self.app_client_key.fill(0);
        self.app_client_iv.fill(0);
        self.expect_fin.fill(0);
        true
    }

    pub(super) fn tls13_encrypted(&mut self, body: &[u8]) -> bool {
        let mut plain = [0u8; 16640];
        let Some(n) = self.open13(body, &mut plain) else {
            return false;
        };
        match self.inner_type {
            CT_HS => self.handle_hs(&plain[..n]),
            CT_APP if self.state == ST_APP => {
                let copy = n.min(self.http.len().saturating_sub(self.http_len));
                self.http[self.http_len..self.http_len + copy].copy_from_slice(&plain[..copy]);
                self.http_len += copy;
                true
            }
            CT_ALERT => false,
            _ => false,
        }
    }

    pub(super) fn seal13(&mut self, inner: u8, plain: &[u8]) -> bool {
        // RFC 8446 §5.4: TLSInnerPlaintext is at most 2^14 + 1
        // (content plus the content-type byte). Callers fragment at 2^14.
        if plain.len() > 16384 {
            return false;
        }
        let inner_n = plain.len() + 1;
        let rec_body = inner_n + 16;
        let mut nonce = xor_iv(&self.server_iv, self.write_seq);
        let aad = aead_aad(rec_body);
        let mut buf = [0u8; 16640];
        buf[..plain.len()].copy_from_slice(plain);
        buf[plain.len()] = inner;
        let cipher = Aes128Gcm::new(Key::<Aes128Gcm>::from_slice(&self.server_key));
        let Ok(tag) = cipher.encrypt_in_place_detached(
            Nonce::from_slice(&nonce),
            &aad,
            &mut buf[..inner_n],
        ) else {
            nonce.fill(0);
            return false;
        };
        nonce.fill(0);
        let mut rec = [0u8; 16640];
        rec[..inner_n].copy_from_slice(&buf[..inner_n]);
        rec[inner_n..rec_body].copy_from_slice(&tag);
        self.write_seq = self.write_seq.wrapping_add(1);
        self.push_record(CT_APP, &rec[..rec_body])
    }

    fn open13(&mut self, rec: &[u8], out: &mut [u8]) -> Option<usize> {
        if rec.len() < 16 {
            return None;
        }
        let rec_body = rec.len();
        let mut nonce = xor_iv(&self.client_iv, self.read_seq);
        let aad = aead_aad(rec_body);
        let ct_n = rec_body - 16;
        if ct_n > out.len() {
            return None;
        }
        out[..ct_n].copy_from_slice(&rec[..ct_n]);
        let mut tag = [0u8; 16];
        tag.copy_from_slice(&rec[ct_n..]);
        let cipher = Aes128Gcm::new(Key::<Aes128Gcm>::from_slice(&self.client_key));
        if cipher
            .decrypt_in_place_detached(Nonce::from_slice(&nonce), &aad, &mut out[..ct_n], (&tag).into())
            .is_err()
        {
            nonce.fill(0);
            return None;
        }
        nonce.fill(0);
        self.read_seq = self.read_seq.wrapping_add(1);
        let mut end = ct_n;
        while end > 0 && out[end - 1] == 0 {
            end -= 1;
        }
        if end == 0 {
            return None;
        }
        self.inner_type = out[end - 1];
        Some(end - 1)
    }

    fn send_hrr(&mut self) -> bool {
        self.send_hrr_opt().unwrap_or(false)
    }

    fn send_hrr_opt(&mut self) -> Option<bool> {
        let ch_hash = th_hash(&self.th)?;
        let mut h = Sha256::new();
        let mut synth = [0u8; 36];
        synth[0] = 254;
        synth[3] = 32;
        synth[4..].copy_from_slice(&ch_hash);
        h.update(&synth);
        self.th = Some(h);
        let mut hrr = [0u8; 128];
        let n = build_sh(&mut hrr, &HRR_RANDOM, &self.sid[..self.sid_len as usize], None)?;
        if !th_update(&mut self.th, &hrr[..n]) {
            return Some(false);
        }
        Some(self.push_record(CT_HS, &hrr[..n]))
    }

    fn send_tls13_flight(&mut self) -> bool {
        self.send_tls13_flight_opt().unwrap_or(false)
    }

    fn send_tls13_flight_opt(&mut self) -> Option<bool> {
        if !self.peer_ok {
            return Some(false);
        }
        // TLS 1.3 forbids the downgrade sentinel and the HRR random.
        if self.server_random[24..] == HRR_RANDOM[24..] || self.server_random[24..] == *b"DOWNGRD\x01"
        {
            self.server_random[31] ^= 0x01;
        }
        let mut rng = MixRng;
        let secret = EphemeralSecret::random(&mut rng);
        let point = secret.public_key().to_encoded_point(false);
        let pb = point.as_bytes();
        if pb.len() != 65 {
            return Some(false);
        }
        let mut share = [0u8; 65];
        share.copy_from_slice(pb);
        let mut sh = [0u8; 192];
        let sh_n = build_sh(
            &mut sh,
            &self.server_random,
            &self.sid[..self.sid_len as usize],
            Some(&share),
        )?;
        if !self.push_record(CT_HS, &sh[..sh_n]) || !self.push_record(CT_CCS, &[1]) {
            return Some(false);
        }
        if !th_update(&mut self.th, &sh[..sh_n]) {
            return Some(false);
        }
        let peer = PublicKey::from_sec1_bytes(&self.peer_point).ok()?;
        let shared = secret.diffie_hellman(&peer);
        let mut dh = [0u8; 32];
        dh.copy_from_slice(shared.raw_secret_bytes().as_ref());
        let hs_hash = th_hash(&self.th)?;
        let keys = handshake_keys(&dh, &hs_hash)?;
        dh.fill(0);
        self.client_key = keys.client_key;
        self.client_iv = keys.client_iv;
        self.server_key = keys.server_key;
        self.server_iv = keys.server_iv;
        self.read_seq = 0;
        self.write_seq = 0;

        let mut flight = [0u8; 4608];
        let mut n = 0;
        let ee_n = push_ee(&mut flight[n..])?;
        if !th_update(&mut self.th, &flight[n..n + ee_n]) {
            return Some(false);
        }
        n += ee_n;
        if self.cert_len == 0 || self.cert_len > self.cert.len() {
            return Some(false);
        }
        let cert_n = push_cert13(&mut flight[n..], &self.cert[..self.cert_len])?;
        if !th_update(&mut self.th, &flight[n..n + cert_n]) {
            return Some(false);
        }
        n += cert_n;
        let cert_hash = th_hash(&self.th)?;
        let sig = self.sign_cert_verify(&cert_hash)?;
        let cv_n = push_cv(&mut flight[n..], &sig)?;
        if !th_update(&mut self.th, &flight[n..n + cv_n]) {
            return Some(false);
        }
        n += cv_n;
        let pre_fin = th_hash(&self.th)?;
        let server_fin = finished_verify(&keys.server_secret, &pre_fin)?;
        let fin_n = push_fin(&mut flight[n..], &server_fin)?;
        if !th_update(&mut self.th, &flight[n..n + fin_n]) {
            return Some(false);
        }
        n += fin_n;
        let done = th_hash(&self.th)?;
        self.expect_fin = finished_verify(&keys.client_secret, &done)?;
        let (app_c_key, app_c_iv, app_s_key, app_s_iv) = app_traffic(&keys.master_ready, &done)?;
        self.app_client_key = app_c_key;
        self.app_client_iv = app_c_iv;
        if !self.seal13(CT_HS, &flight[..n]) {
            return Some(false);
        }
        self.server_key = app_s_key;
        self.server_iv = app_s_iv;
        self.write_seq = 0;
        Some(true)
    }

    fn sign_cert_verify(&self, transcript: &[u8; 32]) -> Option<[u8; 256]> {
        let key = self.key.clone()?;
        let signing = SigningKey::<RsaSha256>::new(key);
        let ctx = CERT_VERIFY_CTX.len();
        let mut tbs = [0u8; 160];
        tbs[..64].fill(0x20);
        tbs[64..64 + ctx].copy_from_slice(CERT_VERIFY_CTX);
        tbs[64 + ctx] = 0;
        tbs[64 + ctx + 1..64 + ctx + 1 + 32].copy_from_slice(transcript);
        let n = 64 + ctx + 1 + 32;
        let sig = signing.try_sign_with_rng(&mut MixRng, &tbs[..n]).ok()?;
        let bytes = sig.to_bytes();
        let bytes = bytes.as_ref();
        if bytes.len() != 256 {
            return None;
        }
        let mut out = [0u8; 256];
        out.copy_from_slice(bytes);
        Some(out)
    }
}

struct HsKeys {
    client_key: [u8; 16],
    client_iv: [u8; 12],
    server_key: [u8; 16],
    server_iv: [u8; 12],
    client_secret: [u8; 32],
    server_secret: [u8; 32],
    master_ready: [u8; 32],
}

fn handshake_keys(dh: &[u8], hs_hash: &[u8]) -> Option<HsKeys> {
    let zero = [0u8; 32];
    let early = hkdf_extract(&zero, &zero);
    let empty = sha256(b"");
    let derived = derive_secret(&early, b"derived", &empty)?;
    let hs = hkdf_extract(&derived, dh);
    let csec = derive_secret(&hs, b"c hs traffic", hs_hash)?;
    let ssec = derive_secret(&hs, b"s hs traffic", hs_hash)?;
    let (client_key, client_iv) = traffic_key_iv(&csec)?;
    let (server_key, server_iv) = traffic_key_iv(&ssec)?;
    Some(HsKeys {
        client_key,
        client_iv,
        server_key,
        server_iv,
        client_secret: csec,
        server_secret: ssec,
        master_ready: hs,
    })
}

fn aead_aad(rec_body: usize) -> [u8; 5] {
    let mut aad = [0u8; 5];
    aad[0] = CT_APP;
    aad[1] = 0x03;
    aad[2] = 0x03;
    aad[3] = (rec_body >> 8) as u8;
    aad[4] = rec_body as u8;
    aad
}

fn xor_iv(iv: &[u8; 12], seq: u64) -> [u8; 12] {
    let mut nonce = *iv;
    let s = seq.to_be_bytes();
    for i in 0..8 {
        nonce[4 + i] ^= s[i];
    }
    nonce
}

struct HelloView {
    tls13: bool,
    offered_13: bool,
    sig_pss: bool,
    group_p256: bool,
    share: Option<[u8; 65]>,
    sid: [u8; 32],
    sid_len: u8,
}

fn walk_hello(body: &[u8]) -> Option<HelloView> {
    if body.len() < 34 {
        return None;
    }
    let mut i = 34;
    let sid_len = *body.get(i)? as usize;
    if sid_len > 32 {
        return None;
    }
    i += 1;
    if i + sid_len > body.len() {
        return None;
    }
    let mut sid = [0u8; 32];
    sid[..sid_len].copy_from_slice(&body[i..i + sid_len]);
    i += sid_len;
    let cs_len = read_u16(body, i)? as usize;
    i += 2;
    if cs_len % 2 != 0 || i + cs_len > body.len() {
        return None;
    }
    let mut cipher13 = false;
    let mut c = i;
    while c + 1 < i + cs_len {
        if read_u16(body, c) == Some(CS_AES128_GCM) {
            cipher13 = true;
        }
        c += 2;
    }
    i += cs_len;
    let comp_len = *body.get(i)? as usize;
    i += 1 + comp_len;
    if i > body.len() {
        return None;
    }
    let mut view = HelloView {
        tls13: false,
        offered_13: false,
        sig_pss: false,
        group_p256: false,
        share: None,
        sid,
        sid_len: sid_len as u8,
    };
    if i == body.len() {
        return Some(view);
    }
    if i + 2 > body.len() {
        return None;
    }
    let ext_len = read_u16(body, i)? as usize;
    i += 2;
    let end = i + ext_len;
    if end != body.len() {
        return None;
    }
    while i + 4 <= end {
        let typ = read_u16(body, i)?;
        let n = read_u16(body, i + 2)? as usize;
        i += 4;
        if i + n > end {
            return None;
        }
        let ext = &body[i..i + n];
        match typ {
            EXT_VERSIONS => {
                if ext.is_empty() {
                    return None;
                }
                let list = ext[0] as usize;
                if 1 + list != ext.len() || list % 2 != 0 {
                    return None;
                }
                let mut v = 1;
                while v + 1 < ext.len() {
                    if read_u16(ext, v) == Some(VER_13) {
                        view.offered_13 = true;
                    }
                    v += 2;
                }
            }
            EXT_GROUPS => {
                if ext.len() < 2 {
                    return None;
                }
                let list = read_u16(ext, 0)? as usize;
                if 2 + list != ext.len() || list % 2 != 0 {
                    return None;
                }
                let mut g = 2;
                while g + 1 < ext.len() {
                    if read_u16(ext, g) == Some(GRP_P256) {
                        view.group_p256 = true;
                    }
                    g += 2;
                }
            }
            EXT_SIGALGS => {
                if ext.len() < 2 {
                    return None;
                }
                let list = read_u16(ext, 0)? as usize;
                if 2 + list != ext.len() || list % 2 != 0 {
                    return None;
                }
                let mut s = 2;
                while s + 1 < ext.len() {
                    if read_u16(ext, s) == Some(SIG_PSS_SHA256) {
                        view.sig_pss = true;
                    }
                    s += 2;
                }
            }
            EXT_KEY_SHARE => {
                if ext.len() < 2 {
                    return None;
                }
                let list = read_u16(ext, 0)? as usize;
                if 2 + list != ext.len() {
                    return None;
                }
                let mut k = 2;
                let kend = 2 + list;
                while k + 4 <= kend {
                    let group = read_u16(ext, k)?;
                    let kn = read_u16(ext, k + 2)? as usize;
                    k += 4;
                    if k + kn > kend {
                        return None;
                    }
                    if group == GRP_P256 && kn == 65 && ext[k] == 0x04 {
                        let mut pt = [0u8; 65];
                        pt.copy_from_slice(&ext[k..k + 65]);
                        view.share = Some(pt);
                    }
                    k += kn;
                }
                if k != kend {
                    return None;
                }
            }
            _ => {}
        }
        i += n;
    }
    if i != end {
        return None;
    }
    view.tls13 = view.offered_13 && cipher13;
    Some(view)
}

/// `share` is `Some` for a real ServerHello and `None` for HelloRetryRequest.
fn build_sh(out: &mut [u8], random: &[u8; 32], sid: &[u8], share: Option<&[u8; 65]>) -> Option<usize> {
    // supported_versions is 6 bytes. key_share is 73 with a point
    // (type, length, group, point length, 65-byte point) and 6 for
    // HelloRetryRequest (type, length, selected group).
    let ext_n: usize = if share.is_some() { 6 + 73 } else { 6 + 6 };
    let body = 2 + 32 + 1 + sid.len() + 2 + 1 + 2 + ext_n;
    let tot = 4 + body;
    if out.len() < tot || sid.len() > 32 {
        return None;
    }
    out[0] = HS_SH;
    out[1] = (body >> 16) as u8;
    out[2] = (body >> 8) as u8;
    out[3] = body as u8;
    out[4] = 0x03;
    out[5] = 0x03;
    out[6..38].copy_from_slice(random);
    out[38] = sid.len() as u8;
    out[39..39 + sid.len()].copy_from_slice(sid);
    let mut i = 39 + sid.len();
    out[i] = 0x13;
    out[i + 1] = 0x01;
    out[i + 2] = 0;
    i += 3;
    out[i] = (ext_n >> 8) as u8;
    out[i + 1] = ext_n as u8;
    i += 2;
    // supported_versions
    out[i..i + 6].copy_from_slice(&[0x00, 0x2b, 0x00, 0x02, 0x03, 0x04]);
    i += 6;
    // key_share
    out[i] = 0x00;
    out[i + 1] = 0x33;
    i += 2;
    if let Some(pt) = share {
        out[i] = 0x00;
        out[i + 1] = 69;
        i += 2;
        out[i] = 0x00;
        out[i + 1] = 0x17;
        i += 2;
        out[i] = 0x00;
        out[i + 1] = 65;
        i += 2;
        out[i..i + 65].copy_from_slice(pt);
    } else {
        out[i] = 0x00;
        out[i + 1] = 0x02;
        out[i + 2] = 0x00;
        out[i + 3] = 0x17;
    }
    Some(tot)
}

fn push_ee(out: &mut [u8]) -> Option<usize> {
    if out.len() < 6 {
        return None;
    }
    out[..6].copy_from_slice(&[0x08, 0x00, 0x00, 0x02, 0x00, 0x00]);
    Some(6)
}

fn push_cert13(out: &mut [u8], cert: &[u8]) -> Option<usize> {
    // context<0..255> = empty, then one entry: cert || empty extensions.
    let list = 3 + cert.len() + 2;
    let body = 1 + 3 + list;
    let tot = 4 + body;
    if out.len() < tot {
        return None;
    }
    out[0] = HS_CERT;
    out[1] = (body >> 16) as u8;
    out[2] = (body >> 8) as u8;
    out[3] = body as u8;
    out[4] = 0;
    out[5] = (list >> 16) as u8;
    out[6] = (list >> 8) as u8;
    out[7] = list as u8;
    out[8] = (cert.len() >> 16) as u8;
    out[9] = (cert.len() >> 8) as u8;
    out[10] = cert.len() as u8;
    out[11..11 + cert.len()].copy_from_slice(cert);
    let e = 11 + cert.len();
    out[e] = 0;
    out[e + 1] = 0;
    Some(tot)
}

fn push_cv(out: &mut [u8], sig: &[u8]) -> Option<usize> {
    let body = 4 + sig.len();
    let tot = 4 + body;
    if out.len() < tot || sig.len() > 512 {
        return None;
    }
    out[0] = 15;
    out[1] = (body >> 16) as u8;
    out[2] = (body >> 8) as u8;
    out[3] = body as u8;
    out[4] = 0x08;
    out[5] = 0x04;
    out[6..8].copy_from_slice(&be16(sig.len() as u16));
    out[8..8 + sig.len()].copy_from_slice(sig);
    Some(tot)
}

fn push_fin(out: &mut [u8], verify: &[u8; 32]) -> Option<usize> {
    if out.len() < 36 {
        return None;
    }
    out[0] = HS_FIN;
    out[1] = 0;
    out[2] = 0;
    out[3] = 32;
    out[4..36].copy_from_slice(verify);
    Some(36)
}

fn app_traffic(hs_secret: &[u8; 32], done: &[u8]) -> Option<([u8; 16], [u8; 12], [u8; 16], [u8; 12])> {
    let empty = sha256(b"");
    let derived = derive_secret(hs_secret, b"derived", &empty)?;
    let zero = [0u8; 32];
    let master = hkdf_extract(&derived, &zero);
    let csec = derive_secret(&master, b"c ap traffic", done)?;
    let ssec = derive_secret(&master, b"s ap traffic", done)?;
    let (ck, ci) = traffic_key_iv(&csec)?;
    let (sk, si) = traffic_key_iv(&ssec)?;
    Some((ck, ci, sk, si))
}

#[cfg(test)]
mod hkdf_test {
    use super::{hkdf_expand, hkdf_extract};

    #[test]
    fn rfc5869_case_1_sha256() {
        let ikm = [0x0bu8; 22];
        let salt = [
            0x00u8, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c,
        ];
        let info = [0xf0u8, 0xf1, 0xf2, 0xf3, 0xf4, 0xf5, 0xf6, 0xf7, 0xf8, 0xf9];
        let prk = hkdf_extract(&salt, &ikm);
        assert_eq!(
            prk,
            [
                0x07, 0x77, 0x09, 0x36, 0x2c, 0x2e, 0x32, 0xdf, 0x0d, 0xdc, 0x3f, 0x0d, 0xc4, 0x7b,
                0xba, 0x63, 0x90, 0xb6, 0xc7, 0x3b, 0xb5, 0x0f, 0x9c, 0x31, 0x22, 0xec, 0x84, 0x4a,
                0xd7, 0xc2, 0xb3, 0xe5
            ]
        );
        let mut okm = [0u8; 42];
        // One-block expand only covers 32 bytes. Check the first 32 of the
        // RFC vector; the flight never asks HKDF-Expand for more than 32.
        assert!(hkdf_expand(&prk, &info, &mut okm[..32]));
        assert_eq!(
            &okm[..32],
            &[
                0x3c, 0xb2, 0x5f, 0x25, 0xfa, 0xac, 0xd5, 0x7a, 0x90, 0x43, 0x4f, 0x64, 0xd0, 0x36,
                0x2f, 0x2a, 0x2d, 0x2d, 0x0a, 0x90, 0xcf, 0x1a, 0x5a, 0x4c, 0x5d, 0xb0, 0x2d, 0x56,
                0xec, 0xc4, 0xc5, 0xbf
            ]
        );
    }
}
