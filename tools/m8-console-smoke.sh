#!/usr/bin/env bash
# M8.3 host/CI smoke: SPA POST /console/keys → guest COM1 → RAYNU-V-M8-CONSOLE-HOST-OK.
# GET /logs/serial stays HV UART. Never prints RAYNU-V-M8-CONSOLE-OK.
# Iron close is typing in Alpine from the SPA on BCM5720, then COM2. Not VNC.
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
if ! grep -q 'FirmwareSpaKeys' "$ROOT/mgmt/console.rs"; then
  echo "error: ConsoleMode::FirmwareSpaKeys required" >&2
  exit 1
fi
if ! grep -q '/console/keys' "$ROOT/mgmt/http.rs" "$ROOT/assets/webui.html"; then
  echo "error: SPA POST /console/keys required" >&2
  exit 1
fi
if grep -q 'println!("RAYNU-V-M8-CONSOLE-OK")' "$ROOT/mgmt/console.rs" "$ROOT/mgmt/http.rs" "$ROOT/mgmt/host_nic_listen.rs"; then
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
