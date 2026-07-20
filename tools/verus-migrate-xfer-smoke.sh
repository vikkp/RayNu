#!/usr/bin/env bash
# M6.3 host/CI smoke: live migration page transfer → RAYNU-V-M6-MIGRATE-XFER-OK.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

MARKER="${MARKER_MIGRATE_XFER:-RAYNU-V-M6-MIGRATE-XFER-OK}"
VERUS_HOME="${VERUS_HOME:-$ROOT/target/verus}"
MODEL_SRC="$ROOT/ept_model/src/lib.rs"

"$ROOT/tools/install-verus.sh"
export PATH="$VERUS_HOME:$PATH"

if [[ ! -f "$ROOT/ept_model/Cargo.toml" ]]; then
  echo "error: missing ept_model crate" >&2
  exit 1
fi
if ! grep -q 'verify = true' "$ROOT/ept_model/Cargo.toml"; then
  echo "error: ept_model must set package.metadata.verus.verify = true" >&2
  exit 1
fi
if [[ ! -f "$MODEL_SRC" ]]; then
  echo "error: missing $MODEL_SRC" >&2
  exit 1
fi

if grep -nE '^\s*admit\s*\(' "$MODEL_SRC"; then
  echo "error: ept_model still contains admit( — M6.3 forbids admit in ept_model" >&2
  exit 1
fi
if ! grep -q 'struct PageTransferStep' "$MODEL_SRC"; then
  echo "error: missing PageTransferStep" >&2
  exit 1
fi
if ! grep -q 'theorem_page_transfer_preserves_exclusive' "$MODEL_SRC"; then
  echo "error: missing theorem_page_transfer_preserves_exclusive" >&2
  exit 1
fi
if ! grep -q 'transfer_enabled' "$MODEL_SRC"; then
  echo "error: missing transfer_enabled" >&2
  exit 1
fi
if ! grep -q 'apply_transfer' "$MODEL_SRC"; then
  echo "error: missing apply_transfer" >&2
  exit 1
fi
if ! grep -q "$MARKER" "$MODEL_SRC"; then
  echo "error: ept_model must embed marker $MARKER" >&2
  exit 1
fi
if ! grep -q 'GAP(CLOSED M6.3): Live migration page transfer' "$ROOT/memory/ept_proof.rs"; then
  echo "error: ept_proof must close Live migration page transfer GAP for M6.3" >&2
  exit 1
fi
if ! grep -q 'TODO(M6.3 CLOSED): Live migration page transfer' "$ROOT/memory/ept_spec.rs"; then
  echo "error: ept_spec must close TODO(M6.3) Live migration page transfer" >&2
  exit 1
fi
if ! grep -q 'fn transfer_page(' "$ROOT/memory/ept.rs"; then
  echo "error: missing transfer_page runtime hook" >&2
  exit 1
fi
if ! grep -q 'fn prop_page_transfer_preserves_exclusive(' "$ROOT/memory/ept.rs"; then
  echo "error: missing prop_page_transfer_preserves_exclusive" >&2
  exit 1
fi

echo "==> cargo test m6_3_migrate_xfer_gate_passes (artifact gate)"
cargo test --lib m6_3_migrate_xfer_gate_passes -- --nocapture

echo "==> cargo verus verify -p ept_model (M6.3 migrate-xfer; prior L3 still green)"
cargo clean -p ept_model >/dev/null 2>&1 || true
out="$(cargo verus verify -p ept_model 2>&1)"
echo "$out"
if ! grep -q '0 errors' <<<"$out"; then
  echo "error: ept_model verification reported errors" >&2
  exit 1
fi
if ! grep -qE '[1-9][0-9]* verified' <<<"$out"; then
  echo "error: ept_model verification produced no positive verified count" >&2
  exit 1
fi

echo "$MARKER"
echo "==> Verus migrate page-transfer smoke PASSED (M6.3)"
