# v0.1.0-m76-snp-http — M7.6 iron close (SNP residual HTTP)

**Closes** `RAYNU-V-M7-UEFI-HTTP-OK` on real PowerEdge R640 (2026-08-16).

## Provenance

| Field | Value |
|-------|-------|
| Git | `b59927e` — `feat(m7.6): SNP+smoltcp PRE-EBS HTTP residual when Tcp4 absent` |
| Path | SNP + smoltcp (firmware Tcp4 absent on Virtual Floppy) |
| Iron listen | `10.99.99.127:8443` |
| Evidence | `docs/evidence/r640/2026-08-16-uefi-http-ok.md` |

COM2 fingerprint of this era (no later UX strings):

```text
boot: falling back to SNP residual (ADR-012)
boot: SNP residual MAC=…
boot: SNP DHCP discover…
boot: mgmt HTTP listening on a.b.c.d:8443 (PRE-EBS SNP window)
RAYNU-V-M7-UEFI-HTTP-OK
```

Does **not** contain `CURL NOW` / `SNP listen window_ms=` (those landed in `8c4352d+`).

## Verify

```bash
( cd releases/v0.1.0-m76-snp-http && shasum -a 256 -c r640-hypervisor.efi.sha256 )
```

**Operator:** also hash the EFI you actually mapped as Virtual Floppy. If it
differs from this kit (Mac vs CI toolchain), paste that SHA256 into evidence —
do not overwrite this rebuild without noting both digests.

## Remap

```bash
./tools/make-boot-media.sh --kit releases/v0.1.0-m76-snp-http
```
