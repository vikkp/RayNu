#!/usr/bin/env bash
# M3.15 host/CI smoke: install pinned Verus, run verus + cargo verus, print marker.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

MARKER="${MARKER_VERUS:-RAYNU-V-M3-VERUS-OK}"
PIN="$ROOT/verus-version.toml"
version="$(sed -n 's/^version = "\([^"]*\)"/\1/p' "$PIN" | head -1)"
VERUS_HOME="${VERUS_HOME:-$ROOT/target/verus}"

"$ROOT/tools/install-verus.sh"
export PATH="$VERUS_HOME:$PATH"

if [[ ! -x "$VERUS_HOME/verus" || ! -x "$VERUS_HOME/cargo-verus" ]]; then
  echo "error: verus / cargo-verus missing under $VERUS_HOME" >&2
  exit 1
fi

echo "==> verus --version"
out="$("$VERUS_HOME/verus" --version)"
echo "$out"
if ! grep -q "$version" <<<"$out"; then
  echo "error: verus --version did not report pinned $version" >&2
  exit 1
fi

echo "==> cargo verus verify (smoke; crate may not opt into proofs yet)"
cargo verus verify --no-default-features

echo "$MARKER"
echo "==> Verus pin smoke PASSED (M3.15)"
