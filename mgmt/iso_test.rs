use super::{
    attach_cdrom_uefi, bind_extract_boot, configure_install_disk, dispatch_iso_rest,
    extract_boot_surface_present, install_disk_surface_present, prop_iso_deploy_package,
    register_iso, IsoDeployPlan, IsoError, DEFAULT_INSTALL_DISK_BYTES, ISO_EXTRACT_BOOT_NOTE,
    ISO_GAP_NOTE, M7_ISO_OK_MARKER,
};
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
