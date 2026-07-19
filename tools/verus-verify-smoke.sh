#!/usr/bin/env bash
# M3.17 host/CI smoke: green cargo verus verify -p ept_model with no admit().
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

MARKER="${MARKER_L3_VERIFY:-RAYNU-V-M3-L3-VERIFY-OK}"
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

# True L3: reject admit( statements (ignore // and //! documentation lines).
if grep -nE '^\s*admit\s*\(' "$MODEL_SRC"; then
  echo "error: ept_model still contains admit( — M3.17 requires full discharge" >&2
  exit 1
fi
if ! grep -q 'theorem_single_guest_4k_map_unmap_exclusive' "$MODEL_SRC"; then
  echo "error: missing theorem_single_guest_4k_map_unmap_exclusive" >&2
  exit 1
fi
if ! grep -q "$MARKER" "$MODEL_SRC"; then
  echo "error: ept_model must embed marker $MARKER" >&2
  exit 1
fi

echo "==> cargo verus verify -p ept_model (M3.17 true L3, no admit)"
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
echo "==> Verus L3-verify smoke PASSED (M3.17)"
