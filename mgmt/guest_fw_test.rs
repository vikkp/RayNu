use super::{
    box_guest_firmware, dispatch_guest_fw_rest, guest_fw_bytes, guest_fw_is_boxed, guest_fw_is_loaded,
    guest_fw_payload, load_guest_firmware, load_ovmf_from_esp, ovmf_esp_is_loaded, ovmf_fv_is_probed,
    ovmf_floor_is_staged, ovmf_guest_is_bound, ovmf_launch_is_prepared, ovmf_slot_is_armed,
    parse_guest_fw, prepare_ovmf_firmware_launch, probe_ovmf_firmware, reset_guest_fw,
    stage_edk2_ovmf_firmware, stage_ovmf_firmware_floor, try_vmlaunch_ovmf_firmware,
    write_edk2_sized_fv, write_guest_fw_header, write_mock_ovmf_fv, write_size_floor_ovmf_fv,
    arm_ovmf_esp_launch, arm_ovmf_firmware_slot, bind_ovmf_firmware_guest, ovmf_edk2_is_staged,
    ovmf_esp_launch_is_armed, GuestFwError,
    GuestFwKind, GUEST_FW_BLOB_LEN, GUEST_FW_HEADER_LEN, GUEST_FW_MAX_COMPRESSED,
    GUEST_FW_MAX_UNCOMPRESSED, GUEST_FW_STUB_PAYLOAD_LEN, MIN_EDK2_OVMF_BYTES, MOCK_OVMF_FV_BYTES,
    OVMF_FW_GUEST_ID, SIZE_FLOOR_FV_BYTES, OVMF_FW_SLOT_ID, SECTION_GUEST_FW,
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
    assert_eq!(parsed.payload_len, GUEST_FW_STUB_PAYLOAD_LEN);
    assert!(parsed.lazy_zstd);
    assert!(!parsed.boxed);
    let payload = guest_fw_payload(bytes).unwrap();
    assert_eq!(payload.len(), GUEST_FW_STUB_PAYLOAD_LEN as usize);
    assert_eq!(SECTION_GUEST_FW, ".asguefw");
}

#[test]
fn box_sets_flag_and_rejects_oversize() {
    reset_guest_fw();
    let boxed = box_guest_firmware(guest_fw_bytes()).unwrap();
    assert!(boxed.boxed);
    assert!(guest_fw_is_boxed());
    assert_eq!(load_guest_firmware(guest_fw_bytes()).unwrap().payload_len, GUEST_FW_STUB_PAYLOAD_LEN);
    assert!(guest_fw_is_loaded());

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

    let missing = dispatch_guest_fw_rest(RestRequest {
        method: RestMethod::Post,
        path: "/fw/load",
        auth_token: Some(BRINGUP_AUTH_TOKEN),
    });
    assert_eq!(missing.status, 409);

    let boxed = dispatch_guest_fw_rest(RestRequest {
        method: RestMethod::Post,
        path: "/fw/box",
        auth_token: Some(BRINGUP_AUTH_TOKEN),
    });
    assert_eq!(boxed.status, 201);
    assert!(guest_fw_is_boxed());

    let loaded = dispatch_guest_fw_rest(RestRequest {
        method: RestMethod::Post,
        path: "/fw/load",
        auth_token: Some(BRINGUP_AUTH_TOKEN),
    });
    assert_eq!(loaded.status, 201);
    assert!(guest_fw_is_loaded());

    let st = dispatch_guest_fw_rest(RestRequest {
        method: RestMethod::Get,
        path: "/fw",
        auth_token: Some(BRINGUP_AUTH_TOKEN),
    });
    assert_eq!(st.status, 200);

    let probed = dispatch_guest_fw_rest(RestRequest {
        method: RestMethod::Post,
        path: "/fw/ovmf",
        auth_token: Some(BRINGUP_AUTH_TOKEN),
    });
    assert_eq!(probed.status, 201);
    assert!(ovmf_fv_is_probed());

    let esp = dispatch_guest_fw_rest(RestRequest {
        method: RestMethod::Post,
        path: "/fw/ovmf/esp",
        auth_token: Some(BRINGUP_AUTH_TOKEN),
    });
    assert_eq!(esp.status, 201);
    assert!(ovmf_esp_is_loaded());

    let slot = dispatch_guest_fw_rest(RestRequest {
        method: RestMethod::Post,
        path: "/fw/slot",
        auth_token: Some(BRINGUP_AUTH_TOKEN),
    });
    assert_eq!(slot.status, 201);
    assert!(ovmf_slot_is_armed());

    let bound = dispatch_guest_fw_rest(RestRequest {
        method: RestMethod::Post,
        path: "/fw/bind",
        auth_token: Some(BRINGUP_AUTH_TOKEN),
    });
    assert_eq!(bound.status, 201);
    assert!(ovmf_guest_is_bound());

    let prepped = dispatch_guest_fw_rest(RestRequest {
        method: RestMethod::Post,
        path: "/fw/prepare",
        auth_token: Some(BRINGUP_AUTH_TOKEN),
    });
    assert_eq!(prepped.status, 201);
    assert!(ovmf_launch_is_prepared());

    let floor = dispatch_guest_fw_rest(RestRequest {
        method: RestMethod::Post,
        path: "/fw/floor",
        auth_token: Some(BRINGUP_AUTH_TOKEN),
    });
    assert_eq!(floor.status, 201);
    assert!(ovmf_floor_is_staged());

    let edk2 = dispatch_guest_fw_rest(RestRequest {
        method: RestMethod::Post,
        path: "/fw/edk2",
        auth_token: Some(BRINGUP_AUTH_TOKEN),
    });
    assert_eq!(edk2.status, 201);
    assert!(ovmf_edk2_is_staged());

    let esp_launch = dispatch_guest_fw_rest(RestRequest {
        method: RestMethod::Post,
        path: "/fw/esp-launch",
        auth_token: Some(BRINGUP_AUTH_TOKEN),
    });
    assert_eq!(esp_launch.status, 201);
    assert!(ovmf_esp_launch_is_armed());

    let refused = dispatch_guest_fw_rest(RestRequest {
        method: RestMethod::Post,
        path: "/fw/vmlaunch",
        auth_token: Some(BRINGUP_AUTH_TOKEN),
    });
    assert_eq!(refused.status, 409);
    reset_guest_fw();
}

