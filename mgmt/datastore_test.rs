use super::{
    catalog_decode, catalog_encode, dispatch_store_rest, load_catalog_host, persist_catalog_host,
    persist_catalog_uefi, prop_datastore_package, ImageKind, ImageTable, StoreError,
    ESP_IMAGES_REL, M7_STORE_OK_MARKER, STORE_GAP_NOTE,
};
use crate::mgmt::api::{ApiReply, RestMethod, RestRequest, BRINGUP_AUTH_TOKEN};

#[test]
fn register_list_delete_roundtrip() {
    let mut t = ImageTable::new();
    t.register(1, ImageKind::Iso, 100, "a.iso").unwrap();
    t.register(2, ImageKind::Disk, 200, "b.disk").unwrap();
    assert_eq!(t.len(), 2);
    assert_eq!(t.get(1).unwrap().name(), "a.iso");
    t.delete(1).unwrap();
    assert!(t.get(1).is_none());
    assert_eq!(t.len(), 1);
    assert_eq!(t.delete(1), Err(StoreError::NotFound));
    assert_eq!(t.register(0, ImageKind::Iso, 0, "x"), Err(StoreError::InvalidId));
}

#[test]
fn store_rest_auth_and_shapes() {
    let mut t = ImageTable::new();
    let tok = Some(BRINGUP_AUTH_TOKEN);
    let created = dispatch_store_rest(
        &mut t,
        RestRequest {
            method: RestMethod::Post,
            path: "/images/3/iso",
            auth_token: tok,
        },
    );
    assert_eq!(created.status, 201);
    let get = dispatch_store_rest(
        &mut t,
        RestRequest {
            method: RestMethod::Get,
            path: "/images/3",
            auth_token: tok,
        },
    );
    assert_eq!(get.status, 200);
    assert_eq!(
        get.reply,
        Some(ApiReply::Image {
            id: 3,
            kind_tag: ImageKind::Iso.tag(),
            size_bytes: 0,
        })
    );
    let denied = dispatch_store_rest(
        &mut t,
        RestRequest {
            method: RestMethod::Delete,
            path: "/images/3",
            auth_token: None,
        },
    );
    assert_eq!(denied.status, 401);
    assert!(t.get(3).is_some());
}

#[test]
fn host_catalog_persist_roundtrip() {
    let dir = std::env::temp_dir().join(format!("raynu-m72-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let mut t = ImageTable::new();
    t.register(7, ImageKind::Template, 42, "tmpl").unwrap();
    persist_catalog_host(&dir, &t).unwrap();
    assert!(dir.join(ESP_IMAGES_REL).join("catalog.txt").is_file());

    let mut loaded = ImageTable::new();
    load_catalog_host(&dir, &mut loaded).unwrap();
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded.get(7).unwrap().name(), "tmpl");
    assert_eq!(loaded.get(7).unwrap().size_bytes, 42);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn catalog_codec_and_uefi_stub() {
    let mut t = ImageTable::new();
    t.register(1, ImageKind::Iso, 9, "x.iso").unwrap();
    let mut buf = [0u8; 256];
    let n = catalog_encode(&t, &mut buf).unwrap();
    let mut u = ImageTable::new();
    catalog_decode(&mut u, core::str::from_utf8(&buf[..n]).unwrap()).unwrap();
    assert_eq!(u.get(1).unwrap().kind, ImageKind::Iso);
    assert_eq!(
        persist_catalog_uefi(&t),
        Err(StoreError::UnsupportedOnFirmware)
    );
}

#[test]
fn datastore_package_prop() {
    assert!(prop_datastore_package());
    assert!(STORE_GAP_NOTE.contains("CLOSED M7.2"));
    assert_eq!(M7_STORE_OK_MARKER, "RAYNU-V-M7-STORE-OK");
}
