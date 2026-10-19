//! A3 SKU card: dedicated-box honesty page. Not Bar A complete.

use super::{
    prop_sku_card_package, prop_sku_catalog_published, SKU_CARD_HOST_OK_MARKER,
    SKU_CARD_RESIDUAL_NOTE,
};

#[test]
fn sku_card_package_passes() {
    assert!(
        prop_sku_card_package(),
        "A3 SKU card must name ships vs does-not"
    );
    assert!(SKU_CARD_RESIDUAL_NOTE.contains("A4 TLS iron DONE"));
    assert!(SKU_CARD_RESIDUAL_NOTE.contains("plaintext HTTP"));
    assert_eq!(SKU_CARD_HOST_OK_MARKER, "RAYNU-V-M8-SKU-CARD-OK");
    println!("{SKU_CARD_HOST_OK_MARKER}");
}

#[test]
fn sku_catalog_is_published_from_adr_021() {
    // ADR-021: one catalog, two shipments, three words. The page renders
    // site/sku.json; the JSON is generated from the ADR; lived needs evidence.
    assert!(prop_sku_catalog_published());
    let json = include_str!("../site/sku.json");
    for area in ["\"id\": \"A\"", "\"id\": \"D\"", "\"id\": \"K\""] {
        assert!(json.contains(area), "area {area}");
    }
    assert!(json.contains("Uplink discovery"), "the :39 lesson is a row");
    assert!(json.contains("Physical link state of both LOM ports"), "link state is a row");
    assert!(!json.contains("\"status\": \"done\""), "only lived / in flight / planned");
}
