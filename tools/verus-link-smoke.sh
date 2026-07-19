#!/usr/bin/env bash
# M3.16 host/CI smoke: frozen Verus pin + cargo verus verify -p ept_model.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

MARKER="${MARKER_L3_LINK:-RAYNU-V-M3-L3-LINK-OK}"
VERUS_HOME="${VERUS_HOME:-$ROOT/target/verus}"

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

echo "==> cargo verus verify -p ept_model (M3.16 link)"
# Clean so Cargo.toml metadata.verus is picked up reliably.
cargo clean -p ept_model >/dev/null 2>&1 || true
out="$(cargo verus verify -p ept_model 2>&1)"
echo "$out"
if ! grep -q '0 errors' <<<"$out"; then
  echo "error: ept_model verification reported errors" >&2
  exit 1
fi
if ! grep -q 'verified' <<<"$out"; then
  echo "error: ept_model verification produced no verified count" >&2
  exit 1
fi

echo "$MARKER"
echo "==> Verus L3-link smoke PASSED (M3.16)"
