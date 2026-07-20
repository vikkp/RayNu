#!/usr/bin/env bash
# M4.8 host/CI smoke: large-page (2M/1G) ghost *spec* (L3 discharge → M5).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

MARKER="${MARKER_LPAGE:-RAYNU-V-M4-LPAGE-OK}"
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
  echo "error: ept_model still contains admit( — M4.8 forbids admit in ept_model" >&2
  exit 1
fi
if ! grep -q 'enum GhostPageSize' "$MODEL_SRC"; then
  echo "error: missing GhostPageSize" >&2
  exit 1
fi
if ! grep -q 'large_map_enabled' "$MODEL_SRC"; then
  echo "error: missing large_map_enabled" >&2
  exit 1
fi
if ! grep -q 'PAGE_2M' "$MODEL_SRC" || ! grep -q 'PAGE_1G' "$MODEL_SRC"; then
  echo "error: missing PAGE_2M / PAGE_1G constants" >&2
  exit 1
fi
if ! grep -q "$MARKER" "$MODEL_SRC"; then
  echo "error: ept_model must embed marker $MARKER" >&2
  exit 1
fi
if ! grep -q 'GAP(CLOSED M4.8): Large pages' "$ROOT/memory/ept_proof.rs"; then
  echo "error: ept_proof must close GAP Large pages for M4.8 (spec)" >&2
  exit 1
fi
if ! grep -q 'GAP: Large-page L3 discharge' "$ROOT/memory/ept_proof.rs"; then
  echo "error: ept_proof must leave Large-page L3 discharge open for M5" >&2
  exit 1
fi
if ! grep -q 'TODO(M4.8 CLOSED): large pages' "$ROOT/memory/ept_spec.rs"; then
  echo "error: ept_spec must close TODO(M4.8) large pages on the spec side" >&2
  exit 1
fi

echo "==> cargo test m4_8_lpage_gate_passes (artifact gate)"
cargo test --lib m4_8_lpage_gate_passes -- --nocapture

echo "==> cargo verus verify -p ept_model (M4.8 large-page spec; prior L3 still green)"
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
echo "==> Verus large-page-spec smoke PASSED (M4.8)"
