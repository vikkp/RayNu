#!/usr/bin/env bash
# M5.3 host/CI smoke: audit ring + hash chain → RAYNU-V-M5-AUDIT-OK.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

MARKER="${MARKER_M5_AUDIT:-RAYNU-V-M5-AUDIT-OK}"

if [[ ! -f "$ROOT/audit/integrity.rs" ]]; then
  echo "error: missing audit/integrity.rs" >&2
  exit 1
fi
if ! grep -q 'fn verify_chain(' "$ROOT/audit/integrity.rs"; then
  echo "error: missing verify_chain" >&2
  exit 1
fi
if ! grep -q 'fn tamper_hash_at(' "$ROOT/audit/integrity.rs"; then
  echo "error: missing tamper_hash_at" >&2
  exit 1
fi
if ! grep -q 'fn boot_ring_verify(' "$ROOT/audit/integrity.rs"; then
  echo "error: missing boot_ring_verify" >&2
  exit 1
fi
if ! grep -q "$MARKER" "$ROOT/audit/integrity.rs"; then
  echo "error: integrity must embed marker $MARKER" >&2
  exit 1
fi
if ! grep -q 'AuditEvent::VmcsCreated' "$ROOT/vmx/vmcs.rs"; then
  echo "error: VmcsCreated not wired in vmcs.rs" >&2
  exit 1
fi
if ! grep -q 'AuditEvent::VmCreated' "$ROOT/mgmt/mod.rs"; then
  echo "error: lifecycle audit events not wired in mgmt" >&2
  exit 1
fi

echo "==> cargo test m5_3_audit_gate_passes (artifact gate)"
out="$(cargo test --lib m5_3_audit_gate_passes -- --nocapture 2>&1)"
echo "$out"
echo "$out" | grep -q 'm5_3_audit_gate_passes ... ok'
echo "$out" | grep -q "$MARKER"

echo "==> cargo test prop_tamper_detected / mandatory chain"
out2="$(cargo test --lib tamper_is_detected -- --nocapture 2>&1)"
echo "$out2"
echo "$out2" | grep -q 'tamper_is_detected ... ok'
out3="$(cargo test --lib mandatory_events_chain -- --nocapture 2>&1)"
echo "$out3"
echo "$out3" | grep -q 'mandatory_events_chain ... ok'

echo "$MARKER"
echo "==> M5.3 audit smoke PASSED"
