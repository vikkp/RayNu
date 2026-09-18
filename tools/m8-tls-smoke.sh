#!/usr/bin/env bash
# M8.1 host/CI smoke: rustls around the HTTP codec → RAYNU-V-M8-TLS-HOST-OK.
# Firmware coexist stays plaintext. Never prints RAYNU-V-M8-TLS-OK.
# Iron close is curl --cacert on 10.99.99.x:8443 after BOOT-OK (not this script).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

MARKER="${MARKER_M8_TLS_HOST:-RAYNU-V-M8-TLS-HOST-OK}"

if [[ ! -f "$ROOT/mgmt/tls.rs" ]]; then
  echo "error: missing mgmt/tls.rs" >&2
  exit 1
fi
if ! grep -q 'fn prop_tls_host_package(' "$ROOT/mgmt/tls.rs"; then
  echo "error: missing prop_tls_host_package" >&2
  exit 1
fi
if ! grep -q 'plaintext HTTP' "$ROOT/mgmt/tls.rs"; then
  echo "error: firmware plaintext note required" >&2
  exit 1
fi
if grep -q 'println!("RAYNU-V-M8-TLS-OK")' "$ROOT/mgmt/tls.rs" "$ROOT/mgmt/http.rs" "$ROOT/mgmt/http_listen.rs"; then
  echo "error: host must never println iron TLS-OK" >&2
  exit 1
fi
if ! grep -q '\[dev-dependencies\]' "$ROOT/Cargo.toml"; then
  echo "error: rustls must stay a host dev-dependency" >&2
  exit 1
fi
if grep -E 'uefi-bin = \[.*rustls' "$ROOT/Cargo.toml"; then
  echo "error: rustls must not join uefi-bin (ADR-003)" >&2
  exit 1
fi

OUT="$(cargo test --lib m8_tls_host_gate_passes -- --nocapture 2>&1)"
echo "$OUT"
if ! echo "$OUT" | grep -q "$MARKER"; then
  echo "error: missing $MARKER" >&2
  exit 1
fi
echo "$MARKER"
