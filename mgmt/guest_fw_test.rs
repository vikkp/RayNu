use super::{
    box_guest_firmware, dispatch_guest_fw_rest, guest_fw_bytes, guest_fw_is_boxed, parse_guest_fw,
    reset_guest_fw, write_guest_fw_header, GuestFwError, GuestFwKind, GUEST_FW_BLOB_LEN,
    GUEST_FW_HEADER_LEN, GUEST_FW_MAX_COMPRESSED, GUEST_FW_MAX_UNCOMPRESSED, SECTION_GUEST_FW,
};
use crate::mgmt::api::{RestMethod, RestRequest, BRINGUP_AUTH_TOKEN};

#[test]
fn embedded_placeholder_parses() {
    reset_guest_fw();
    let bytes = guest_fw_bytes();
    assert_eq!(bytes.len(), GUEST_FW_BLOB_LEN);
    let parsed = parse_guest_fw(bytes).unwrap();
    assert_eq!(parsed.kind, GuestFwKind::UefiEnvelope);
    assert_eq!(parsed.uncompressed_len, GUEST_FW_MAX_UNCOMPRESSED);
    assert_eq!(parsed.compressed_len, GUEST_FW_MAX_COMPRESSED);
    assert_eq!(parsed.payload_len, 0);
    assert!(parsed.lazy_zstd);
    assert!(!parsed.boxed);
    assert_eq!(SECTION_GUEST_FW, ".asguefw");
}

#[test]
fn box_sets_flag_and_rejects_oversize() {
    reset_guest_fw();
    let boxed = box_guest_firmware(guest_fw_bytes()).unwrap();
    assert!(boxed.boxed);
    assert!(guest_fw_is_boxed());

    let mut bad = [0u8; GUEST_FW_HEADER_LEN];
    write_guest_fw_header(&mut bad, GUEST_FW_MAX_UNCOMPRESSED + 1, 1024, 0).unwrap();
    assert_eq!(parse_guest_fw(&bad), Err(GuestFwError::TooLarge));
    reset_guest_fw();
}

#[test]
fn rest_box_requires_bearer() {
    reset_guest_fw();
    let denied = dispatch_guest_fw_rest(RestRequest {
        method: RestMethod::Post,
        path: "/fw/box",
        auth_token: None,
    });
    assert_eq!(denied.status, 401);

    let boxed = dispatch_guest_fw_rest(RestRequest {
        method: RestMethod::Post,
        path: "/fw/box",
        auth_token: Some(BRINGUP_AUTH_TOKEN),
    });
    assert_eq!(boxed.status, 201);
    assert!(guest_fw_is_boxed());

    let st = dispatch_guest_fw_rest(RestRequest {
        method: RestMethod::Get,
        path: "/fw",
        auth_token: Some(BRINGUP_AUTH_TOKEN),
    });
    assert_eq!(st.status, 200);
    reset_guest_fw();
}
