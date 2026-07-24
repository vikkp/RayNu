#!/usr/bin/env bash
# ADR-011 evidence-mode smoke gate.
#
# Builds the EFI, stages paperverbose.txt on the ESP via EVIDENCE_MODE=1,
# boots under QEMU, and requires:
#   RAYNU-V-EVIDENCE-MODE-ON
#   RAYNU-V-AUDIT: EvidenceModeActivated
#   === RAYNU-V EVIDENCE BUNDLE BEGIN ===
#
# Also requires the normal M0 boot marker so a silent hang still fails.
# Does not require full M1–M4 VMX path (works under TCG).
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

MARKER_M0="${MARKER_M0:-RAYNU-V-M0-BOOT-OK}"
MARKER_EVIDENCE="${MARKER_EVIDENCE:-RAYNU-V-EVIDENCE-MODE-ON}"
MARKER_AUDIT="${MARKER_AUDIT:-RAYNU-V-AUDIT: EvidenceModeActivated}"
MARKER_BUNDLE="${MARKER_BUNDLE:-=== RAYNU-V EVIDENCE BUNDLE BEGIN ===}"
TIMEOUT_SECS="${TIMEOUT_SECS:-120}"
SERIAL_LOG="${SERIAL_LOG:-$ROOT/target/evidence-serial.log}"
ESP="${ESP:-$ROOT/target/evidence-esp}"

mkdir -p "$(dirname "$SERIAL_LOG")" "$ESP/EFI/BOOT"

echo "==> Building EFI"
"$ROOT/tools/build.sh"

echo "==> Evidence-mode QEMU smoke (timeout ${TIMEOUT_SECS}s)"
rm -f "$SERIAL_LOG"
: >"$SERIAL_LOG"

set +e
timeout --signal=KILL "$TIMEOUT_SECS" \
  env ESP="$ESP" SERIAL_CHARDEV="file:$SERIAL_LOG" EVIDENCE_MODE=1 \
  "$ROOT/tools/run-qemu.sh" \
  >"$ROOT/target/evidence-qemu-stdout.log" 2>"$ROOT/target/evidence-qemu-stderr.log"
QEMU_STATUS=$?
set -e

echo "==> QEMU exit status: $QEMU_STATUS"
echo "==> Serial log: $SERIAL_LOG"
if [[ ! -s "$SERIAL_LOG" ]]; then
  echo "error: serial log empty or missing" >&2
  echo "----- qemu stderr -----"
  cat "$ROOT/target/evidence-qemu-stderr.log" || true
  exit 1
fi

echo "----- serial begin -----"
cat "$SERIAL_LOG" || true
echo "----- serial end -----"

fail=0
for m in "$MARKER_M0" "$MARKER_EVIDENCE" "$MARKER_AUDIT" "$MARKER_BUNDLE"; do
  if ! grep -qF "$m" "$SERIAL_LOG"; then
    echo "error: marker '$m' not found on serial output" >&2
    fail=1
  else
    echo "==> found: $m"
  fi
done

if [[ "$fail" -ne 0 ]]; then
  echo "----- qemu stderr -----"
  cat "$ROOT/target/evidence-qemu-stderr.log" || true
  exit 1
fi

echo "==> Evidence-mode smoke PASSED (qemu status=$QEMU_STATUS)"
exit 0
