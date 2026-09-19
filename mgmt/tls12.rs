//! Freestanding TLS 1.2 server for coexist (outside Proven Core).
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** (ADR-002 / ADR-018). Do not touch VMX/EPT.
//! VERIFICATION: L1 host tests (rustls TLS 1.2 client ↔ this server).
//!
//! Cipher: `TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256` (0xC02F) + Extended
//! Master Secret. rustls/ring stay **out** of `uefi-bin`. Lab millicert is
//! `assets/tls/lab.crt.der`. Iron close is still `curl --cacert` on the R640
//! (`RAYNU-V-M8-TLS-OK`). Host/CI never print that marker.

#![cfg(any(test, feature = "uefi-bin"))]

extern crate alloc;

use crate::mgmt::tls_coexist::{request_complete, COEXIST_HTTP_OUT_N, COEXIST_RX_ACC_N};
use aes_gcm::aead::{AeadInPlace, KeyInit};
use aes_gcm::{Aes128Gcm, Key, Nonce};
use hmac::{Hmac, Mac};
use p256::ecdh::EphemeralSecret;
use p256::elliptic_curve::rand_core::{CryptoRng, RngCore};
use p256::elliptic_curve::sec1::ToEncodedPoint;
use p256::PublicKey;
use rsa::pkcs1v15::Pkcs1v15Sign;
use rsa::pkcs8::DecodePrivateKey;
use rsa::sha2::Sha256 as RsaSha256;
use rsa::RsaPrivateKey;
use sha2::{Digest, Sha256};

type HmacSha256 = Hmac<Sha256>;

/// Host/CI: rustls TLS 1.2 client spoke to this server. Not iron.
pub const M8_TLS12_HOST_OK_MARKER: &str = "RAYNU-V-M8-TLS12-HOST-OK";

/// Honesty: in-tree TLS 1.2 ≠ iron HTTPS.
pub const TLS12_RESIDUAL_NOTE: &str =
    "residual: freestanding TLS 1.2 ECDHE-RSA-AES128-GCM is not iron RAYNU-V-M8-TLS-OK; rustls/ring stay out of uefi-bin; nested QEMU ≠ R640; do not print iron TLS-OK from host/CI";

const LAB_CERT: &[u8] = include_bytes!("../assets/tls/lab.crt.der");
const LAB_KEY: &[u8] = include_bytes!("../assets/tls/lab.key.der");
const LAB_CA: &[u8] = include_bytes!("../assets/tls/lab-ca.crt.der");

const TLS12: u16 = 0x0303;
const CT_CCS: u8 = 20;
const CT_ALERT: u8 = 21;
const CT_HS: u8 = 22;
const CT_APP: u8 = 23;
const HS_CH: u8 = 1;
const HS_SH: u8 = 2;
const HS_CERT: u8 = 11;
const HS_SKE: u8 = 12;
const HS_SHD: u8 = 14;
const HS_CKE: u8 = 16;
const HS_FIN: u8 = 20;
const CS_ECDHE_RSA_AES128_GCM: u16 = 0xC02F;
const EXT_EMS: u16 = 0x0017;
const EXT_RENEG: u16 = 0xff01;
const TRANS_N: usize = 4096;
const KEY_BLOCK_N: usize = 40;

const ST_CH: u8 = 0;
const ST_CKE: u8 = 1;
const ST_CCS: u8 = 2;
const ST_FIN: u8 = 3;
const ST_APP: u8 = 4;
const ST_FAIL: u8 = 5;

struct MixRng;

impl MixRng {
    fn word(&mut self) -> u64 {
        if let Some(x) = rdrand64() {
            return x;
        }
        crate::arch::cpu::rdtsc().wrapping_mul(0x9E37_79B9_7F4A_7C15)
    }
}

impl RngCore for MixRng {
    fn next_u32(&mut self) -> u32 {
        self.word() as u32
    }
    fn next_u64(&mut self) -> u64 {
        self.word()
    }
    fn fill_bytes(&mut self, dest: &mut [u8]) {
        let mut i = 0;
        while i < dest.len() {
            let w = self.word().to_le_bytes();
            let n = (dest.len() - i).min(8);
            dest[i..i + n].copy_from_slice(&w[..n]);
            i += n;
        }
    }
    fn try_fill_bytes(
        &mut self,
        dest: &mut [u8],
    ) -> Result<(), p256::elliptic_curve::rand_core::Error> {
        self.fill_bytes(dest);
        Ok(())
    }
}

