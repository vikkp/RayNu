#!/usr/bin/env bash
# M7.2 host/CI smoke: datastore / image library → RAYNU-V-M7-STORE-OK.
# Proves register/list/delete + REST shapes + ESP-shaped host catalog.
# UEFI SimpleFileSystem persist remains stubbed (see docs/runbooks/datastore.md).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

MARKER="${MARKER_M7_STORE:-RAYNU-V-M7-STORE-OK}"

if [[ ! -f "$ROOT/mgmt/datastore.rs" ]]; then
  echo "error: missing mgmt/datastore.rs" >&2
  exit 1
fi
if ! grep -q 'fn prop_datastore_package(' "$ROOT/mgmt/datastore.rs"; then
  echo "error: missing prop_datastore_package" >&2
  exit 1
fi
if ! grep -q 'GAP(CLOSED M7.2): Datastore' "$ROOT/mgmt/datastore.rs"; then
  echo "error: Datastore GAP must be CLOSED M7.2" >&2
  exit 1
fi
if ! grep -q 'EFI/RAYNU/images' "$ROOT/mgmt/datastore.rs"; then
  echo "error: ESP-shaped images path required" >&2
  exit 1
fi
if ! grep -q 'UnsupportedOnFirmware' "$ROOT/mgmt/datastore.rs"; then
  echo "error: UEFI persist stub must be honest" >&2
  exit 1
fi
if [[ ! -f "$ROOT/docs/runbooks/datastore.md" ]]; then
  echo "error: missing docs/runbooks/datastore.md" >&2
  exit 1
fi

echo "==> cargo test m7_2_store_gate_passes (artifact gate)"
cargo test --lib m7_2_store_gate_passes -- --nocapture

echo "==> cargo test datastore_package"
cargo test --lib datastore_package -- --nocapture

echo "==> cargo test host_catalog_persist_roundtrip"
cargo test --lib host_catalog_persist_roundtrip -- --nocapture

echo "$MARKER"
echo "==> M7.2 datastore smoke PASSED"
