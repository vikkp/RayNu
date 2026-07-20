#!/usr/bin/env bash
# M6.0 host/CI smoke: EPT-violation exclusivity → RAYNU-V-M6-EPTVIO-OK.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

MARKER="${MARKER_EPTVIO:-RAYNU-V-M6-EPTVIO-OK}"
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
  echo "error: ept_model still contains admit( — M6.0 forbids admit in ept_model" >&2
  exit 1
fi
if ! grep -q 'enum EptViolationDisposition' "$MODEL_SRC"; then
  echo "error: missing EptViolationDisposition" >&2
  exit 1
fi
if ! grep -q 'theorem_ept_violation_preserves_exclusive' "$MODEL_SRC"; then
  echo "error: missing theorem_ept_violation_preserves_exclusive" >&2
  exit 1
fi
if ! grep -q 'violation_enabled' "$MODEL_SRC"; then
  echo "error: missing violation_enabled" >&2
  exit 1
fi
if ! grep -q "$MARKER" "$MODEL_SRC"; then
  echo "error: ept_model must embed marker $MARKER" >&2
  exit 1
fi
if ! grep -q 'GAP(CLOSED M6.0): EPT violation handler preserves exclusivity' "$ROOT/memory/ept_proof.rs"; then
  echo "error: ept_proof must close EPT violation handler GAP for M6.0" >&2
  exit 1
fi
if ! grep -q 'TODO(M6.0 CLOSED): EPT-violation exclusivity' "$ROOT/memory/ept_spec.rs"; then
  echo "error: ept_spec must close TODO(M6.0) EPT-violation exclusivity" >&2
  exit 1
fi
if ! grep -q 'fn apply_violation_disposition(' "$ROOT/memory/ept.rs"; then
  echo "error: missing apply_violation_disposition runtime hook" >&2
  exit 1
fi

echo "==> cargo test m6_0_eptvio_gate_passes (artifact gate)"
cargo test --lib m6_0_eptvio_gate_passes -- --nocapture

echo "==> cargo verus verify -p ept_model (M6.0 EPT-violation; prior L3 still green)"
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
echo "==> Verus EPT-violation smoke PASSED (M6.0)"