impl CryptoRng for MixRng {}

fn rdrand64() -> Option<u64> {
    // Firmware: never emit RDRAND. TCG qemu64 CPUID can advertise the bit
    // while TCG still #UDs (`1647a8d8` / `06d37b10` HOST-NIC smoke died
    // after `HOST-NIC e1000 MAC=` inside `Tls12Listen::new`). MixRng falls
    // back to rdtsc mix. Host tests keep the CPUID-gated intrinsic.
    #[cfg(all(target_arch = "x86_64", not(feature = "uefi-bin")))]
    {
        if !crate::arch::cpu::rdrand_supported() {
            return None;
        }
        let mut v = 0u64;
        for _ in 0..32 {
            // SAFETY: CPUID.1:ECX.RDRAND is set; 0 means retry.
            // KANI-TARGET: host tests use MixRng; uefi-bin never takes this path.
            if unsafe { core::arch::x86_64::_rdrand64_step(&mut v) } == 1 {
                return Some(v);
            }
        }
        None
    }
    #[cfg(not(all(target_arch = "x86_64", not(feature = "uefi-bin"))))]
    {
        None
    }
}

fn be16(n: u16) -> [u8; 2] {
    n.to_be_bytes()
}

fn read_u16(b: &[u8], i: usize) -> Option<u16> {
    Some(u16::from_be_bytes([*b.get(i)?, *b.get(i + 1)?]))
}

fn read_u24(b: &[u8], i: usize) -> Option<usize> {
    Some(((*b.get(i)? as usize) << 16) | ((*b.get(i + 1)? as usize) << 8) | (*b.get(i + 2)? as usize))
}

fn hmac_sha256(key: &[u8], data: &[u8], out: &mut [u8; 32]) -> bool {
    let Ok(mut m) = <HmacSha256 as Mac>::new_from_slice(key) else {
        return false;
    };
    m.update(data);
    out.copy_from_slice(&m.finalize().into_bytes());
    true
}

fn p_hash(secret: &[u8], seed: &[u8], out: &mut [u8]) -> bool {
    let mut a = [0u8; 32];
    if !hmac_sha256(secret, seed, &mut a) {
        return false;
    }
    let mut off = 0;
    while off < out.len() {
        let mut buf = [0u8; 32 + 256];
        if seed.len() > 256 {
            return false;
        }
        buf[..32].copy_from_slice(&a);
        buf[32..32 + seed.len()].copy_from_slice(seed);
        let mut chunk = [0u8; 32];
        if !hmac_sha256(secret, &buf[..32 + seed.len()], &mut chunk) {
            return false;
        }
        let n = (out.len() - off).min(32);
        out[off..off + n].copy_from_slice(&chunk[..n]);
        off += n;
        if off < out.len() {
            let prev = a;
            if !hmac_sha256(secret, &prev, &mut a) {
                return false;
            }
        }
    }
    true
}

fn prf(secret: &[u8], label: &[u8], seed: &[u8], out: &mut [u8]) -> bool {
    let mut cat = [0u8; 96];
    let n = label.len() + seed.len();
    if n > cat.len() {
        return false;
    }
    cat[..label.len()].copy_from_slice(label);
    cat[label.len()..n].copy_from_slice(seed);
    p_hash(secret, &cat[..n], out)
}

fn sha256(data: &[u8]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(data);
    let d = h.finalize();
    let mut o = [0u8; 32];
    o.copy_from_slice(&d);
    o
}

/// Coexist TLS 1.2 session (one TCP slot).
pub struct Tls12Listen {
    key: Option<RsaPrivateKey>,
    ecdhe: Option<EphemeralSecret>,
    rx: [u8; COEXIST_RX_ACC_N],
    rx_len: usize,
    tx: [u8; COEXIST_HTTP_OUT_N],
    tx_len: usize,
    http: [u8; COEXIST_RX_ACC_N],
    http_len: usize,
    trans: [u8; TRANS_N],
    trans_len: usize,
    client_random: [u8; 32],
    server_random: [u8; 32],
    master: [u8; 48],
    client_key: [u8; 16],
    server_key: [u8; 16],
    client_iv: [u8; 4],
    server_iv: [u8; 4],
    read_seq: u64,
    write_seq: u64,
    ems: bool,
    state: u8,
}

