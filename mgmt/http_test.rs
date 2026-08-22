use super::{
    auth_token_from_headers, extract_bearer_token, format_http_response, handle_http_request,
    parse_http_request, prop_http_mgmt_package, HttpParseError, HTTP_GAP_NOTE, HTTP_LAB_NOTE,
    M7_HTTP_OK_MARKER, MGMT_HTTP_DEFAULT_PORT,
};
use crate::mgmt::api::{RestMethod, BRINGUP_AUTH_TOKEN};
use crate::mgmt::datastore::ImageTable;
use crate::mgmt::iso::IsoDeployPlan;
use crate::mgmt::iso_install::InstallToDiskPlan;
use crate::mgmt::VmTable;

#[test]
fn parses_spa_get() {
    let p = parse_http_request("GET / HTTP/1.1\r\nHost: x\r\n\r\n").unwrap();
    assert!(p.is_spa);
    assert_eq!(p.method, RestMethod::Get);
}

#[test]
fn parses_bearer_auth() {
    assert_eq!(
        extract_bearer_token("Bearer raynu-v-bringup"),
        Some(BRINGUP_AUTH_TOKEN)
    );
    let h = "Host: x\r\nAuthorization: Bearer raynu-v-bringup\r\n";
    assert_eq!(auth_token_from_headers(h), Some(BRINGUP_AUTH_TOKEN));
}

#[test]
fn rejects_bad_method() {
    assert_eq!(
        parse_http_request("PUT /vms HTTP/1.1\r\n\r\n"),
        Err(HttpParseError::BadMethod)
    );
}

