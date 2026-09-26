#!/usr/bin/env bash
# M8.7 host/CI smoke: frame pack + fwstate gate → RAYNU-V-M8-PERC-HOST-OK.
# Never prints RAYNU-V-M8-PERC-LUN-OK. No doorbell. durable_lun still skips PERC.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

MARKER="${MARKER_M8_PERC_HOST:-RAYNU-V-M8-PERC-HOST-OK}"

if [[ ! -f "$ROOT/mgmt/megaraid.rs" ]]; then
  echo "error: missing mgmt/megaraid.rs" >&2
  exit 1
fi
if ! grep -q 'fn pick_spare(' "$ROOT/mgmt/megaraid.rs"; then
  echo "error: missing pick_spare" >&2
  exit 1
fi
if ! grep -q 'fn adapter_reset_is_allowed(' "$ROOT/mgmt/megaraid.rs"; then
  echo "error: reset policy required" >&2
  exit 1
fi
if ! grep -q 'fn perc_fwstate_probe(' "$ROOT/mgmt/megaraid.rs"; then
  echo "error: missing perc_fwstate_probe" >&2
  exit 1
fi
if ! grep -q 'fn pci_cmd_for_fwstate_load(' "$ROOT/mgmt/megaraid.rs"; then
  echo "error: missing memory-space command policy" >&2
  exit 1
fi
if grep -q 'pci_write32(bus, dev, func, 0x00' "$ROOT/mgmt/megaraid.rs"; then
  echo "error: fwstate must not store a doorbell" >&2
  exit 1
fi
if ! grep -q 'perc_fwstate_probe()' "$ROOT/boot/handoff.rs"; then
  echo "error: fwstate probe is not on the post-EBS path" >&2
  exit 1
fi
if grep -q 'println!("RAYNU-V-M8-PERC-LUN-OK")' "$ROOT/mgmt/megaraid.rs" "$ROOT/mgmt/durable_lun.rs" "$ROOT/mgmt/m8_perc_gate.rs"; then
  echo "error: host must never println iron PERC-LUN-OK" >&2
  exit 1
fi

OUT="$(cargo test --lib m8_perc_host_gate_passes -- --nocapture 2>&1)"
echo "$OUT"
if ! echo "$OUT" | grep -q "$MARKER"; then
  echo "error: missing $MARKER" >&2
  exit 1
fi
if echo "$OUT" | grep -F 'RAYNU-V-M8-PERC-LUN-OK' | grep -q .; then
  echo "error: iron PERC-LUN-OK printed" >&2
  exit 1
fi
echo "$MARKER"
