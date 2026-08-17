#!/usr/bin/env bash
# ADR-013 Phase C QEMU lab: post-EBS GET / on host-owned e1000.
#
# Lab marker RAYNU-V-M7-HOST-NIC-QEMU-OK only.
# Never print iron RAYNU-V-M7-HOST-NIC-HTTP-OK.
# TCG is enough (listen is after EBS, before VMX).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

QEMU_OK="${MARKER_M7_HOST_NIC_QEMU:-RAYNU-V-M7-HOST-NIC-QEMU-OK}"
IRON="${MARKER_M7_HOST_NIC_HTTP:-RAYNU-V-M7-HOST-NIC-HTTP-OK}"
LAB_ARM="${MARKER_HOST_NIC_LAB_ARM:-boot: ADR-013 Phase C lab hostnic.txt armed (QEMU e1000)}"
LISTEN="${MARKER_HOST_NIC_LISTEN:-boot: HOST-NIC listening on 10.0.2.15:8443 (post-EBS e1000)}"
TIMEOUT_SECS="${TIMEOUT_SECS:-120}"
SERIAL_LOG="${SERIAL_LOG:-$ROOT/target/host-nic-qemu-serial.log}"
ESP="${ESP:-$ROOT/target/host-nic-qemu-esp}"
HOST_NIC_FWD="${HOST_NIC_FWD:-18443}"
QEMU_PID=""

cleanup() {
  if [[ -n "${QEMU_PID}" ]] && kill -0 "$QEMU_PID" 2>/dev/null; then
    kill "$QEMU_PID" 2>/dev/null || true
    wait "$QEMU_PID" 2>/dev/null || true
  fi
}
trap cleanup EXIT

echo "==> cargo test m7_8_host_nic_scaffold_passes (host)"
cargo test --lib m7_8_host_nic_scaffold_passes -- --nocapture

echo "==> Building EFI"
"$ROOT/tools/build.sh"

mkdir -p "$(dirname "$SERIAL_LOG")" "$ESP/EFI/BOOT"
rm -f "$SERIAL_LOG"
: >"$SERIAL_LOG"

echo "==> QEMU HOST-NIC lab (timeout ${TIMEOUT_SECS}s, hostfwd :${HOST_NIC_FWD})"
set +e
HOST_NIC_LAB=1 HOST_NIC_FWD="$HOST_NIC_FWD" ESP="$ESP" SERIAL_CHARDEV="file:$SERIAL_LOG" \
  QEMU_ACCEL="${QEMU_ACCEL:-tcg}" \
  timeout --signal=KILL "${TIMEOUT_SECS}" "$ROOT/tools/run-qemu.sh" \
  >"$ROOT/target/host-nic-qemu-stdout.log" 2>"$ROOT/target/host-nic-qemu-stderr.log" &
QEMU_PID=$!
set -e

deadline=$((SECONDS + TIMEOUT_SECS))
heard_listen=0
while (( SECONDS < deadline )); do
  if grep -qF "$LISTEN" "$SERIAL_LOG" 2>/dev/null; then
    heard_listen=1
    break
  fi
  if ! kill -0 "$QEMU_PID" 2>/dev/null; then
    break
  fi
  sleep 1
done

if [[ "$heard_listen" != "1" ]]; then
  echo "error: missing listen banner: $LISTEN" >&2
  tail -n 80 "$SERIAL_LOG" >&2 || true
  exit 1
fi
echo "==> listen banner OK"

curl_ok=0
body="$(mktemp)"
for _ in $(seq 1 30); do
  if curl -fsS --max-time 2 "http://127.0.0.1:${HOST_NIC_FWD}/" >"$body" 2>/dev/null; then
    if grep -qiE '<html|RayNu|text/html' "$body"; then
      curl_ok=1
      break
    fi
  fi
  sleep 1
done
rm -f "$body"

if [[ "$curl_ok" != "1" ]]; then
  echo "error: GET / via hostfwd :${HOST_NIC_FWD} failed" >&2
  tail -n 80 "$SERIAL_LOG" >&2 || true
  exit 1
fi
echo "==> curl GET / OK"

wait_deadline=$((SECONDS + 30))
while (( SECONDS < wait_deadline )); do
  if grep -qF "$QEMU_OK" "$SERIAL_LOG" 2>/dev/null; then
    break
  fi
  sleep 1
done

if ! grep -qF "$LAB_ARM" "$SERIAL_LOG"; then
  echo "error: missing lab arm: $LAB_ARM" >&2
  tail -n 80 "$SERIAL_LOG" >&2 || true
  exit 1
fi
if ! grep -qF "$QEMU_OK" "$SERIAL_LOG"; then
  echo "error: missing $QEMU_OK" >&2
  tail -n 80 "$SERIAL_LOG" >&2 || true
  exit 1
fi
if grep -qF "$IRON" "$SERIAL_LOG"; then
  echo "error: serial must not claim iron $IRON" >&2
  exit 1
fi

echo "$QEMU_OK"
echo "==> M7.8 HOST-NIC QEMU GET / PASSED (never print iron ${IRON})"
echo "never print iron marker from host smoke"
