#!/usr/bin/env bash
# M6.1 host/CI smoke: HW PTE bit-decode correspondence → RAYNU-V-M6-HWPTE-OK.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

MARKER="${MARKER_HWPTE:-RAYNU-V-M6-HWPTE-OK}"
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
  echo "error: ept_model still contains admit( — M6.1 forbids admit in ept_model" >&2
  exit 1
fi
if ! grep -q 'ept_leaf_large_enc' "$MODEL_SRC"; then
  echo "error: missing ept_leaf_large_enc" >&2
  exit 1
fi
if ! grep -q 'theorem_hw_2m_leaf_refines_identity' "$MODEL_SRC"; then
  echo "error: missing theorem_hw_2m_leaf_refines_identity" >&2
  exit 1
fi
if ! grep -q 'hw_2m_identity_leaf_ok' "$MODEL_SRC"; then
  echo "error: missing hw_2m_identity_leaf_ok" >&2
  exit 1
fi
if ! grep -q 'lemma_ept_leaf_large_decode' "$MODEL_SRC"; then
  echo "error: missing lemma_ept_leaf_large_decode" >&2
  exit 1
fi
if ! grep -q "$MARKER" "$MODEL_SRC"; then
  echo "error: ept_model must embed marker $MARKER" >&2
  exit 1
fi
if ! grep -q 'GAP(CLOSED M6.1): Hardware EPT PTE bit-decode' "$ROOT/memory/ept_proof.rs"; then
  echo "error: ept_proof must close Hardware EPT PTE bit-decode GAP for M6.1" >&2
  exit 1
fi
if ! grep -q 'TODO(M6.1 CLOSED): HW PTE bit-decode' "$ROOT/memory/ept_spec.rs"; then
  echo "error: ept_spec must close TODO(M6.1) HW PTE bit-decode" >&2
  exit 1
fi
if ! grep -q 'fn prop_hw_pte_identity_correspondence(' "$ROOT/memory/ept_hw.rs"; then
  echo "error: missing prop_hw_pte_identity_correspondence runtime prop" >&2
  exit 1
fi
if ! grep -q 'fn ept_leaf_large(' "$ROOT/memory/ept_hw.rs"; then
  echo "error: missing public ept_leaf_large" >&2
  exit 1
fi

echo "==> cargo test m6_1_hwpte_gate_passes (artifact gate)"
cargo test --lib m6_1_hwpte_gate_passes -- --nocapture

echo "==> cargo verus verify -p ept_model (M6.1 HW PTE; prior L3 still green)"
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
echo "==> Verus HW PTE smoke PASSED (M6.1)"
