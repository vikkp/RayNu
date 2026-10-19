//! A3 SKU card (outside Proven Core).
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** (ADR-002 / ADR-018). Docs close, not VMX/EPT.
//! VERIFICATION: L1 host tests (honesty phrases on the one-pager).
//!
//! Bar A A3 is a one-page “what you are buying.” Dedicated-box vs fleet.
//! USB 8 GiB slice. native HTTPS (lab millicert). PRE-EBS SNP plaintext HTTP.
//! lab latch until ESP `auth.token`. not PERC. not cluster. not Windows.
//! A4 TLS iron DONE. A5/A6 remain.
//!
//! 2026-10-19: the LOI scoreboard (`docs/loihda.md`, `site/loi.json`) is
//! retired. `site/loi.html` is a derived view of `site/sku.json` (ADR-021
//! Letters table). Three pages, three jobs: HDA = how we got here, SKU =
//! what you get, LOI = are we ready to talk.
//!
//! ADR-021 (2026-10-19): the card also carries **Shipment 1** and the
//! **final SKU**, both rendered from `site/sku.json`, which
//! `./tools/sync-sku-site.sh` generates from the ADR-021 catalog. The
//! "today" honesty phrases stay on the card in front of both shipments.

/// Host/CI: the SKU page names ships vs does-not. Not an iron COM2 marker.
pub const SKU_CARD_HOST_OK_MARKER: &str = "RAYNU-V-M8-SKU-CARD-OK";

/// Honesty: a docs close is not Bar A complete and not a fleet SKU.
pub const SKU_CARD_RESIDUAL_NOTE: &str =
    "residual: A3 SKU card is docs; A4 TLS iron DONE (lab millicert); PRE-EBS SNP plaintext HTTP remains; lab latch until auth.token; 8 GiB USB slice; not PERC; not cluster; not Windows; nested QEMU ≠ R640";

/// True when the SKU page stays honest and the LOI page derives from it.
pub fn prop_sku_card_package() -> bool {
    let sku = include_str!("../docs/sku.md");
    let html = include_str!("../site/sku.html");
    let loi = include_str!("../site/loi.html");
    let loi_js = include_str!("../site/loi.js");
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
        && loi.contains("Bar A")
        && loi.contains("Bar B")
        && loi.contains("sku.json")
        && !loi.contains("loi.json")
        && !loi.contains("loihda")
        && loi_js.contains("fetch(\"sku.json\"")
        && !loi_js.contains("overall_pct")
        && !loi_js.contains("months_to_loi")
        && SKU_CARD_RESIDUAL_NOTE.contains("A4 TLS iron DONE")
        && SKU_CARD_RESIDUAL_NOTE.contains("not PERC")
        && prop_sku_catalog_published()
}

/// True when the card and the page publish Shipment 1 and the final SKU
/// from the ADR-021 catalog, and the sync tool enforces the evidence rule.
pub fn prop_sku_catalog_published() -> bool {
    let sku = include_str!("../docs/sku.md");
    let html = include_str!("../site/sku.html");
    let adr = include_str!("../docs/adr/ADR-021.md");
    let sync = include_str!("../tools/sync-sku-site.sh");
    let json = include_str!("../site/sku.json");
    sku.contains("Shipment 1")
        && sku.contains("final SKU")
        && sku.contains("ADR-021")
        && sku.contains("sync-sku-site.sh")
        && html.contains("Shipment 1")
        && html.contains("ADR-021")
        && html.contains("sku.js")
        && html.contains("id=\"today\"")
        && adr.contains("## Catalog")
        && adr.contains("| ID | Capability | Status | Ships | Ref | Note |")
        && adr.contains("### K. Cluster and elasticity (M9)")
        && sync.contains("lived rows need evidence")
        && sync.contains("--check")
        && json.contains("\"generated_from\": \"docs/adr/ADR-021.md\"")
        && json.contains("\"status\": \"lived\"")
        && json.contains("\"ships\": \"S1\"")
        && adr.contains("## Build order")
        && adr.contains("| Wave | Theme | Rows | Iron thread | Why here |")
        && sync.contains("--next")
        && sync.contains("not in any wave")
        && json.contains("\"current_wave\": \"")
        && json.contains("\"next_up\": [")
        && html.contains("id=\"sku-now\"")
        && include_str!("../.cursor/rules/build-order.mdc").contains("sync-sku-site.sh --next")
        && adr.contains("## Letters")
        && adr.contains("| Letter | Name | Rows | Meaning |")
        && sync.contains("letters table is empty")
        && json.contains("\"letters\": [")
}

#[cfg(test)]
#[path = "sku_card_test.rs"]
mod sku_card_test;
