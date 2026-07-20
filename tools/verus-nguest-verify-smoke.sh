#!/usr/bin/env bash
# M4.7 host/CI smoke: true L3 N-guest exclusivity (green verify, no admit).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

MARKER="${MARKER_NGUEST_VERIFY:-RAYNU-V-M4-NGUEST-VERIFY-OK}"
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
  echo "error: ept_model still contains admit( — M4.7 requires full N-guest discharge" >&2
  exit 1
fi
if ! grep -q 'theorem_n_guest_4k_map_unmap_exclusive' "$MODEL_SRC"; then
  echo "error: missing theorem_n_guest_4k_map_unmap_exclusive" >&2
  exit 1
fi
if ! grep -q 'lemma_two_guests_map_distinct_frames_exclusive' "$MODEL_SRC"; then
  echo "error: missing lemma_two_guests_map_distinct_frames_exclusive (≥2-guest post)" >&2
  exit 1
fi
if ! grep -q "$MARKER" "$MODEL_SRC"; then
  echo "error: ept_model must embed marker $MARKER" >&2
  exit 1
fi
if ! grep -q 'GAP(CLOSED M4.7): N-guest L3 discharge' "$ROOT/memory/ept_proof.rs"; then
  echo "error: ept_proof must close GAP N-guest L3 discharge for M4.7" >&2
  exit 1
fi
if ! grep -q 'TODO(M4.7 CLOSED): ADR-006 L3 for N-guest' "$ROOT/memory/ept_spec.rs"; then
  echo "error: ept_spec must close TODO(M4.7) ADR-006 L3 claim" >&2
  exit 1
fi

echo "==> cargo test m4_7_nguest_verify_gate_passes (artifact gate)"
cargo test --lib m4_7_nguest_verify_gate_passes -- --nocapture

echo "==> cargo verus verify -p ept_model (M4.7 N-guest L3, no admit)"
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
echo "==> Verus N-guest L3-verify smoke PASSED (M4.7)"
