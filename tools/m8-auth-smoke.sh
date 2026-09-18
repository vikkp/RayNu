#!/usr/bin/env bash
# M8.2 host/CI smoke: operator token is product latch → RAYNU-V-M8-AUTH-HOST-OK.
# Firmware REST still accepts raynu-v-bringup when no ESP auth.token.
# Never prints RAYNU-V-M8-AUTH-OK. Iron close is ESP token required after BOOT-OK.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

MARKER="${MARKER_M8_AUTH_HOST:-RAYNU-V-M8-AUTH-HOST-OK}"

if [[ ! -f "$ROOT/mgmt/auth.rs" ]]; then
  echo "error: missing mgmt/auth.rs" >&2
  exit 1
fi
if ! grep -q 'fn prop_auth_host_package(' "$ROOT/mgmt/auth.rs"; then
  echo "error: missing prop_auth_host_package" >&2
  exit 1
fi
if ! grep -q 'HostReady' "$ROOT/mgmt/auth.rs"; then
  echo "error: AuthMode::HostReady required" >&2
  exit 1
fi
if grep -q 'println!("RAYNU-V-M8-AUTH-OK")' "$ROOT/mgmt/auth.rs" "$ROOT/mgmt/http.rs" "$ROOT/mgmt/api.rs"; then
  echo "error: host must never println iron AUTH-OK" >&2
  exit 1
fi

OUT="$(cargo test --lib m8_auth_host_gate_passes -- --nocapture 2>&1)"
echo "$OUT"
if ! echo "$OUT" | grep -q "$MARKER"; then
  echo "error: missing $MARKER" >&2
  exit 1
fi
# HOST-OK is a different string from AUTH-OK (AUTH-HOST-OK does not contain AUTH-OK).
if echo "$OUT" | grep -F 'RAYNU-V-M8-AUTH-OK' | grep -vF 'RAYNU-V-M8-AUTH-HOST-OK' | grep -q .; then
  echo "error: iron AUTH-OK printed" >&2
  exit 1
fi
echo "$MARKER"
