#!/usr/bin/env bash
# Build the single r640-hypervisor.efi [Z].
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

TARGET="${TARGET:-x86_64-unknown-uefi}"
PROFILE="${PROFILE:-release}"

echo "==> Building r640-hypervisor ($PROFILE / $TARGET)"
cargo build \
  --"$PROFILE" \
  --target "$TARGET" \
  --features uefi-bin

OUT="target/${TARGET}/${PROFILE}/r640-hypervisor.efi"
if [[ ! -f "$OUT" ]]; then
  # rustc/uefi may emit .efi or a PE without suffix depending on toolchain
  ALT="target/${TARGET}/${PROFILE}/r640-hypervisor.efi"
  BIN="target/${TARGET}/${PROFILE}/r640-hypervisor"
  if [[ -f "$BIN" && ! -f "$OUT" ]]; then
    cp "$BIN" "$OUT"
  fi
fi

echo "==> Output: $OUT"
ls -la "$OUT" 2>/dev/null || ls -la "target/${TARGET}/${PROFILE}/r640-hypervisor"*
