//! rustls TLS 1.2 client ↔ freestanding server. Never prints iron TLS-OK.

use super::{prop_tls12_package, Tls12Listen, LAB_CA, M8_TLS12_HOST_OK_MARKER};
use crate::mgmt::http::handle_http_request;
use crate::mgmt::datastore::ImageTable;
use crate::mgmt::iso::IsoDeployPlan;
use crate::mgmt::iso_install::InstallToDiskPlan;
use crate::mgmt::tls::{host_never_prints_iron_tls_ok, M8_TLS_OK_MARKER};
use crate::mgmt::tls_coexist::COEXIST_HTTP_OUT_N;
use crate::mgmt::VmTable;
use rustls::pki_types::{CertificateDer, ServerName};
use rustls::{ClientConfig, ClientConnection, RootCertStore};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

fn lab_ca() -> CertificateDer<'static> {
    CertificateDer::from(LAB_CA.to_vec())
}

fn serve_http(raw: &[u8], out: &mut [u8]) -> usize {
    let s = core::str::from_utf8(raw).unwrap_or("");
    let mut table = VmTable::new();
    let mut images = ImageTable::new();
    let mut iso_plan = IsoDeployPlan::empty();
    let mut iso_install = InstallToDiskPlan::empty();
    handle_http_request(
        &mut table,
        &mut images,
        &mut iso_plan,
        &mut iso_install,
        s,
        out,
    )
    .unwrap_or(0)
}

fn serve_one(listener: TcpListener) {
    let (mut sock, _) = listener.accept().expect("tls12 accept");
    let _ = sock.set_read_timeout(Some(Duration::from_secs(3)));
    let _ = sock.set_write_timeout(Some(Duration::from_secs(3)));
    let mut listen = Tls12Listen::new();
    let mut buf = [0u8; 4096];
    for _ in 0..64 {
        if listen.take_http().is_some() {
            break;
        }
        let n = match sock.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(_) => break,
        };
        listen.feed_tcp(&buf[..n]);
        let mut out = [0u8; COEXIST_HTTP_OUT_N];
        let wn = listen.drain_tcp(&mut out);
        if wn == 0 && listen.debug_state() == 5 {
            panic!(
                "tls12 fail after {}B state={} head={:02x?}",
                n,
                listen.debug_state(),
                &buf[..n.min(80)]
            );
        }
        if wn > 0 {
            let _ = sock.write_all(&out[..wn]);
        }
    }
    let raw = listen.take_http().expect("tls12 HTTP");
    let mut http_out = [0u8; COEXIST_HTTP_OUT_N];
    let wn = serve_http(raw, &mut http_out);
    assert!(wn > 0);
    let mut cipher = [0u8; COEXIST_HTTP_OUT_N];
    let cn = listen.wrap_http(&http_out[..wn], &mut cipher);
    assert!(cn > 0, "tls12 wrap empty");
    sock.write_all(&cipher[..cn]).expect("tls12 send");
}

fn tls12_get(port: u16, req: &[u8]) -> String {
    let mut roots = RootCertStore::empty();
    roots.add(lab_ca()).expect("tls12 trust");
    let cfg = ClientConfig::builder_with_protocol_versions(&[&rustls::version::TLS12])
        .with_root_certificates(roots)
        .with_no_client_auth();
    let mut sock = TcpStream::connect(("127.0.0.1", port)).expect("tls12 connect");
    let name = ServerName::try_from("localhost").expect("tls12 name");
    let mut conn = ClientConnection::new(Arc::new(cfg), name).expect("tls12 client");
    let mut tls = rustls::Stream::new(&mut conn, &mut sock);
    tls.write_all(req).expect("tls12 client write");
    let _ = tls.flush();
    let mut resp = Vec::new();
    let _ = tls.read_to_end(&mut resp);
    String::from_utf8_lossy(&resp).into_owned()
}

#[test]
fn tls12_package_holds() {
    assert!(prop_tls12_package());
    assert_eq!(M8_TLS12_HOST_OK_MARKER, "RAYNU-V-M8-TLS12-HOST-OK");
    assert_eq!(M8_TLS_OK_MARKER, "RAYNU-V-M8-TLS-OK");
    assert_ne!(M8_TLS12_HOST_OK_MARKER, M8_TLS_OK_MARKER);
    assert!(host_never_prints_iron_tls_ok());
    let s = Tls12Listen::new();
    assert!(s.key_ready(), "lab millicert PKCS#8 must parse");
}

