#!/usr/bin/env bash
# M3.21 — hard-fail Kani smoke for M2.6 harnesses → RAYNU-V-M3-KANI-OK
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

MARKER="${MARKER_KANI:-RAYNU-V-M3-KANI-OK}"
PIN_FILE="$ROOT/kani-version.toml"
VERSION="$(sed -n 's/^version = "\([^"]*\)"/\1/p' "$PIN_FILE" | head -1)"
if [[ -z "$VERSION" ]]; then
  echo "error: cannot parse kani version from $PIN_FILE" >&2
  exit 1
fi

echo "==> Kani pin: kani-verifier ${VERSION}"

# Install with stable so the repo rust-toolchain.toml nightly does not hijack build.
if ! cargo +stable install --list 2>/dev/null | grep -q "kani-verifier v${VERSION}"; then
  cargo +stable install --locked "kani-verifier@${VERSION}"
fi

cargo kani setup
# --lib --tests: library unit-test modules only (skip [[bin]] uefi-bin).
# Unwind budget matches #[kani::unwind(16)] + MAP_CAP=8 under cfg(kani).
cargo kani --lib --tests \
  --default-unwind 16 \
  --harness kani_no_double_map_same_hpa \
  --harness kani_alloc_no_alias_double_free_rejected

echo "$MARKER"
echo "==> Kani smoke PASSED (M3.21)"
