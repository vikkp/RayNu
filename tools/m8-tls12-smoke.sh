#!/usr/bin/env bash
# M8.1 freestanding TLS 1.2 host/CI smoke: rustls client ↔ Tls12Listen →
# RAYNU-V-M8-TLS12-HOST-OK. Never prints RAYNU-V-M8-TLS-OK.
# Iron close is curl --cacert on 10.99.99.x:8443 after BOOT-OK (not this script).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

MARKER="${MARKER_M8_TLS12_HOST:-RAYNU-V-M8-TLS12-HOST-OK}"

if [[ ! -f "$ROOT/mgmt/tls12.rs" ]]; then
  echo "error: missing mgmt/tls12.rs" >&2
  exit 1
fi
if ! grep -q 'struct Tls12Listen' "$ROOT/mgmt/tls12.rs"; then
  echo "error: missing Tls12Listen" >&2
  exit 1
fi
if grep -q 'println!("RAYNU-V-M8-TLS-OK")' "$ROOT/mgmt/tls12.rs"; then
  echo "error: host must never println iron TLS-OK" >&2
  exit 1
fi
if grep -E 'uefi-bin = \[.*rustls' "$ROOT/Cargo.toml"; then
  echo "error: rustls must not join uefi-bin (ring needs libc)" >&2
  exit 1
fi

OUT="$(cargo test --lib rustls_tls12_client_gets_spa_from_freestanding_server -- --nocapture --test-threads=1 2>&1)"
echo "$OUT"
if ! echo "$OUT" | grep -q "$MARKER"; then
  echo "error: missing $MARKER" >&2
  exit 1
fi
echo "$MARKER"