#[test]
fn serves_spa_and_rest() {
    let mut table = VmTable::new();
    let mut images = ImageTable::new();
    let mut iso_plan = IsoDeployPlan::empty();
    let mut iso_install = InstallToDiskPlan::empty();
    let mut out = [0u8; 16384];
    let n = handle_http_request(
        &mut table,
        &mut images,
        &mut iso_plan,
        &mut iso_install,
        "GET / HTTP/1.1\r\n\r\n",
        &mut out,
    )
    .unwrap();
    let s = core::str::from_utf8(&out[..n]).unwrap();
    assert!(s.contains("HTTP/1.1 200"));
    assert!(s.contains("text/html"));
    assert!(s.contains("data-raynu-webui") || s.contains("RayNu"));

    let n = handle_http_request(
        &mut table,
        &mut images,
        &mut iso_plan,
        &mut iso_install,
        "GET /vms HTTP/1.1\r\nAuthorization: Bearer raynu-v-bringup\r\n\r\n",
        &mut out,
    )
    .unwrap();
    let s = core::str::from_utf8(&out[..n]).unwrap();
    assert!(s.contains("HTTP/1.1 200"));

    let n = handle_http_request(
        &mut table,
        &mut images,
        &mut iso_plan,
        &mut iso_install,
        "GET /vms HTTP/1.1\r\n\r\n",
        &mut out,
    )
    .unwrap();
    let s = core::str::from_utf8(&out[..n]).unwrap();
    assert!(s.contains("HTTP/1.1 401"));

    let n = handle_http_request(
        &mut table,
        &mut images,
        &mut iso_plan,
        &mut iso_install,
        "POST /images/4/iso HTTP/1.1\r\nAuthorization: Bearer raynu-v-bringup\r\n\r\n",
        &mut out,
    )
    .unwrap();
    let s = core::str::from_utf8(&out[..n]).unwrap();
    assert!(s.contains("HTTP/1.1 201"), "{s}");
    assert!(images.get(4).is_some());

    let n = handle_http_request(
        &mut table,
        &mut images,
        &mut iso_plan,
        &mut iso_install,
        "POST /iso/4/deploy HTTP/1.1\r\nAuthorization: Bearer raynu-v-bringup\r\n\r\n",
        &mut out,
    )
    .unwrap();
    let s = core::str::from_utf8(&out[..n]).unwrap();
    assert!(s.contains("HTTP/1.1 201"), "{s}");
    assert!(iso_plan.is_ready());

    let n = handle_http_request(
        &mut table,
        &mut images,
        &mut iso_plan,
        &mut iso_install,
        "POST /iso/4/install HTTP/1.1\r\nAuthorization: Bearer raynu-v-bringup\r\n\r\n",
        &mut out,
    )
    .unwrap();
    let s = core::str::from_utf8(&out[..n]).unwrap();
    assert!(s.contains("HTTP/1.1 201"), "{s}");
    assert!(iso_install.is_contract_ready());

    crate::mgmt::iso::reset_host_cdrom();
    let n = handle_http_request(
        &mut table,
        &mut images,
        &mut iso_plan,
        &mut iso_install,
        "POST /iso/4/attach/linux_iso HTTP/1.1\r\nAuthorization: Bearer raynu-v-bringup\r\n\r\n",
        &mut out,
    )
    .unwrap();
    let s = core::str::from_utf8(&out[..n]).unwrap();
    assert!(s.contains("HTTP/1.1 201"), "{s}");

    let n = handle_http_request(
        &mut table,
        &mut images,
        &mut iso_plan,
        &mut iso_install,
        "POST /iso/4/firmware HTTP/1.1\r\nAuthorization: Bearer raynu-v-bringup\r\n\r\n",
        &mut out,
    )
    .unwrap();
    let s = core::str::from_utf8(&out[..n]).unwrap();
    assert!(s.contains("HTTP/1.1 201"), "{s}");

    crate::mgmt::guest_fw::reset_guest_fw();
    let n = handle_http_request(
        &mut table,
        &mut images,
        &mut iso_plan,
        &mut iso_install,
        "POST /fw/box HTTP/1.1\r\nAuthorization: Bearer raynu-v-bringup\r\n\r\n",
        &mut out,
    )
    .unwrap();
    let s = core::str::from_utf8(&out[..n]).unwrap();
    assert!(s.contains("HTTP/1.1 201"), "{s}");
    let n = handle_http_request(
        &mut table,
        &mut images,
        &mut iso_plan,
        &mut iso_install,
        "POST /fw/load HTTP/1.1\r\nAuthorization: Bearer raynu-v-bringup\r\n\r\n",
        &mut out,
    )
    .unwrap();
    let s = core::str::from_utf8(&out[..n]).unwrap();
    assert!(s.contains("HTTP/1.1 201"), "{s}");
    let n = handle_http_request(
        &mut table,
        &mut images,
        &mut iso_plan,
        &mut iso_install,
        "POST /fw/ovmf HTTP/1.1\r\nAuthorization: Bearer raynu-v-bringup\r\n\r\n",
        &mut out,
    )
    .unwrap();
    let s = core::str::from_utf8(&out[..n]).unwrap();
    assert!(s.contains("HTTP/1.1 201"), "{s}");
    let n = handle_http_request(
        &mut table,
        &mut images,
        &mut iso_plan,
        &mut iso_install,
        "POST /fw/ovmf/esp HTTP/1.1\r\nAuthorization: Bearer raynu-v-bringup\r\n\r\n",
        &mut out,
    )
    .unwrap();
    let s = core::str::from_utf8(&out[..n]).unwrap();
    assert!(s.contains("HTTP/1.1 201"), "{s}");
    let n = handle_http_request(
        &mut table,
        &mut images,
        &mut iso_plan,
        &mut iso_install,
        "POST /fw/slot HTTP/1.1\r\nAuthorization: Bearer raynu-v-bringup\r\n\r\n",
        &mut out,
    )
    .unwrap();
    let s = core::str::from_utf8(&out[..n]).unwrap();
    assert!(s.contains("HTTP/1.1 201"), "{s}");
    let n = handle_http_request(
        &mut table,
        &mut images,
        &mut iso_plan,
        &mut iso_install,
        "POST /fw/bind HTTP/1.1\r\nAuthorization: Bearer raynu-v-bringup\r\n\r\n",
        &mut out,
    )
    .unwrap();
    let s = core::str::from_utf8(&out[..n]).unwrap();
    assert!(s.contains("HTTP/1.1 201"), "{s}");
    let n = handle_http_request(
        &mut table,
        &mut images,
        &mut iso_plan,
        &mut iso_install,
        "POST /fw/prepare HTTP/1.1\r\nAuthorization: Bearer raynu-v-bringup\r\n\r\n",
        &mut out,
    )
    .unwrap();
    let s = core::str::from_utf8(&out[..n]).unwrap();
    assert!(s.contains("HTTP/1.1 201"), "{s}");
    let n = handle_http_request(
        &mut table,
        &mut images,
        &mut iso_plan,
        &mut iso_install,
        "POST /fw/floor HTTP/1.1\r\nAuthorization: Bearer raynu-v-bringup\r\n\r\n",
        &mut out,
    )
    .unwrap();
    let s = core::str::from_utf8(&out[..n]).unwrap();
    assert!(s.contains("HTTP/1.1 201"), "{s}");
    let n = handle_http_request(
        &mut table,
        &mut images,
        &mut iso_plan,
        &mut iso_install,
        "POST /fw/edk2 HTTP/1.1\r\nAuthorization: Bearer raynu-v-bringup\r\n\r\n",
        &mut out,
    )
    .unwrap();
    let s = core::str::from_utf8(&out[..n]).unwrap();
    assert!(s.contains("HTTP/1.1 201"), "{s}");
    let n = handle_http_request(
        &mut table,
        &mut images,
        &mut iso_plan,
        &mut iso_install,
        "POST /fw/esp-launch HTTP/1.1\r\nAuthorization: Bearer raynu-v-bringup\r\n\r\n",
        &mut out,
    )
    .unwrap();
    let s = core::str::from_utf8(&out[..n]).unwrap();
    assert!(s.contains("HTTP/1.1 201"), "{s}");
    let n = handle_http_request(
        &mut table,
        &mut images,
        &mut iso_plan,
        &mut iso_install,
        "POST /fw/esp-map HTTP/1.1\r\nAuthorization: Bearer raynu-v-bringup\r\n\r\n",
        &mut out,
    )
    .unwrap();
    let s = core::str::from_utf8(&out[..n]).unwrap();
    assert!(s.contains("HTTP/1.1 201"), "{s}");
    let n = handle_http_request(
        &mut table,
        &mut images,
        &mut iso_plan,
        &mut iso_install,
        "POST /fw/reset-vec HTTP/1.1\r\nAuthorization: Bearer raynu-v-bringup\r\n\r\n",
        &mut out,
    )
    .unwrap();
    let s = core::str::from_utf8(&out[..n]).unwrap();
    assert!(s.contains("HTTP/1.1 201"), "{s}");
    let n = handle_http_request(
        &mut table,
        &mut images,
        &mut iso_plan,
        &mut iso_install,
        "POST /fw/alias HTTP/1.1\r\nAuthorization: Bearer raynu-v-bringup\r\n\r\n",
        &mut out,
    )
    .unwrap();
    let s = core::str::from_utf8(&out[..n]).unwrap();
    assert!(s.contains("HTTP/1.1 201"), "{s}");
    let n = handle_http_request(
        &mut table,
        &mut images,
        &mut iso_plan,
        &mut iso_install,
        "POST /fw/alias-ept HTTP/1.1\r\nAuthorization: Bearer raynu-v-bringup\r\n\r\n",
        &mut out,
    )
    .unwrap();
    let s = core::str::from_utf8(&out[..n]).unwrap();
    assert!(s.contains("HTTP/1.1 201"), "{s}");
    let n = handle_http_request(
        &mut table,
        &mut images,
        &mut iso_plan,
        &mut iso_install,
        "POST /fw/ept-install HTTP/1.1\r\nAuthorization: Bearer raynu-v-bringup\r\n\r\n",
        &mut out,
    )
    .unwrap();
    let s = core::str::from_utf8(&out[..n]).unwrap();
    assert!(s.contains("HTTP/1.1 201"), "{s}");
    let n = handle_http_request(
        &mut table,
        &mut images,
        &mut iso_plan,
        &mut iso_install,
        "POST /fw/real-esp HTTP/1.1\r\nAuthorization: Bearer raynu-v-bringup\r\n\r\n",
        &mut out,
    )
    .unwrap();
    let s = core::str::from_utf8(&out[..n]).unwrap();
    assert!(s.contains("HTTP/1.1 201"), "{s}");
    let n = handle_http_request(
        &mut table,
        &mut images,
        &mut iso_plan,
        &mut iso_install,
        "POST /fw/real-launch HTTP/1.1\r\nAuthorization: Bearer raynu-v-bringup\r\n\r\n",
        &mut out,
    )
    .unwrap();
    let s = core::str::from_utf8(&out[..n]).unwrap();
    assert!(s.contains("HTTP/1.1 201"), "{s}");
    let n = handle_http_request(
        &mut table,
        &mut images,
        &mut iso_plan,
        &mut iso_install,
        "POST /fw/live-exec HTTP/1.1\r\nAuthorization: Bearer raynu-v-bringup\r\n\r\n",
        &mut out,
    )
    .unwrap();
    let s = core::str::from_utf8(&out[..n]).unwrap();
    assert!(s.contains("HTTP/1.1 201"), "{s}");
    let n = handle_http_request(
        &mut table,
        &mut images,
        &mut iso_plan,
        &mut iso_install,
        "POST /fw/priv-vmcs HTTP/1.1\r\nAuthorization: Bearer raynu-v-bringup\r\n\r\n",
        &mut out,
    )
    .unwrap();
    let s = core::str::from_utf8(&out[..n]).unwrap();
    assert!(s.contains("HTTP/1.1 201"), "{s}");
    let n = handle_http_request(
        &mut table,
        &mut images,
        &mut iso_plan,
        &mut iso_install,
        "POST /fw/live-issue HTTP/1.1\r\nAuthorization: Bearer raynu-v-bringup\r\n\r\n",
        &mut out,
    )
    .unwrap();
    let s = core::str::from_utf8(&out[..n]).unwrap();
    assert!(s.contains("HTTP/1.1 201"), "{s}");
    let n = handle_http_request(
        &mut table,
        &mut images,
        &mut iso_plan,
        &mut iso_install,
        "POST /fw/live-bytes HTTP/1.1\r\nAuthorization: Bearer raynu-v-bringup\r\n\r\n",
        &mut out,
    )
    .unwrap();
    let s = core::str::from_utf8(&out[..n]).unwrap();
    assert!(s.contains("HTTP/1.1 201"), "{s}");
    let n = handle_http_request(
        &mut table,
        &mut images,
        &mut iso_plan,
        &mut iso_install,
        "POST /fw/live-fd HTTP/1.1\r\nAuthorization: Bearer raynu-v-bringup\r\n\r\n",
        &mut out,
    )
    .unwrap();
    let s = core::str::from_utf8(&out[..n]).unwrap();
    assert!(s.contains("HTTP/1.1 201"), "{s}");
    let n = handle_http_request(
        &mut table,
        &mut images,
        &mut iso_plan,
        &mut iso_install,
        "POST /fw/live-present HTTP/1.1\r\nAuthorization: Bearer raynu-v-bringup\r\n\r\n",
        &mut out,
    )
    .unwrap();
    let s = core::str::from_utf8(&out[..n]).unwrap();
    assert!(s.contains("HTTP/1.1 201"), "{s}");
    let n = handle_http_request(
        &mut table,
        &mut images,
        &mut iso_plan,
        &mut iso_install,
        "POST /fw/live-admit HTTP/1.1\r\nAuthorization: Bearer raynu-v-bringup\r\n\r\n",
        &mut out,
    )
    .unwrap();
    let s = core::str::from_utf8(&out[..n]).unwrap();
    assert!(s.contains("HTTP/1.1 201"), "{s}");
    let n = handle_http_request(
        &mut table,
        &mut images,
        &mut iso_plan,
        &mut iso_install,
        "POST /fw/live-read HTTP/1.1\r\nAuthorization: Bearer raynu-v-bringup\r\n\r\n",
        &mut out,
    )
    .unwrap();
    let s = core::str::from_utf8(&out[..n]).unwrap();
    assert!(s.contains("HTTP/1.1 201"), "{s}");
    let n = handle_http_request(
        &mut table,
        &mut images,
        &mut iso_plan,
        &mut iso_install,
        "POST /fw/live-copy HTTP/1.1\r\nAuthorization: Bearer raynu-v-bringup\r\n\r\n",
        &mut out,
    )
    .unwrap();
    let s = core::str::from_utf8(&out[..n]).unwrap();
    assert!(s.contains("HTTP/1.1 201"), "{s}");
    let n = handle_http_request(
        &mut table,
        &mut images,
        &mut iso_plan,
        &mut iso_install,
        "POST /fw/live-place HTTP/1.1\r\nAuthorization: Bearer raynu-v-bringup\r\n\r\n",
        &mut out,
    )
    .unwrap();
    let s = core::str::from_utf8(&out[..n]).unwrap();
    assert!(s.contains("HTTP/1.1 201"), "{s}");
    let n = handle_http_request(
        &mut table,
        &mut images,
        &mut iso_plan,
        &mut iso_install,
        "POST /fw/vmlaunch HTTP/1.1\r\nAuthorization: Bearer raynu-v-bringup\r\n\r\n",
        &mut out,
    )
    .unwrap();
    let s = core::str::from_utf8(&out[..n]).unwrap();
    assert!(s.contains("HTTP/1.1 409"), "{s}");
    crate::mgmt::guest_fw::reset_guest_fw();
}

#[test]
fn formats_response() {
    let mut out = [0u8; 256];
    let n = format_http_response(200, "text/plain", b"ok", &mut out).unwrap();
    let s = core::str::from_utf8(&out[..n]).unwrap();
    assert!(s.starts_with("HTTP/1.1 200 OK"));
    assert!(s.contains("Content-Length: 2"));
}

#[test]
fn http_mgmt_package() {
    assert_eq!(M7_HTTP_OK_MARKER, "RAYNU-V-M7-HTTP-OK");
    assert!(HTTP_GAP_NOTE.contains("CLOSED M7.1"));
    assert!(HTTP_LAB_NOTE.contains("plaintext HTTP"));
    assert_eq!(MGMT_HTTP_DEFAULT_PORT, 8443);
    assert!(prop_http_mgmt_package());
    println!("RAYNU-V-M7-HTTP-OK");
}
