#!/usr/bin/env bash
# Build the single r640-hypervisor.efi [Z].
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

TARGET="${TARGET:-x86_64-unknown-uefi}"
PROFILE="${PROFILE:-release}"

# Build identity for the M0 banner (`build: sha=…`). CI supplies GITHUB_SHA;
# local builds fall back to HEAD. `option_env!` in src/lib.rs reads this.
if [[ -z "${RAYNU_BUILD_SHA:-}" ]]; then
  RAYNU_BUILD_SHA="${GITHUB_SHA:-$(git rev-parse HEAD 2>/dev/null || echo unknown)}"
fi
RAYNU_BUILD_SHA="${RAYNU_BUILD_SHA:0:12}"
export RAYNU_BUILD_SHA

echo "==> Building r640-hypervisor ($PROFILE / $TARGET) sha=$RAYNU_BUILD_SHA"
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

# M3.22: PE .askern / .asinit must be present (ADR-003).
if [[ -f "$OUT" ]]; then
  "$ROOT/tools/check-pe-assets.sh" "$OUT"
elif [[ -f "target/${TARGET}/${PROFILE}/r640-hypervisor" ]]; then
  "$ROOT/tools/check-pe-assets.sh" "target/${TARGET}/${PROFILE}/r640-hypervisor"
fi
