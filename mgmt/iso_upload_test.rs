//! M8.4 host ISO upload: PUT/POST bytes into a datastore blob. Not iron.

use super::{
    dispatch_iso_upload_host, firmware_upload_is_esp_staged, get_iso_blob,
    host_never_prints_iron_iso_upload_ok, prop_iso_upload_host_package, put_iso_blob, IsoBlobTable,
    UploadError, UploadMode, ISO_UPLOAD_HOST_RESIDUAL_NOTE, M8_ISO_UPLOAD_HOST_OK_MARKER,
    M8_ISO_UPLOAD_OK_MARKER, UPLOAD_FIRMWARE_ESP_NOTE,
};
use crate::mgmt::api::RestMethod;
use crate::mgmt::datastore::ImageTable;
use crate::mgmt::http::handle_http_request;
use crate::mgmt::iso::IsoDeployPlan;
use crate::mgmt::iso_install::InstallToDiskPlan;
use crate::mgmt::VmTable;

#[test]
fn firmware_upload_stays_esp_staged() {
    assert_eq!(super::FIRMWARE_UPLOAD_MODE, UploadMode::EspStaged);
    assert!(firmware_upload_is_esp_staged());
    assert!(UPLOAD_FIRMWARE_ESP_NOTE.contains("ESP-staged stays valid"));
    assert_eq!(M8_ISO_UPLOAD_OK_MARKER, "RAYNU-V-M8-ISO-UPLOAD-OK");
    assert_eq!(
        M8_ISO_UPLOAD_HOST_OK_MARKER,
        "RAYNU-V-M8-ISO-UPLOAD-HOST-OK"
    );
    assert_ne!(M8_ISO_UPLOAD_OK_MARKER, M8_ISO_UPLOAD_HOST_OK_MARKER);
    assert!(host_never_prints_iron_iso_upload_ok());
    let mut store = ImageTable::new();
    let mut blobs = IsoBlobTable::new();
    assert_eq!(
        put_iso_blob(
            UploadMode::EspStaged,
            &mut store,
            &mut blobs,
            1,
            "linux.iso",
            b"x"
        ),
        Err(UploadError::EspStagedOnly)
    );
    assert!(prop_iso_upload_host_package());
}

#[test]
fn host_ready_iso_blob_roundtrip() {
    assert!(prop_iso_upload_host_package());

    let mut store = ImageTable::new();
    let mut blobs = IsoBlobTable::new();
    let bytes = b"CD001-HOST-ISO";
    assert_eq!(
        put_iso_blob(
            UploadMode::HostReady,
            &mut store,
            &mut blobs,
            9,
            "alpine.iso",
            bytes
        ),
        Ok(bytes.len())
    );
    let mut out = [0u8; 32];
    let n = get_iso_blob(&blobs, 9, &mut out).expect("blob");
    assert_eq!(&out[..n], bytes);
    assert_eq!(store.get(9).unwrap().size_bytes, bytes.len() as u64);

    let mut table = VmTable::new();
    let mut images = ImageTable::new();
    let mut iso_plan = IsoDeployPlan::empty();
    let mut iso_install = InstallToDiskPlan::empty();
    let mut http_out = [0u8; 16384];
    let hn = handle_http_request(
        &mut table,
        &mut images,
        &mut iso_plan,
        &mut iso_install,
        "GET / HTTP/1.1\r\nHost: localhost\r\n\r\n",
        &mut http_out,
    )
    .unwrap_or(0);
    let spa = core::str::from_utf8(&http_out[..hn]).unwrap_or("");
    assert!(spa.contains("HTTP/1.1 200"), "{spa}");
    assert!(!spa.contains("/blob"), "{spa}");
    assert!(!spa.contains("type=\"file\""), "{spa}");
    assert_eq!(
        dispatch_iso_upload_host(
            UploadMode::EspStaged,
            &mut store,
            &mut blobs,
            RestMethod::Post,
            "/iso/9/blob",
            bytes
        ),
        501
    );
    assert!(ISO_UPLOAD_HOST_RESIDUAL_NOTE.contains("ESP-staged stays valid"));
    assert!(firmware_upload_is_esp_staged());
    println!("{M8_ISO_UPLOAD_HOST_OK_MARKER}");
}