impl Tls12Listen {
    pub fn new() -> Self {
        let mut s = Self {
            key: RsaPrivateKey::from_pkcs8_der(LAB_KEY).ok(),
            ecdhe: None,
            rx: [0; COEXIST_RX_ACC_N],
            rx_len: 0,
            tx: [0; COEXIST_HTTP_OUT_N],
            tx_len: 0,
            http: [0; COEXIST_RX_ACC_N],
            http_len: 0,
            trans: [0; TRANS_N],
            trans_len: 0,
            client_random: [0; 32],
            server_random: [0; 32],
            master: [0; 48],
            client_key: [0; 16],
            server_key: [0; 16],
            client_iv: [0; 4],
            server_iv: [0; 4],
            read_seq: 0,
            write_seq: 0,
            ems: false,
            state: ST_CH,
        };
        MixRng.fill_bytes(&mut s.server_random);
        s
    }

    pub const fn empty() -> Self {
        Self {
            key: None,
            ecdhe: None,
            rx: [0; COEXIST_RX_ACC_N],
            rx_len: 0,
            tx: [0; COEXIST_HTTP_OUT_N],
            tx_len: 0,
            http: [0; COEXIST_RX_ACC_N],
            http_len: 0,
            trans: [0; TRANS_N],
            trans_len: 0,
            client_random: [0; 32],
            server_random: [0; 32],
            master: [0; 48],
            client_key: [0; 16],
            server_key: [0; 16],
            client_iv: [0; 4],
            server_iv: [0; 4],
            read_seq: 0,
            write_seq: 0,
            ems: false,
            state: ST_CH,
        }
    }

    pub fn load_lab_material(&mut self) {
        if self.key.is_none() {
            self.key = RsaPrivateKey::from_pkcs8_der(LAB_KEY).ok();
        }
        MixRng.fill_bytes(&mut self.server_random);
        self.state = ST_CH;
    }

    pub fn reset(&mut self) {
        let key = self.key.take();
        self.ecdhe = None;
        self.rx_len = 0;
        self.tx_len = 0;
        self.http_len = 0;
        self.trans_len = 0;
        self.client_random = [0; 32];
        self.server_random = [0; 32];
        self.master = [0; 48];
        self.client_key = [0; 16];
        self.server_key = [0; 16];
        self.client_iv = [0; 4];
        self.server_iv = [0; 4];
        self.read_seq = 0;
        self.write_seq = 0;
        self.ems = false;
        self.state = ST_CH;
        self.key = key.or_else(|| RsaPrivateKey::from_pkcs8_der(LAB_KEY).ok());
        MixRng.fill_bytes(&mut self.server_random);
    }

    pub fn feed_tcp(&mut self, chunk: &[u8]) -> usize {
        if chunk.is_empty() || self.state == ST_FAIL {
            return 0;
        }
        let copy = chunk.len().min(self.rx.len().saturating_sub(self.rx_len));
        if copy == 0 {
            return 0;
        }
        self.rx[self.rx_len..self.rx_len + copy].copy_from_slice(&chunk[..copy]);
        self.rx_len += copy;
        self.pump();
        copy
    }

    pub fn drain_tcp(&mut self, out: &mut [u8]) -> usize {
        let n = self.tx_len.min(out.len());
        if n == 0 {
            return 0;
        }
        out[..n].copy_from_slice(&self.tx[..n]);
        if n < self.tx_len {
            self.tx.copy_within(n..self.tx_len, 0);
            self.tx_len -= n;
        } else {
            self.tx_len = 0;
        }
        n
    }

    pub fn take_http(&self) -> Option<&[u8]> {
        if self.state == ST_APP && request_complete(&self.http[..self.http_len]) {
            Some(&self.http[..self.http_len])
        } else {
            None
        }
    }

    pub fn wrap_http(&mut self, http: &[u8], tcp_out: &mut [u8]) -> usize {
        if self.state != ST_APP || http.is_empty() {
            return 0;
        }
        // TLS 1.2 application_data plaintext max is 2^14. Fragment SPA >16KiB.
        let mut off = 0;
        while off < http.len() {
            let n = (http.len() - off).min(16384);
            if !self.seal(CT_APP, &http[off..off + n]) {
                return 0;
            }
            off += n;
        }
        self.drain_tcp(tcp_out)
    }

