#!/usr/bin/env bash
# Enforce ADR-003 binary size budget: target 15 MB, hard limit 20 MB.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

TARGET="${TARGET:-x86_64-unknown-uefi}"
PROFILE="${PROFILE:-release}"
EFI="target/${TARGET}/${PROFILE}/r640-hypervisor.efi"
TARGET_BYTES=$((15 * 1024 * 1024))
HARD_BYTES=$((20 * 1024 * 1024))

if [[ ! -f "$EFI" ]]; then
  echo "error: missing $EFI — run tools/build.sh first" >&2
  exit 1
fi

SIZE=$(wc -c < "$EFI" | tr -d ' ')
echo "r640-hypervisor.efi size: ${SIZE} bytes (target <= ${TARGET_BYTES}, hard <= ${HARD_BYTES})"

if (( SIZE > HARD_BYTES )); then
  echo "FAIL: exceeds 20 MB hard limit (ADR-003) — size audit required" >&2
  exit 2
fi

if (( SIZE > TARGET_BYTES )); then
  echo "WARN: exceeds 15 MB target (ADR-003)"
  exit 0
fi

echo "OK: within 15 MB target"
