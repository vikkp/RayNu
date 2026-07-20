#!/usr/bin/env bash
# M3.22 / M5.2 / M5.4 — verify PE sections .askern / .asinit / .aswebui / .aschema (ADR-003).
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

OUT="${1:-target/x86_64-unknown-uefi/release/r640-hypervisor.efi}"
if [[ ! -f "$OUT" ]]; then
  # Some toolchains emit unsuffixed PE.
  ALT="target/x86_64-unknown-uefi/release/r640-hypervisor"
  if [[ -f "$ALT" ]]; then
    OUT="$ALT"
  else
    echo "error: EFI not found at $OUT (build with ./tools/build.sh first)" >&2
    exit 1
  fi
fi

DUMP=""
if command -v llvm-objdump >/dev/null 2>&1; then
  DUMP="llvm-objdump"
elif command -v objdump >/dev/null 2>&1; then
  DUMP="objdump"
else
  echo "error: need llvm-objdump or objdump to inspect PE sections" >&2
  exit 1
fi

HEADERS="$("$DUMP" -h "$OUT" 2>/dev/null || true)"
if ! grep -qE '\.askern' <<<"$HEADERS"; then
  echo "error: PE section .askern (assets.kernel) missing in $OUT" >&2
  echo "$HEADERS" >&2
  exit 1
fi
if ! grep -qE '\.asinit' <<<"$HEADERS"; then
  echo "error: PE section .asinit (assets.initrd) missing in $OUT" >&2
  echo "$HEADERS" >&2
  exit 1
fi
if ! grep -qE '\.aswebui' <<<"$HEADERS"; then
  echo "error: PE section .aswebui (assets.webui) missing in $OUT" >&2
  echo "$HEADERS" >&2
  exit 1
fi
if ! grep -qE '\.aschema' <<<"$HEADERS"; then
  echo "error: PE section .aschema (assets.schemas) missing in $OUT" >&2
  echo "$HEADERS" >&2
  exit 1
fi

echo "==> PE assets OK (.askern + .asinit + .aswebui + .aschema) in $OUT"
ls -la "$OUT"