#[test]
fn ovmf_probe_requires_load() {
    reset_guest_fw();
    let mut fv = [0u8; MOCK_OVMF_FV_BYTES];
    write_mock_ovmf_fv(&mut fv).unwrap();
    assert_eq!(
        probe_ovmf_firmware(&fv),
        Err(GuestFwError::NotLoaded)
    );
    assert!(!ovmf_fv_is_probed());

    let missing = dispatch_guest_fw_rest(RestRequest {
        method: RestMethod::Post,
        path: "/fw/ovmf",
        auth_token: Some(BRINGUP_AUTH_TOKEN),
    });
    assert_eq!(missing.status, 409);

    box_guest_firmware(guest_fw_bytes()).unwrap();
    load_guest_firmware(guest_fw_bytes()).unwrap();
    let probed = probe_ovmf_firmware(&fv).unwrap();
    assert_eq!(probed.fv_len, MOCK_OVMF_FV_BYTES as u64);
    assert!(ovmf_fv_is_probed());
    assert_eq!(load_ovmf_from_esp(&[]), Err(GuestFwError::MissingEsp));
    let esp = load_ovmf_from_esp(&fv).unwrap();
    assert_eq!(esp.fv_len, MOCK_OVMF_FV_BYTES as u64);
    assert!(ovmf_esp_is_loaded());
    reset_guest_fw();
}

#[test]
fn ovmf_esp_requires_probe() {
    reset_guest_fw();
    let mut fv = [0u8; MOCK_OVMF_FV_BYTES];
    write_mock_ovmf_fv(&mut fv).unwrap();
    assert_eq!(load_ovmf_from_esp(&fv), Err(GuestFwError::NotProbed));
    assert!(!ovmf_esp_is_loaded());

    let missing = dispatch_guest_fw_rest(RestRequest {
        method: RestMethod::Post,
        path: "/fw/ovmf/esp",
        auth_token: Some(BRINGUP_AUTH_TOKEN),
    });
    assert_eq!(missing.status, 409);
    reset_guest_fw();
}

#[test]
fn ovmf_slot_requires_esp() {
    reset_guest_fw();
    assert_eq!(
        arm_ovmf_firmware_slot(),
        Err(GuestFwError::NotEspLoaded)
    );
    assert!(!ovmf_slot_is_armed());

    let missing = dispatch_guest_fw_rest(RestRequest {
        method: RestMethod::Post,
        path: "/fw/slot",
        auth_token: Some(BRINGUP_AUTH_TOKEN),
    });
    assert_eq!(missing.status, 409);

    box_guest_firmware(guest_fw_bytes()).unwrap();
    load_guest_firmware(guest_fw_bytes()).unwrap();
    let mut fv = [0u8; MOCK_OVMF_FV_BYTES];
    write_mock_ovmf_fv(&mut fv).unwrap();
    probe_ovmf_firmware(&fv).unwrap();
    load_ovmf_from_esp(&fv).unwrap();
    let slot = arm_ovmf_firmware_slot().unwrap();
    assert_eq!(slot.slot_id, OVMF_FW_SLOT_ID);
    assert!(ovmf_slot_is_armed());
    reset_guest_fw();
}

