# v0.1.0-adr013-baseline — WARN-only idle + ADR-013 Accepted

**Preserve point** before ADR-013 Phase C (native host NIC). Last known-good
iron EFI: `RAYNU-V-R640-BOOT-OK` then SNP idle **WARN** — **no RSOD**.

Do **not** claim `RAYNU-V-M7-POST-EBS-HTTP-OK` or Mount Everest.

## What this freezes

| Area | State |
|------|--------|
| E2 | `RAYNU-V-R640-BOOT-OK` (closed) |
| E3 | PRE-EBS SNP HTTP (`RAYNU-V-M7-UEFI-HTTP-OK`) (closed) |
| E3b | Open — [ADR-013](../../docs/adr/ADR-013.md) **Accepted**; no native driver yet |
| E5 | LBA stamp persist (`BOOTED-FROM-DISK`) (closed; not a distro installer) |
| Post-EBS SNP | **Rejected.** Idle is WARN-only. Do not poll firmware SNP/Tcp4 after EBS. |

## Provenance

| Field | Value |
|-------|--------|
| Git (pack) | `f1c0aae` — ADR-013 Proposed pin; idle code from `d353d96` |
| CI EFI SHA256 | `6bc417b1485cd094e2e2d5776fb0127bee6add0993ab564c96919de6281fcd9a` |
| Iron | Cruzer Micro, front USB 2, BIOS 2.2.11, lease `10.99.99.133:8443` |
| Evidence | `docs/evidence/r640/2026-08-17-post-ebs-snp-dead.md` |
| COM2 (no RSOD) | `docs/evidence/r640/logs/2026-08-17-snp-warn-no-rsod-com2.txt` |

Mac/operator rebuilds of the same source may differ in digest (toolchain).
Do not treat mismatch as a wrong iron boot if COM2 shows the WARN line below.

COM2 fingerprint (WARN-only close, 2026-08-17):

```text
boot: VMXOFF ok
boot: E2 marker build=r640-boot-ok-marker
RAYNU-V-R640-BOOT-OK
boot: WARN — POST-EBS SNP idle skipped; firmware SNP dead after EBS lease=10.99.99.133:8443 (PRE-EBS was the mgmt window; do not chase SNP/Tcp4)
```

Must **not** contain `CURL NOW (post-EBS)`.

## Verify / remap

```bash
( cd releases/v0.1.0-adr013-baseline && shasum -a 256 -c r640-hypervisor.efi.sha256 )
./tools/make-boot-media.sh --kit releases/v0.1.0-adr013-baseline
```

Cruzer: replace **only** `EFI/BOOT/BOOTX64.EFI`. Leave `EFI/RayNu/installdisk.bin`.
Do not flash SHA `924af894…` (immediate-poll hang).