#[test]
fn server_answers_minimal_client_hello() {
    // TLS 1.2 ClientHello offering C02F + EMS.
    let ch = [
        0x16, 0x03, 0x03, 0x00, 0x33, 0x01, 0x00, 0x00, 0x2f, 0x03, 0x03, 0x00, 0x01, 0x02, 0x03,
        0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10, 0x11, 0x12,
        0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f, 0x00, 0x00,
        0x02, 0xc0, 0x2f, 0x01, 0x00, 0x00, 0x04, 0x00, 0x17, 0x00, 0x00,
    ];
    let mut s = Tls12Listen::new();
    assert!(s.key_ready());
    let n = s.feed_tcp(&ch);
    assert_eq!(n, ch.len());
    let mut out = [0u8; 2048];
    let wn = s.drain_tcp(&mut out);
    assert!(wn > 50, "server flight empty state={}", s.debug_state());
    assert_eq!(out[0], 0x16, "want handshake record");
    assert_eq!(s.debug_state(), 1, "want WaitCKE");
}

#[test]
fn server_ignores_tls13_dummy_ccs_after_client_hello() {
    // TLS 1.2 ClientHello offering C02F + EMS, then RFC 8446 D.4 dummy CCS
    // (Safari/Chrome send this; curl --tlsv1.2 does not).
    let ch = [
        0x16, 0x03, 0x01, 0x00, 0x33, 0x01, 0x00, 0x00, 0x2f, 0x03, 0x03, 0x00, 0x01, 0x02, 0x03,
        0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10, 0x11, 0x12,
        0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f, 0x00, 0x00,
        0x02, 0xc0, 0x2f, 0x01, 0x00, 0x00, 0x04, 0x00, 0x17, 0x00, 0x00,
    ];
    let ccs = [0x14, 0x03, 0x03, 0x00, 0x01, 0x01];
    let mut s = Tls12Listen::new();
    assert!(s.key_ready());
    assert_eq!(s.feed_tcp(&ch), ch.len());
    assert_eq!(s.debug_state(), 1, "want WaitCKE before dummy CCS");
    assert_eq!(s.feed_tcp(&ccs), ccs.len());
    assert!(!s.handshake_failed(), "dummy CCS must not fail the millicert");
    assert_eq!(s.debug_state(), 1, "want still WaitCKE after dummy CCS");
    let mut out = [0u8; 2048];
    let wn = s.drain_tcp(&mut out);
    assert!(wn > 50, "server flight empty state={}", s.debug_state());
}

#[test]
fn rustls_default_client_negotiates_tls13_spa() {
    // A browser offers TLS 1.3 first, with an X25519 key share. The server
    // asks for secp256r1 (HelloRetryRequest) and the page comes back on
    // TLS_AES_128_GCM_SHA256. curl --tlsv1.2 still takes the 1.2 path.
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = thread::spawn(move || serve_one(listener));
    let mut roots = RootCertStore::empty();
    roots.add(lab_ca()).expect("tls13 trust");
    let cfg = ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let mut sock = TcpStream::connect(("127.0.0.1", port)).expect("tls13 connect");
    let name = ServerName::try_from("localhost").expect("tls13 name");
    let mut conn = ClientConnection::new(Arc::new(cfg), name).expect("tls13 client");
    let body = {
        let mut tls = rustls::Stream::new(&mut conn, &mut sock);
        tls.write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
            .expect("default client write");
        let _ = tls.flush();
        let mut resp = Vec::new();
        let _ = tls.read_to_end(&mut resp);
        String::from_utf8_lossy(&resp).into_owned()
    };
    assert_eq!(
        conn.protocol_version(),
        Some(rustls::ProtocolVersion::TLSv1_3),
        "default client must finish on TLS 1.3"
    );
    assert!(body.contains("HTTP/1.1 200"), "{body}");
    assert!(body.contains("data-go=\"overview\""), "{body}");
    assert!(server.join().is_ok());
    assert_ne!(super::tls13::E4_TLS13_HOST_OK, M8_TLS_OK_MARKER);
    println!("{}", super::tls13::E4_TLS13_HOST_OK);
}

