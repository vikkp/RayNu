#!/usr/bin/env bash
# M5.9 host/CI smoke: allocator↔EPT refine + scoped identity → RAYNU-V-M5-ALLOC-REFINE-OK.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

MARKER="${MARKER_ALLOC_REFINE:-RAYNU-V-M5-ALLOC-REFINE-OK}"
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
  echo "error: ept_model still contains admit( — M5.9 forbids admit in ept_model" >&2
  exit 1
fi
if ! grep -q 'struct GhostFramePool' "$MODEL_SRC"; then
  echo "error: missing GhostFramePool" >&2
  exit 1
fi
if ! grep -q 'alloc_ept_refines' "$MODEL_SRC"; then
  echo "error: missing alloc_ept_refines" >&2
  exit 1
fi
if ! grep -q 'theorem_alloc_map_unmap_refines' "$MODEL_SRC"; then
  echo "error: missing theorem_alloc_map_unmap_refines" >&2
  exit 1
fi
if ! grep -q 'identity_leaf_ok' "$MODEL_SRC"; then
  echo "error: missing identity_leaf_ok" >&2
  exit 1
fi
if ! grep -q 'PRECISE_IDENTITY_FRAMES' "$MODEL_SRC"; then
  echo "error: missing PRECISE_IDENTITY_FRAMES" >&2
  exit 1
fi
if ! grep -q "$MARKER" "$MODEL_SRC"; then
  echo "error: ept_model must embed marker $MARKER" >&2
  exit 1
fi
if ! grep -q 'GAP(CLOSED M5.9): Frame-allocator ↔ EPT L3 coupling' "$ROOT/memory/ept_proof.rs"; then
  echo "error: ept_proof must close Frame-allocator ↔ EPT L3 coupling for M5.9" >&2
  exit 1
fi
if ! grep -q 'GAP(CLOSED M5.9): Precise-identity GPA==HPA correspondence' "$ROOT/memory/ept_proof.rs"; then
  echo "error: ept_proof must close Precise-identity correspondence for M5.9" >&2
  exit 1
fi
if ! grep -q 'GAP: Hardware EPT PTE bit-decode / EPT-violation (M6)' "$ROOT/memory/ept_proof.rs"; then
  echo "error: ept_proof must document HW PTE bit-decode GAP → M6" >&2
  exit 1
fi
if ! grep -q 'TODO(M5.9 CLOSED): allocator↔EPT refine' "$ROOT/memory/ept_spec.rs"; then
  echo "error: ept_spec must close TODO(M5.9) allocator↔EPT refine" >&2
  exit 1
fi

echo "==> cargo test m5_9_alloc_refine_gate_passes (artifact gate)"
cargo test --lib m5_9_alloc_refine_gate_passes -- --nocapture

echo "==> cargo verus verify -p ept_model (M5.9 alloc refine; prior L3 still green)"
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
echo "==> Verus alloc-refine smoke PASSED (M5.9)"