    pub fn debug_state(&self) -> u8 {
        self.state
    }

    pub fn key_ready(&self) -> bool {
        self.key.is_some()
    }

    fn pump(&mut self) {
        loop {
            if self.rx_len < 5 {
                return;
            }
            let typ = self.rx[0];
            let len = match read_u16(&self.rx, 3) {
                Some(n) => n as usize,
                None => return,
            };
            let rec = 5 + len;
            if self.rx_len < rec {
                return;
            }
            let mut body = [0u8; 16640];
            if len > body.len() {
                self.state = ST_FAIL;
                return;
            }
            body[..len].copy_from_slice(&self.rx[5..rec]);
            self.rx.copy_within(rec..self.rx_len, 0);
            self.rx_len -= rec;
            if !self.handle_record(typ, &body[..len]) {
                self.state = ST_FAIL;
                return;
            }
        }
    }

    fn handle_record(&mut self, typ: u8, body: &[u8]) -> bool {
        match typ {
            CT_ALERT => false,
            CT_CCS => {
                if self.state != ST_CCS || body != [1] {
                    return false;
                }
                self.state = ST_FIN;
                true
            }
            CT_HS if self.state == ST_FIN || self.state == ST_APP => {
                let mut plain = [0u8; 16640];
                let n = match self.open(CT_HS, body, &mut plain) {
                    Some(n) => n,
                    None => return false,
                };
                self.handle_hs(&plain[..n])
            }
            CT_HS => self.handle_hs(body),
            CT_APP if self.state == ST_APP => {
                let mut plain = [0u8; 16640];
                let n = match self.open(CT_APP, body, &mut plain) {
                    Some(n) => n,
                    None => return false,
                };
                let copy = n.min(self.http.len().saturating_sub(self.http_len));
                self.http[self.http_len..self.http_len + copy].copy_from_slice(&plain[..copy]);
                self.http_len += copy;
                true
            }
            _ => false,
        }
    }

    fn handle_hs(&mut self, mut msg: &[u8]) -> bool {
        while msg.len() >= 4 {
            let typ = msg[0];
            let n = match read_u24(msg, 1) {
                Some(n) => n,
                None => return false,
            };
            if msg.len() < 4 + n {
                return false;
            }
            let body = &msg[4..4 + n];
            let whole = &msg[..4 + n];
            match (self.state, typ) {
                (ST_CH, HS_CH) => {
                    if !self.parse_ch(body) {
                        return false;
                    }
                    if !self.append_trans(whole) {
                        return false;
                    }
                    if !self.send_server_flight() {
                        return false;
                    }
                    self.state = ST_CKE;
                }
                (ST_CKE, HS_CKE) => {
                    if !self.parse_cke(body) {
                        return false;
                    }
                    if !self.append_trans(whole) {
                        return false;
                    }
                    if !self.compute_master() {
                        return false;
                    }
                    if !self.derive_keys() {
                        return false;
                    }
                    self.state = ST_CCS;
                }
                (ST_FIN, HS_FIN) => {
                    if n != 12 || !self.check_client_finished(body) {
                        return false;
                    }
                    if !self.append_trans(whole) {
                        return false;
                    }
                    if !self.send_server_finished() {
                        return false;
                    }
                    self.state = ST_APP;
                }
                _ => return false,
            }
            msg = &msg[4 + n..];
        }
        msg.is_empty()
    }

