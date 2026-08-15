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
| `2026-08-15-boot-ok-marker-com2.png` | Operator macOS Terminal.app (iDRAC SOL) | Site status + Stories + paper figure |

Checksums: [`SHA256SUMS`](SHA256SUMS). Narrative:
[`../2026-08-15-r640-first-light.md`](../2026-08-15-r640-first-light.md).

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
