#!/usr/bin/env bash
# Generate assets/bzImage — minimal bzImage with proto-kernel at PM+0x200 (M3.7).
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
OUT="${1:-$ROOT/assets/bzImage}"
mkdir -p "$(dirname "$OUT")"
export RAYNU_WRITE_BZIMAGE="$OUT"
cargo test --no-default-features \
  guest::linux_boot::linux_boot_test::write_minimal_bzimage_fixture \
  -- --exact --nocapture
ls -la "$OUT"
echo "==> wrote $OUT"