    fn parse_ch(&mut self, body: &[u8]) -> bool {
        if body.len() < 34 {
            return false;
        }
        self.client_random.copy_from_slice(&body[2..34]);
        let mut i = 34;
        let Some(&sid_len_b) = body.get(i) else {
            return false;
        };
        i += 1 + sid_len_b as usize;
        let Some(cs_len) = read_u16(body, i).map(|n| n as usize) else {
            return false;
        };
        i += 2;
        if i + cs_len > body.len() || cs_len % 2 != 0 {
            return false;
        }
        let mut want = false;
        let mut c = i;
        while c + 1 < i + cs_len {
            if read_u16(body, c) == Some(CS_ECDHE_RSA_AES128_GCM) {
                want = true;
            }
            c += 2;
        }
        if !want {
            return false;
        }
        i += cs_len;
        let Some(&comp_len_b) = body.get(i) else {
            return false;
        };
        i += 1 + comp_len_b as usize;
        self.ems = false;
        if i + 2 <= body.len() {
            let Some(ext_len) = read_u16(body, i).map(|n| n as usize) else {
                return false;
            };
            i += 2;
            let end = i + ext_len;
            if end > body.len() {
                return false;
            }
            while i + 4 <= end {
                let Some(typ) = read_u16(body, i) else {
                    return false;
                };
                let Some(n) = read_u16(body, i + 2).map(|n| n as usize) else {
                    return false;
                };
                i += 4;
                if i + n > end {
                    return false;
                }
                if typ == EXT_EMS {
                    self.ems = true;
                }
                i += n;
            }
        }
        true
    }

    fn send_server_flight(&mut self) -> bool {
        let mut rng = MixRng;
        let secret = EphemeralSecret::random(&mut rng);
        let point = secret.public_key().to_encoded_point(false);
        let pb = point.as_bytes();
        if pb.len() != 65 {
            return false;
        }
        let mut params = [0u8; 4 + 65];
        params[0] = 3;
        params[1] = 0x00;
        params[2] = 0x17;
        params[3] = 65;
        params[4..].copy_from_slice(pb);
        let mut tbs = [0u8; 32 + 32 + 4 + 65];
        tbs[..32].copy_from_slice(&self.client_random);
        tbs[32..64].copy_from_slice(&self.server_random);
        tbs[64..].copy_from_slice(&params);
        let hash = sha256(&tbs);
        let Some(key) = self.key.as_ref() else {
            return false;
        };
        let pad = Pkcs1v15Sign::new::<RsaSha256>();
        let Ok(sig) = key.sign(pad, &hash) else {
            return false;
        };
        if sig.len() != 256 {
            return false;
        }
        self.ecdhe = Some(secret);

        let mut hs = [0u8; 2048];
        let mut n = 0;
        let Some(a) = push_sh(&mut hs[n..], &self.server_random, self.ems) else {
            return false;
        };
        n += a;
        let Some(b) = push_cert(&mut hs[n..], LAB_CERT) else {
            return false;
        };
        n += b;
        let Some(c) = push_ske(&mut hs[n..], &params, &sig) else {
            return false;
        };
        n += c;
        let Some(d) = push_shd(&mut hs[n..]) else {
            return false;
        };
        n += d;
        if !self.append_trans(&hs[..n]) {
            return false;
        }
        self.push_record(CT_HS, &hs[..n])
    }

    fn parse_cke(&mut self, body: &[u8]) -> bool {
        if body.len() != 66 || body[0] != 65 {
            return false;
        }
        let Ok(peer) = PublicKey::from_sec1_bytes(&body[1..]) else {
            return false;
        };
        let Some(secret) = self.ecdhe.take() else {
            return false;
        };
        let shared = secret.diffie_hellman(&peer);
        let premaster = shared.raw_secret_bytes();
        self.master[..32].copy_from_slice(premaster.as_ref());
        self.master[32..].fill(0);
        true
    }

    fn compute_master(&mut self) -> bool {
        let mut premaster = [0u8; 32];
        premaster.copy_from_slice(&self.master[..32]);
        let hash = sha256(&self.trans[..self.trans_len]);
        let mut seed = [0u8; 64];
        let (label, seedn) = if self.ems {
            seed[..32].copy_from_slice(&hash);
            (b"extended master secret".as_slice(), 32usize)
        } else {
            seed[..32].copy_from_slice(&self.client_random);
            seed[32..].copy_from_slice(&self.server_random);
            (b"master secret".as_slice(), 64usize)
        };
        self.master.fill(0);
        prf(&premaster, label, &seed[..seedn], &mut self.master)
    }

    fn derive_keys(&mut self) -> bool {
        let mut seed = [0u8; 64];
        seed[..32].copy_from_slice(&self.server_random);
        seed[32..].copy_from_slice(&self.client_random);
        let mut block = [0u8; KEY_BLOCK_N];
        if !prf(&self.master, b"key expansion", &seed, &mut block) {
            return false;
        }
        self.client_key.copy_from_slice(&block[0..16]);
        self.server_key.copy_from_slice(&block[16..32]);
        self.client_iv.copy_from_slice(&block[32..36]);
        self.server_iv.copy_from_slice(&block[36..40]);
        self.read_seq = 0;
        self.write_seq = 0;
        true
    }

