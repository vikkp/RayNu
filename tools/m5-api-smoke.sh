#!/usr/bin/env bash
# M5.1 host/CI smoke: CLI + REST control plane → RAYNU-V-M5-API-OK.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

MARKER="${MARKER_M5_API:-RAYNU-V-M5-API-OK}"

if [[ ! -f "$ROOT/mgmt/api.rs" ]]; then
  echo "error: missing mgmt/api.rs" >&2
  exit 1
fi
if ! grep -q 'fn parse_cli(' "$ROOT/mgmt/api.rs"; then
  echo "error: missing parse_cli" >&2
  exit 1
fi
if ! grep -q 'fn dispatch_rest(' "$ROOT/mgmt/api.rs"; then
  echo "error: missing dispatch_rest" >&2
  exit 1
fi
if ! grep -qE 'GAP: REST auth stubbed|GAP\(CLOSED M6\.4\): REST auth stubbed' "$ROOT/mgmt/api.rs"; then
  echo "error: auth GAP note missing (open or CLOSED M6.4)" >&2
  exit 1
fi
if ! grep -q "$MARKER" "$ROOT/mgmt/api.rs"; then
  echo "error: api must embed marker $MARKER" >&2
  exit 1
fi
if ! grep -q 'fn list(' "$ROOT/mgmt/mod.rs"; then
  echo "error: missing VmTable::list" >&2
  exit 1
fi

echo "==> cargo test m5_1_api_gate_passes (artifact gate)"
cargo test --lib m5_1_api_gate_passes -- --nocapture

echo "==> cargo test cli_rest_roundtrip"
cargo test --lib cli_rest_roundtrip -- --nocapture

echo "$MARKER"
echo "==> M5.1 API smoke PASSED"
