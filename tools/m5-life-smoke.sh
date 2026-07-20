#!/usr/bin/env bash
# M5.0 host/CI smoke: VM lifecycle API → RAYNU-V-M5-LIFE-OK.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

MARKER="${MARKER_M5_LIFE:-RAYNU-V-M5-LIFE-OK}"

if [[ ! -f "$ROOT/mgmt/mod.rs" ]]; then
  echo "error: missing mgmt/mod.rs" >&2
  exit 1
fi
if ! grep -q 'fn create(' "$ROOT/mgmt/mod.rs"; then
  echo "error: missing VmTable::create" >&2
  exit 1
fi
if ! grep -q 'fn destroy(' "$ROOT/mgmt/mod.rs"; then
  echo "error: missing VmTable::destroy" >&2
  exit 1
fi
if ! grep -q "$MARKER" "$ROOT/mgmt/mod.rs"; then
  echo "error: mgmt must embed marker $MARKER" >&2
  exit 1
fi
if ! grep -q 'VmCreated' "$ROOT/audit/integrity.rs"; then
  echo "error: audit missing VmCreated event" >&2
  exit 1
fi

echo "==> cargo test m5_0_life_gate_passes (artifact gate)"
cargo test --lib m5_0_life_gate_passes -- --nocapture

echo "==> cargo test prop/lifecycle roundtrip"
cargo test --lib create_start_stop_destroy_roundtrip -- --nocapture

echo "$MARKER"
echo "==> M5.0 lifecycle smoke PASSED"
