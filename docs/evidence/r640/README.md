# R640 iron evidence

**Status file:** [`STATUS`](STATUS) — `STATUS=closed` after real PowerEdge R640
first light (2026-08-15). See [`2026-08-15-r640-first-light.md`](2026-08-15-r640-first-light.md).

**M7.6 HTTP:** [`2026-08-16-uefi-http-ok.md`](2026-08-16-uefi-http-ok.md)  
**Tcp4 absent (Floppy root cause):** [`2026-08-16-uefi-tcp4-absent-root-cause.md`](2026-08-16-uefi-tcp4-absent-root-cause.md)  
**E4 SPA + install arm:** [`2026-08-16-e4-spa-install-arm.md`](2026-08-16-e4-spa-install-arm.md)  
**E5 persist write + detect (BLK fail):** [`2026-08-16-e5-persist-detect-blk-fail.md`](2026-08-16-e5-persist-detect-blk-fail.md)  
**E5 iron reboot-to-disk (close):** [`2026-08-16-e5-iso-install.md`](2026-08-16-e5-iso-install.md)  
**Preserve kit:** [`releases/v0.1.0-adr013-baseline/`](../../../releases/v0.1.0-adr013-baseline/) — WARN-only idle + ADR-013 Accepted (before native NIC)  
**Prior preserve:** [`releases/v0.1.0-e4-spa-arm/`](../../../releases/v0.1.0-e4-spa-arm/) — checkpoint before networking deep-dive  
**E5 ISO install:** [`STATUS-iso-install`](STATUS-iso-install) — **closed** (stamp persist; not distro installer)  
**Post-EBS SNP dead:** [`2026-08-17-post-ebs-snp-dead.md`](2026-08-17-post-ebs-snp-dead.md) — firmware SNP hang + curl timeout + RSOD; do not claim `POST-EBS-HTTP-OK`  
**ADR-013 Phase 0 census:** [`2026-08-17-phase0-census.md`](2026-08-17-phase0-census.md) — iron pick **`14e4:165f`** BCM5720 dual-port  
**E3b / M7.8 iron close:** [`2026-08-20-e3b-host-nic-http-ok.md`](2026-08-20-e3b-host-nic-http-ok.md) — **`RAYNU-V-M7-HOST-NIC-HTTP-OK`** after `BOOT-OK` on `:38` / `10.99.99.144:8443`  
**Next architecture:** [`docs/adr/ADR-013.md`](../../adr/ADR-013.md) — management network (E3b, **Accepted**; Phase D **closed on iron** 2026-08-20)

**COM2 serial archives (paper §6):** [`logs/`](logs/) — full operator pastes +
SHA256SUMS (keepconfix residual, xsavesfix close, confirming rebuild, and
literal `RAYNU-V-R640-BOOT-OK` marker capture + screenshot).

**Template:** [`TEMPLATE.md`](TEMPLATE.md) — copy to a dated file for future
campaigns / soak.

**Runbook:** [`docs/runbooks/r640_boot.md`](../../runbooks/r640_boot.md)  
**Living paper:** [`docs/paper/RayNu-V-Verification-Paper.md`](../../paper/RayNu-V-Verification-Paper.md) · site [`paper.html`](../../../site/paper.html)

Host scaffold smoke (`./tools/m7-r640-smoke.sh`) proves this directory and the
runbook exist; it does **not** print `RAYNU-V-R640-BOOT-OK`. That iron marker
is claimed only via filled evidence + `STATUS=closed`.
