#!/usr/bin/env bash
# M7.1 host/CI smoke: network HTTP mgmt plane → RAYNU-V-M7-HTTP-OK.
# Proves in-binary HTTP codec + host TCP listener (SPA + Bearer REST).
# Firmware NIC listen remains stubbed (see docs/runbooks/mgmt_http.md).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

MARKER="${MARKER_M7_HTTP:-RAYNU-V-M7-HTTP-OK}"

if [[ ! -f "$ROOT/mgmt/http.rs" ]]; then
  echo "error: missing mgmt/http.rs" >&2
  exit 1
fi
if ! grep -q 'fn prop_http_mgmt_package(' "$ROOT/mgmt/http.rs"; then
  echo "error: missing prop_http_mgmt_package" >&2
  exit 1
fi
if ! grep -q 'GAP(CLOSED M7.1): Network HTTPS/HTTP mgmt' "$ROOT/mgmt/http.rs"; then
  echo "error: HTTP GAP must be CLOSED M7.1" >&2
  exit 1
fi
if ! grep -q 'plaintext HTTP' "$ROOT/mgmt/http.rs"; then
  echo "error: lab plaintext HTTP note required" >&2
  exit 1
fi
if [[ ! -f "$ROOT/docs/runbooks/mgmt_http.md" ]]; then
  echo "error: missing docs/runbooks/mgmt_http.md" >&2
  exit 1
fi
if ! grep -q 'hostfwd' "$ROOT/docs/runbooks/mgmt_http.md"; then
  echo "error: runbook must document QEMU hostfwd" >&2
  exit 1
fi
if [[ ! -f "$ROOT/mgmt/http_listen.rs" ]]; then
  echo "error: missing mgmt/http_listen.rs" >&2
  exit 1
fi
if ! grep -q 'UnsupportedOnFirmware' "$ROOT/mgmt/http_listen.rs"; then
  echo "error: UEFI listen stub must be honest" >&2
  exit 1
fi

echo "==> cargo test m7_1_http_gate_passes (artifact gate)"
cargo test --lib m7_1_http_gate_passes -- --nocapture

echo "==> cargo test http_mgmt_package"
cargo test --lib http_mgmt_package -- --nocapture

echo "==> cargo test host_tcp_serves_spa_and_authed_rest"
cargo test --lib host_tcp_serves_spa_and_authed_rest -- --nocapture

echo "$MARKER"
echo "==> M7.1 network HTTP mgmt smoke PASSED"
