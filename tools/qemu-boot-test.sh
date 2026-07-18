#!/usr/bin/env bash
# Boot integration gate: build EFI, boot under QEMU, require serial markers.
# M0: RAYNU-V-M0-BOOT-OK
# M1.0: RAYNU-V-M1-EBS-OK (must appear after ExitBootServices)
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

MARKER_M0="${MARKER_M0:-RAYNU-V-M0-BOOT-OK}"
MARKER_M1="${MARKER_M1:-RAYNU-V-M1-EBS-OK}"
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

fail=0
if ! grep -qF "$MARKER_M0" "$SERIAL_LOG"; then
  echo "error: M0 marker '$MARKER_M0' not found on serial output" >&2
  fail=1
fi
if ! grep -qF "$MARKER_M1" "$SERIAL_LOG"; then
  echo "error: M1.0 marker '$MARKER_M1' not found on serial output" >&2
  fail=1
fi

# M1.0 marker must appear after M0 marker in the log (ordering check).
if [[ "$fail" -eq 0 ]]; then
  m0_line=$(grep -nF "$MARKER_M0" "$SERIAL_LOG" | head -1 | cut -d: -f1)
  m1_line=$(grep -nF "$MARKER_M1" "$SERIAL_LOG" | head -1 | cut -d: -f1)
  if [[ -n "$m0_line" && -n "$m1_line" && "$m1_line" -le "$m0_line" ]]; then
    echo "error: M1.0 marker appeared before M0 marker (ordering)" >&2
    fail=1
  fi
fi

if [[ "$fail" -ne 0 ]]; then
  echo "----- qemu stderr -----"
  cat "$ROOT/target/m0-qemu-stderr.log" || true
  exit 1
fi

echo "==> Boot gate PASSED (M0 + M1.0 markers; qemu status=$QEMU_STATUS)"
exit 0