    fn check_client_finished(&self, verify: &[u8]) -> bool {
        let mut out = [0u8; 12];
        let hash = sha256(&self.trans[..self.trans_len]);
        if !prf(&self.master, b"client finished", &hash, &mut out) {
            return false;
        }
        out == verify
    }

    fn send_server_finished(&mut self) -> bool {
        if !self.push_record(CT_CCS, &[1]) {
            return false;
        }
        let hash = sha256(&self.trans[..self.trans_len]);
        let mut verify = [0u8; 12];
        if !prf(&self.master, b"server finished", &hash, &mut verify) {
            return false;
        }
        let mut fin = [0u8; 16];
        fin[0] = HS_FIN;
        fin[1] = 0;
        fin[2] = 0;
        fin[3] = 12;
        fin[4..].copy_from_slice(&verify);
        if !self.append_trans(&fin) {
            return false;
        }
        self.seal(CT_HS, &fin)
    }

    fn append_trans(&mut self, m: &[u8]) -> bool {
        if self.trans_len + m.len() > self.trans.len() {
            return false;
        }
        self.trans[self.trans_len..self.trans_len + m.len()].copy_from_slice(m);
        self.trans_len += m.len();
        true
    }

    fn push_record(&mut self, typ: u8, body: &[u8]) -> bool {
        let n = 5 + body.len();
        if self.tx_len + n > self.tx.len() {
            return false;
        }
        let i = self.tx_len;
        self.tx[i] = typ;
        self.tx[i + 1] = 0x03;
        self.tx[i + 2] = 0x03;
        self.tx[i + 3] = (body.len() >> 8) as u8;
        self.tx[i + 4] = body.len() as u8;
        self.tx[i + 5..i + n].copy_from_slice(body);
        self.tx_len += n;
        true
    }

    fn seal(&mut self, typ: u8, plain: &[u8]) -> bool {
        if plain.len() > 16384 {
            return false;
        }
        let mut nonce = [0u8; 12];
        nonce[..4].copy_from_slice(&self.server_iv);
        nonce[4..].copy_from_slice(&self.write_seq.to_be_bytes());
        let mut aad = [0u8; 13];
        aad[..8].copy_from_slice(&self.write_seq.to_be_bytes());
        aad[8] = typ;
        aad[9] = 0x03;
        aad[10] = 0x03;
        aad[11..13].copy_from_slice(&(plain.len() as u16).to_be_bytes());
        let mut buf = [0u8; 16640];
        buf[..plain.len()].copy_from_slice(plain);
        let cipher = Aes128Gcm::new(Key::<Aes128Gcm>::from_slice(&self.server_key));
        let Ok(tag) = cipher.encrypt_in_place_detached(
            Nonce::from_slice(&nonce),
            &aad,
            &mut buf[..plain.len()],
        ) else {
            return false;
        };
        let rec_len = 8 + plain.len() + 16;
        let mut rec = [0u8; 16640];
        rec[..8].copy_from_slice(&self.write_seq.to_be_bytes());
        rec[8..8 + plain.len()].copy_from_slice(&buf[..plain.len()]);
        rec[8 + plain.len()..rec_len].copy_from_slice(&tag);
        self.write_seq = self.write_seq.wrapping_add(1);
        self.push_record(typ, &rec[..rec_len])
    }

    fn open(&mut self, typ: u8, rec: &[u8], out: &mut [u8]) -> Option<usize> {
        if rec.len() < 8 + 16 {
            return None;
        }
        let explicit = &rec[..8];
        let tag = &rec[rec.len() - 16..];
        let ct = &rec[8..rec.len() - 16];
        let mut nonce = [0u8; 12];
        nonce[..4].copy_from_slice(&self.client_iv);
        nonce[4..].copy_from_slice(explicit);
        let mut aad = [0u8; 13];
        aad[..8].copy_from_slice(&self.read_seq.to_be_bytes());
        aad[8] = typ;
        aad[9] = 0x03;
        aad[10] = 0x03;
        aad[11..13].copy_from_slice(&(ct.len() as u16).to_be_bytes());
        if ct.len() > out.len() {
            return None;
        }
        out[..ct.len()].copy_from_slice(ct);
        let cipher = Aes128Gcm::new(Key::<Aes128Gcm>::from_slice(&self.client_key));
        let mut tag_arr = [0u8; 16];
        tag_arr.copy_from_slice(tag);
        cipher
            .decrypt_in_place_detached(
                Nonce::from_slice(&nonce),
                &aad,
                &mut out[..ct.len()],
                (&tag_arr).into(),
            )
            .ok()?;
        self.read_seq = self.read_seq.wrapping_add(1);
        Some(ct.len())
    }
}

