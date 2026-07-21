# Runbook — External audit + spec review (M6.9)

**Marker:** `RAYNU-V-M6-EXT-OK`  
**Smoke:** `./tools/m6-ext-smoke.sh`

## Auditor path (frozen pin)

```bash
cd ~/raynu   # or clone
./tools/install-verus.sh
export PATH="$PWD/target/verus:$PATH"
./tools/verus-smoke.sh                 # pin identity
cargo verus verify -p ept_model        # proof-of-record
./tools/m6-ext-smoke.sh                # M6.9 gate + docs
```

Never install Verus via GitHub `latest` (ADR-008).

## Artifacts the gate requires

| Artifact | Path |
|----------|------|
| Spec review (R09) | `docs/reviews/m6_spec_review.md` |
| Findings register | `docs/findings/m6_external.md` (`Open critical findings: **0**`) |
| Proof-maintenance dry-run | `docs/reviews/m6_proof_maintenance.md` |
| Pin | `verus-version.toml` |

## Sign-off meaning

`RAYNU-V-M6-EXT-OK` means: pin is auditor-reproducible, R09 review note is
filed, findings have no open CRITICAL/HIGH, and the maintenance dry-run was
recorded. It does not replace a customer’s own contracted third-party audit.
