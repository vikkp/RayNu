#!/usr/bin/env bash
# M5.6 host/CI smoke: Dell Tier-1 iDRAC → RAYNU-V-M5-IDRAC-OK.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

MARKER="${MARKER_M5_IDRAC:-RAYNU-V-M5-IDRAC-OK}"

if [[ ! -f "$ROOT/assets/idrac/mock_redfish.json" ]]; then
  echo "error: missing mock_redfish.json" >&2
  exit 1
fi
if [[ ! -f "$ROOT/assets/idrac/mock_topology.txt" ]]; then
  echo "error: missing mock_topology.txt" >&2
  exit 1
fi
if ! grep -q 'Temperatures' "$ROOT/assets/idrac/mock_redfish.json"; then
  echo "error: mock Redfish missing Temperatures" >&2
  exit 1
fi
if ! grep -q 'Fans' "$ROOT/assets/idrac/mock_redfish.json"; then
  echo "error: mock Redfish missing Fans" >&2
  exit 1
fi
if ! grep -q 'PowerSupplies' "$ROOT/assets/idrac/mock_redfish.json"; then
  echo "error: mock Redfish missing PowerSupplies" >&2
  exit 1
fi
if ! grep -q '^dimm ' "$ROOT/assets/idrac/mock_topology.txt"; then
  echo "error: topology missing dimm rows" >&2
  exit 1
fi
if ! grep -q '^cpu ' "$ROOT/assets/idrac/mock_topology.txt"; then
  echo "error: topology missing cpu (MADT) rows" >&2
  exit 1
fi
if ! grep -q '^numa ' "$ROOT/assets/idrac/mock_topology.txt"; then
  echo "error: topology missing numa (SRAT) rows" >&2
  exit 1
fi
if [[ ! -f "$ROOT/idrac/mod.rs" ]]; then
  echo "error: missing idrac/mod.rs" >&2
  exit 1
fi
if ! grep -q 'fn read_tier1_health(' "$ROOT/idrac/mod.rs"; then
  echo "error: missing read_tier1_health" >&2
  exit 1
fi
if ! grep -q 'fn parse_topology(' "$ROOT/idrac/mod.rs"; then
  echo "error: missing parse_topology" >&2
  exit 1
fi
if ! grep -q "$MARKER" "$ROOT/idrac/mod.rs"; then
  echo "error: idrac must embed marker $MARKER" >&2
  exit 1
fi
if ! grep -q 'GAP: Dell Tier-2 OEM' "$ROOT/idrac/mod.rs"; then
  echo "error: idrac must document Tier-2 GAP" >&2
  exit 1
fi
if ! grep -q 'GAP: live Redfish BMC' "$ROOT/idrac/mod.rs"; then
  echo "error: idrac must document live BMC GAP" >&2
  exit 1
fi

echo "==> cargo test m5_6_idrac_gate_passes (artifact gate)"
out="$(cargo test --lib m5_6_idrac_gate_passes -- --nocapture 2>&1)"
echo "$out"
echo "$out" | grep -q 'm5_6_idrac_gate_passes ... ok'
echo "$out" | grep -q "$MARKER"

echo "==> cargo test tier1_health_and_topology"
out2="$(cargo test --lib tier1_health_and_topology -- --nocapture 2>&1)"
echo "$out2"
echo "$out2" | grep -q 'tier1_health_and_topology ... ok'

echo "$MARKER"
echo "==> M5.6 idrac smoke PASSED"
