use super::{listen_mgmt_http_uefi, prop_listen_surface, serve_one_connection_host, MgmtListenError};
use crate::mgmt::http::{M7_HTTP_OK_MARKER, MGMT_HTTP_DEFAULT_PORT};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::thread;
use std::time::Duration;

#[test]
fn uefi_listen_stub_is_honest() {
    assert_eq!(
        listen_mgmt_http_uefi(MGMT_HTTP_DEFAULT_PORT),
        Err(MgmtListenError::UnsupportedOnFirmware)
    );
    assert!(prop_listen_surface());
}

#[test]
fn host_tcp_serves_spa_and_authed_rest() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    let server = thread::spawn(move || serve_one_connection_host(port).expect("serve"));

    thread::sleep(Duration::from_millis(50));
    let mut client = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    client
        .write_all(b"GET / HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
        .unwrap();
    let mut resp = Vec::new();
    client.read_to_end(&mut resp).unwrap();
    let s = String::from_utf8_lossy(&resp);
    assert!(s.contains("HTTP/1.1 200"), "{s}");
    assert!(s.contains("text/html"), "{s}");
    assert!(server.join().unwrap() == port);

    // Second connection: authed REST via a fresh one-shot server.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let server = thread::spawn(move || serve_one_connection_host(port).expect("serve2"));
    thread::sleep(Duration::from_millis(50));
    let mut client = TcpStream::connect(("127.0.0.1", port)).expect("connect2");
    client
        .write_all(
            b"GET /vms HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer raynu-v-bringup\r\nConnection: close\r\n\r\n",
        )
        .unwrap();
    let mut resp = Vec::new();
    client.read_to_end(&mut resp).unwrap();
    let s = String::from_utf8_lossy(&resp);
    assert!(s.contains("HTTP/1.1 200"), "{s}");
    assert!(server.join().is_ok());
    println!("{M7_HTTP_OK_MARKER}");
}
