#!/usr/bin/env bash
# M8.1 firmware-wrap host/CI smoke: coexist feed/take/wrap → RAYNU-V-M8-TLS-FW-HOST-OK.
# Firmware coexist is TLS 1.2 (Tls12Listen). Never prints RAYNU-V-M8-TLS-OK.
# Iron close is curl --cacert on 10.99.99.x:8443 after BOOT-OK (not this script).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

MARKER="${MARKER_M8_TLS_FW_HOST:-RAYNU-V-M8-TLS-FW-HOST-OK}"

if [[ ! -f "$ROOT/mgmt/tls_coexist.rs" ]]; then
  echo "error: missing mgmt/tls_coexist.rs" >&2
  exit 1
fi
if ! grep -q 'fn prop_tls_fw_wrap_package(' "$ROOT/mgmt/tls_coexist.rs"; then
  echo "error: missing prop_tls_fw_wrap_package" >&2
  exit 1
fi
if ! grep -q 'Tls12Listen' "$ROOT/mgmt/host_nic_listen.rs"; then
  echo "error: coexist must feed Tls12Listen" >&2
  exit 1
fi
if grep -q 'println!("RAYNU-V-M8-TLS-OK")' "$ROOT/mgmt/tls_coexist.rs" "$ROOT/mgmt/host_nic_listen.rs" "$ROOT/mgmt/tls12.rs"; then
  echo "error: host must never println iron TLS-OK" >&2
  exit 1
fi
if grep -E 'uefi-bin = \[.*rustls' "$ROOT/Cargo.toml"; then
  echo "error: rustls must not join uefi-bin (ring needs libc)" >&2
  exit 1
fi

OUT="$(cargo test --lib m8_tls_fw_host_gate_passes -- --nocapture 2>&1)"
echo "$OUT"
if ! echo "$OUT" | grep -q "$MARKER"; then
  echo "error: missing $MARKER" >&2
  exit 1
fi
echo "$MARKER"
