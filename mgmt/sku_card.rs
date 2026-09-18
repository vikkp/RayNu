//! A3 SKU card (outside Proven Core).
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** (ADR-002 / ADR-018). Docs close, not VMX/EPT.
//! VERIFICATION: L1 host tests (honesty phrases on the one-pager).
//!
//! Bar A A3 is a one-page “what you are buying.” Dedicated-box vs fleet.
//! USB 8 GiB slice. plaintext HTTP. lab latch until ESP `auth.token`.
//! not PERC. not cluster. not Windows. A4 TLS iron remains NOW.

/// Host/CI: the SKU page names ships vs does-not. Not an iron COM2 marker.
pub const SKU_CARD_HOST_OK_MARKER: &str = "RAYNU-V-M8-SKU-CARD-OK";

/// Honesty: a docs close is not Bar A complete and not a fleet SKU.
pub const SKU_CARD_RESIDUAL_NOTE: &str =
    "residual: A3 SKU card is docs; A4 TLS iron remains NOW; plaintext HTTP; lab latch until auth.token; 8 GiB USB slice; not PERC; not cluster; not Windows; nested QEMU ≠ R640";

/// True when the SKU page and LOIHDA A3 row stay honest.
pub fn prop_sku_card_package() -> bool {
    let sku = include_str!("../docs/sku.md");
    let html = include_str!("../site/sku.html");
    let loi = include_str!("../docs/loihda.md");
    sku.contains("dedicated-box")
        && sku.contains("8 GiB")
        && sku.contains("plaintext HTTP")
        && sku.contains("lab latch")
        && sku.contains("auth.token")
        && sku.contains("not PERC")
        && sku.contains("not cluster")
        && sku.contains("not Windows")
        && sku.contains("A3 This SKU card")
        && sku.contains("**DONE**")
        && html.contains("dedicated-box")
        && html.contains("8 GiB")
        && html.contains("plaintext HTTP")
        && html.contains("lab latch")
        && html.contains("auth.token")
        && html.contains("not PERC")
        && html.contains("not cluster")
        && html.contains("not Windows")
        && loi.contains("| A3 |")
        && loi.contains("SKU card")
        && SKU_CARD_RESIDUAL_NOTE.contains("A4 TLS iron remains NOW")
        && SKU_CARD_RESIDUAL_NOTE.contains("not PERC")
}

#[cfg(test)]
#[path = "sku_card_test.rs"]
mod sku_card_test;
