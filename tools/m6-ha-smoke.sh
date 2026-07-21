#!/usr/bin/env bash
# M6.6 host/CI smoke: HA failover + harden → RAYNU-V-M6-HA-OK.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

MARKER="${MARKER_M6_HA:-RAYNU-V-M6-HA-OK}"

if [[ ! -f "$ROOT/mgmt/ha.rs" ]]; then
  echo "error: missing mgmt/ha.rs" >&2
  exit 1
fi
if ! grep -q 'fn failover_to_standby(' "$ROOT/mgmt/ha.rs"; then
  echo "error: missing failover_to_standby" >&2
  exit 1
fi
if ! grep -q 'fn prop_ha_failover_restart(' "$ROOT/mgmt/ha.rs"; then
  echo "error: missing prop_ha_failover_restart" >&2
  exit 1
fi
if ! grep -q 'fn prop_security_harden_checklist(' "$ROOT/mgmt/ha.rs"; then
  echo "error: missing prop_security_harden_checklist" >&2
  exit 1
fi
if ! grep -q 'GAP(CLOSED M6.6): HA / security harden' "$ROOT/mgmt/ha.rs"; then
  echo "error: HA GAP must be CLOSED M6.6" >&2
  exit 1
fi
if ! grep -q "$MARKER" "$ROOT/mgmt/ha.rs"; then
  echo "error: ha must embed marker $MARKER" >&2
  exit 1
fi
if ! grep -q 'HaFailoverStarted' "$ROOT/audit/integrity.rs"; then
  echo "error: missing AuditEvent::HaFailoverStarted" >&2
  exit 1
fi
if ! grep -q 'HaFailoverCompleted' "$ROOT/audit/integrity.rs"; then
  echo "error: missing AuditEvent::HaFailoverCompleted" >&2
  exit 1
fi
if [[ ! -f "$ROOT/docs/runbooks/ha.md" ]]; then
  echo "error: missing docs/runbooks/ha.md" >&2
  exit 1
fi

echo "==> cargo test m6_6_ha_gate_passes (artifact gate)"
cargo test --lib m6_6_ha_gate_passes -- --nocapture

echo "==> cargo test ha_failover_restart"
cargo test --lib ha_failover_restart -- --nocapture

echo "==> cargo test security_harden_checklist"
cargo test --lib security_harden_checklist -- --nocapture

echo "$MARKER"
echo "==> M6.6 HA smoke PASSED"
