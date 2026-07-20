#!/usr/bin/env bash
# M5.7 host/CI smoke: large-page L3 verify (green verify, no admit).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

MARKER="${MARKER_LPAGE_VERIFY:-RAYNU-V-M5-LPAGE-VERIFY-OK}"
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
  echo "error: ept_model still contains admit( — M5.7 requires large-page L3 discharge" >&2
  exit 1
fi
if ! grep -q 'theorem_large_page_map_unmap_exclusive' "$MODEL_SRC"; then
  echo "error: missing theorem_large_page_map_unmap_exclusive" >&2
  exit 1
fi
if ! grep -q 'lemma_2m_map_unmap_exclusive' "$MODEL_SRC"; then
  echo "error: missing lemma_2m_map_unmap_exclusive" >&2
  exit 1
fi
if ! grep -q 'lemma_1g_map_unmap_exclusive' "$MODEL_SRC"; then
  echo "error: missing lemma_1g_map_unmap_exclusive" >&2
  exit 1
fi
if ! grep -q 'lemma_two_guests_large_map_distinct_spans_exclusive' "$MODEL_SRC"; then
  echo "error: missing lemma_two_guests_large_map_distinct_spans_exclusive" >&2
  exit 1
fi
if ! grep -q "$MARKER" "$MODEL_SRC"; then
  echo "error: ept_model must embed marker $MARKER" >&2
  exit 1
fi
if ! grep -q 'GAP(CLOSED M5.7): Large-page L3 discharge' "$ROOT/memory/ept_proof.rs"; then
  echo "error: ept_proof must close GAP Large-page L3 discharge for M5.7" >&2
  exit 1
fi
if ! grep -q 'TODO(M5.7 CLOSED): large-page L3' "$ROOT/memory/ept_spec.rs"; then
  echo "error: ept_spec must close TODO(M5.7) large-page L3" >&2
  exit 1
fi

echo "==> cargo test m5_7_lpage_verify_gate_passes (artifact gate)"
cargo test --lib m5_7_lpage_verify_gate_passes -- --nocapture

echo "==> cargo verus verify -p ept_model (M5.7 large-page L3, no admit)"
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
echo "==> Verus large-page L3-verify smoke PASSED (M5.7)"
