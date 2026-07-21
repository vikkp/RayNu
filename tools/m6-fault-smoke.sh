#!/usr/bin/env bash
# M6.7 host/CI smoke: fault injection suite → RAYNU-V-M6-FAULT-OK.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

MARKER="${MARKER_M6_FAULT:-RAYNU-V-M6-FAULT-OK}"

if [[ ! -f "$ROOT/mgmt/fault.rs" ]]; then
  echo "error: missing mgmt/fault.rs" >&2
  exit 1
fi
if ! grep -q 'fn prop_fault_suite(' "$ROOT/mgmt/fault.rs"; then
  echo "error: missing prop_fault_suite" >&2
  exit 1
fi
if ! grep -q 'fn prop_kill_vcpu_recover(' "$ROOT/mgmt/fault.rs"; then
  echo "error: missing prop_kill_vcpu_recover" >&2
  exit 1
fi
if ! grep -q 'fn prop_corrupt_page_fail_closed(' "$ROOT/mgmt/fault.rs"; then
  echo "error: missing prop_corrupt_page_fail_closed" >&2
  exit 1
fi
if ! grep -q 'fn prop_drop_irq_fail_closed(' "$ROOT/mgmt/fault.rs"; then
  echo "error: missing prop_drop_irq_fail_closed" >&2
  exit 1
fi
if ! grep -q 'fn prop_net_partition_recover(' "$ROOT/mgmt/fault.rs"; then
  echo "error: missing prop_net_partition_recover" >&2
  exit 1
fi
if ! grep -q 'GAP(CLOSED M6.7): Fault injection' "$ROOT/mgmt/fault.rs"; then
  echo "error: fault GAP must be CLOSED M6.7" >&2
  exit 1
fi
if ! grep -q "$MARKER" "$ROOT/mgmt/fault.rs"; then
  echo "error: fault must embed marker $MARKER" >&2
  exit 1
fi
if ! grep -q 'fn tear_down(' "$ROOT/sched/vcpu.rs"; then
  echo "error: missing Vcpu::tear_down" >&2
  exit 1
fi
if ! grep -q 'fn set_partitioned(' "$ROOT/net/mod.rs"; then
  echo "error: missing VSwitch::set_partitioned" >&2
  exit 1
fi
if ! grep -q 'FaultInjected' "$ROOT/audit/integrity.rs"; then
  echo "error: missing AuditEvent::FaultInjected" >&2
  exit 1
fi
if [[ ! -f "$ROOT/docs/runbooks/fault.md" ]]; then
  echo "error: missing docs/runbooks/fault.md" >&2
  exit 1
fi

echo "==> cargo test m6_7_fault_gate_passes (artifact gate)"
cargo test --lib m6_7_fault_gate_passes -- --nocapture

echo "==> cargo test prop_fault_suite"
cargo test --lib fault_suite -- --nocapture

echo "$MARKER"
echo "==> M6.7 fault smoke PASSED"
