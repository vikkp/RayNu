# M6 Proof-Maintenance Dry Run (ADR-008)

**Gate:** M6.9 → `RAYNU-V-M6-EXT-OK`  
**Date:** 2026-07-21

## Procedure (upgrade dry-run checklist)

1. Confirm `verus-version.toml` has exact `tag` + 40-char `commit` + 64-char `sha256_linux`.
2. Confirm policy text forbids `latest` / rolling.
3. Run `./tools/install-verus.sh` and `./tools/verus-smoke.sh` (pin install).
4. Re-verify proof-of-record: `cargo verus verify -p ept_model` (expect `0 errors`, positive verified count).
5. Record breakage: none observed against frozen pin `0.2026.07.12.0b42f4c`.
6. **Do not** bump the pin in this dry-run — upgrades remain a dedicated PR.

## Result

| Step | Result |
|------|--------|
| Pin concrete | PASS |
| Install + smoke | PASS (via M6.9 / M3.15 tooling) |
| ept_model verify | PASS (M6.3 migrate-xfer path still green; `80 verified, 0 errors` on Latitude record) |
| Pin bump | Deferred (no upgrade this dry-run) |

**Breakage measured:** 0 (frozen pin unchanged).

This satisfies M6.9’s “proof-maintenance dry run” sub-check without treating an
unplanned Verus upgrade as in-scope for the gate.
