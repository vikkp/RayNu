#!/usr/bin/env bash
# M6.4 host/CI smoke: REST auth → RAYNU-V-M6-AUTH-OK.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

MARKER="${MARKER_M6_AUTH:-RAYNU-V-M6-AUTH-OK}"

if [[ ! -f "$ROOT/mgmt/api.rs" ]]; then
  echo "error: missing mgmt/api.rs" >&2
  exit 1
fi
if ! grep -q 'fn auth_allows(' "$ROOT/mgmt/api.rs"; then
  echo "error: missing auth_allows" >&2
  exit 1
fi
if ! grep -q 'BRINGUP_AUTH_TOKEN' "$ROOT/mgmt/api.rs"; then
  echo "error: missing BRINGUP_AUTH_TOKEN" >&2
  exit 1
fi
if ! grep -q 'prop_auth_deny_allow' "$ROOT/mgmt/api.rs"; then
  echo "error: missing prop_auth_deny_allow" >&2
  exit 1
fi
if ! grep -q 'GAP(CLOSED M6.4): REST auth stubbed' "$ROOT/mgmt/api.rs"; then
  echo "error: auth GAP must be CLOSED M6.4" >&2
  exit 1
fi
if ! grep -q "$MARKER" "$ROOT/mgmt/api.rs"; then
  echo "error: api must embed marker $MARKER" >&2
  exit 1
fi
if ! grep -q 'AuthDenied' "$ROOT/audit/integrity.rs"; then
  echo "error: missing AuditEvent::AuthDenied" >&2
  exit 1
fi
if ! grep -q 'AuthAllowed' "$ROOT/audit/integrity.rs"; then
  echo "error: missing AuditEvent::AuthAllowed" >&2
  exit 1
fi

echo "==> cargo test m6_4_auth_gate_passes (artifact gate)"
cargo test --lib m6_4_auth_gate_passes -- --nocapture

echo "==> cargo test auth_deny_allow"
cargo test --lib auth_deny_allow -- --nocapture

echo "$MARKER"
echo "==> M6.4 REST auth smoke PASSED"
