#!/usr/bin/env bash
# M8.3 host/CI smoke: operator keys → guest COM1 → RAYNU-V-M8-CONSOLE-HOST-OK.
# Firmware SPA is still GET /logs/serial (HV UART). Never prints RAYNU-V-M8-CONSOLE-OK.
# Iron close is typing in the guest from the SPA after BOOT-OK. Not VNC.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

MARKER="${MARKER_M8_CONSOLE_HOST:-RAYNU-V-M8-CONSOLE-HOST-OK}"

if [[ ! -f "$ROOT/mgmt/console.rs" ]]; then
  echo "error: missing mgmt/console.rs" >&2
  exit 1
fi
if ! grep -q 'fn prop_console_host_package(' "$ROOT/mgmt/console.rs"; then
  echo "error: missing prop_console_host_package" >&2
  exit 1
fi
if ! grep -q 'HostReady' "$ROOT/mgmt/console.rs"; then
  echo "error: ConsoleMode::HostReady required" >&2
  exit 1
fi
if grep -q 'println!("RAYNU-V-M8-CONSOLE-OK")' "$ROOT/mgmt/console.rs" "$ROOT/mgmt/http.rs"; then
  echo "error: host must never println iron CONSOLE-OK" >&2
  exit 1
fi

OUT="$(cargo test --lib m8_console_host_gate_passes -- --nocapture 2>&1)"
echo "$OUT"
if ! echo "$OUT" | grep -q "$MARKER"; then
  echo "error: missing $MARKER" >&2
  exit 1
fi
# HOST-OK is a different string from CONSOLE-OK (CONSOLE-HOST-OK does not contain CONSOLE-OK).
if echo "$OUT" | grep -F 'RAYNU-V-M8-CONSOLE-OK' | grep -vF 'RAYNU-V-M8-CONSOLE-HOST-OK' | grep -q .; then
  echo "error: iron CONSOLE-OK printed" >&2
  exit 1
fi
echo "$MARKER"
