#!/usr/bin/env bash
# M6.8 host/CI smoke: 72-hr soak thresholds → RAYNU-V-M6-SOAK-OK.
# Accelerated simulation (72 ticks); see docs/runbooks/soak.md for wall-clock iron.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

MARKER="${MARKER_M6_SOAK:-RAYNU-V-M6-SOAK-OK}"

if [[ ! -f "$ROOT/mgmt/soak.rs" ]]; then
  echo "error: missing mgmt/soak.rs" >&2
  exit 1
fi
if ! grep -q 'fn run_soak_simulation(' "$ROOT/mgmt/soak.rs"; then
  echo "error: missing run_soak_simulation" >&2
  exit 1
fi
if ! grep -q 'fn prop_soak_72h_thresholds(' "$ROOT/mgmt/soak.rs"; then
  echo "error: missing prop_soak_72h_thresholds" >&2
  exit 1
fi
if ! grep -q 'SOAK_TARGET_HOURS: u32 = 72' "$ROOT/mgmt/soak.rs"; then
  echo "error: SOAK_TARGET_HOURS must be 72" >&2
  exit 1
fi
if ! grep -q 'GAP(CLOSED M6.8): 72-hr soak' "$ROOT/mgmt/soak.rs"; then
  echo "error: soak GAP must be CLOSED M6.8" >&2
  exit 1
fi
if ! grep -q "$MARKER" "$ROOT/mgmt/soak.rs"; then
  echo "error: soak must embed marker $MARKER" >&2
  exit 1
fi
if ! grep -q 'SoakStarted' "$ROOT/audit/integrity.rs"; then
  echo "error: missing AuditEvent::SoakStarted" >&2
  exit 1
fi
if [[ ! -f "$ROOT/docs/runbooks/soak.md" ]]; then
  echo "error: missing docs/runbooks/soak.md" >&2
  exit 1
fi

echo "==> cargo test m6_8_soak_gate_passes (artifact gate)"
cargo test --lib m6_8_soak_gate_passes -- --nocapture

echo "==> cargo test soak_72h_thresholds"
cargo test --lib soak_72h_thresholds -- --nocapture

echo "$MARKER"
echo "==> M6.8 soak smoke PASSED"
