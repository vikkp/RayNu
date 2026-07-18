#!/usr/bin/env bash
# M0 integration gate: build EFI, boot under QEMU, require serial marker.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

MARKER="${MARKER:-RAYNU-V-M0-BOOT-OK}"
TIMEOUT_SECS="${TIMEOUT_SECS:-60}"
SERIAL_LOG="${SERIAL_LOG:-$ROOT/target/m0-serial.log}"
ESP="${ESP:-$ROOT/target/m0-esp}"

mkdir -p "$(dirname "$SERIAL_LOG")" "$ESP/EFI/BOOT"

echo "==> Building EFI"
"$ROOT/tools/build.sh"

echo "==> Running QEMU boot test (timeout ${TIMEOUT_SECS}s)"
rm -f "$SERIAL_LOG"
: >"$SERIAL_LOG"

set +e
# isa-debug-exit with outb(0xf4, 0x10) yields process status 33.
timeout --signal=KILL "$TIMEOUT_SECS" \
  env ESP="$ESP" SERIAL_CHARDEV="file:$SERIAL_LOG" \
  "$ROOT/tools/run-qemu.sh" \
  >"$ROOT/target/m0-qemu-stdout.log" 2>"$ROOT/target/m0-qemu-stderr.log"
QEMU_STATUS=$?
set -e

echo "==> QEMU exit status: $QEMU_STATUS"
echo "==> Serial log: $SERIAL_LOG"
if [[ ! -s "$SERIAL_LOG" ]]; then
  echo "error: serial log empty or missing" >&2
  echo "----- qemu stderr -----"
  cat "$ROOT/target/m0-qemu-stderr.log" || true
  echo "----- qemu stdout -----"
  cat "$ROOT/target/m0-qemu-stdout.log" || true
  exit 1
fi

echo "----- serial begin -----"
cat "$SERIAL_LOG" || true
echo "----- serial end -----"

if ! grep -qF "$MARKER" "$SERIAL_LOG"; then
  echo "error: M0 marker '$MARKER' not found on serial output" >&2
  echo "----- qemu stderr -----"
  cat "$ROOT/target/m0-qemu-stderr.log" || true
  exit 1
fi

echo "==> M0 QEMU boot gate PASSED (marker found; qemu status=$QEMU_STATUS)"
exit 0
