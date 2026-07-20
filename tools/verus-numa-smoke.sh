#!/usr/bin/env bash
# M5.8 host/CI smoke: NUMA in ghost *spec* (SRAT/SLIT; affinity L3 → M6).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

MARKER="${MARKER_NUMA:-RAYNU-V-M5-NUMA-OK}"
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
  echo "error: ept_model still contains admit( — M5.8 forbids admit in ept_model" >&2
  exit 1
fi
if ! grep -q 'struct GhostNumaTopology' "$MODEL_SRC"; then
  echo "error: missing GhostNumaTopology" >&2
  exit 1
fi
if ! grep -q 'numa_map_enabled' "$MODEL_SRC"; then
  echo "error: missing numa_map_enabled" >&2
  exit 1
fi
if ! grep -q 'mock_bringup_numa' "$MODEL_SRC"; then
  echo "error: missing mock_bringup_numa" >&2
  exit 1
fi
if ! grep -q 'lemma_numa_map_ok_exclusive' "$MODEL_SRC"; then
  echo "error: missing lemma_numa_map_ok_exclusive" >&2
  exit 1
fi
if ! grep -q "$MARKER" "$MODEL_SRC"; then
  echo "error: ept_model must embed marker $MARKER" >&2
  exit 1
fi
if ! grep -q 'GAP(CLOSED M5.8): NUMA in ghost spec' "$ROOT/memory/ept_proof.rs"; then
  echo "error: ept_proof must close GAP NUMA in ghost spec for M5.8" >&2
  exit 1
fi
if ! grep -qE 'GAP: NUMA affinity / exclusivity L3 \(M6\)|GAP\(CLOSED M6\.2\): NUMA affinity / exclusivity L3' "$ROOT/memory/ept_proof.rs"; then
  echo "error: ept_proof must document or close NUMA affinity L3 GAP" >&2
  exit 1
fi
if ! grep -q 'TODO(M5.8 CLOSED): NUMA in ghost spec' "$ROOT/memory/ept_spec.rs"; then
  echo "error: ept_spec must close TODO(M5.8) NUMA on the spec side" >&2
  exit 1
fi
if [[ ! -f "$ROOT/assets/idrac/mock_topology.txt" ]]; then
  echo "error: missing mock_topology.txt (SRAT/SLIT runtime hook)" >&2
  exit 1
fi
if ! grep -q '^numa ' "$ROOT/assets/idrac/mock_topology.txt"; then
  echo "error: mock topology missing numa (SRAT) rows" >&2
  exit 1
fi
if ! grep -q '^slit ' "$ROOT/assets/idrac/mock_topology.txt"; then
  echo "error: mock topology missing slit rows" >&2
  exit 1
fi

echo "==> cargo test m5_8_numa_gate_passes (artifact gate)"
cargo test --lib m5_8_numa_gate_passes -- --nocapture

echo "==> cargo test mock_numa_runtime_well_formed"
cargo test --lib mock_numa_runtime_well_formed -- --nocapture

echo "==> cargo verus verify -p ept_model (M5.8 NUMA spec; prior L3 still green)"
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
echo "==> Verus NUMA-spec smoke PASSED (M5.8)"
