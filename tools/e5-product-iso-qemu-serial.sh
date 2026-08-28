#!/usr/bin/env bash
# Nested QEMU serial capture of Stage 46 product ISO (alpine-virt).
#
# Not ISO-INSTALL-OK. Nested product-ISO HOLDS and seeds leftover DRAM
# above PRECISE (run-qemu.sh defaults QEMU_MEM=2560M). iso=0 stays 512M
# and does not seed. Host/CI must never print RAYNU-V-M7-ISO-INSTALL-OK.
# Iron close stays Cruzer flash of cursor/e5-stage46-iso-a623.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

TIMEOUT_SECS="${TIMEOUT_SECS:-480}"
SERIAL_LOG="${SERIAL_LOG:-$ROOT/target/e5-iso-serial.log}"
ESP="${ESP:-$ROOT/target/e5-iso-esp}"
ISO_URL="${ALPINE_ISO_URL:-https://dl-cdn.alpinelinux.org/alpine/v3.21/releases/x86_64/alpine-virt-3.21.3-x86_64.iso}"
ISO_PATH="${PRODUCT_ISO:-$ROOT/target/alpine-virt-3.21.3-x86_64.iso}"

mkdir -p "$(dirname "$SERIAL_LOG")" "$ESP" "$(dirname "$ISO_PATH")"

if [[ ! -f "$ISO_PATH" ]]; then
  echo "==> fetching alpine-virt ISO to $ISO_PATH"
  curl -fsSL -o "$ISO_PATH" "$ISO_URL"
fi
psz=$(wc -c <"$ISO_PATH" | tr -d ' ')
if (( psz <= 73728 )); then
  echo "error: PRODUCT_ISO is lab-stub sized ($psz)" >&2
  exit 1
fi

if [[ -e /dev/kvm ]]; then
  sudo chmod a+rw /dev/kvm || true
fi
if [[ -x "$ROOT/tools/enable-nested-kvm.sh" ]]; then
  sudo "$ROOT/tools/enable-nested-kvm.sh" || true
fi

rm -f "$SERIAL_LOG"
: >"$SERIAL_LOG"

echo "==> product ISO nested serial timeout=${TIMEOUT_SECS}s iso=$psz bytes (not ISO-INSTALL-OK)"
set +e
timeout --signal=KILL "$TIMEOUT_SECS" \
  env PRODUCT_ISO="$ISO_PATH" ESP="$ESP" SERIAL_CHARDEV="file:$SERIAL_LOG" \
  QEMU_ACCEL="${QEMU_ACCEL:-kvm}" \
  "$ROOT/tools/run-qemu.sh" \
  >"$ROOT/target/e5-iso-qemu-stdout.log" 2>"$ROOT/target/e5-iso-qemu-stderr.log"
QEMU_STATUS=$?
set -e

echo "==> QEMU exit status: $QEMU_STATUS (124/137 = timeout; expected)"
if [[ ! -s "$SERIAL_LOG" ]]; then
  echo "error: serial log empty" >&2
  cat "$ROOT/target/e5-iso-qemu-stderr.log" || true
  exit 1
fi

echo "==> marker scan (not ISO-INSTALL-OK):"
grep -E -n 'VMLAUNCH-OK|OVMF-ELTORITO-OK|RN-ELT|Loaded initrd|linux deliver|linux cpuid|linux skip-|invlpg miss|Linux version|invalid opcode|Oops:|ISO-INSTALL-OK|report-RAM extra|stop n=' \
  "$SERIAL_LOG" | head -n 80 || true

if grep -qF 'RAYNU-V-M7-ISO-INSTALL-OK' "$SERIAL_LOG"; then
  echo "error: nested/host printed iron ISO-INSTALL-OK" >&2
  exit 1
fi
if ! grep -qF 'RAYNU-V-M7-E5-OVMF-VMLAUNCH-OK' "$SERIAL_LOG"; then
  echo "error: no guest-UEFI VMLAUNCH in serial" >&2
  exit 1
fi
echo "==> VMLAUNCH seen; nested serial captured (not ISO-INSTALL-OK)"
exit 0
