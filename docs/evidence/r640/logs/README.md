# R640 COM2 serial archives (paper reproducibility)

Operator-captured iDRAC SOL (`console com2`) logs from the 2026-08-15 iron
campaign. These are the primary runtime artifacts for the living verification
paper (§6) and for claiming `RAYNU-V-R640-BOOT-OK`.

## Files

| Log | Kit / media | Role |
|-----|-------------|------|
| `2026-08-15-keepconfix-com2.txt` | `releases/v0.1.0-keepconfix/` | Pre-close residual: `Run /init` → TASK stack guard / panic |
| `2026-08-15-xsavesfix-com2.txt` | `releases/v0.1.0-xsavesfix/` | First closing run: `SHELL-OK` → `M4-SMP-OK` |
| `2026-08-15-confirm-rebuild-com2.txt` | `dist/` rebuild (same EFI SHA as xsavesfix) | Confirming retest: SHELL→M4→`VMXOFF` |
| `2026-08-15-boot-ok-marker-com2.txt` | `r640-boot-ok-marker` finish_boot print | **Literal** `RAYNU-V-R640-BOOT-OK` on COM2 after VMXOFF |
| `2026-08-16-uefi-http-tcp4-absent-com2.txt` | M7.6 iron soft-fail | `Tcp4 stack absent` after M7.6 binary; E2 BOOT-OK still printed |
| `2026-08-16-uefi-http-net-probe-zero-com2.txt` | M7.6 probe | `snp=0 mnp=0 ip4=0 dhcp4=0 tcp4=0` — no UEFI net stack on floppy boot |
| `2026-08-16-uefi-http-snp12-after-pci-com2.txt` | M7.6 ConnectController | `after-pci snp=12` — SNP up, Tcp4 still 0 → SNP residual |

| `2026-08-16-uefi-tcp4-census-com2.txt` | Tcp4 extra census | `extra-after pxe=8 http=4 ip4cfg=4` still `tcp4=0`; SNP residual OK |

Tcp4-absent analysis:
[`../2026-08-16-uefi-tcp4-absent-root-cause.md`](../2026-08-16-uefi-tcp4-absent-root-cause.md).
| `2026-08-16-uefi-http-snp-listen-accept-timeout-com2.txt` | M7.6 SNP residual | DHCP+listen `10.99.99.127:8443`; accept timeout before curl |
| `2026-08-16-uefi-http-ok-com2.txt` | **M7.6 iron close** | `RAYNU-V-M7-UEFI-HTTP-OK` after listen on `10.99.99.127:8443` |
| `2026-08-16-uefi-http-ok-com2-retest.txt` | M7.6 clarifying retest | listen → `AuthAllowed` → `UEFI-HTTP-OK` → `R640-BOOT-OK` |
| `2026-08-16-uefi-http-ok-curl.txt` | M7.6 client proof | Mac `GET /` → HTTP 200 SPA (`Content-Length: 10757`) |
| `2026-08-16-uefi-http-ok-curl-retest.txt` | M7.6 clarifying retest | `GET /` → 200 SPA + `GET /vms` Bearer → `{"ok":true,"listed":true}` |
| `2026-08-16-e4-spa-install-arm-com2.txt` | E4 tip `46090df` | SPA create-VM + 64 MiB install arm → `DISK-WRITTEN`/`LAB-OK`/`REBOOT-PENDING` |
| `2026-08-16-e5-persist-write-com2.txt` | Cruzer Micro boot1 | SPA Install → `persist wrote installdisk.bin bytes=1024` + 64 MiB arm |
| `2026-08-16-e5-persist-detect-blk-fail-com2.txt` | Cruzer Micro boot2 | `persist-detect` + preload, then `HLT without DRIVER_OK` |
| `2026-08-17-snp-warn-no-rsod-com2.txt` | `releases/v0.1.0-adr013-baseline/` | WARN-only idle after `BOOT-OK`; **no RSOD**; Phase A closed |
| `2026-08-17-phase0-census-com2.txt` | ADR-013 Phase 0 iron | `14e4:165f` BCM5720 dual-port; SNP is `01:00.1`; **no HTTP-OK** |
| `2026-08-20-host-nic-http-ok-com2.txt` | **M7.8 / E3b iron close** | `grc=bswap+wswap`; listen `10.99.99.144:8443`; `HOST-NIC-HTTP-OK` |
| `2026-08-20-phase-f-coexist-ok-com2.txt` | **ADR-013 Phase F iron close** | coexist `10.99.99.149:8443` VMX on; G0 scheduled; G1–G3 parked |

Checksums: [`SHA256SUMS`](SHA256SUMS). Narratives:
[`../2026-08-15-r640-first-light.md`](../2026-08-15-r640-first-light.md) ·
[`../2026-08-16-e4-spa-install-arm.md`](../2026-08-16-e4-spa-install-arm.md) ·
[`../2026-08-16-e5-persist-detect-blk-fail.md`](../2026-08-16-e5-persist-detect-blk-fail.md) ·
[`../2026-08-16-e5-iso-install.md`](../2026-08-16-e5-iso-install.md) ·
[`../2026-08-17-post-ebs-snp-dead.md`](../2026-08-17-post-ebs-snp-dead.md).

## Reproduce on iron

```bash
git fetch origin && git checkout cursor/r640-boot-ok-marker-a623   # or main after merge
./tools/rebuild-marker-boot-media.sh
# iDRAC: map dist/.../raynu-v-0.1.0-uefi-boot.img as Virtual Floppy
# SSH → console com2; one-time boot
# Expect M0→SHELL→M4→VMXOFF then:
#   boot: E2 marker build=r640-boot-ok-marker
#   RAYNU-V-R640-BOOT-OK
```

Substance-close EFI SHA256 (xsavesfix + confirming rebuild):

```
c3a688d0f5bb7c45395d3c1f7566272074f6d118276eaedc073b2f29ba28d611  r640-hypervisor.efi
```

## Honesty

- Logs are **runtime / gate** evidence (ADR-010 maturity ≤ L1 for EFI-emitted
  claims). They do **not** upgrade Verus L3 coverage by themselves.
- Host `./tools/m7-r640-smoke.sh` never prints `RAYNU-V-R640-BOOT-OK`.
- `xsavesfix` / confirming archives prove M0→SHELL→M4→VMXOFF without the
  literal claim string; `boot-ok-marker-com2` is the first iron paste that
  contains `RAYNU-V-R640-BOOT-OK` (firmware `finish_boot` after `VMXOFF`).
- Intermediate kit residuals (com2 → eptfix → … → keepconfix) are summarized in
  the first-light narrative.
