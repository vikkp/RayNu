# M6 External Audit Findings Register

**Gate:** M6.9 → `RAYNU-V-M6-EXT-OK`  
**Pin:** ADR-008 / `verus-version.toml` (auditor must not use `latest`)

## Summary

Open critical findings: **0**

| Severity | Open | Closed | Waived |
|----------|------|--------|--------|
| CRITICAL | 0 | 0 | 0 |
| HIGH | 0 | 0 | 0 |
| MEDIUM | 0 | 0 | 0 |
| LOW / INFO | 0 | 1 | 0 |

## Findings

| ID | Severity | Status | Title | Notes |
|----|----------|--------|-------|-------|
| M6-EXT-001 | INFO | CLOSED | Bring-up self-audit under frozen pin | Auditor path = `./tools/m6-ext-smoke.sh` + `cargo verus verify -p ept_model`; no CRITICAL defects found in M6 proof row |

## Policy

- CRITICAL/HIGH must be CLOSED or WAIVED (with ADR note) before `RAYNU-V-M6-EXT-OK`.
- Waivers require an ADR amendment referencing the finding ID.
- New findings append rows; never delete history.