#[test]
fn ovmf_bind_requires_slot() {
    reset_guest_fw();
    assert_eq!(
        bind_ovmf_firmware_guest(),
        Err(GuestFwError::NotSlotArmed)
    );
    assert!(!ovmf_guest_is_bound());

    let missing = dispatch_guest_fw_rest(RestRequest {
        method: RestMethod::Post,
        path: "/fw/bind",
        auth_token: Some(BRINGUP_AUTH_TOKEN),
    });
    assert_eq!(missing.status, 409);

    box_guest_firmware(guest_fw_bytes()).unwrap();
    load_guest_firmware(guest_fw_bytes()).unwrap();
    let mut fv = [0u8; MOCK_OVMF_FV_BYTES];
    write_mock_ovmf_fv(&mut fv).unwrap();
    probe_ovmf_firmware(&fv).unwrap();
    load_ovmf_from_esp(&fv).unwrap();
    arm_ovmf_firmware_slot().unwrap();
    let bound = bind_ovmf_firmware_guest().unwrap();
    assert_eq!(bound.guest_id, OVMF_FW_GUEST_ID);
    assert_eq!(bound.slot_id, OVMF_FW_SLOT_ID);
    assert!(ovmf_guest_is_bound());
    reset_guest_fw();
}

#[test]
fn ovmf_prep_requires_bind_and_refuses_mock() {
    reset_guest_fw();
    assert_eq!(
        prepare_ovmf_firmware_launch(),
        Err(GuestFwError::NotGuestBound)
    );
    assert_eq!(
        try_vmlaunch_ovmf_firmware(),
        Err(GuestFwError::NotGuestBound)
    );
    assert!(!ovmf_launch_is_prepared());

    let missing = dispatch_guest_fw_rest(RestRequest {
        method: RestMethod::Post,
        path: "/fw/prepare",
        auth_token: Some(BRINGUP_AUTH_TOKEN),
    });
    assert_eq!(missing.status, 409);

    box_guest_firmware(guest_fw_bytes()).unwrap();
    load_guest_firmware(guest_fw_bytes()).unwrap();
    let mut fv = [0u8; MOCK_OVMF_FV_BYTES];
    write_mock_ovmf_fv(&mut fv).unwrap();
    probe_ovmf_firmware(&fv).unwrap();
    load_ovmf_from_esp(&fv).unwrap();
    arm_ovmf_firmware_slot().unwrap();
    bind_ovmf_firmware_guest().unwrap();
    let prepped = prepare_ovmf_firmware_launch().unwrap();
    assert_eq!(prepped.guest_id, OVMF_FW_GUEST_ID);
    assert_eq!(prepped.slot_id, OVMF_FW_SLOT_ID);
    assert!(ovmf_launch_is_prepared());
    assert_eq!(
        try_vmlaunch_ovmf_firmware(),
        Err(GuestFwError::MockFirmwareRefused)
    );
    reset_guest_fw();
}

#[test]
fn ovmf_floor_requires_prep_and_refuses_edk2_claim() {
    reset_guest_fw();
    let mut floor = [0u8; SIZE_FLOOR_FV_BYTES];
    write_size_floor_ovmf_fv(&mut floor).unwrap();
    assert_eq!(
        stage_ovmf_firmware_floor(&floor),
        Err(GuestFwError::NotGuestBound)
    );
    assert!(!ovmf_floor_is_staged());

    let missing = dispatch_guest_fw_rest(RestRequest {
        method: RestMethod::Post,
        path: "/fw/floor",
        auth_token: Some(BRINGUP_AUTH_TOKEN),
    });
    assert_eq!(missing.status, 409);

    box_guest_firmware(guest_fw_bytes()).unwrap();
    load_guest_firmware(guest_fw_bytes()).unwrap();
    let mut mock = [0u8; MOCK_OVMF_FV_BYTES];
    write_mock_ovmf_fv(&mut mock).unwrap();
    probe_ovmf_firmware(&mock).unwrap();
    load_ovmf_from_esp(&mock).unwrap();
    arm_ovmf_firmware_slot().unwrap();
    bind_ovmf_firmware_guest().unwrap();
    prepare_ovmf_firmware_launch().unwrap();
    assert_eq!(
        stage_ovmf_firmware_floor(&mock),
        Err(GuestFwError::TooSmall)
    );
    let staged = stage_ovmf_firmware_floor(&floor).unwrap();
    assert_eq!(staged.bytes_len, SIZE_FLOOR_FV_BYTES as u64);
    assert!(ovmf_floor_is_staged());
    assert_eq!(
        try_vmlaunch_ovmf_firmware(),
        Err(GuestFwError::NotRealFirmware)
    );
    reset_guest_fw();
}

