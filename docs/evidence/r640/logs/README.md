# R640 COM2 serial archives (paper reproducibility)

Operator-captured iDRAC SOL (`console com2`) logs from the 2026-08-15 iron
campaign. These are the primary runtime artifacts for the living verification
paper (§6) and for reclaiming `RAYNU-V-R640-BOOT-OK`.

## Files

| Log | Kit | Role |
|-----|-----|------|
| `2026-08-15-keepconfix-com2.txt` | `releases/v0.1.0-keepconfix/` | Pre-close residual: `Run /init` → TASK stack guard / panic |
| `2026-08-15-xsavesfix-com2.txt` | `releases/v0.1.0-xsavesfix/` | Closing run: `SHELL-OK` → `M4-SMP-OK` |

Checksums: [`SHA256SUMS`](SHA256SUMS). Narrative:
[`../2026-08-15-r640-first-light.md`](../2026-08-15-r640-first-light.md).

## Reproduce on iron

```bash
git fetch origin && git checkout <commit-with-kit>
( cd releases/v0.1.0-xsavesfix && shasum -a 256 -c r640-hypervisor.efi.sha256 )
./tools/make-boot-media.sh --kit releases/v0.1.0-xsavesfix
# iDRAC: map *-uefi-boot.img as Virtual Floppy; SSH → console com2; one-time boot
# Expect markers through RAYNU-V-M4-SMP-OK (see closing log)
```

Closing EFI SHA256:

```
c3a688d0f5bb7c45395d3c1f7566272074f6d118276eaedc073b2f29ba28d611  r640-hypervisor.efi
```

## Honesty

- Logs are **runtime / gate** evidence (ADR-010 maturity ≤ L1 for EFI-emitted
  claims). They do **not** upgrade Verus L3 coverage by themselves.
- Host `./tools/m7-r640-smoke.sh` never prints `RAYNU-V-R640-BOOT-OK`.
- Intermediate kit residuals (com2 → eptfix → … → keepconfix) are summarized in
  the first-light narrative; full closing + keepconfix serials are archived here.
