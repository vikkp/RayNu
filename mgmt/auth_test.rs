//! M8.2 host auth: operator token is the product latch. Not iron.

use super::{
    auth_allows_for, firmware_auth_is_lab_bringup, host_never_prints_iron_auth_ok,
    prop_auth_host_package, AuthMode, M8_AUTH_HOST_OK_MARKER, M8_AUTH_OK_MARKER,
    AUTH_FIRMWARE_LAB_NOTE, AUTH_HOST_RESIDUAL_NOTE,
};
use crate::mgmt::api::{
    clear_operator_token, dispatch_rest, set_operator_token, RestMethod, RestRequest,
    BRINGUP_AUTH_TOKEN,
};
use crate::mgmt::http::handle_http_request;
use crate::mgmt::datastore::ImageTable;
use crate::mgmt::iso::IsoDeployPlan;
use crate::mgmt::iso_install::InstallToDiskPlan;
use crate::mgmt::VmTable;

fn rest_status(mode: AuthMode, token: Option<&str>) -> u16 {
    if !auth_allows_for(mode, token) {
        return 401;
    }
    let mut table = VmTable::new();
    dispatch_rest(
        &mut table,
        RestRequest {
            method: RestMethod::Get,
            path: "/vms",
            auth_token: token,
        },
    )
    .status
}

#[test]
fn firmware_auth_stays_lab_bringup() {
    clear_operator_token();
    assert_eq!(super::FIRMWARE_AUTH_MODE, AuthMode::LabBringUp);
    assert!(firmware_auth_is_lab_bringup());
    assert!(AUTH_FIRMWARE_LAB_NOTE.contains("BRINGUP_AUTH_TOKEN"));
    assert_eq!(M8_AUTH_OK_MARKER, "RAYNU-V-M8-AUTH-OK");
    assert_eq!(M8_AUTH_HOST_OK_MARKER, "RAYNU-V-M8-AUTH-HOST-OK");
    assert_ne!(M8_AUTH_OK_MARKER, M8_AUTH_HOST_OK_MARKER);
    assert!(host_never_prints_iron_auth_ok());
    assert!(auth_allows_for(AuthMode::LabBringUp, Some(BRINGUP_AUTH_TOKEN)));
    assert!(prop_auth_host_package());
}

#[test]
fn host_ready_rejects_bringup_and_serves_operator_token() {
    clear_operator_token();
    assert_eq!(rest_status(AuthMode::HostReady, None), 401);
    assert_eq!(
        rest_status(AuthMode::HostReady, Some(BRINGUP_AUTH_TOKEN)),
        401
    );
    assert_eq!(rest_status(AuthMode::LabBringUp, Some(BRINGUP_AUTH_TOKEN)), 200);

    set_operator_token(b"raynu-v-op-m82").expect("op token");
    assert_eq!(
        rest_status(AuthMode::HostReady, Some(BRINGUP_AUTH_TOKEN)),
        401
    );
    assert_eq!(rest_status(AuthMode::HostReady, Some("wrong-op")), 401);
    assert_eq!(
        rest_status(AuthMode::HostReady, Some("raynu-v-op-m82")),
        200
    );
    clear_operator_token();

    let mut table = VmTable::new();
    let mut images = ImageTable::new();
    let mut iso_plan = IsoDeployPlan::empty();
    let mut iso_install = InstallToDiskPlan::empty();
    let mut out = [0u8; crate::mgmt::http::HTTP_RESPONSE_CAP];
    let n = handle_http_request(
        &mut table,
        &mut images,
        &mut iso_plan,
        &mut iso_install,
        "GET / HTTP/1.1\r\nHost: localhost\r\n\r\n",
        &mut out,
    )
    .unwrap_or(0);
    let spa = core::str::from_utf8(&out[..n]).unwrap_or("");
    assert!(spa.contains("HTTP/1.1 200"), "{spa}");
    assert!(spa.contains("data-go=\"overview\""), "{spa}");
    assert!(AUTH_HOST_RESIDUAL_NOTE.contains("not iron"));
    assert!(firmware_auth_is_lab_bringup());
    println!("{M8_AUTH_HOST_OK_MARKER}");
}