#[test]
fn ovmf_edk2_requires_floor_and_refuses_launch() {
    reset_guest_fw();
    let mut edk2 = vec![0u8; MIN_EDK2_OVMF_BYTES];
    write_edk2_sized_fv(&mut edk2).unwrap();
    assert_eq!(
        stage_edk2_ovmf_firmware(&edk2),
        Err(GuestFwError::NotRealFirmware)
    );
    assert!(!ovmf_edk2_is_staged());

    let missing = dispatch_guest_fw_rest(RestRequest {
        method: RestMethod::Post,
        path: "/fw/edk2",
        auth_token: Some(BRINGUP_AUTH_TOKEN),
    });
    assert_eq!(missing.status, 409);

    box_guest_firmware(guest_fw_bytes()).unwrap();
    load_guest_firmware(guest_fw_bytes()).unwrap();
    let mut mock = [0u8; MOCK_OVMF_FV_BYTES];
    write_mock_ovmf_fv(&mut mock).unwrap();
    probe_ovmf_firmware(&mock).unwrap();
    load_ovmf_from_esp(&mock).unwrap();
    arm_ovmf_firmware_slot().unwrap();
    bind_ovmf_firmware_guest().unwrap();
    prepare_ovmf_firmware_launch().unwrap();
    let mut floor = [0u8; SIZE_FLOOR_FV_BYTES];
    write_size_floor_ovmf_fv(&mut floor).unwrap();
    stage_ovmf_firmware_floor(&floor).unwrap();
    assert_eq!(
        stage_edk2_ovmf_firmware(&mock),
        Err(GuestFwError::TooSmall)
    );
    assert_eq!(
        stage_edk2_ovmf_firmware(&floor),
        Err(GuestFwError::TooSmall)
    );
    let staged = stage_edk2_ovmf_firmware(&edk2).unwrap();
    assert_eq!(staged.bytes_len, MIN_EDK2_OVMF_BYTES as u64);
    assert!(ovmf_edk2_is_staged());
    assert_eq!(
        try_vmlaunch_ovmf_firmware(),
        Err(GuestFwError::LaunchNotWired)
    );
    reset_guest_fw();
}

#[test]
fn ovmf_esp_launch_requires_edk2_and_refuses_without_live_file() {
    reset_guest_fw();
    assert_eq!(
        arm_ovmf_esp_launch(),
        Err(GuestFwError::LaunchNotWired)
    );
    assert!(!ovmf_esp_launch_is_armed());

    let missing = dispatch_guest_fw_rest(RestRequest {
        method: RestMethod::Post,
        path: "/fw/esp-launch",
        auth_token: Some(BRINGUP_AUTH_TOKEN),
    });
    assert_eq!(missing.status, 409);

    box_guest_firmware(guest_fw_bytes()).unwrap();
    load_guest_firmware(guest_fw_bytes()).unwrap();
    let mut mock = [0u8; MOCK_OVMF_FV_BYTES];
    write_mock_ovmf_fv(&mut mock).unwrap();
    probe_ovmf_firmware(&mock).unwrap();
    load_ovmf_from_esp(&mock).unwrap();
    arm_ovmf_firmware_slot().unwrap();
    bind_ovmf_firmware_guest().unwrap();
    prepare_ovmf_firmware_launch().unwrap();
    let mut floor = [0u8; SIZE_FLOOR_FV_BYTES];
    write_size_floor_ovmf_fv(&mut floor).unwrap();
    stage_ovmf_firmware_floor(&floor).unwrap();
    let mut edk2 = vec![0u8; MIN_EDK2_OVMF_BYTES];
    write_edk2_sized_fv(&mut edk2).unwrap();
    stage_edk2_ovmf_firmware(&edk2).unwrap();
    let armed = arm_ovmf_esp_launch().unwrap();
    assert_eq!(armed.guest_id, OVMF_FW_GUEST_ID);
    assert_eq!(armed.slot_id, OVMF_FW_SLOT_ID);
    assert!(ovmf_esp_launch_is_armed());
    assert_eq!(
        try_vmlaunch_ovmf_firmware(),
        Err(GuestFwError::MissingEsp)
    );
    reset_guest_fw();
}
