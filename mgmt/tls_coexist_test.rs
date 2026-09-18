//! Coexist-shaped TLS wrap: rustls session uses feed/take/wrap. Not iron.

use super::{
    headers_complete, prop_tls_fw_wrap_package, wrap_plaintext_http, PlaintextListen,
    M8_TLS_FW_HOST_OK_MARKER, TLS_FW_CURL_NOTE, TLS_FW_WRAP_NOTE, COEXIST_HTTP_OUT_N,
    COEXIST_RX_ACC_N,
};
use crate::mgmt::http::handle_http_request;
use crate::mgmt::datastore::ImageTable;
use crate::mgmt::iso::IsoDeployPlan;
use crate::mgmt::iso_install::InstallToDiskPlan;
use crate::mgmt::tls::firmware_listen_is_plaintext;
use crate::mgmt::VmTable;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName};
use rustls::{ClientConfig, ClientConnection, RootCertStore, ServerConfig, ServerConnection};
use std::io::{Cursor, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::thread;

struct LabCert {
    cert: CertificateDer<'static>,
    key: PrivateKeyDer<'static>,
}

fn lab_self_signed() -> LabCert {
    let pair = rcgen::KeyPair::generate().expect("tls key");
    let mut params = rcgen::CertificateParams::new(vec!["localhost".into()]).expect("tls params");
    params
        .distinguished_name
        .push(rcgen::DnType::CommonName, "raynu-v-m8-tls-fw");
    let cert = params.self_signed(&pair).expect("tls cert");
    LabCert {
        cert: CertificateDer::from(cert.der().to_vec()),
        key: PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(pair.serialize_der())),
    }
}

struct HostTlsListen {
    conn: ServerConnection,
    plain: Vec<u8>,
}

impl HostTlsListen {
    fn new(lab: LabCert) -> Self {
        let cfg = ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![lab.cert], lab.key)
            .expect("tls fw server cert");
        Self {
            conn: ServerConnection::new(Arc::new(cfg)).expect("tls fw conn"),
            plain: Vec::new(),
        }
    }

    fn feed_tcp(&mut self, chunk: &[u8]) {
        if chunk.is_empty() {
            return;
        }
        let mut cur = Cursor::new(chunk);
        let _ = self.conn.read_tls(&mut cur);
        let _ = self.conn.process_new_packets();
        let mut buf = [0u8; 2048];
        loop {
            match self.conn.reader().read(&mut buf) {
                Ok(0) => break,
                Ok(n) => self.plain.extend_from_slice(&buf[..n]),
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(_) => break,
            }
        }
    }

    fn take_http(&self) -> Option<&[u8]> {
        if headers_complete(&self.plain) {
            Some(&self.plain)
        } else {
            None
        }
    }

    fn wrap_http_response(&mut self, http: &[u8], tcp_out: &mut [u8]) -> usize {
        let _ = self.conn.writer().write_all(http);
        let _ = self.conn.writer().flush();
        self.drain_handshake(tcp_out)
    }

    fn drain_handshake(&mut self, tcp_out: &mut [u8]) -> usize {
        let mut wrote = 0usize;
        while self.conn.wants_write() && wrote < tcp_out.len() {
            let mut cur = Cursor::new(&mut tcp_out[wrote..]);
            match self.conn.write_tls(&mut cur) {
                Ok(0) => break,
                Ok(n) => wrote += n,
                Err(_) => break,
            }
        }
        wrote
    }
}

fn tls_get_via_stream(port: u16, cert: CertificateDer<'static>, req: &[u8]) -> String {
    let mut roots = RootCertStore::empty();
    roots.add(cert).expect("tls fw trust");
    let cfg = ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let mut sock = TcpStream::connect(("127.0.0.1", port)).expect("tls fw connect");
    let name = ServerName::try_from("localhost").expect("tls fw name");
    let mut conn = ClientConnection::new(Arc::new(cfg), name).expect("tls fw client");
    let mut tls = rustls::Stream::new(&mut conn, &mut sock);
    tls.write_all(req).expect("tls fw client write");
    let _ = tls.flush();
    let mut resp = Vec::new();
    let _ = tls.read_to_end(&mut resp);
    String::from_utf8_lossy(&resp).into_owned()
}

fn serve_one_coexist_tls(listener: TcpListener, lab: LabCert) {
    let (mut sock, _) = listener.accept().expect("tls fw accept");
    let mut listen = HostTlsListen::new(lab);
    let mut buf = [0u8; 4096];
    for _ in 0..32 {
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
        let wn = listen.drain_handshake(&mut out);
        if wn > 0 {
            let _ = sock.write_all(&out[..wn]);
        }
    }
    let raw = listen.take_http().expect("tls plaintext HTTP");
    assert!(headers_complete(raw));
    let mut http_out = [0u8; COEXIST_HTTP_OUT_N];
    let wn = serve_http(raw, &mut http_out);
    assert!(wn > 0);
    let mut cipher = [0u8; COEXIST_HTTP_OUT_N];
    let cn = listen.wrap_http_response(&http_out[..wn], &mut cipher);
    assert!(cn > 0);
    sock.write_all(&cipher[..cn]).expect("tls fw send");
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

#[test]
fn plaintext_listen_matches_coexist_buffers() {
    assert_eq!(COEXIST_RX_ACC_N, 8192);
    assert_eq!(COEXIST_HTTP_OUT_N, 16384);
    let mut s = PlaintextListen::new();
    assert!(s.take_http().is_none());
    assert_eq!(s.feed_tcp(b"GET / HTTP/1.1\r\nHost: x\r\n\r\n"), 27);
    let http = s.take_http().expect("headers");
    assert!(headers_complete(http));
    let mut out = [0u8; 64];
    let n = wrap_plaintext_http(b"HTTP/1.1 200 OK\r\n\r\n", &mut out);
    assert_eq!(&out[..n], b"HTTP/1.1 200 OK\r\n\r\n");
    s.reset();
    assert!(s.take_http().is_none());
}

#[test]
fn tls_fw_wrap_package_holds() {
    assert!(firmware_listen_is_plaintext());
    assert!(TLS_FW_WRAP_NOTE.contains("PlaintextListen"));
    assert!(TLS_FW_CURL_NOTE.contains("http://"));
    assert!(prop_tls_fw_wrap_package());
}

#[test]
fn host_rustls_coexist_feed_take_wrap_serves_spa() {
    let lab = lab_self_signed();
    let client_cert = lab.cert.clone();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = thread::spawn(move || serve_one_coexist_tls(listener, lab));
    let body = tls_get_via_stream(
        port,
        client_cert,
        b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
    );
    assert!(body.contains("HTTP/1.1 200"), "{body}");
    assert!(body.contains("data-go=\"overview\""), "{body}");
    assert!(body.contains("d-host"), "{body}");
    assert!(body.contains("data-raynu-phase-b"), "{body}");
    assert!(server.join().is_ok());
    assert!(firmware_listen_is_plaintext());
    println!("{M8_TLS_FW_HOST_OK_MARKER}");
}
