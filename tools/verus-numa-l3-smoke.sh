#!/usr/bin/env bash
# M6.2 host/CI smoke: NUMA affinity L3 → RAYNU-V-M6-NUMA-L3-OK.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

MARKER="${MARKER_NUMA_L3:-RAYNU-V-M6-NUMA-L3-OK}"
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
  echo "error: ept_model still contains admit( — M6.2 forbids admit in ept_model" >&2
  exit 1
fi
if ! grep -q 'theorem_numa_map_unmap_affinity' "$MODEL_SRC"; then
  echo "error: missing theorem_numa_map_unmap_affinity" >&2
  exit 1
fi
if ! grep -q 'lemma_numa_map_establishes_affinity' "$MODEL_SRC"; then
  echo "error: missing lemma_numa_map_establishes_affinity" >&2
  exit 1
fi
if ! grep -q 'lemma_numa_unmap_preserves_affinity' "$MODEL_SRC"; then
  echo "error: missing lemma_numa_unmap_preserves_affinity" >&2
  exit 1
fi
if ! grep -q 'guest_frames_on_node' "$MODEL_SRC"; then
  echo "error: missing guest_frames_on_node" >&2
  exit 1
fi
if ! grep -q "$MARKER" "$MODEL_SRC"; then
  echo "error: ept_model must embed marker $MARKER" >&2
  exit 1
fi
if ! grep -q 'GAP(CLOSED M6.2): NUMA affinity / exclusivity L3' "$ROOT/memory/ept_proof.rs"; then
  echo "error: ept_proof must close NUMA affinity L3 GAP for M6.2" >&2
  exit 1
fi
if ! grep -q 'TODO(M6.2 CLOSED): NUMA affinity / exclusivity L3' "$ROOT/memory/ept_spec.rs"; then
  echo "error: ept_spec must close TODO(M6.2) NUMA affinity L3" >&2
  exit 1
fi
if ! grep -q 'fn prop_numa_affinity_l3(' "$ROOT/memory/numa.rs"; then
  echo "error: missing prop_numa_affinity_l3 runtime prop" >&2
  exit 1
fi

echo "==> cargo test m6_2_numa_l3_gate_passes (artifact gate)"
cargo test --lib m6_2_numa_l3_gate_passes -- --nocapture

echo "==> cargo verus verify -p ept_model (M6.2 NUMA affinity L3; prior L3 still green)"
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
echo "==> Verus NUMA affinity L3 smoke PASSED (M6.2)"