#[test]
fn rustls_p256_client_negotiates_tls13_without_retry() {
    // The share is already secp256r1, so the server flight is the first
    // reply. Browsers take the HelloRetryRequest path instead.
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = thread::spawn(move || serve_one(listener));
    let mut roots = RootCertStore::empty();
    roots.add(lab_ca()).expect("p256 trust");
    let mut provider = rustls::crypto::ring::default_provider();
    provider.kx_groups = vec![rustls::crypto::ring::kx_group::SECP256R1];
    let cfg = ClientConfig::builder_with_provider(provider.into())
        .with_protocol_versions(&[&rustls::version::TLS13])
        .expect("tls13 versions")
        .with_root_certificates(roots)
        .with_no_client_auth();
    let mut sock = TcpStream::connect(("127.0.0.1", port)).expect("p256 connect");
    let name = ServerName::try_from("localhost").expect("p256 name");
    let mut conn = ClientConnection::new(Arc::new(cfg), name).expect("p256 client");
    let body = {
        let mut tls = rustls::Stream::new(&mut conn, &mut sock);
        tls.write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
            .expect("p256 write");
        let _ = tls.flush();
        let mut resp = Vec::new();
        let _ = tls.read_to_end(&mut resp);
        String::from_utf8_lossy(&resp).into_owned()
    };
    assert_eq!(
        conn.protocol_version(),
        Some(rustls::ProtocolVersion::TLSv1_3)
    );
    assert!(body.contains("HTTP/1.1 200"), "{body}");
    assert!(server.join().is_ok());
}

#[test]
fn rustls_tls12_client_gets_spa_from_freestanding_server() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = thread::spawn(move || serve_one(listener));
    let body = tls12_get(
        port,
        b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
    );
    assert!(body.contains("HTTP/1.1 200"), "{body}");
    assert!(body.contains("data-go=\"overview\""), "{body}");
    assert!(body.contains("d-host"), "{body}");
    assert!(body.contains("data-raynu-phase-b"), "{body}");
    assert!(server.join().is_ok());
    println!("{M8_TLS12_HOST_OK_MARKER}");
}

#[test]
fn split_client_hello_still_draws_the_server_flight() {
    let ch = [
        0x16u8, 0x03, 0x03, 0x00, 0x33, 0x01, 0x00, 0x00, 0x2f, 0x03, 0x03, 0x00, 0x01, 0x02,
        0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10, 0x11,
        0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f, 0x00,
        0x00, 0x02, 0xc0, 0x2f, 0x01, 0x00, 0x00, 0x04, 0x00, 0x17, 0x00, 0x00,
    ];
    let mut s = Tls12Listen::new();
    assert_eq!(s.feed_tcp(&ch[..20]), 20);
    assert!(s.rx_pending(), "the record is not whole yet");
    assert!(s.handshake_waiting_client_hello());
    assert_eq!(s.feed_tcp(&ch[20..]), ch.len() - 20);
    assert!(!s.handshake_failed(), "split hello must not fail");
    let mut out = [0u8; 2048];
    assert!(s.drain_tcp(&mut out) > 50, "flight after the second segment");
}

#[test]
fn prefix_tx_puts_a_short_send_back() {
    let mut s = Tls12Listen::new();
    let ch = [
        0x16u8, 0x03, 0x03, 0x00, 0x33, 0x01, 0x00, 0x00, 0x2f, 0x03, 0x03, 0x00, 0x01, 0x02,
        0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10, 0x11,
        0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f, 0x00,
        0x00, 0x02, 0xc0, 0x2f, 0x01, 0x00, 0x00, 0x04, 0x00, 0x17, 0x00, 0x00,
    ];
    assert_eq!(s.feed_tcp(&ch), ch.len());
    let mut out = [0u8; 2048];
    let n = s.drain_tcp(&mut out);
    assert!(n > 40);
    assert!(s.prefix_tx(&out[..n]));
    let mut again = [0u8; 2048];
    assert_eq!(s.drain_tcp(&mut again), n);
    assert_eq!(&again[..n], &out[..n]);
}

#[test]
fn empty_bss_session_loads_lab_material() {
    let mut s = Tls12Listen::empty();
    assert!(!s.key_ready());
    s.load_lab_material();
    assert!(s.key_ready(), "bss empty + PKCS#8 must parse");
    s.reset();
    assert!(s.key_ready());
}
