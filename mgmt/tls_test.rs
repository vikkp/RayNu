//! Host TLS: rustls server wraps the HTTP codec. Never prints iron TLS-OK.

use super::{
    firmware_listen_is_plaintext, firmware_listen_is_tls12, host_never_prints_iron_tls_ok,
    maybe_print_iron_tls_ok, prop_tls_host_package, tls_ok_clear_printed, TlsMode,
    FIRMWARE_TLS_MODE, M8_TLS_HOST_OK_MARKER, M8_TLS_OK_MARKER, TLS_FIRMWARE_PLAINTEXT_NOTE,
};
use crate::mgmt::http::{handle_http_request, MGMT_HTTP_DEFAULT_PORT};
use crate::mgmt::datastore::ImageTable;
use crate::mgmt::iso::IsoDeployPlan;
use crate::mgmt::iso_install::InstallToDiskPlan;
use crate::mgmt::VmTable;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName};
use rustls::{ClientConfig, ClientConnection, RootCertStore, ServerConfig, ServerConnection};
use std::io::{Read, Write};
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
        .push(rcgen::DnType::CommonName, "raynu-v-m8-tls-host");
    let cert = params.self_signed(&pair).expect("tls cert");
    LabCert {
        cert: CertificateDer::from(cert.der().to_vec()),
        key: PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(pair.serialize_der())),
    }
}

fn serve_one_tls_http(listener: TcpListener, cert: LabCert) {
    let cfg = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(vec![cert.cert], cert.key)
        .expect("tls server cert");
    let (mut sock, _) = listener.accept().expect("tls accept");
    let mut conn = ServerConnection::new(Arc::new(cfg)).expect("tls conn");
    let mut tls = rustls::Stream::new(&mut conn, &mut sock);
    let mut buf = [0u8; 8192];
    let n = tls.read(&mut buf).unwrap_or(0);
    let raw = core::str::from_utf8(&buf[..n]).unwrap_or("");
    let mut table = VmTable::new();
    let mut images = ImageTable::new();
    let mut iso_plan = IsoDeployPlan::empty();
    let mut iso_install = InstallToDiskPlan::empty();
    let mut out = [0u8; 16384];
    let wn = handle_http_request(
        &mut table,
        &mut images,
        &mut iso_plan,
        &mut iso_install,
        raw,
        &mut out,
    )
    .unwrap_or(0);
    tls.write_all(&out[..wn]).expect("tls write");
    let _ = tls.flush();
}

fn tls_get(port: u16, cert: CertificateDer<'static>, req: &[u8]) -> String {
    let mut roots = RootCertStore::empty();
    roots.add(cert).expect("tls trust");
    let cfg = ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let mut sock = TcpStream::connect(("127.0.0.1", port)).expect("tls connect");
    let name = ServerName::try_from("localhost").expect("tls name");
    let mut conn = ClientConnection::new(Arc::new(cfg), name).expect("tls client");
    let mut tls = rustls::Stream::new(&mut conn, &mut sock);
    tls.write_all(req).expect("tls client write");
    let _ = tls.flush();
    let mut resp = Vec::new();
    let _ = tls.read_to_end(&mut resp);
    String::from_utf8_lossy(&resp).into_owned()
}

#[test]
fn firmware_listen_is_tls12_in_tree() {
    assert_eq!(FIRMWARE_TLS_MODE, TlsMode::FirmwareTls12);
    assert!(firmware_listen_is_tls12());
    assert!(!firmware_listen_is_plaintext());
    assert!(TLS_FIRMWARE_PLAINTEXT_NOTE.contains("TLS 1.2"));
    assert!(TLS_FIRMWARE_PLAINTEXT_NOTE.contains("plaintext HTTP remains a lab fallback"));
    assert_eq!(M8_TLS_OK_MARKER, "RAYNU-V-M8-TLS-OK");
    assert_eq!(M8_TLS_HOST_OK_MARKER, "RAYNU-V-M8-TLS-HOST-OK");
    assert_ne!(M8_TLS_OK_MARKER, M8_TLS_HOST_OK_MARKER);
    assert!(host_never_prints_iron_tls_ok());
    assert!(prop_tls_host_package());
    tls_ok_clear_printed();
    assert!(maybe_print_iron_tls_ok(true, true));
    assert!(!maybe_print_iron_tls_ok(true, true));
    tls_ok_clear_printed();
    assert!(!maybe_print_iron_tls_ok(true, false));
    let _ = MGMT_HTTP_DEFAULT_PORT;
}

#[test]
fn host_rustls_serves_spa_and_authed_rest() {
    let lab = lab_self_signed();
    let client_cert = lab.cert.clone();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = thread::spawn(move || serve_one_tls_http(listener, lab));
    let s = tls_get(
        port,
        client_cert.clone(),
        b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
    );
    assert!(s.contains("HTTP/1.1 200"), "{s}");
    assert!(s.contains("text/html"), "{s}");
    assert!(s.contains("data-go=\"overview\""), "{s}");
    assert!(s.contains("d-host"), "{s}");
    assert!(s.contains("data-raynu-phase-b"), "{s}");
    assert!(s.contains("RayNu-F"), "{s}");
    assert!(!s.contains("Hold live ESP"), "{s}");
    assert!(server.join().is_ok());

    let lab = lab_self_signed();
    let client_cert = lab.cert.clone();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = thread::spawn(move || serve_one_tls_http(listener, lab));
    let s = tls_get(
        port,
        client_cert,
        b"GET /vms HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer raynu-v-bringup\r\nConnection: close\r\n\r\n",
    );
    assert!(s.contains("HTTP/1.1 200"), "{s}");
    assert!(server.join().is_ok());
    println!("{M8_TLS_HOST_OK_MARKER}");
}
