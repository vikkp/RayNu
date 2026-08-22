use super::{
    attach_cdrom_firmware, attach_cdrom_host, attach_cdrom_uefi, bind_extract_boot,
    configure_install_disk, dispatch_iso_attach_rest, dispatch_iso_firmware_rest, dispatch_iso_rest,
    extract_boot_surface_present, install_disk_surface_present, prop_iso_deploy_package,
    register_iso, CdromAttachState, CdromTable, IsoDeployPlan, IsoError,
    DEFAULT_INSTALL_DISK_BYTES, ISO_EXTRACT_BOOT_NOTE, ISO_GAP_NOTE, M7_ISO_OK_MARKER,
};
use crate::mgmt::el_torito::{write_mock_efi_iso, MOCK_EFI_ISO_BYTES};
use crate::mgmt::guest_image::GuestImageType;
use crate::mgmt::api::{ApiReply, RestMethod, RestRequest, BRINGUP_AUTH_TOKEN};
use crate::mgmt::datastore::{ImageKind, ImageTable};

#[test]
fn register_bind_install_roundtrip() {
    let mut store = ImageTable::new();
    let mut plan = IsoDeployPlan::empty();
    register_iso(&mut store, 2, 1000, "debian.iso").unwrap();
    assert_eq!(store.get(2).unwrap().kind, ImageKind::Iso);
    bind_extract_boot(&store, &mut plan, 2).unwrap();
    configure_install_disk(&mut plan, DEFAULT_INSTALL_DISK_BYTES).unwrap();
    assert!(plan.is_ready());
    assert_eq!(
        configure_install_disk(&mut plan, 100),
        Err(IsoError::BadState)
    );
}

#[test]
fn cdrom_stub_honest() {
    assert_eq!(
        attach_cdrom_uefi(1),
        Err(IsoError::UnsupportedOnFirmware)
    );
    assert!(ISO_EXTRACT_BOOT_NOTE.contains("kernel-extract"));
}

#[test]
fn surfaces_present() {
    assert!(extract_boot_surface_present());
    assert!(install_disk_surface_present());
}

#[test]
fn iso_rest_deploy() {
    let mut store = ImageTable::new();
    let mut plan = IsoDeployPlan::empty();
    let tok = Some(BRINGUP_AUTH_TOKEN);
    let r = dispatch_iso_rest(
        &mut store,
        &mut plan,
        RestRequest {
            method: RestMethod::Post,
            path: "/iso/5/deploy",
            auth_token: tok,
        },
    );
    assert_eq!(r.status, 201);
    assert!(plan.is_ready());
    assert!(store.get(5).is_some());
    let st = dispatch_iso_rest(
        &mut store,
        &mut plan,
        RestRequest {
            method: RestMethod::Get,
            path: "/iso/deploy",
            auth_token: tok,
        },
    );
    assert_eq!(st.reply, Some(ApiReply::Listed { count: 1 }));
    let denied = dispatch_iso_rest(
        &mut store,
        &mut plan,
        RestRequest {
            method: RestMethod::Post,
            path: "/iso/6/deploy",
            auth_token: None,
        },
    );
    assert_eq!(denied.status, 401);
}

#[test]
fn iso_deploy_package_prop() {
    assert!(prop_iso_deploy_package());
    assert!(ISO_GAP_NOTE.contains("CLOSED M7.3"));
    assert_eq!(M7_ISO_OK_MARKER, "RAYNU-V-M7-ISO-OK");
}

#[test]
fn host_cdrom_attach_mock_efi() {
    let mut iso = [0u8; MOCK_EFI_ISO_BYTES];
    write_mock_efi_iso(&mut iso).unwrap();
    let rec = attach_cdrom_host(&iso, 2, GuestImageType::GenericUefi).unwrap();
    assert_eq!(rec.state, CdromAttachState::AttachedHost);
    assert!(rec.efi);
    assert_eq!(rec.image_type, GuestImageType::GenericUefi);
    assert_eq!(
        attach_cdrom_uefi(2),
        Err(IsoError::UnsupportedOnFirmware)
    );
}

#[test]
fn iso_rest_host_attach() {
    let mut store = crate::mgmt::datastore::ImageTable::new();
    let mut cdrom = CdromTable::empty();
    let tok = Some(BRINGUP_AUTH_TOKEN);
    let r = dispatch_iso_attach_rest(
        &mut store,
        &mut cdrom,
        RestRequest {
            method: RestMethod::Post,
            path: "/iso/5/attach",
            auth_token: tok,
        },
    );
    assert_eq!(r.status, 201);
    assert_eq!(cdrom.attached_count(), 1);
    let st = dispatch_iso_attach_rest(
        &mut store,
        &mut cdrom,
        RestRequest {
            method: RestMethod::Get,
            path: "/iso/attach",
            auth_token: tok,
        },
    );
    assert_eq!(st.reply, Some(ApiReply::Listed { count: 1 }));
}

#[test]
fn host_then_firmware_arm() {
    let mut iso = [0u8; MOCK_EFI_ISO_BYTES];
    write_mock_efi_iso(&mut iso).unwrap();
    let host = attach_cdrom_host(&iso, 2, GuestImageType::LinuxIso).unwrap();
    let armed = attach_cdrom_firmware(&iso, host).unwrap();
    assert_eq!(armed.state, CdromAttachState::FirmwareArmed);
    assert_eq!(
        attach_cdrom_uefi(2),
        Err(IsoError::UnsupportedOnFirmware)
    );
}

#[test]
fn iso_rest_firmware_requires_host_attach() {
    let mut store = crate::mgmt::datastore::ImageTable::new();
    let mut cdrom = CdromTable::empty();
    let tok = Some(BRINGUP_AUTH_TOKEN);
    let missing = dispatch_iso_firmware_rest(
        &mut store,
        &mut cdrom,
        RestRequest {
            method: RestMethod::Post,
            path: "/iso/5/firmware",
            auth_token: tok,
        },
    );
    assert_eq!(missing.status, 409);
    let _ = dispatch_iso_attach_rest(
        &mut store,
        &mut cdrom,
        RestRequest {
            method: RestMethod::Post,
            path: "/iso/5/attach",
            auth_token: tok,
        },
    );
    let armed = dispatch_iso_firmware_rest(
        &mut store,
        &mut cdrom,
        RestRequest {
            method: RestMethod::Post,
            path: "/iso/5/firmware",
            auth_token: tok,
        },
    );
    assert_eq!(armed.status, 201);
    assert_eq!(cdrom.firmware_armed_count(), 1);
}
