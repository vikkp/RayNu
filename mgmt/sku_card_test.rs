//! A3 SKU card: dedicated-box honesty page. Not Bar A complete.

use super::{prop_sku_card_package, SKU_CARD_HOST_OK_MARKER, SKU_CARD_RESIDUAL_NOTE};

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