fn push_sh(out: &mut [u8], random: &[u8; 32], ems: bool) -> Option<usize> {
    let ext_n = if ems { 4 + 5 } else { 5 };
    let body_n = 2 + 32 + 1 + 2 + 1 + 2 + ext_n;
    let tot = 4 + body_n;
    if out.len() < tot {
        return None;
    }
    out[0] = HS_SH;
    write_u24(&mut out[1..], body_n);
    out[4] = 0x03;
    out[5] = 0x03;
    out[6..38].copy_from_slice(random);
    out[38] = 0;
    out[39] = 0xC0;
    out[40] = 0x2F;
    out[41] = 0;
    out[42] = (ext_n >> 8) as u8;
    out[43] = ext_n as u8;
    let mut i = 44;
    if ems {
        out[i..i + 4].copy_from_slice(&[0x00, 0x17, 0x00, 0x00]);
        i += 4;
    }
    out[i..i + 5].copy_from_slice(&[0xff, 0x01, 0x00, 0x01, 0x00]);
    Some(tot)
}

fn push_cert(out: &mut [u8], cert: &[u8]) -> Option<usize> {
    let inner = 3 + cert.len();
    let body = 3 + inner;
    let tot = 4 + body;
    if out.len() < tot {
        return None;
    }
    out[0] = HS_CERT;
    write_u24(&mut out[1..], body);
    write_u24(&mut out[4..], inner);
    write_u24(&mut out[7..], cert.len());
    out[10..10 + cert.len()].copy_from_slice(cert);
    Some(tot)
}

fn push_ske(out: &mut [u8], params: &[u8], sig: &[u8]) -> Option<usize> {
    let body = params.len() + 2 + 2 + sig.len();
    let tot = 4 + body;
    if out.len() < tot {
        return None;
    }
    out[0] = HS_SKE;
    write_u24(&mut out[1..], body);
    out[4..4 + params.len()].copy_from_slice(params);
    let i = 4 + params.len();
    out[i] = 0x04;
    out[i + 1] = 0x01;
    out[i + 2..i + 4].copy_from_slice(&be16(sig.len() as u16));
    out[i + 4..i + 4 + sig.len()].copy_from_slice(sig);
    Some(tot)
}

fn push_shd(out: &mut [u8]) -> Option<usize> {
    if out.len() < 4 {
        return None;
    }
    out[0] = HS_SHD;
    out[1] = 0;
    out[2] = 0;
    out[3] = 0;
    Some(4)
}

fn write_u24(out: &mut [u8], n: usize) {
    out[0] = (n >> 16) as u8;
    out[1] = (n >> 8) as u8;
    out[2] = n as u8;
}

/// Package: millicert baked, rustls not in uefi-bin, iron marker unprinted.
pub fn prop_tls12_package() -> bool {
    let cargo = include_str!("../Cargo.toml");
    let uefi_feat = cargo
        .lines()
        .find(|l| l.contains("uefi-bin = ["))
        .unwrap_or("");
    LAB_CERT.len() > 200
        && LAB_CA.len() > 200
        && M8_TLS12_HOST_OK_MARKER == "RAYNU-V-M8-TLS12-HOST-OK"
        && TLS12_RESIDUAL_NOTE.contains("not iron")
        && cargo.contains("aes-gcm")
        && cargo.contains("p256")
        && cargo.contains("rsa")
        && cargo.contains("[dev-dependencies]")
        && cargo.contains("rustls")
        && !uefi_feat.contains("rustls")
        && crate::mgmt::tls::host_never_prints_iron_tls_ok()
}

#[cfg(test)]
#[path = "tls12_test.rs"]
mod tls12_test;
