#!/usr/bin/env bash
# M4.9 host/CI smoke: N-guest ghost↔exec refine (green verify, no admit).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

MARKER="${MARKER_NGUEST_REFINE:-RAYNU-V-M4-REFINE-OK}"
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
  echo "error: ept_model still contains admit( — M4.9 requires full N-guest refine discharge" >&2
  exit 1
fi
if ! grep -q 'theorem_concrete_n_guest_4k_refine' "$MODEL_SRC"; then
  echo "error: missing theorem_concrete_n_guest_4k_refine" >&2
  exit 1
fi
if ! grep -q 'lemma_concrete_two_guests_map_refines' "$MODEL_SRC"; then
  echo "error: missing lemma_concrete_two_guests_map_refines" >&2
  exit 1
fi
if ! grep -q 'pub open spec fn abs(' "$MODEL_SRC"; then
  echo "error: missing abs refinement function" >&2
  exit 1
fi
if ! grep -q 'pub open spec fn refines(' "$MODEL_SRC"; then
  echo "error: missing refines predicate" >&2
  exit 1
fi
if ! grep -q "$MARKER" "$MODEL_SRC"; then
  echo "error: ept_model must embed marker $MARKER" >&2
  exit 1
fi
if ! grep -q 'GAP(CLOSED M4.9): N-guest ghost↔exec refine' "$ROOT/memory/ept_proof.rs"; then
  echo "error: ept_proof must close GAP N-guest refine for M4.9" >&2
  exit 1
fi
if ! grep -q 'TODO(M4.9 CLOSED): N-guest ghost↔exec refine' "$ROOT/memory/ept_spec.rs"; then
  echo "error: ept_spec must close TODO(M4.9) N-guest refine" >&2
  exit 1
fi

echo "==> cargo test m4_9_nguest_refine_gate_passes (artifact gate)"
cargo test --lib m4_9_nguest_refine_gate_passes -- --nocapture

echo "==> cargo verus verify -p ept_model (M4.9 N-guest refine, no admit)"
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
echo "==> Verus N-guest-refine smoke PASSED (M4.9)"
